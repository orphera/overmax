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

struct LastGeometry {
    window: Value,
    output_logical: Value,
    geometry: ResolvedGeometry,
}

#[derive(Default)]
struct StreamState {
    windows: HashMap<u64, Value>,
    workspaces: HashMap<u64, Value>,
    last_geometry: Option<LastGeometry>,
}

impl StreamState {
    fn resolve_update(
        &mut self,
        title: &str,
        outputs: &HashMap<String, Value>,
        layers: &[Value],
    ) -> Option<ResolvedGeometry> {
        let mut matching = self.windows.values().filter(|window| {
            window.get("title").and_then(Value::as_str) == Some(title)
                || window.get("app_id").and_then(Value::as_str) == Some(title)
        });
        let window = matching.next().filter(|_| matching.next().is_none());
        let resolved = resolve_geometry(title, self, outputs, layers);
        if let (Some(window), Some(geometry)) = (window, resolved) {
            self.last_geometry = Some(LastGeometry {
                window: window.clone(),
                output_logical: outputs[&geometry.output]["logical"].clone(),
                geometry: geometry.clone(),
            });
            return Some(geometry);
        }

        // niri withdraws both fields during an interactive floating-window move.
        // Retain only this window's last validated geometry in an unchanged work area.
        let retained = window
            .zip(self.last_geometry.as_ref())
            .filter(|(window, last)| {
                let workspace = json_u64(last.window.get("workspace_id"))
                    .and_then(|id| self.workspaces.get(&id));
                window.get("id") == last.window.get("id")
                    && ((window.get("workspace_id") == Some(&Value::Null)
                        && window["layout"].get("tile_pos_in_workspace_view") == Some(&Value::Null))
                        // WorkspaceActiveWindowChanged arrives before the window update.
                        || (window.get("workspace_id") == last.window.get("workspace_id")
                            && window.get("layout") == last.window.get("layout")))
                    && window.get("is_floating").and_then(Value::as_bool) == Some(true)
                    && window.get("is_focused").and_then(Value::as_bool) == Some(true)
                    && window["layout"]["window_size"] == last.window["layout"]["window_size"]
                    && workspace.is_some_and(|workspace| {
                        workspace.get("is_active").and_then(Value::as_bool) == Some(true)
                            && workspace.get("output").and_then(Value::as_str)
                                == Some(last.geometry.output.as_str())
                    })
                    && outputs
                        .get(&last.geometry.output)
                        .and_then(|output| output.get("logical"))
                        == Some(&last.output_logical)
                    && !layers.iter().any(|layer| {
                        layer.get("output").and_then(Value::as_str)
                            == Some(last.geometry.output.as_str())
                            && layer.get("namespace").and_then(Value::as_str) != Some("overmax")
                    })
            })
            .map(|(_, last)| last.geometry.clone());
        if retained.is_none() {
            self.last_geometry = None;
        }
        retained
    }

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
            "WindowFocusChanged" => {
                let focused_id = json_u64(body.get("id"));
                for (id, window) in &mut self.windows {
                    window["is_focused"] = Value::Bool(Some(*id) == focused_id);
                }
                Ok(true)
            }
            "WindowClosed" => {
                let id = json_u64(body.get("id"))
                    .ok_or_else(|| "WindowClosed has no valid id".to_string())?;
                self.windows.remove(&id);
                if self
                    .last_geometry
                    .as_ref()
                    .is_some_and(|last| json_u64(last.window.get("id")) == Some(id))
                {
                    self.last_geometry = None;
                }
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
                    refresh_geometry(socket, title, &mut state, sink, generation, on_change)?;
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
                refresh_geometry(socket, title, &mut state, sink, generation, on_change)?;
                last_metadata_refresh = Instant::now();
            }
            Err(error) => return Err(format!("read Niri event stream: {error}")),
        }
    }
}

