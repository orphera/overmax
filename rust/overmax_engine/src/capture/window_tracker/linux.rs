use super::{FocusObservation, FocusSource, FocusState, WindowRect, WindowSnapshot};
use std::cell::Cell;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, GetPropertyReply, Window,
};
use x11rb::protocol::ErrorKind;
use x11rb::rust_connection::RustConnection;

const POINTER_ROOT: Window = 1;

x11rb::atom_manager! {
    Atoms:
    AtomsCookie {
        _NET_WM_NAME,
        UTF8_STRING,
        _NET_SUPPORTED,
        _NET_ACTIVE_WINDOW,
        _NET_WM_STATE,
        _NET_WM_STATE_FOCUSED,
        _NET_WM_STATE_FULLSCREEN,
        _NET_CLIENT_LIST,
    }
}

struct TrackerConnection {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
    supported_atoms: Vec<Atom>,
    last_focus: Cell<Option<FocusObservation>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowFocusTelemetry {
    pub target_xid: u64,
    pub active_xid: Option<u64>,
    pub active_in_client_list: Option<bool>,
}

struct WindowSearch {
    target: Option<Window>,
    #[cfg_attr(not(any(debug_assertions, feature = "telemetry")), allow(dead_code))]
    client_list: Option<Vec<Window>>,
}

struct RawFocusObservation {
    focus: FocusObservation,
    #[cfg_attr(not(any(debug_assertions, feature = "telemetry")), allow(dead_code))]
    active_window: Option<Window>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManagedClient {
    Target,
    Other,
}

#[derive(Clone, Copy)]
struct FocusEvidence {
    active: Option<ManagedClient>,
    input: Option<ManagedClient>,
    target_focused_state: bool,
}

pub struct WindowTracker {
    title: Vec<u8>,
    connection: Result<TrackerConnection, String>,
}

impl WindowTracker {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.as_bytes().to_vec(),
            connection: TrackerConnection::new(),
        }
    }

    pub fn game_snapshot(&self) -> Result<Option<(WindowSnapshot, FocusObservation)>, String> {
        self.connection
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|connection| connection.game_snapshot(&self.title))
            .map(|(snapshot, _)| snapshot)
    }

    #[cfg(any(debug_assertions, feature = "telemetry"))]
    pub(crate) fn game_snapshot_with_telemetry(
        &self,
    ) -> Result<
        (
            Option<(WindowSnapshot, FocusObservation)>,
            Option<WindowFocusTelemetry>,
        ),
        String,
    > {
        self.connection
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|connection| connection.game_snapshot(&self.title))
    }
}

impl TrackerConnection {
    fn new() -> Result<Self, String> {
        let (conn, screen_num) = x11rb::connect(None).map_err(|error| error.to_string())?;
        let root = conn
            .setup()
            .roots
            .get(screen_num)
            .ok_or_else(|| format!("X11 screen {screen_num} is missing"))?
            .root;
        let atoms = Atoms::new(&conn)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;
        let supported_atoms = query_supported_atoms(&conn, root, atoms._NET_SUPPORTED)?;
        Ok(Self {
            conn,
            root,
            atoms,
            supported_atoms,
            last_focus: Cell::new(None),
        })
    }

    fn game_snapshot(
        &self,
        title: &[u8],
    ) -> Result<
        (
            Option<(WindowSnapshot, FocusObservation)>,
            Option<WindowFocusTelemetry>,
        ),
        String,
    > {
        let search = self.find_window(title)?;
        let Some(window) = search.target else {
            return Ok((None, None));
        };
        let Some(geometry) = reply_or_window_gone(
            self.conn
                .get_geometry(window)
                .map_err(|error| error.to_string())?
                .reply(),
        )?
        else {
            return Ok((None, None));
        };
        let Some(position) = reply_or_window_gone(
            self.conn
                .translate_coordinates(window, self.root, 0, 0)
                .map_err(|error| error.to_string())?
                .reply(),
        )?
        else {
            return Ok((None, None));
        };
        let rect = WindowRect {
            left: i32::from(position.dst_x),
            top: i32::from(position.dst_y),
            width: i32::from(geometry.width),
            height: i32::from(geometry.height),
        };
        if !rect.is_valid() {
            return Ok((None, None));
        }
        let focus = self.focus_observation(window, search.client_list.as_deref())?;
        let Some(fullscreen) = self.is_fullscreen(window)? else {
            return Ok((None, None));
        };
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        let focus_telemetry = Some(WindowFocusTelemetry {
            target_xid: u64::from(window),
            active_xid: focus.active_window.map(u64::from),
            active_in_client_list: active_window_membership(
                focus.active_window,
                search.client_list.as_deref(),
            ),
        });
        #[cfg(not(any(debug_assertions, feature = "telemetry")))]
        let focus_telemetry = None;
        Ok((
            Some((
                WindowSnapshot {
                    window: u64::from(window),
                    rect,
                    foreground: focus.focus.state == FocusState::Focused,
                    fullscreen,
                },
                focus.focus,
            )),
            focus_telemetry,
        ))
    }

