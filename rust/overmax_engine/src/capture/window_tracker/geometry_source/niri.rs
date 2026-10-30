use super::{publish_geometry, GameGeometrySource, ResolvedGeometry, SharedGameGeometry};
use crate::capture::window_tracker::WindowRect;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_REPLY_BYTES: u64 = 4 * 1024 * 1024;
const MAX_EVENT_BYTES: u64 = 1024 * 1024;
const SOCKET_TIMEOUT: Duration = Duration::from_secs(3);
const METADATA_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

pub(super) struct NiriGeometrySource {
    socket: PathBuf,
}

impl NiriGeometrySource {
    pub(super) fn new(socket: PathBuf) -> Self {
        Self { socket }
    }
}

impl GameGeometrySource for NiriGeometrySource {
    fn name(&self) -> &'static str {
        "niri"
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
            .name("overmax-niri-geometry".to_string())
            .spawn(move || {
                let mut generation = 0;
                loop {
                    if let Err(error) =
                        run_connection(&socket, &title, &sink, &mut generation, &on_change)
                    {
                        eprintln!("[GeometrySource:Niri] {error}");
                        publish_geometry(&sink, None, &mut generation, &on_change);
                        std::thread::sleep(RECONNECT_DELAY);
                    }
                }
            })
            .map_err(|error| format!("start Niri geometry source: {error}"))?;
        Ok(())
    }
}

#[derive(Default)]
struct StreamState {
    windows: HashMap<u64, Value>,
    workspaces: HashMap<u64, Value>,
}

