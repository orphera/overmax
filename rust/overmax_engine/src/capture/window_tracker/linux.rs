use super::{WindowRect, WindowSnapshot};
use std::cell::Cell;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, GetPropertyReply, Window,
};
use x11rb::protocol::ErrorKind;
use x11rb::rust_connection::RustConnection;

const POINTER_ROOT: Window = 1;

x11rb::atom_manager! {
    Atoms:
    AtomsCookie {
        _NET_WM_NAME,
        UTF8_STRING,
        _NET_ACTIVE_WINDOW,
        _NET_WM_STATE,
        _NET_WM_STATE_FULLSCREEN,
        _NET_CLIENT_LIST,
    }
}

struct TrackerConnection {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
}

pub struct WindowTracker {
    title: Vec<u8>,
    connection: Result<TrackerConnection, String>,
    cached_snapshot: Cell<Option<WindowSnapshot>>,
}

impl WindowTracker {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.as_bytes().to_vec(),
            connection: TrackerConnection::new(),
            cached_snapshot: Cell::new(None),
        }
    }

    pub fn game_snapshot(&self) -> Result<Option<WindowSnapshot>, String> {
        let result = self
            .connection
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|connection| connection.game_snapshot(&self.title));
        self.cached_snapshot
            .set(result.as_ref().ok().copied().flatten());
        result
    }

    pub fn game_rect(&self) -> Option<WindowRect> {
        self.cached_snapshot.get().map(|snapshot| snapshot.rect)
    }

    pub fn is_foreground(&self) -> bool {
        self.cached_snapshot
            .get()
            .is_some_and(|snapshot| snapshot.foreground)
    }

    pub fn is_fullscreen(&self) -> bool {
        self.cached_snapshot
            .get()
            .is_some_and(|snapshot| snapshot.fullscreen)
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
        Ok(Self { conn, root, atoms })
    }

    fn game_snapshot(&self, title: &[u8]) -> Result<Option<WindowSnapshot>, String> {
        let Some(window) = self.find_window(title)? else {
            return Ok(None);
        };
        let Some(geometry) = reply_or_window_gone(
            self.conn
                .get_geometry(window)
                .map_err(|error| error.to_string())?
                .reply(),
        )?
        else {
            return Ok(None);
        };
        let Some(position) = reply_or_window_gone(
            self.conn
                .translate_coordinates(window, self.root, 0, 0)
                .map_err(|error| error.to_string())?
                .reply(),
        )?
        else {
            return Ok(None);
        };
        let rect = WindowRect {
            left: i32::from(position.dst_x),
            top: i32::from(position.dst_y),
            width: i32::from(geometry.width),
            height: i32::from(geometry.height),
        };
        if !rect.is_valid() {
            return Ok(None);
        }
        let foreground = self.is_foreground(window)?;
        let Some(fullscreen) = self.is_fullscreen(window)? else {
            return Ok(None);
        };
        Ok(Some(WindowSnapshot {
            window: u64::from(window),
            rect,
            foreground,
            fullscreen,
        }))
    }

    fn find_window(&self, title: &[u8]) -> Result<Option<Window>, String> {
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
        let candidates = match parse_window_list(&reply)? {
            Some(windows) => windows,
            None => {
                self.conn
                    .query_tree(self.root)
                    .map_err(|error| error.to_string())?
                    .reply()
                    .map_err(|error| error.to_string())?
                    .children
            }
        };
        let mut matches = Vec::new();
        for window in candidates {
            if self.title_matches(window, title)? {
                matches.push(window);
            }
        }
        Ok(unique_window(&matches))
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

    fn is_foreground(&self, target: Window) -> Result<bool, String> {
        let reply = self
            .conn
            .get_property(
                false,
                self.root,
                self.atoms._NET_ACTIVE_WINDOW,
                AtomEnum::WINDOW,
                0,
                1,
            )
            .map_err(|error| error.to_string())?
            .reply();
        let reply = match reply {
            Ok(reply) => reply,
            Err(ReplyError::X11Error(_)) => return Ok(false),
            Err(ReplyError::ConnectionError(error)) => return Err(error.to_string()),
        };
        if reply.type_ != u32::from(AtomEnum::WINDOW) || reply.format != 32 {
            return Ok(false);
        }
        let Some(mut window) = reply.value32().and_then(|mut values| values.next()) else {
            return Ok(false);
        };
        if window == x11rb::NONE || window == POINTER_ROOT {
            return Ok(false);
        }

        for _ in 0..32 {
            if window == target {
                return Ok(true);
            }
            let tree = match self
                .conn
                .query_tree(window)
                .map_err(|error| error.to_string())?
                .reply()
            {
                Ok(tree) => tree,
                Err(ReplyError::X11Error(_)) => return Ok(false),
                Err(ReplyError::ConnectionError(error)) => return Err(error.to_string()),
            };
            if tree.parent == x11rb::NONE || tree.parent == self.root || tree.parent == window {
                return Ok(false);
            }
            window = tree.parent;
        }
        Ok(false)
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
    let Ok(Some(snapshot)) = tracker.game_snapshot() else {
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
        parse_window_list, reply_or_window_gone, restore_foreground_by_title, unique_window, Atoms,
        WindowTracker,
    };
    use std::time::{Duration, Instant};
    use x11rb::connection::Connection;
    use x11rb::errors::{ConnectionError, ReplyError};
    use x11rb::protocol::xproto::{
        AtomEnum, ConnectionExt as _, CreateWindowAux, GetPropertyReply, PropMode, WindowClass,
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
        let title = format!("overmax-m1-lifecycle-{}", std::process::id());
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
                Ok(Some(snapshot)) if snapshot.window == u64::from(window) => break snapshot,
                Ok(_) => {}
                Err(error) => panic!("tracker failed: {error}"),
            }
            assert!(Instant::now() < deadline, "EWMH client list was not ready");
            std::thread::sleep(Duration::from_millis(25));
        };
        assert!(snapshot.rect.is_valid());
        assert!(snapshot.fullscreen);

        if !snapshot.foreground {
            assert!(restore_foreground_by_title(&title));
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if tracker
                    .game_snapshot()
                    .expect("query activated window")
                    .is_some_and(|snapshot| snapshot.foreground)
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