fn refresh_geometry(
    socket: &Path,
    title: &str,
    state: &mut StreamState,
    sink: &SharedGameGeometry,
    generation: &mut u64,
    on_change: &Arc<dyn Fn() + Send + Sync>,
) -> Result<(), String> {
    let (outputs, layers) = query_compositor_metadata(socket)?;
    let geometry = state.resolve_update(title, &outputs, &layers);
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

    // Exercise the same metadata refresh and publication path as the IPC worker.
    fn refresh_fixture(
        state: &mut StreamState,
        outputs: &HashMap<String, Value>,
        layers: &[Value],
        sink: &super::SharedGameGeometry,
        generation: &mut u64,
    ) {
        use std::os::unix::net::UnixListener;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "overmax-niri-test-{}-{}.sock",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
        ));
        let listener = UnixListener::bind(&path).unwrap();
        let replies = [
            serde_json::json!({"Ok":{"Outputs":outputs}}),
            serde_json::json!({"Ok":{"Layers":layers}}),
        ];
        let server = std::thread::spawn(move || {
            use std::io::BufRead;
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut request)
                    .unwrap();
                writeln!(stream, "{reply}").unwrap();
            }
        });
        let on_change: super::super::GeometryChangedCallback = std::sync::Arc::new(|| {});
        super::refresh_geometry(
            &path,
            "DJMAX RESPECT V",
            state,
            sink,
            generation,
            &on_change,
        )
        .unwrap();
        server.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }

    fn begin_drag(state: &mut StreamState) {
        let mut window = state.windows[&42].clone();
        window["workspace_id"] = Value::Null;
        window["layout"]["tile_pos_in_workspace_view"] = Value::Null;
        state
            .apply_event(&serde_json::json!({
                "WindowOpenedOrChanged":{"window":window}
            }))
            .unwrap();
    }

    #[test]
    fn retains_published_geometry_during_drag_and_updates_on_drop() {
        let (mut state, outputs, layers) = fixture();
        let sink = std::sync::Arc::new(std::sync::Mutex::new(None));
        let mut generation = 0;
        refresh_fixture(&mut state, &outputs, &layers, &sink, &mut generation);
        let before = sink.lock().unwrap().clone().unwrap();
        // niri 26.04 emits this before withdrawing the dragged window's workspace/layout.
        state
            .apply_event(&serde_json::json!({
                "WorkspaceActiveWindowChanged":{"workspace_id":7,"active_window_id":99}
            }))
            .unwrap();
        refresh_fixture(&mut state, &outputs, &layers, &sink, &mut generation);
        assert_eq!(*sink.lock().unwrap(), Some(before.clone()));
        begin_drag(&mut state);
        for _ in 0..2 {
            refresh_fixture(&mut state, &outputs, &layers, &sink, &mut generation);
            assert_eq!(
                *sink.lock().unwrap(),
                Some(before.clone()),
                "drag must retain the published position instead of falling back to (40, 40)"
            );
            assert_eq!(generation, before.generation);
        }
        state
            .apply_event(&serde_json::json!({
                "WorkspaceActiveWindowChanged":{"workspace_id":7,"active_window_id":42}
            }))
            .unwrap();
        refresh_fixture(&mut state, &outputs, &layers, &sink, &mut generation);
        assert_eq!(*sink.lock().unwrap(), Some(before.clone()));
        let window = state.windows.get_mut(&42).unwrap();
        window["workspace_id"] = Value::from(7);
        window["layout"]["tile_pos_in_workspace_view"] = serde_json::json!([500.0, 200.0]);
        refresh_fixture(&mut state, &outputs, &layers, &sink, &mut generation);
        let after = sink.lock().unwrap().clone().unwrap();
        assert_eq!(after.rect.left, -1418);
        assert_eq!(after.rect.top, 201);
        assert_eq!(generation, before.generation + 1);
    }

    #[test]
    fn drag_without_previous_valid_geometry_stays_unavailable() {
        let (mut state, outputs, layers) = fixture();
        begin_drag(&mut state);
        let sink = std::sync::Arc::new(std::sync::Mutex::new(None));
        let mut generation = 0;
        refresh_fixture(&mut state, &outputs, &layers, &sink, &mut generation);
        assert!(sink.lock().unwrap().is_none());
        assert_eq!(generation, 0);
    }

    #[test]
    fn invalid_drag_context_clears_geometry_instead_of_retaining_stale_position() {
        for case in [
            "closed",
            "duplicate",
            "replacement",
            "tiled",
            "unfocused",
            "inactive",
            "focus_event",
            "output_removed",
            "output_moved",
            "scale_changed",
            "layer_added",
            "resized",
            "missing_workspace_field",
            "missing_position_field",
            "close_reopen",
        ] {
            let (mut state, mut outputs, mut layers) = fixture();
            let sink = std::sync::Arc::new(std::sync::Mutex::new(None));
            let mut generation = 0;
            refresh_fixture(&mut state, &outputs, &layers, &sink, &mut generation);
            begin_drag(&mut state);
            match case {
                "closed" => {
                    state
                        .apply_event(&serde_json::json!({"WindowClosed":{"id":42}}))
                        .unwrap();
                }
                "duplicate" => {
                    state.windows.insert(43, state.windows[&42].clone());
                }
                "replacement" => {
                    state.windows.get_mut(&42).unwrap()["id"] = Value::from(43);
                }
                "tiled" => {
                    state.windows.get_mut(&42).unwrap()["is_floating"] = Value::Bool(false);
                }
                "unfocused" => {
                    state.windows.get_mut(&42).unwrap()["is_focused"] = Value::Bool(false);
                }
                "inactive" => {
                    state.workspaces.get_mut(&7).unwrap()["is_active"] = Value::Bool(false);
                }
                "focus_event" => {
                    state
                        .apply_event(&serde_json::json!({"WindowFocusChanged":{"id":99}}))
                        .unwrap();
                }
                "output_removed" => {
                    outputs.clear();
                }
                "output_moved" => {
                    outputs.get_mut("DP-1").unwrap()["logical"]["x"] = Value::from(0);
                }
                "scale_changed" => {
                    outputs.get_mut("DP-1").unwrap()["logical"]["scale"] = Value::from(2.0);
                }
                "layer_added" => {
                    layers.push(serde_json::json!({"namespace":"waybar","output":"DP-1"}));
                }
                "resized" => {
                    state.windows.get_mut(&42).unwrap()["layout"]["window_size"] =
                        serde_json::json!([640, 480]);
                }
                "missing_workspace_field" => {
                    state
                        .windows
                        .get_mut(&42)
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .remove("workspace_id");
                }
                "missing_position_field" => {
                    state.windows.get_mut(&42).unwrap()["layout"]
                        .as_object_mut()
                        .unwrap()
                        .remove("tile_pos_in_workspace_view");
                }
                "close_reopen" => {
                    let window = state.windows[&42].clone();
                    state
                        .apply_event(&serde_json::json!({"WindowClosed":{"id":42}}))
                        .unwrap();
                    state
                        .apply_event(
                            &serde_json::json!({"WindowOpenedOrChanged":{"window":window}}),
                        )
                        .unwrap();
                }
                _ => unreachable!(),
            }
            refresh_fixture(&mut state, &outputs, &layers, &sink, &mut generation);
            assert!(
                sink.lock().unwrap().is_none(),
                "must clear stale geometry: {case}"
            );
            assert!(
                state.last_geometry.is_none(),
                "must invalidate retained history: {case}"
            );
        }
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