impl StreamState {
    fn apply_event(&mut self, event: &Value) -> Result<bool, String> {
        let Some(event) = event.as_object() else {
            return Err("Niri event is not a JSON object".to_string());
        };
        if event.len() != 1 {
            return Err("Niri event does not have exactly one variant".to_string());
        }
        let (kind, body) = event.iter().next().expect("one event variant was checked");
        match kind.as_str() {
            "WindowsChanged" => {
                self.windows = indexed_objects(body.get("windows"), "window")?;
                Ok(true)
            }
            "WindowOpenedOrChanged" => {
                let window = body
                    .get("window")
                    .ok_or_else(|| "WindowOpenedOrChanged has no window".to_string())?;
                let id = json_u64(window.get("id"))
                    .ok_or_else(|| "Niri window has no valid id".to_string())?;
                self.windows.insert(id, window.clone());
                Ok(true)
            }
            "WindowClosed" => {
                let id = json_u64(body.get("id"))
                    .ok_or_else(|| "WindowClosed has no valid id".to_string())?;
                self.windows.remove(&id);
                Ok(true)
            }
            "WindowLayoutsChanged" => {
                let changes = body
                    .get("changes")
                    .and_then(Value::as_array)
                    .ok_or_else(|| "WindowLayoutsChanged has no changes array".to_string())?;
                for change in changes {
                    let Some(pair) = change.as_array().filter(|pair| pair.len() == 2) else {
                        return Err("Niri window layout change is not a pair".to_string());
                    };
                    let Some(id) = json_u64(pair.first()) else {
                        return Err("Niri window layout change has an invalid id".to_string());
                    };
                    if let Some(window) = self.windows.get_mut(&id) {
                        window["layout"] = pair[1].clone();
                    }
                }
                Ok(true)
            }
            "WorkspacesChanged" => {
                self.workspaces = indexed_objects(body.get("workspaces"), "workspace")?;
                Ok(true)
            }
            "WorkspaceActivated" => {
                let id = json_u64(body.get("id"))
                    .ok_or_else(|| "WorkspaceActivated has no valid id".to_string())?;
                let focused = body.get("focused").and_then(Value::as_bool) == Some(true);
                let output = self
                    .workspaces
                    .get(&id)
                    .and_then(|workspace| workspace.get("output"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                for (workspace_id, workspace) in &mut self.workspaces {
                    if focused {
                        workspace["is_focused"] = Value::Bool(false);
                    }
                    if *workspace_id == id {
                        workspace["is_active"] = Value::Bool(true);
                        workspace["is_focused"] = Value::Bool(focused);
                    } else if output.as_deref().is_some_and(|output| {
                        workspace.get("output").and_then(Value::as_str) == Some(output)
                    }) {
                        workspace["is_active"] = Value::Bool(false);
                        workspace["is_focused"] = Value::Bool(false);
                    }
                }
                Ok(true)
            }
            "WorkspaceActiveWindowChanged" => {
                let workspace_id = json_u64(body.get("workspace_id")).ok_or_else(|| {
                    "WorkspaceActiveWindowChanged has no valid workspace id".to_string()
                })?;
                if let Some(workspace) = self.workspaces.get_mut(&workspace_id) {
                    workspace["active_window_id"] =
                        body.get("active_window_id").cloned().unwrap_or(Value::Null);
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

fn indexed_objects(values: Option<&Value>, kind: &str) -> Result<HashMap<u64, Value>, String> {
    let values = values
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Niri {kind} state is not an array"))?;
    let mut indexed = HashMap::with_capacity(values.len());
    for value in values {
        let id = json_u64(value.get("id")).ok_or_else(|| format!("Niri {kind} has no valid id"))?;
        indexed.insert(id, value.clone());
    }
    Ok(indexed)
}

fn run_connection(
    socket: &Path,
    title: &str,
    sink: &SharedGameGeometry,
    generation: &mut u64,
    on_change: &Arc<dyn Fn() + Send + Sync>,
) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|error| format!("connect Niri socket {}: {error}", socket.display()))?;
    stream
        .set_read_timeout(Some(METADATA_REFRESH_INTERVAL))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(b"\"EventStream\"\n")
        .map_err(|error| format!("send Niri event stream request: {error}"))?;

    let mut events = BufReader::new(stream);
    let mut line = Vec::new();
    if read_line_limited(&mut events, &mut line, MAX_EVENT_BYTES)
        .map_err(|error| format!("read Niri event stream reply: {error}"))?
        == 0
    {
        return Err("Niri event stream closed before its reply".to_string());
    }
    let reply: Value = serde_json::from_slice(&line)
        .map_err(|error| format!("invalid Niri event stream reply: {error}"))?;
    if reply.get("Ok").and_then(Value::as_str) != Some("Handled") {
        return Err("Niri did not accept the event stream request".to_string());
    }

    let mut state = StreamState::default();
    let mut last_metadata_refresh = Instant::now();
    line.clear();
    loop {
        match read_line_limited(&mut events, &mut line, MAX_EVENT_BYTES) {
            Ok(0) => return Err("Niri event stream closed".to_string()),
            Ok(_) => {
                let event: Value = serde_json::from_slice(&line)
                    .map_err(|error| format!("invalid Niri event JSON: {error}"))?;
                let state_changed = state.apply_event(&event)?;
                if state_changed || last_metadata_refresh.elapsed() >= METADATA_REFRESH_INTERVAL {
                    refresh_geometry(socket, title, &state, sink, generation, on_change)?;
                    last_metadata_refresh = Instant::now();
                }
                line.clear();
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                refresh_geometry(socket, title, &state, sink, generation, on_change)?;
                last_metadata_refresh = Instant::now();
            }
            Err(error) => return Err(format!("read Niri event stream: {error}")),
        }
    }
}

fn refresh_geometry(
    socket: &Path,
    title: &str,
    state: &StreamState,
    sink: &SharedGameGeometry,
    generation: &mut u64,
    on_change: &Arc<dyn Fn() + Send + Sync>,
) -> Result<(), String> {
    let (outputs, layers) = query_compositor_metadata(socket)?;
    let geometry = resolve_geometry(title, state, &outputs, &layers);
    publish_geometry(sink, geometry, generation, on_change);
    Ok(())
}

fn query_compositor_metadata(
    socket: &Path,
) -> Result<(HashMap<String, Value>, Vec<Value>), String> {
    let outputs = request(socket, b"\"Outputs\"\n")?;
    let outputs = outputs
        .get("Ok")
        .and_then(|reply| reply.get("Outputs"))
        .and_then(Value::as_object)
        .ok_or_else(|| "Niri Outputs reply has an unexpected shape".to_string())?
        .iter()
        .map(|(name, output)| (name.clone(), output.clone()))
        .collect();

    let layers = request(socket, b"\"Layers\"\n")?;
    let layers = layers
        .get("Ok")
        .and_then(|reply| reply.get("Layers"))
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "Niri Layers reply has an unexpected shape".to_string())?;
    Ok((outputs, layers))
}

fn request(socket: &Path, request: &[u8]) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|error| format!("connect Niri socket {}: {error}", socket.display()))?;
    stream
        .set_read_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(request)
        .and_then(|()| stream.shutdown(Shutdown::Write))
        .map_err(|error| format!("send Niri IPC request: {error}"))?;

    let mut response = Vec::new();
    stream
        .take(MAX_REPLY_BYTES + 1)
        .read_to_end(&mut response)
        .map_err(|error| format!("read Niri IPC response: {error}"))?;
    if response.len() as u64 > MAX_REPLY_BYTES {
        return Err("Niri IPC response exceeded 4 MiB".to_string());
    }
    serde_json::from_slice(&response).map_err(|error| format!("invalid Niri IPC JSON: {error}"))
}

fn read_line_limited<R: Read>(
    reader: &mut BufReader<R>,
    line: &mut Vec<u8>,
    limit: u64,
) -> io::Result<usize> {
    let buffered = line.len() as u64;
    if buffered > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Niri event exceeded {limit} bytes"),
        ));
    }
    let bytes = reader.take(limit - buffered + 1).read_until(b'\n', line)?;
    if line.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Niri event exceeded {limit} bytes"),
        ));
    }
    Ok(bytes)
}