    fn find_window(&self, title: &[u8]) -> Result<WindowSearch, String> {
        let client_list = if self.supports(self.atoms._NET_CLIENT_LIST) {
            let reply = self
                .conn
                .get_property(
                    false,
                    self.root,
                    self.atoms._NET_CLIENT_LIST,
                    AtomEnum::WINDOW,
                    0,
                    u32::MAX,
                )
                .map_err(|error| error.to_string())?
                .reply()
                .map_err(|error| error.to_string())?;
            parse_window_list(&reply)?
        } else {
            None
        };
        let fallback_candidates;
        let candidates = match client_list.as_deref() {
            Some(windows) => windows,
            None => {
                fallback_candidates = self
                    .conn
                    .query_tree(self.root)
                    .map_err(|error| error.to_string())?
                    .reply()
                    .map_err(|error| error.to_string())?
                    .children;
                &fallback_candidates
            }
        };
        let mut matches = Vec::new();
        for &window in candidates {
            if self.title_matches(window, title)? {
                matches.push(window);
            }
        }
        Ok(WindowSearch {
            target: unique_window(&matches),
            client_list,
        })
    }

    fn title_matches(&self, window: Window, title: &[u8]) -> Result<bool, String> {
        let Some(net_name) = reply_or_window_gone(
            self.conn
                .get_property(
                    false,
                    window,
                    self.atoms._NET_WM_NAME,
                    self.atoms.UTF8_STRING,
                    0,
                    256,
                )
                .map_err(|error| error.to_string())?
                .reply(),
        )?
        else {
            return Ok(false);
        };
        if net_name.type_ == self.atoms.UTF8_STRING && net_name.format == 8 {
            return Ok(net_name.bytes_after == 0 && net_name.value == title);
        }

        let Some(wm_name) = reply_or_window_gone(
            self.conn
                .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256)
                .map_err(|error| error.to_string())?
                .reply(),
        )?
        else {
            return Ok(false);
        };
        Ok(wm_name.type_ == u32::from(AtomEnum::STRING)
            && wm_name.format == 8
            && wm_name.bytes_after == 0
            && wm_name.value == title)
    }

    fn focus_observation(
        &self,
        target: Window,
        client_list: Option<&[Window]>,
    ) -> Result<RawFocusObservation, String> {
        let active_window = if self.supports(self.atoms._NET_ACTIVE_WINDOW) {
            self.property_window(self.root, self.atoms._NET_ACTIVE_WINDOW)?
        } else {
            None
        };
        let input_window = match self
            .conn
            .get_input_focus()
            .map_err(|error| error.to_string())?
            .reply()
        {
            Ok(reply) => Some(reply.focus),
            Err(ReplyError::X11Error(_)) => None,
            Err(ReplyError::ConnectionError(error)) => return Err(error.to_string()),
        };
        let target_focused_state = self.supports(self.atoms._NET_WM_STATE)
            && self.supports(self.atoms._NET_WM_STATE_FOCUSED)
            && self.window_has_atom(
                target,
                self.atoms._NET_WM_STATE,
                self.atoms._NET_WM_STATE_FOCUSED,
            )?;
        let evidence = FocusEvidence {
            active: self.normalize_managed_client(active_window, target, client_list)?,
            input: self.normalize_managed_client(input_window, target, client_list)?,
            target_focused_state,
        };
        let (state, source) = decide_focus(evidence);
        Ok(RawFocusObservation {
            focus: self.commit_focus(state, source),
            active_window,
        })
    }

    fn property_window(&self, window: Window, property: Atom) -> Result<Option<Window>, String> {
        let reply = self
            .conn
            .get_property(false, window, property, AtomEnum::WINDOW, 0, 1)
            .map_err(|error| error.to_string())?
            .reply();
        let reply = match reply {
            Ok(reply) => reply,
            Err(ReplyError::X11Error(_)) => return Ok(None),
            Err(ReplyError::ConnectionError(error)) => return Err(error.to_string()),
        };
        Ok(
            (reply.type_ == u32::from(AtomEnum::WINDOW) && reply.format == 32)
                .then(|| reply.value32().and_then(|mut values| values.next()))
                .flatten(),
        )
    }

    fn window_has_atom(
        &self,
        window: Window,
        property: Atom,
        expected: Atom,
    ) -> Result<bool, String> {
        let reply = self
            .conn
            .get_property(false, window, property, AtomEnum::ATOM, 0, 32)
            .map_err(|error| error.to_string())?
            .reply();
        let reply = match reply {
            Ok(reply) => reply,
            Err(ReplyError::X11Error(_)) => return Ok(false),
            Err(ReplyError::ConnectionError(error)) => return Err(error.to_string()),
        };
        Ok(reply.type_ == u32::from(AtomEnum::ATOM)
            && reply.format == 32
            && reply
                .value32()
                .is_some_and(|atoms| atoms.into_iter().any(|atom| atom == expected)))
    }

    fn normalize_managed_client(
        &self,
        window: Option<Window>,
        target: Window,
        client_list: Option<&[Window]>,
    ) -> Result<Option<ManagedClient>, String> {
        let Some(mut window) = window else {
            return Ok(None);
        };
        for _ in 0..32 {
            if window == x11rb::NONE || window == POINTER_ROOT || window == self.root {
                return Ok(None);
            }
            if window == target {
                return Ok(Some(ManagedClient::Target));
            }
            if client_list.is_some_and(|clients| clients.contains(&window)) {
                return Ok(Some(ManagedClient::Other));
            }
            let tree = match self
                .conn
                .query_tree(window)
                .map_err(|error| error.to_string())?
                .reply()
            {
                Ok(tree) => tree,
                Err(ReplyError::X11Error(_)) => return Ok(None),
                Err(ReplyError::ConnectionError(error)) => return Err(error.to_string()),
            };
            if tree.parent == x11rb::NONE || tree.parent == window {
                return Ok(None);
            }
            window = tree.parent;
        }
        Ok(None)
    }

    fn commit_focus(&self, state: FocusState, source: FocusSource) -> FocusObservation {
        if let Some(previous) = self.last_focus.get() {
            if previous.state == state && previous.source == source {
                return previous;
            }
        }
        let generation = self
            .last_focus
            .get()
            .map_or(1, |previous| previous.generation.wrapping_add(1));
        let observation = FocusObservation {
            state,
            source,
            generation,
        };
        self.last_focus.set(Some(observation));
        observation
    }

    fn supports(&self, atom: Atom) -> bool {
        self.supported_atoms.contains(&atom)
    }

    fn is_fullscreen(&self, window: Window) -> Result<Option<bool>, String> {
        let reply = reply_or_window_gone(
            self.conn
                .get_property(
                    false,
                    window,
                    self.atoms._NET_WM_STATE,
                    AtomEnum::ATOM,
                    0,
                    32,
                )
                .map_err(|error| error.to_string())?
                .reply(),
        )?;
        Ok(reply.map(|reply| {
            reply.type_ == u32::from(AtomEnum::ATOM)
                && reply.format == 32
                && reply.value32().is_some_and(|atoms| {
                    atoms
                        .into_iter()
                        .any(|atom| atom == self.atoms._NET_WM_STATE_FULLSCREEN)
                })
        }))
    }

    fn activate(&self, window: Window) -> Result<(), String> {
        let event = ClientMessageEvent::new(
            32,
            window,
            self.atoms._NET_ACTIVE_WINDOW,
            [1, x11rb::CURRENT_TIME, 0, 0, 0],
        );
        self.conn
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .map_err(|error| error.to_string())?
            .check()
            .map_err(|error| error.to_string())?;
        self.conn.flush().map_err(|error| error.to_string())
    }
}

