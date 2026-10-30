//! Compositor-coordinate game window geometry sources.
//!
//! Wayland gives clients no global window coordinates, so per-compositor IPC adapters
//! publish the game window rect here. Contract: docs/plans/2026-10-28-linux-window-geometry-sources.md

use super::WindowRect;
use std::sync::{Arc, Mutex};

/// Game window geometry in the compositor's global logical coordinate space.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameGeometry {
    /// `wl_output` name the window is on; the overlay ignores geometry for other outputs.
    pub output: String,
    pub rect: WindowRect,
    pub generation: u64,
}

/// Written by a source thread, read by the overlay. `None` means "unknown": sources must
/// clear it on lost tracking, ambiguous matches, hidden workspaces or IPC disconnects.
pub type SharedGameGeometry = Arc<Mutex<Option<GameGeometry>>>;
pub type GeometryChangedCallback = Arc<dyn Fn() + Send + Sync>;

pub trait GameGeometrySource: Send {
    fn name(&self) -> &'static str;

    /// Starts publishing geometry for `title`; call `on_change` after a changed value is
    /// published and the slot lock has been released.
    fn start(
        self: Box<Self>,
        title: &str,
        sink: SharedGameGeometry,
        on_change: GeometryChangedCallback,
    ) -> Result<(), String>;
}

/// Default for compositors without a position IPC: never publishes, so the overlay keeps
/// its current manual placement.
pub struct NoGeometrySource;

impl GameGeometrySource for NoGeometrySource {
    fn name(&self) -> &'static str {
        "none"
    }

    fn start(
        self: Box<Self>,
        _title: &str,
        _sink: SharedGameGeometry,
        _on_change: GeometryChangedCallback,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// Picks the geometry source for the running session. Compositor adapters (TASKS 12.2)
/// register here by environment detection.
pub fn select_geometry_source() -> Box<dyn GameGeometrySource> {
    Box::new(NoGeometrySource)
}

/// Starts the selected source and returns the shared slot it writes to.
pub fn spawn_game_geometry_source(
    title: &str,
    on_change: GeometryChangedCallback,
) -> (SharedGameGeometry, Result<&'static str, String>) {
    let sink: SharedGameGeometry = Arc::new(Mutex::new(None));
    let source = select_geometry_source();
    let name = source.name();
    let started = source.start(title, sink.clone(), on_change).map(|()| name);
    (sink, started)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct PublishingSource;

    impl GameGeometrySource for PublishingSource {
        fn name(&self) -> &'static str {
            "test"
        }

        fn start(
            self: Box<Self>,
            _title: &str,
            sink: SharedGameGeometry,
            on_change: GeometryChangedCallback,
        ) -> Result<(), String> {
            *sink.lock().unwrap() = Some(GameGeometry {
                output: "DP-1".to_string(),
                rect: WindowRect {
                    left: 10,
                    top: 20,
                    width: 1920,
                    height: 1080,
                },
                generation: 1,
            });
            on_change();
            Ok(())
        }
    }

    #[test]
    fn no_source_never_publishes() {
        let (sink, started) = spawn_game_geometry_source("DJMAX RESPECT V", Arc::new(|| {}));
        assert_eq!(started, Ok("none"));
        assert!(sink.lock().unwrap().is_none());
    }

    #[test]
    fn geometry_source_notifies_after_publishing() {
        let sink: SharedGameGeometry = Arc::new(Mutex::new(None));
        let callback_sink = sink.clone();
        let notifications = Arc::new(AtomicUsize::new(0));
        let callback_notifications = notifications.clone();
        let on_change: GeometryChangedCallback = Arc::new(move || {
            assert!(callback_sink.try_lock().unwrap().is_some());
            callback_notifications.fetch_add(1, Ordering::Relaxed);
        });

        Box::new(PublishingSource)
            .start("DJMAX RESPECT V", sink.clone(), on_change)
            .unwrap();

        assert_eq!(notifications.load(Ordering::Relaxed), 1);
        assert_eq!(
            sink.lock()
                .unwrap()
                .as_ref()
                .map(|geometry| geometry.generation),
            Some(1)
        );
    }
}