fn resolve_geometry(
    title: &str,
    state: &StreamState,
    outputs: &HashMap<String, Value>,
    layers: &[Value],
) -> Option<ResolvedGeometry> {
    let mut matching_windows = state.windows.values().filter(|window| {
        window.get("title").and_then(Value::as_str) == Some(title)
            || window.get("app_id").and_then(Value::as_str) == Some(title)
    });
    let window = matching_windows.next()?;
    if matching_windows.next().is_some() {
        return None;
    }

    let window_id = json_u64(window.get("id"))?;
    let workspace_id = json_u64(window.get("workspace_id"))?;
    let workspace = state.workspaces.get(&workspace_id)?;
    if workspace.get("is_active").and_then(Value::as_bool) != Some(true)
        || json_u64(workspace.get("active_window_id")) != Some(window_id)
    {
        return None;
    }
    let output_name = workspace.get("output")?.as_str()?;
    // Niri's Layers reply omits exclusive-zone geometry, which can shift the workspace origin.
    if output_name.is_empty()
        || layers.iter().any(|layer| {
            layer.get("output").and_then(Value::as_str) == Some(output_name)
                && layer.get("namespace").and_then(Value::as_str) != Some("overmax")
        })
    {
        return None;
    }

    let output = outputs.get(output_name)?;
    let logical = output.get("logical")?;
    let output_x = json_f64(logical.get("x"))?;
    let output_y = json_f64(logical.get("y"))?;
    let output_width = json_f64(logical.get("width"))?;
    let output_height = json_f64(logical.get("height"))?;
    if output_width <= 0.0 || output_height <= 0.0 {
        return None;
    }

    let layout = window.get("layout")?;
    let tile_position = json_pair_f64(layout.get("tile_pos_in_workspace_view"))?;
    let window_offset = json_pair_f64(layout.get("window_offset_in_tile"))?;
    let (width, height) = json_pair_i32(layout.get("window_size"))?;
    let left = output_x + tile_position.0 + window_offset.0;
    let top = output_y + tile_position.1 + window_offset.1;
    let rect = WindowRect {
        left: round_i32(left)?,
        top: round_i32(top)?,
        width,
        height,
    };
    if !rect.is_valid() {
        return None;
    }

    let rect_right = f64::from(rect.left) + f64::from(rect.width);
    let rect_bottom = f64::from(rect.top) + f64::from(rect.height);
    if rect_right <= output_x
        || rect_bottom <= output_y
        || f64::from(rect.left) >= output_x + output_width
        || f64::from(rect.top) >= output_y + output_height
    {
        return None;
    }

    Some(ResolvedGeometry {
        output: output_name.to_string(),
        rect,
    })
}

fn json_u64(value: Option<&Value>) -> Option<u64> {
    value?.as_u64()
}

fn json_f64(value: Option<&Value>) -> Option<f64> {
    let value = value?.as_f64()?;
    value.is_finite().then_some(value)
}

fn json_pair_f64(value: Option<&Value>) -> Option<(f64, f64)> {
    let values = value?.as_array()?;
    if values.len() != 2 {
        return None;
    }
    Some((json_f64(values.first())?, json_f64(values.get(1))?))
}

fn json_pair_i32(value: Option<&Value>) -> Option<(i32, i32)> {
    let values = value?.as_array()?;
    if values.len() != 2 {
        return None;
    }
    Some((
        i32::try_from(values.first()?.as_i64()?).ok()?,
        i32::try_from(values.get(1)?.as_i64()?).ok()?,
    ))
}

fn round_i32(value: f64) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    i32::try_from(value.round() as i64).ok()
}