pub fn restore_foreground_by_title(title: &str) -> bool {
    let tracker = WindowTracker::new(title);
    let Ok(Some((snapshot, _))) = tracker.game_snapshot() else {
        return false;
    };
    let Ok(window) = Window::try_from(snapshot.window) else {
        return false;
    };
    tracker
        .connection
        .as_ref()
        .is_ok_and(|connection| connection.activate(window).is_ok())
}

fn decide_focus(evidence: FocusEvidence) -> (FocusState, FocusSource) {
    let target = matches!(evidence.active, Some(ManagedClient::Target))
        || matches!(evidence.input, Some(ManagedClient::Target));
    let other = matches!(evidence.active, Some(ManagedClient::Other))
        || matches!(evidence.input, Some(ManagedClient::Other));
    if (target && other) || (evidence.target_focused_state && other) {
        return (FocusState::Unknown, FocusSource::None);
    }
    if matches!(evidence.active, Some(ManagedClient::Target)) {
        return (FocusState::Focused, FocusSource::EwmhActiveWindow);
    }
    if matches!(evidence.input, Some(ManagedClient::Target)) {
        return (FocusState::Focused, FocusSource::XInputFocus);
    }
    if matches!(evidence.active, Some(ManagedClient::Other)) {
        return (FocusState::Background, FocusSource::EwmhActiveWindow);
    }
    if matches!(evidence.input, Some(ManagedClient::Other)) {
        return (FocusState::Background, FocusSource::XInputFocus);
    }
    if evidence.target_focused_state {
        return (FocusState::Unknown, FocusSource::EwmhFocusedState);
    }
    (FocusState::Unknown, FocusSource::None)
}

