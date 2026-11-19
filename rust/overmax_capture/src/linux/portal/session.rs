use super::{receiver, Mailbox, StreamCommand};
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;
use zbus::{
    zvariant::{OwnedFd, OwnedObjectPath, OwnedValue, Value},
    Connection, Proxy,
};

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
type Properties = HashMap<String, OwnedValue>;

// The raw D-Bus response preserves v6 pipewire-serial, which ashpd 0.11's
// fixed StreamProperties struct discards. No backend-specific IDs are inferred.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct StreamTarget {
    pub node: u32,
    pub serial: Option<u64>,
}

fn target(streams: Vec<(u32, Properties)>, version: u32) -> Result<StreamTarget, String> {
    let [(node, props)] = streams.as_slice() else {
        return Err("Portal must return exactly one window stream".into());
    };
    let kind = props.get("source_type").and_then(|v| u32::try_from(v).ok());
    if kind != Some(2) {
        return Err("Portal did not confirm a window source".into());
    }
    let serial = props
        .get("pipewire-serial")
        .map(u64::try_from)
        .transpose()
        .map_err(|e| e.to_string())?;
    if version >= 6 && serial.is_none() {
        return Err("Portal v6 omitted the PipeWire serial".into());
    }
    if *node == u32::MAX || serial == Some(0) {
        return Err("Portal returned an invalid stream target".into());
    }
    Ok(StreamTarget {
        node: *node,
        serial,
    })
}

async fn proxy<'a>(
    connection: &'a Connection,
    path: &'a str,
    interface: &'a str,
) -> Result<Proxy<'a>, String> {
    Proxy::new(connection, DEST, path, interface)
        .await
        .map_err(|e| e.to_string())
}

async fn request<B>(
    connection: &Connection,
    portal: &Proxy<'_>,
    token: &str,
    method: &str,
    body: &B,
) -> Result<Properties, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let sender = connection
        .unique_name()
        .ok_or("missing D-Bus unique name")?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = proxy(connection, &path, "org.freedesktop.portal.Request").await?;
    let mut responses = request
        .receive_signal("Response")
        .await
        .map_err(|e| e.to_string())?;
    let returned: OwnedObjectPath = portal.call(method, body).await.map_err(|e| e.to_string())?;
    if returned.as_str() != path {
        return Err("Portal returned an unexpected request handle".into());
    }
    let response = responses
        .next()
        .await
        .ok_or("Portal response stream closed")?;
    let (code, properties): (u32, Properties) =
        response.body().deserialize().map_err(|e| e.to_string())?;
    match code {
        0 => Ok(properties),
        1 => Err("Window selection cancelled".into()),
        _ => Err(format!("Portal {method} failed (response {code})")),
    }
}

