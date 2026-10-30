use super::{publish_geometry, GameGeometrySource, ResolvedGeometry, SharedGameGeometry};
use crate::capture::window_tracker::WindowRect;
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const MAGIC: &[u8; 6] = b"i3-ipc";
const GET_TREE: u32 = 4;
const SUBSCRIBE: u32 = 2;
const WINDOW_EVENT: u32 = 0x8000_0003;
const OUTPUT_EVENT: u32 = 0x8000_0001;
const MAX_MESSAGE_BYTES: u32 = 16 * 1024 * 1024;
const SOCKET_TIMEOUT: Duration = Duration::from_secs(3);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

fn is_geometry_event(message_type: u32) -> bool {
    matches!(message_type, WINDOW_EVENT | OUTPUT_EVENT)
}

pub(super) struct SwayGeometrySource {
    socket: PathBuf,
}

impl SwayGeometrySource {
    pub(super) fn new(socket: PathBuf) -> Self {
        Self { socket }
    }
}

impl GameGeometrySource for SwayGeometrySource {
    fn name(&self) -> &'static str {
        "sway"
    }

    fn start(
        self: Box<Self>,
        title: &str,
        sink: SharedGameGeometry,
        on_change: super::GeometryChangedCallback,
    ) -> Result<(), String> {
        let title = title.to_string();
        let socket = self.socket;
        std::thread::Builder::new()
            .name("overmax-sway-geometry".to_string())
            .spawn(move || {
                let mut generation = 0;
                loop {
                    if let Err(error) =
                        run_connection(&socket, &title, &sink, &mut generation, &on_change)
                    {
                        eprintln!("[GeometrySource:Sway] {error}");
                        publish_geometry(&sink, None, &mut generation, &on_change);
                        std::thread::sleep(RECONNECT_DELAY);
                    }
                }
            })
            .map_err(|error| format!("start Sway geometry source: {error}"))?;
        Ok(())
    }
}

fn run_connection(
    socket: &PathBuf,
    title: &str,
    sink: &SharedGameGeometry,
    generation: &mut u64,
    on_change: &Arc<dyn Fn() + Send + Sync>,
) -> Result<(), String> {
    let mut events = UnixStream::connect(socket)
        .map_err(|error| format!("connect event socket {}: {error}", socket.display()))?;
    write_message(&mut events, SUBSCRIBE, br#"["window","output"]"#)?;
    let (reply_type, reply) = read_message(&mut events)?;
    if reply_type != SUBSCRIBE
        || serde_json::from_slice::<Value>(&reply)
            .ok()
            .and_then(|value| value.get("success").and_then(Value::as_bool))
            != Some(true)
    {
        return Err("Sway did not accept the window event subscription".to_string());
    }

    refresh_geometry(socket, title, sink, generation, on_change)?;
    loop {
        let (message_type, _) = read_message(&mut events)?;
        if is_geometry_event(message_type) {
            refresh_geometry(socket, title, sink, generation, on_change)?;
        }
    }
}

fn refresh_geometry(
    socket: &PathBuf,
    title: &str,
    sink: &SharedGameGeometry,
    generation: &mut u64,
    on_change: &Arc<dyn Fn() + Send + Sync>,
) -> Result<(), String> {
    let mut command = UnixStream::connect(socket)
        .map_err(|error| format!("connect command socket {}: {error}", socket.display()))?;
    command
        .set_read_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|error| error.to_string())?;
    command
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|error| error.to_string())?;
    write_message(&mut command, GET_TREE, &[])?;
    let (reply_type, reply) = read_message(&mut command)?;
    if reply_type != GET_TREE {
        return Err(format!("unexpected Sway response type {reply_type}"));
    }
    let geometry = parse_tree(title, &reply)?;
    publish_geometry(sink, geometry, generation, on_change);
    Ok(())
}

fn write_message(stream: &mut UnixStream, message_type: u32, payload: &[u8]) -> Result<(), String> {
    let payload_len =
        u32::try_from(payload.len()).map_err(|_| "Sway IPC request is too large".to_string())?;
    stream
        .write_all(MAGIC)
        .and_then(|()| stream.write_all(&payload_len.to_le_bytes()))
        .and_then(|()| stream.write_all(&message_type.to_le_bytes()))
        .and_then(|()| stream.write_all(payload))
        .map_err(|error| format!("write Sway IPC request: {error}"))
}

fn read_message(stream: &mut UnixStream) -> Result<(u32, Vec<u8>), String> {
    let mut header = [0; 14];
    stream
        .read_exact(&mut header)
        .map_err(|error| format!("read Sway IPC header: {error}"))?;
    if &header[..6] != MAGIC {
        return Err("invalid Sway IPC magic".to_string());
    }
    let payload_len = u32::from_le_bytes(header[6..10].try_into().expect("fixed header slice"));
    if payload_len > MAX_MESSAGE_BYTES {
        return Err(format!(
            "Sway IPC message exceeded {MAX_MESSAGE_BYTES} bytes"
        ));
    }
    let message_type = u32::from_le_bytes(header[10..14].try_into().expect("fixed header slice"));
    let mut payload = vec![0; payload_len as usize];
    stream
        .read_exact(&mut payload)
        .map_err(|error| format!("read Sway IPC payload: {error}"))?;
    Ok((message_type, payload))
}

