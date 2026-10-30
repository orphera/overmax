use super::{publish_geometry, GameGeometrySource, ResolvedGeometry, SharedGameGeometry};
use crate::capture::window_tracker::WindowRect;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const MAX_REPLY_BYTES: u64 = 4 * 1024 * 1024;
const SOCKET_TIMEOUT: Duration = Duration::from_secs(3);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

pub(super) struct HyprlandGeometrySource {
    command_socket: PathBuf,
    event_socket: PathBuf,
}

impl HyprlandGeometrySource {
    pub(super) fn new(runtime_dir: PathBuf, signature: String) -> Self {
        let instance_dir = runtime_dir.join("hypr").join(signature);
        Self {
            command_socket: instance_dir.join(".socket.sock"),
            event_socket: instance_dir.join(".socket2.sock"),
        }
    }
}

impl GameGeometrySource for HyprlandGeometrySource {
    fn name(&self) -> &'static str {
        "hyprland"
    }

    fn start(
        self: Box<Self>,
        title: &str,
        sink: SharedGameGeometry,
        on_change: super::GeometryChangedCallback,
    ) -> Result<(), String> {
        let title = title.to_string();
        let command_socket = self.command_socket;
        let event_socket = self.event_socket;
        std::thread::Builder::new()
            .name("overmax-hyprland-geometry".to_string())
            .spawn(move || {
                let mut generation = 0;
                loop {
                    if let Err(error) = run_connection(
                        &command_socket,
                        &event_socket,
                        &title,
                        &sink,
                        &mut generation,
                        &on_change,
                    ) {
                        eprintln!("[GeometrySource:Hyprland] {error}");
                        publish_geometry(&sink, None, &mut generation, &on_change);
                        std::thread::sleep(RECONNECT_DELAY);
                    }
                }
            })
            .map_err(|error| format!("start Hyprland geometry source: {error}"))?;
        Ok(())
    }
}

fn run_connection(
    command_socket: &PathBuf,
    event_socket: &PathBuf,
    title: &str,
    sink: &SharedGameGeometry,
    generation: &mut u64,
    on_change: &Arc<dyn Fn() + Send + Sync>,
) -> Result<(), String> {
    let event_stream = UnixStream::connect(event_socket)
        .map_err(|error| format!("connect event socket {}: {error}", event_socket.display()))?;
    let mut event_stream = std::io::BufReader::new(event_stream);
    refresh_geometry(command_socket, title, sink, generation, on_change)?;

    let mut line = String::new();
    loop {
        line.clear();
        let bytes = std::io::BufRead::read_line(&mut event_stream, &mut line)
            .map_err(|error| format!("read event socket: {error}"))?;
        if bytes == 0 {
            return Err("event socket closed".to_string());
        }
        if relevant_event(&line) {
            refresh_geometry(command_socket, title, sink, generation, on_change)?;
        }
    }
}

fn refresh_geometry(
    command_socket: &PathBuf,
    title: &str,
    sink: &SharedGameGeometry,
    generation: &mut u64,
    on_change: &Arc<dyn Fn() + Send + Sync>,
) -> Result<(), String> {
    let clients = command_reply(command_socket, b"j/clients")?;
    let monitors = command_reply(command_socket, b"j/monitors")?;
    let geometry = parse_geometry(title, &clients, &monitors)?;
    publish_geometry(sink, geometry, generation, on_change);
    Ok(())
}

fn command_reply(socket_path: &PathBuf, command: &[u8]) -> Result<serde_json::Value, String> {
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|error| format!("connect command socket {}: {error}", socket_path.display()))?;
    stream
        .set_read_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(command)
        .map_err(|error| format!("send command: {error}"))?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|error| format!("finish command: {error}"))?;

    let mut response = Vec::new();
    stream
        .take(MAX_REPLY_BYTES + 1)
        .read_to_end(&mut response)
        .map_err(|error| format!("read command response: {error}"))?;
    if response.len() as u64 > MAX_REPLY_BYTES {
        return Err("command response exceeded 4 MiB".to_string());
    }
    serde_json::from_slice(&response).map_err(|error| format!("invalid command JSON: {error}"))
}

fn relevant_event(line: &str) -> bool {
    matches!(
        line.split_once(">>").map(|(name, _)| name),
        Some(
            "activewindow"
                | "activewindowv2"
                | "openwindow"
                | "closewindow"
                | "movewindow"
                | "movewindowv2"
                | "workspace"
                | "focusedmon"
                | "fullscreen"
                | "windowtitle"
                | "windowtitlev2"
                | "changefloatingmode"
                | "configreloaded"
                | "monitoradded"
                | "monitoraddedv2"
                | "monitorremoved"
                | "monitorremovedv2"
        )
    )
}