#[cfg(test)]
mod tests {
    use super::{read_line_limited, resolve_geometry, StreamState, MAX_EVENT_BYTES};
    use serde_json::Value;
    use std::collections::HashMap;
    use std::io::{BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    const WINDOW: &str = r#"{
        "id":42,
        "title":"DJMAX RESPECT V",
        "app_id":"steam_app_960170",
        "workspace_id":7,
        "is_focused":true,
        "is_floating":true,
        "layout":{
            "window_size":[1280,720],
            "tile_pos_in_workspace_view":[340.4,100.8],
            "window_offset_in_tile":[1.6,0.8]
        }
    }"#;
    const WORKSPACE: &str = r#"{
        "id":7,
        "output":"DP-1",
        "is_active":true,
        "is_focused":true,
        "active_window_id":42
    }"#;
    const OUTPUT: &str = r#"{
        "name":"DP-1",
        "logical":{"x":-1920,"y":0,"width":1536,"height":864,"scale":1.25,"transform":"Normal"}
    }"#;
    const LAYERS: &str = r#"[
        {"namespace":"overmax","output":"DP-1","layer":"Overlay","keyboard_interactivity":"None"}
    ]"#;

    fn fixture() -> (StreamState, HashMap<String, Value>, Vec<Value>) {
        let window: Value = serde_json::from_str(WINDOW).unwrap();
        let workspace: Value = serde_json::from_str(WORKSPACE).unwrap();
        let output: Value = serde_json::from_str(OUTPUT).unwrap();
        let layers: Vec<Value> = serde_json::from_str(LAYERS).unwrap();
        let mut state = StreamState::default();
        state.windows.insert(42, window);
        state.workspaces.insert(7, workspace);
        (state, HashMap::from([("DP-1".to_string(), output)]), layers)
    }

    #[test]
    fn resolves_active_window_from_output_and_workspace_logical_coordinates() {
        let (state, outputs, layers) = fixture();
        let geometry = resolve_geometry("DJMAX RESPECT V", &state, &outputs, &layers).unwrap();
        assert_eq!(geometry.output, "DP-1");
        assert_eq!(geometry.rect.left, -1578);
        assert_eq!(geometry.rect.top, 102);
        assert_eq!(geometry.rect.width, 1280);
        assert_eq!(geometry.rect.height, 720);
    }

    #[test]
    fn keeps_manual_placement_for_hidden_ambiguous_or_unmapped_windows() {
        let (mut state, outputs, layers) = fixture();
        state.workspaces.get_mut(&7).unwrap()["is_active"] = Value::Bool(false);
        assert!(resolve_geometry("DJMAX RESPECT V", &state, &outputs, &layers).is_none());

        let (mut state, outputs, layers) = fixture();
        state.windows.insert(
            43,
            serde_json::from_str(&WINDOW.replace("42", "43")).unwrap(),
        );
        assert!(resolve_geometry("DJMAX RESPECT V", &state, &outputs, &layers).is_none());

        let (mut state, outputs, layers) = fixture();
        state.workspaces.get_mut(&7).unwrap()["active_window_id"] = Value::from(41);
        assert!(resolve_geometry("DJMAX RESPECT V", &state, &outputs, &layers).is_none());
    }

    #[test]
    fn keeps_manual_placement_when_external_layer_surfaces_make_work_area_unknown() {
        let (state, outputs, mut layers) = fixture();
        layers.push(serde_json::json!({
            "namespace": "waybar",
            "output": "DP-1",
            "layer": "Top",
            "keyboard_interactivity": "None"
        }));
        assert!(resolve_geometry("DJMAX RESPECT V", &state, &outputs, &layers).is_none());
    }

    #[test]
    fn applies_layout_updates_and_window_removal_events() {
        let mut state = StreamState::default();
        state
            .windows
            .insert(42, serde_json::from_str(WINDOW).unwrap());

        let changed: Value = serde_json::from_str(
            r#"{"WindowLayoutsChanged":{"changes":[[42,{
                "window_size":[1000,700],
                "tile_pos_in_workspace_view":[200.0,100.0],
                "window_offset_in_tile":[0.0,0.0]
            }]]}}"#,
        )
        .unwrap();
        assert!(state.apply_event(&changed).unwrap());
        assert_eq!(state.windows[&42]["layout"]["window_size"][0], 1000);

        let closed: Value = serde_json::from_str(r#"{"WindowClosed":{"id":42}}"#).unwrap();
        assert!(state.apply_event(&closed).unwrap());
        assert!(!state.windows.contains_key(&42));
    }

    #[test]
    fn partial_event_line_survives_idle_read_timeout() {
        let (mut writer, reader) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();

        writer.write_all(b"{\"TouchpadToggled\":").unwrap();
        let error = read_line_limited(&mut reader, &mut line, MAX_EVENT_BYTES).unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ));
        assert_eq!(line, b"{\"TouchpadToggled\":");

        writer.write_all(b"{\"on\":true}}\n").unwrap();
        assert!(read_line_limited(&mut reader, &mut line, MAX_EVENT_BYTES).unwrap() > 0);
        assert_eq!(line, b"{\"TouchpadToggled\":{\"on\":true}}\n");
    }
}