fn query_supported_atoms(
    conn: &RustConnection,
    root: Window,
    property: Atom,
) -> Result<Vec<Atom>, String> {
    let reply = conn
        .get_property(false, root, property, AtomEnum::ATOM, 0, u32::MAX)
        .map_err(|error| error.to_string())?
        .reply();
    match reply {
        Ok(reply) => Ok(parse_atom_list(&reply).unwrap_or_default()),
        Err(ReplyError::X11Error(_)) => Ok(Vec::new()),
        Err(ReplyError::ConnectionError(error)) => Err(error.to_string()),
    }
}

fn parse_atom_list(reply: &GetPropertyReply) -> Option<Vec<Atom>> {
    (reply.type_ == u32::from(AtomEnum::ATOM) && reply.format == 32)
        .then(|| reply.value32().map(Iterator::collect))
        .flatten()
}

#[cfg(any(debug_assertions, feature = "telemetry"))]
fn active_window_membership(
    active_window: Option<Window>,
    client_list: Option<&[Window]>,
) -> Option<bool> {
    Some(client_list?.contains(&active_window?))
}

fn parse_window_list(reply: &GetPropertyReply) -> Result<Option<Vec<Window>>, String> {
    if reply.type_ == x11rb::NONE
        && reply.format == 0
        && reply.value_len == 0
        && reply.value.is_empty()
    {
        return Ok(None);
    }
    if reply.type_ != u32::from(AtomEnum::WINDOW) || reply.format != 32 {
        return Err("malformed _NET_CLIENT_LIST property".to_string());
    }
    let expected_len = usize::try_from(reply.value_len)
        .ok()
        .and_then(|len| len.checked_mul(4))
        .ok_or_else(|| "oversized _NET_CLIENT_LIST property".to_string())?;
    if reply.bytes_after != 0 || reply.value.len() != expected_len {
        return Err("truncated _NET_CLIENT_LIST property".to_string());
    }
    let windows = reply
        .value32()
        .ok_or_else(|| "malformed _NET_CLIENT_LIST property".to_string())?
        .collect::<Vec<_>>();
    Ok((!windows.is_empty()).then_some(windows))
}

fn unique_window(windows: &[Window]) -> Option<Window> {
    match windows {
        [window] => Some(*window),
        _ => None,
    }
}