fn parse_tree(title: &str, payload: &[u8]) -> Result<Option<ResolvedGeometry>, String> {
    let tree: Value =
        serde_json::from_slice(payload).map_err(|error| format!("invalid tree JSON: {error}"))?;
    let mut matches = Vec::new();
    collect_matches(&tree, None, true, title, &mut matches);
    if matches.len() != 1 {
        return Ok(None);
    }
    let (visible, geometry) = matches.pop().expect("one match was checked");
    Ok(visible.then_some(geometry).flatten())
}

fn collect_matches<'a>(
    node: &'a Value,
    inherited_output: Option<&'a str>,
    ancestors_visible: bool,
    title: &str,
    matches: &mut Vec<(bool, Option<ResolvedGeometry>)>,
) {
    let Some(node_type) = node.get("type").and_then(Value::as_str) else {
        return;
    };
    let output = if node_type == "output" {
        node.get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
    } else {
        inherited_output
    };
    let visible = match node_type {
        "output" => ancestors_visible && node.get("active").and_then(Value::as_bool) == Some(true),
        "workspace" => {
            ancestors_visible && node.get("visible").and_then(Value::as_bool) == Some(true)
        }
        _ => ancestors_visible && node.get("visible").and_then(Value::as_bool).unwrap_or(true),
    };

    if node_type == "con" && matches_title(node, title) {
        let geometry = output.and_then(|output| {
            let rect = node.get("rect")?;
            let rect = WindowRect {
                left: json_i32(rect.get("x")?)?,
                top: json_i32(rect.get("y")?)?,
                width: json_i32(rect.get("width")?)?,
                height: json_i32(rect.get("height")?)?,
            };
            rect.is_valid().then(|| ResolvedGeometry {
                output: output.to_string(),
                rect,
            })
        });
        matches.push((
            visible && node.get("visible").and_then(Value::as_bool) == Some(true),
            geometry,
        ));
    }

    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(key).and_then(Value::as_array) {
            for child in children {
                collect_matches(child, output, visible, title, matches);
            }
        }
    }
}

fn matches_title(node: &Value, title: &str) -> bool {
    node.get("name").and_then(Value::as_str) == Some(title)
        || node.get("app_id").and_then(Value::as_str) == Some(title)
        || node
            .get("window_properties")
            .and_then(|properties| properties.get("title"))
            .and_then(Value::as_str)
            == Some(title)
}

fn json_i32(value: &Value) -> Option<i32> {
    i32::try_from(value.as_i64()?).ok()
}

#[cfg(test)]
mod tests {
    use super::{is_geometry_event, parse_tree, OUTPUT_EVENT, WINDOW_EVENT};

    const TREE: &str = r#"{
        "type":"root",
        "nodes":[{
            "type":"output",
            "name":"DP-1",
            "active":true,
            "nodes":[{
                "type":"workspace",
                "name":"4",
                "visible":true,
                "nodes":[{
                    "type":"con",
                    "name":"DJMAX RESPECT V",
                    "window":42,
                    "visible":true,
                    "rect":{"x":320,"y":180,"width":1280,"height":720},
                    "nodes":[]
                }]
            }]
        }]
    }"#;

    #[test]
    fn parses_visible_window_geometry_and_output() {
        let geometry = parse_tree("DJMAX RESPECT V", TREE.as_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(geometry.output, "DP-1");
        assert_eq!(geometry.rect.left, 320);
        assert_eq!(geometry.rect.top, 180);
        assert_eq!(geometry.rect.width, 1280);
        assert_eq!(geometry.rect.height, 720);
    }

    #[test]
    fn ignores_missing_hidden_and_ambiguous_windows() {
        assert!(parse_tree("other", TREE.as_bytes()).unwrap().is_none());
        let hidden = TREE.replace("\"visible\":true", "\"visible\":false");
        assert!(parse_tree("DJMAX RESPECT V", hidden.as_bytes())
            .unwrap()
            .is_none());
        let duplicate = TREE.replace(
            "\"nodes\":[]",
            "\"nodes\":[{\"type\":\"con\",\"name\":\"DJMAX RESPECT V\",\"visible\":true,\"rect\":{\"x\":0,\"y\":0,\"width\":10,\"height\":10}}]",
        );
        assert!(parse_tree("DJMAX RESPECT V", duplicate.as_bytes())
            .unwrap()
            .is_none());
    }

    #[test]
    fn requires_a_visible_output_and_valid_rectangle() {
        let inactive = TREE.replace("\"active\":true", "\"active\":false");
        assert!(parse_tree("DJMAX RESPECT V", inactive.as_bytes())
            .unwrap()
            .is_none());
        let invalid_rect = TREE.replace("\"width\":1280", "\"width\":0");
        assert!(parse_tree("DJMAX RESPECT V", invalid_rect.as_bytes())
            .unwrap()
            .is_none());
    }

    #[test]
    fn refreshes_geometry_for_window_and_output_events() {
        assert_eq!(OUTPUT_EVENT, 0x8000_0001);
        assert!(is_geometry_event(WINDOW_EVENT));
        assert!(is_geometry_event(OUTPUT_EVENT));
        assert!(!is_geometry_event(0x8000_0006));
    }
}