fn parse_geometry(
    title: &str,
    clients: &serde_json::Value,
    monitors: &serde_json::Value,
) -> Result<Option<ResolvedGeometry>, String> {
    let clients = clients
        .as_array()
        .ok_or_else(|| "clients response is not an array".to_string())?;
    let monitors = monitors
        .as_array()
        .ok_or_else(|| "monitors response is not an array".to_string())?;

    let mut matches = clients
        .iter()
        .filter(|client| client.get("title").and_then(serde_json::Value::as_str) == Some(title));
    let Some(client) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Ok(None);
    }

    let Some(workspace_id) = client
        .get("workspace")
        .and_then(|workspace| workspace.get("id"))
        .and_then(serde_json::Value::as_i64)
    else {
        return Ok(None);
    };
    let Some(monitor_id) = client.get("monitor").and_then(serde_json::Value::as_i64) else {
        return Ok(None);
    };
    let mut matching_monitors = monitors.iter().filter(|monitor| {
        monitor.get("id").and_then(serde_json::Value::as_i64) == Some(monitor_id)
    });
    let Some(monitor) = matching_monitors.next() else {
        return Ok(None);
    };
    if matching_monitors.next().is_some()
        || monitor
            .get("activeWorkspace")
            .and_then(|workspace| workspace.get("id"))
            .and_then(serde_json::Value::as_i64)
            != Some(workspace_id)
    {
        return Ok(None);
    }
    let Some(output) = monitor.get("name").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    let Some((left, top)) = pair_i32(client.get("at")) else {
        return Ok(None);
    };
    let Some((width, height)) = pair_i32(client.get("size")) else {
        return Ok(None);
    };
    let rect = WindowRect {
        left,
        top,
        width,
        height,
    };
    if output.is_empty() || !rect.is_valid() {
        return Ok(None);
    }
    Ok(Some(ResolvedGeometry {
        output: output.to_string(),
        rect,
    }))
}

fn pair_i32(value: Option<&serde_json::Value>) -> Option<(i32, i32)> {
    let values = value?.as_array()?;
    if values.len() != 2 {
        return None;
    }
    Some((
        i32::try_from(values[0].as_i64()?).ok()?,
        i32::try_from(values[1].as_i64()?).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{parse_geometry, relevant_event};
    use serde_json::Value;

    const CLIENTS: &str = r#"[{
        "title":"DJMAX RESPECT V",
        "at":[320,180],
        "size":[1280,720],
        "monitor":1,
        "workspace":{"id":4}
    }]"#;
    const MONITORS: &str = r#"[{
        "id":1,
        "name":"DP-1",
        "activeWorkspace":{"id":4}
    }]"#;

    fn parse(title: &str, clients: &str, monitors: &str) -> Option<super::ResolvedGeometry> {
        let clients: Value = serde_json::from_str(clients).unwrap();
        let monitors: Value = serde_json::from_str(monitors).unwrap();
        parse_geometry(title, &clients, &monitors).unwrap()
    }

    #[test]
    fn parses_visible_client_geometry_and_output() {
        let geometry = parse("DJMAX RESPECT V", CLIENTS, MONITORS).unwrap();
        assert_eq!(geometry.output, "DP-1");
        assert_eq!(geometry.rect.left, 320);
        assert_eq!(geometry.rect.top, 180);
        assert_eq!(geometry.rect.width, 1280);
        assert_eq!(geometry.rect.height, 720);
    }

    #[test]
    fn ignores_missing_ambiguous_and_inactive_clients() {
        assert!(parse("other", CLIENTS, MONITORS).is_none());
        let duplicate = CLIENTS.replace("}]", "},{\"title\":\"DJMAX RESPECT V\"}]");
        assert!(parse("DJMAX RESPECT V", &duplicate, MONITORS).is_none());
        let inactive = MONITORS.replace("\"id\":4", "\"id\":3");
        assert!(parse("DJMAX RESPECT V", CLIENTS, &inactive).is_none());
    }

    #[test]
    fn ignores_unrelated_events() {
        assert!(relevant_event("movewindowv2>>0x123,4,workspace"));
        assert!(relevant_event("closewindow>>0x123"));
        assert!(relevant_event("monitoradded>>DP-2"));
        assert!(relevant_event("monitoraddedv2>>2,DP-2,Generic monitor"));
        assert!(relevant_event("monitorremoved>>DP-2"));
        assert!(relevant_event("monitorremovedv2>>2,DP-2,Generic monitor"));
        assert!(!relevant_event("mouseaxis>>vertical"));
    }
}