fn reply_or_window_gone<T>(reply: Result<T, ReplyError>) -> Result<Option<T>, String> {
    match reply {
        Ok(reply) => Ok(Some(reply)),
        Err(ReplyError::X11Error(error)) if error.error_kind == ErrorKind::Window => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        active_window_membership, decide_focus, parse_window_list, reply_or_window_gone,
        restore_foreground_by_title, unique_window, Atoms, FocusEvidence, ManagedClient,
        WindowTracker,
    };
    use crate::capture::window_tracker::{FocusSource, FocusState};
    use std::time::{Duration, Instant};
    use x11rb::connection::Connection;
    use x11rb::errors::{ConnectionError, ReplyError};
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux,
        EventMask, GetPropertyReply, PropMode, WindowClass,
    };
    use x11rb::wrapper::ConnectionExt as _;

    fn window_list(windows: &[u32]) -> GetPropertyReply {
        GetPropertyReply {
            format: 32,
            type_: u32::from(AtomEnum::WINDOW),
            value_len: windows.len() as u32,
            value: windows
                .iter()
                .flat_map(|window| window.to_ne_bytes())
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn parses_present_and_absent_client_lists() {
        assert_eq!(
            parse_window_list(&window_list(&[7, 9])).unwrap(),
            Some(vec![7, 9])
        );
        assert_eq!(
            parse_window_list(&GetPropertyReply::default()).unwrap(),
            None
        );
        assert_eq!(parse_window_list(&window_list(&[])).unwrap(), None);
    }

    #[test]
    fn rejects_malformed_client_list() {
        let mut reply = window_list(&[7]);
        reply.format = 8;
        assert!(parse_window_list(&reply).is_err());
    }

    #[test]
    fn duplicate_match_fails_closed() {
        assert_eq!(unique_window(&[7]), Some(7));
        assert_eq!(unique_window(&[7, 9]), None);
        assert_eq!(unique_window(&[]), None);
    }

    #[test]
    fn mango_placeholder_active_window_fixture_is_not_a_client() {
        // Sanitized Mango observation: target is registered, active XID is an
        // unrelated XWayland placeholder absent from _NET_CLIENT_LIST.
        const TARGET_XID: u32 = 0x1000;
        const PLACEHOLDER_ACTIVE_XID: u32 = 0x2000;
        const CLIENT_LIST: &[u32] = &[TARGET_XID, 0x3000];

        assert_eq!(
            active_window_membership(Some(TARGET_XID), Some(CLIENT_LIST)),
            Some(true)
        );
        assert_eq!(
            active_window_membership(Some(PLACEHOLDER_ACTIVE_XID), Some(CLIENT_LIST)),
            Some(false)
        );
        assert_eq!(
            decide_focus(FocusEvidence {
                active: None,
                input: None,
                target_focused_state: false,
            }),
            (FocusState::Unknown, FocusSource::None)
        );
    }

    #[test]
    fn decides_focus_only_from_positive_managed_client_evidence() {
        assert_eq!(
            decide_focus(FocusEvidence {
                active: Some(ManagedClient::Target),
                input: None,
                target_focused_state: false,
            }),
            (FocusState::Focused, FocusSource::EwmhActiveWindow)
        );
        assert_eq!(
            decide_focus(FocusEvidence {
                active: Some(ManagedClient::Other),
                input: None,
                target_focused_state: false,
            }),
            (FocusState::Background, FocusSource::EwmhActiveWindow)
        );
        assert_eq!(
            decide_focus(FocusEvidence {
                active: None,
                input: Some(ManagedClient::Target),
                target_focused_state: false,
            }),
            (FocusState::Focused, FocusSource::XInputFocus)
        );
        assert_eq!(
            decide_focus(FocusEvidence {
                active: Some(ManagedClient::Other),
                input: None,
                target_focused_state: true,
            }),
            (FocusState::Unknown, FocusSource::None)
        );
        assert_eq!(
            decide_focus(FocusEvidence {
                active: None,
                input: None,
                target_focused_state: true,
            }),
            (FocusState::Unknown, FocusSource::EwmhFocusedState)
        );
    }

    #[test]
    fn transport_error_is_not_window_missing() {
        let reply: Result<(), ReplyError> =
            Err(ReplyError::ConnectionError(ConnectionError::UnknownError));
        assert!(reply_or_window_gone(reply).is_err());
    }

    #[test]
    #[ignore = "requires DISPLAY with an EWMH window manager"]
    fn x11_ewmh_snapshot_lifecycle() {
        let (conn, screen_num) = x11rb::connect(None).expect("connect test X11 client");
        let screen = &conn.setup().roots[screen_num];
        let window = conn.generate_id().expect("allocate test window ID");
        let title = format!("overmax-vertical-slice-lifecycle-{}", std::process::id());
        let atoms = Atoms::new(&conn)
            .expect("request atoms")
            .reply()
            .expect("intern atoms");

        conn.create_window(
            screen.root_depth,
            window,
            screen.root,
            20,
            20,
            320,
            180,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new().background_pixel(screen.black_pixel),
        )
        .expect("send create test window")
        .check()
        .expect("create test window");
        conn.change_property8(
            PropMode::REPLACE,
            window,
            atoms._NET_WM_NAME,
            atoms.UTF8_STRING,
            title.as_bytes(),
        )
        .expect("set _NET_WM_NAME");
        conn.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        )
        .expect("set WM_NAME");
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atoms._NET_WM_STATE,
            AtomEnum::ATOM,
            &[atoms._NET_WM_STATE_FULLSCREEN],
        )
        .expect("set fullscreen state");
        conn.map_window(window)
            .expect("send map test window")
            .check()
            .expect("map test window");
        conn.flush().expect("flush test window");

        let tracker = WindowTracker::new(&title);
        let deadline = Instant::now() + Duration::from_secs(5);
        let snapshot = loop {
            match tracker.game_snapshot() {
                Ok(Some((snapshot, _))) if snapshot.window == u64::from(window) => break snapshot,
                Ok(_) => {}
                Err(error) => panic!("tracker failed: {error}"),
            }
            assert!(Instant::now() < deadline, "EWMH client list was not ready");
            std::thread::sleep(Duration::from_millis(25));
        };
        assert!(snapshot.rect.is_valid());
        assert!(snapshot.fullscreen);

        let fullscreen_off = ClientMessageEvent::new(
            32,
            window,
            atoms._NET_WM_STATE,
            [0, atoms._NET_WM_STATE_FULLSCREEN, 0, 1, 0],
        );
        conn.send_event(
            false,
            screen.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            fullscreen_off,
        )
        .expect("send fullscreen removal")
        .check()
        .expect("remove fullscreen state");
        conn.flush().expect("flush fullscreen removal");
        let deadline = Instant::now() + Duration::from_secs(5);
        let windowed = loop {
            match tracker.game_snapshot() {
                Ok(Some((snapshot, _)))
                    if snapshot.window == u64::from(window) && !snapshot.fullscreen =>
                {
                    break snapshot;
                }
                Ok(_) => {}
                Err(error) => panic!("tracker failed after leaving fullscreen: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "fullscreen state was not removed"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        let initial_rect = windowed.rect;

        conn.configure_window(window, &ConfigureWindowAux::new().x(180).y(120))
            .expect("send move test window")
            .check()
            .expect("move test window");
        conn.flush().expect("flush move");
        let deadline = Instant::now() + Duration::from_secs(5);
        let moved = loop {
            match tracker.game_snapshot() {
                Ok(Some((snapshot, _)))
                    if snapshot.window == u64::from(window)
                        && (snapshot.rect.left != initial_rect.left
                            || snapshot.rect.top != initial_rect.top) =>
                {
                    break snapshot;
                }
                Ok(_) => {}
                Err(error) => panic!("tracker failed after move: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "moved window geometry was not observed"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        assert_ne!(moved.rect.left, snapshot.rect.left);
        assert_ne!(moved.rect.top, snapshot.rect.top);

        conn.configure_window(window, &ConfigureWindowAux::new().width(420).height(240))
            .expect("send resize test window")
            .check()
            .expect("resize test window");
        conn.flush().expect("flush resize");
        let deadline = Instant::now() + Duration::from_secs(5);
        let resized = loop {
            match tracker.game_snapshot() {
                Ok(Some((snapshot, _)))
                    if snapshot.window == u64::from(window)
                        && snapshot.rect.width == 420
                        && snapshot.rect.height == 240 =>
                {
                    break snapshot;
                }
                Ok(_) => {}
                Err(error) => panic!("tracker failed after resize: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "resized window geometry was not observed"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        assert_eq!(resized.rect.width, 420);
        assert_eq!(resized.rect.height, 240);

        if !snapshot.foreground {
            assert!(restore_foreground_by_title(&title));
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if tracker
                    .game_snapshot()
                    .expect("query activated window")
                    .is_some_and(|(snapshot, _)| snapshot.foreground)
                {
                    break;
                }
                assert!(Instant::now() < deadline, "window was not activated");
                std::thread::sleep(Duration::from_millis(25));
            }
        }

        conn.destroy_window(window)
            .expect("send destroy test window")
            .check()
            .expect("destroy test window");
        conn.flush().expect("flush destroy");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if tracker
                .game_snapshot()
                .expect("query destroyed window")
                .is_none()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "destroyed window remained visible"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
