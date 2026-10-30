//! Compositor-coordinate game window geometry sources.
//!
//! Wayland gives clients no global window coordinates, so per-compositor IPC adapters
//! publish the game window rect here. Contract: docs/plans/2026-10-28-linux-window-geometry-sources.md

use super::WindowRect;
use std::sync::{Arc, Mutex};

mod hyprland;
mod sway;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedGeometry {
    output: String,
    rect: WindowRect,
}

fn publish_geometry(
    sink: &SharedGameGeometry,
    candidate: Option<ResolvedGeometry>,
    generation: &mut u64,
    on_change: &GeometryChangedCallback,
) {
    let changed = {
        let Ok(mut current) = sink.lock() else {
            eprintln!("[GeometrySource] shared geometry slot is poisoned");
            return;
        };
        let same_geometry = match (current.as_ref(), candidate.as_ref()) {
            (Some(current), Some(candidate)) => {
                current.output == candidate.output && current.rect == candidate.rect
            }
            (None, None) => true,
            _ => false,
        };
        if same_geometry {
            false
        } else {
            *current = candidate.map(|candidate| {
                *generation = generation.wrapping_add(1).max(1);
                GameGeometry {
                    output: candidate.output,
                    rect: candidate.rect,
                    generation: *generation,
                }
            });
            true
        }
    };
    if changed {
        on_change();
    }
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceSelection {
    None,
    Hyprland,
    Sway,
    Ambiguous,
}

fn source_selection(has_hyprland: bool, has_sway: bool) -> SourceSelection {
    match (has_hyprland, has_sway) {
        (true, false) => SourceSelection::Hyprland,
        (false, true) => SourceSelection::Sway,
        (false, false) => SourceSelection::None,
        (true, true) => SourceSelection::Ambiguous,
    }
}

/// Picks a source from compositor-provided session environment variables.
pub fn select_geometry_source() -> Box<dyn GameGeometrySource> {
    let hyprland_signature =
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").filter(|value| !value.is_empty());
    let sway_socket = std::env::var_os("SWAYSOCK").filter(|value| !value.is_empty());
    match source_selection(hyprland_signature.is_some(), sway_socket.is_some()) {
        SourceSelection::Hyprland => {
            let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") else {
                eprintln!("[GeometrySource] XDG_RUNTIME_DIR is required for Hyprland IPC");
                return Box::new(NoGeometrySource);
            };
            let Some(signature) = hyprland_signature else {
                eprintln!("[GeometrySource] HYPRLAND_INSTANCE_SIGNATURE is unavailable");
                return Box::new(NoGeometrySource);
            };
            let Some(signature) = signature.to_str() else {
                eprintln!("[GeometrySource] HYPRLAND_INSTANCE_SIGNATURE is not valid UTF-8");
                return Box::new(NoGeometrySource);
            };
            Box::new(hyprland::HyprlandGeometrySource::new(
                runtime_dir.into(),
                signature.to_string(),
            ))
        }
        SourceSelection::Sway => {
            let Some(socket) = sway_socket else {
                eprintln!("[GeometrySource] SWAYSOCK is unavailable");
                return Box::new(NoGeometrySource);
            };
            Box::new(sway::SwayGeometrySource::new(socket.into()))
        }
        SourceSelection::Ambiguous => {
            eprintln!(
                "[GeometrySource] both Hyprland and Sway IPC variables are set; keeping manual placement"
            );
            Box::new(NoGeometrySource)
        }
        SourceSelection::None => Box::new(NoGeometrySource),
    }
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
        let sink: SharedGameGeometry = Arc::new(Mutex::new(None));
        let started =
            Box::new(NoGeometrySource).start("DJMAX RESPECT V", sink.clone(), Arc::new(|| {}));
        assert_eq!(started, Ok(()));
        assert!(sink.lock().unwrap().is_none());
    }

    #[test]
    fn geometry_source_notifies_after_publishing() {
        let sink: SharedGameGeometry = Arc::new(Mutex::new(None));
        let callback_sink = sink.clone();
        let notifications = Arc::new(AtomicUsize::new(0));
        let callback_notifications = notifications.clone();
        let on_change: GeometryChangedCallback = Arc::new(move || {
            assert!(callback_sink.try_lock().is_ok());
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

    #[test]
    fn compositor_source_is_selected_only_when_unambiguous() {
        assert_eq!(source_selection(false, false), SourceSelection::None);
        assert_eq!(source_selection(true, false), SourceSelection::Hyprland);
        assert_eq!(source_selection(false, true), SourceSelection::Sway);
        assert_eq!(source_selection(true, true), SourceSelection::Ambiguous);
    }

    #[test]
    fn geometry_updates_notify_only_after_changed_values_are_published() {
        let sink: SharedGameGeometry = Arc::new(Mutex::new(None));
        let callback_sink = sink.clone();
        let notifications = Arc::new(AtomicUsize::new(0));
        let callback_notifications = notifications.clone();
        let on_change: GeometryChangedCallback = Arc::new(move || {
            assert!(callback_sink.try_lock().is_ok());
            callback_notifications.fetch_add(1, Ordering::Relaxed);
        });
        let geometry = ResolvedGeometry {
            output: "DP-1".to_string(),
            rect: WindowRect {
                left: 10,
                top: 20,
                width: 1280,
                height: 720,
            },
        };
        let mut generation = 0;

        publish_geometry(&sink, Some(geometry.clone()), &mut generation, &on_change);
        publish_geometry(&sink, Some(geometry.clone()), &mut generation, &on_change);
        assert_eq!(notifications.load(Ordering::Relaxed), 1);
        assert_eq!(sink.lock().unwrap().as_ref().unwrap().generation, 1);

        let changed = ResolvedGeometry {
            rect: WindowRect {
                left: 30,
                ..geometry.rect
            },
            ..geometry
        };
        publish_geometry(&sink, Some(changed), &mut generation, &on_change);
        assert_eq!(notifications.load(Ordering::Relaxed), 2);
        assert_eq!(sink.lock().unwrap().as_ref().unwrap().generation, 2);

        publish_geometry(&sink, None, &mut generation, &on_change);
        assert_eq!(notifications.load(Ordering::Relaxed), 3);
        assert!(sink.lock().unwrap().is_none());
    }
}