async fn capture(
    connection: &Connection,
    commands: pipewire::channel::Receiver<StreamCommand>,
    stop_stream: pipewire::channel::Sender<StreamCommand>,
    shared: Arc<Mutex<Mailbox>>,
    demand: Arc<AtomicBool>,
    max_fps: u32,
    stop: &mut watch::Receiver<bool>,
) -> Result<(), String> {
    let portal = proxy(connection, PATH, "org.freedesktop.portal.ScreenCast").await?;
    let version: u32 = portal
        .get_property("version")
        .await
        .map_err(|e| e.to_string())?;
    let sources: u32 = portal
        .get_property("AvailableSourceTypes")
        .await
        .map_err(|e| e.to_string())?;
    if sources & 2 == 0 {
        return Err("This Portal backend does not support window capture".into());
    }
    if version < 3 {
        return Err("Portal v3 or newer is required to validate window source type".into());
    }
    let cursors: u32 = portal
        .get_property("AvailableCursorModes")
        .await
        .map_err(|e| e.to_string())?;
    if cursors & 1 == 0 {
        return Err("Portal cannot capture without embedding the cursor".into());
    }
    let options = HashMap::from([
        ("handle_token", Value::from("create")),
        ("session_handle_token", Value::from("capture")),
    ]);
    let created = request(connection, &portal, "create", "CreateSession", &(options,)).await?;
    let path = created
        .get("session_handle")
        .ok_or("Portal omitted session_handle")?;
    let path = <&str>::try_from(path).map_err(|e| e.to_string())?;
    let session_path = OwnedObjectPath::try_from(path.to_owned()).map_err(|e| e.to_string())?;
    let session = proxy(
        connection,
        session_path.as_str(),
        "org.freedesktop.portal.Session",
    )
    .await?;
    let mut closed = session
        .receive_signal("Closed")
        .await
        .map_err(|e| e.to_string())?;
    // Subscribing before SelectSources also catches a closure during the picker.
    let setup = async {
        let options = HashMap::from([
            ("handle_token", Value::from("select")),
            ("types", Value::from(2u32)),
            ("multiple", Value::from(false)),
            ("cursor_mode", Value::from(1u32)),
        ]);
        request(
            connection,
            &portal,
            "select",
            "SelectSources",
            &(&session_path, options),
        )
        .await?;
        let options = HashMap::from([("handle_token", Value::from("start"))]);
        let mut started = request(
            connection,
            &portal,
            "start",
            "Start",
            &(&session_path, "", options),
        )
        .await?;
        let streams: Vec<(u32, Properties)> = started
            .remove("streams")
            .ok_or("Portal omitted streams")?
            .try_into()
            .map_err(|e: zbus::zvariant::Error| e.to_string())?;
        let target = target(streams, version)?;
        let fd: OwnedFd = portal
            .call(
                "OpenPipeWireRemote",
                &(&session_path, HashMap::<&str, Value<'_>>::new()),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok::<_, String>((target, fd))
    };
    let (target, fd) = tokio::select! {
        result = setup => result?,
        _ = closed.next() => return Err("Portal session closed during selection".into()),
        _ = stop.changed() => return Ok(()),
    };
    let mut receiver = tokio::task::spawn_blocking(move || {
        receiver::run(fd.into(), target, commands, shared, demand, max_fps)
    });
    let outcome = tokio::select! {
        result = &mut receiver => return result.map_err(|e| e.to_string())?,
        _ = closed.next() => Err("Portal session closed".into()),
        _ = stop.changed() => Ok(()),
    };
    let _ = stop_stream.send(StreamCommand::Stop);
    let _ = receiver.await;
    outcome
}

pub(super) async fn run(
    mut stop: watch::Receiver<bool>,
    commands: pipewire::channel::Receiver<StreamCommand>,
    stop_stream: pipewire::channel::Sender<StreamCommand>,
    shared: Arc<Mutex<Mailbox>>,
    demand: Arc<AtomicBool>,
    max_fps: u32,
) -> Result<(), String> {
    if *stop.borrow() {
        return Ok(());
    }
    let connection = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(10), Connection::session()) => result.map_err(|_| "Portal bus connection timed out")?.map_err(|e| e.to_string())?,
        _ = stop.changed() => return Ok(()),
    };
    // Cancellation must work even before a session handle has been returned.
    // This connection belongs only to this capture. Closing it also terminates
    // outstanding Portal requests and sessions owned by its unique bus name.
    let mut setup_stop = stop.clone();
    let result = tokio::select! {
        result = capture(&connection, commands, stop_stream.clone(), shared, demand, max_fps, &mut stop) => result,
        _ = setup_stop.changed() => {
            let _ = stop_stream.send(StreamCommand::Stop);
            Ok(())
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), connection.close()).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_window_count_and_prefers_serial_without_inferring_identity() {
        let window = || HashMap::from([("source_type".into(), OwnedValue::from(2u32))]);
        assert!(target(vec![], 4).is_err());
        assert!(target(vec![(1, window()), (2, window())], 4).is_err());
        assert!(target(vec![(1, HashMap::new())], 4).is_err());
        assert!(target(
            vec![(
                1,
                HashMap::from([("source_type".into(), OwnedValue::from(1u32))])
            )],
            4
        )
        .is_err());
        assert_eq!(
            target(vec![(42, window())], 4).unwrap(),
            StreamTarget {
                node: 42,
                serial: None
            }
        );
        assert!(target(vec![(42, window())], 6).is_err());
        let mut props = window();
        props.insert("pipewire-serial".into(), OwnedValue::from(123u64));
        assert_eq!(target(vec![(42, props)], 6).unwrap().serial, Some(123));
    }
}
