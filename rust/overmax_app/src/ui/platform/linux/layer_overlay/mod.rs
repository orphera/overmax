//! Native Wayland layer-shell overlay used by the Linux port.

use crate::ui::components::ToastMessage;
use crate::ui::overlay_recommend_ui::PatternTabInfo;
use crate::ui::overlay_ui::{self, OverlayActions, OverlayProps};
use crate::ui::ui_command::UiCommand;
use overmax_capture::{
    FocusState, GameGeometry, PresentationObservation, SharedPresentationObservation,
};
use overmax_core::{GameSessionState, RecordValue};
use overmax_data::{RecommendResult, RecordManager};
use overmax_frame::{WindowRect, WindowSnapshot};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat,
    output::{OutputHandler, OutputInfo, OutputState},
    reexports::protocols::wp::{
        fractional_scale::v1::client::{
            wp_fractional_scale_manager_v1::{self, WpFractionalScaleManagerV1},
            wp_fractional_scale_v1::{self, WpFractionalScaleV1},
        },
        viewporter::client::{
            wp_viewport::{self, WpViewport},
            wp_viewporter::{self, WpViewporter},
        },
    },
    reexports::protocols_wlr::foreign_toplevel::v1::client::{
        zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
        zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
};
use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wayland_client::{
    backend::WaylandError,
    globals::registry_queue_init,
    protocol::{wl_output, wl_pointer, wl_seat, wl_surface},
    Connection, Dispatch, Proxy, QueueHandle,
};

const DEFAULT_MARGIN: i32 = 40;
const SNAP_MARGIN: i32 = 16;
const DEGRADED_WIDTH: f32 = 320.0;
const DEGRADED_HEIGHT: f32 = 116.0;
const CONTROLS_HEIGHT: f32 = 26.0;

#[derive(Clone)]
pub struct LinuxOverlaySnapshot {
    pub state: GameSessionState,
    pub song_label: String,
    pub pattern_tabs: Vec<PatternTabInfo>,
    pub recommendations: RecommendResult,
    pub settings_open: Arc<AtomicBool>,
    pub sync_open: Arc<AtomicBool>,
    pub scale: f32,
    pub opacity: f32,
    pub varchive_upload_needed: bool,
    pub varchive_upload_progress: Option<f32>,
    pub varchive_account_configured: bool,
    pub lite_mode: bool,
    pub always_visible: bool,
    pub snap: String,
    pub position: Option<(i32, i32)>,
    pub record_manager: Arc<RecordManager>,
    pub session_initial_record: Option<RecordValue>,
    pub toast: Option<ToastMessage>,
    pub window_snapshot: Option<WindowSnapshot>,
    /// Confirmed Portal target presentation; contains no fabricated X11 geometry.
    pub portal_presentation: Option<PresentationObservation>,
    /// 컴포지터 좌표의 게임 창 위치 (`GameGeometrySource`). 창 모드 스냅 기준으로만 쓴다.
    pub game_geometry: Option<GameGeometry>,
    pub capture_fatal: Option<String>,
    /// IPC `set_overlay_visibility` 로 강제된 표시 상태. `None` 이면 강제 없음.
    /// Windows 렌더 경로(`native_app_viewports.rs` 의 `overlay_on`)와 동일한 계약이다.
    pub overlay_visible_override: Option<bool>,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    pub delivery_telemetry: Option<overmax_detect::DetectionDeliveryTelemetry>,
}

#[derive(Clone)]
pub struct LinuxLayerOverlayHandle {
    published: Arc<Mutex<PublishedSnapshots>>,
    wake_writer: Arc<UnixStream>,
    runtime_failure: Arc<Mutex<Option<String>>>,
    presentation_observation: SharedPresentationObservation,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    runtime_telemetry: Option<Arc<overmax_detect::RuntimeTelemetry>>,
}

#[derive(Default)]
struct PublishedSnapshots {
    latest: Option<Arc<LinuxOverlaySnapshot>>,
    last: Option<LastPublishedSnapshot>,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    last_delivery_generation: u64,
}

struct LastPublishedSnapshot {
    snapshot: Arc<LinuxOverlaySnapshot>,
    settings_open: bool,
    sync_open: bool,
}

impl LinuxLayerOverlayHandle {
    #[allow(unused_mut)]
    pub fn publish(&self, mut snapshot: LinuxOverlaySnapshot) {
        let settings_open = snapshot.settings_open.load(Ordering::Relaxed);
        let sync_open = snapshot.sync_open.load(Ordering::Relaxed);
        let Ok(mut published) = self.published.lock() else {
            return;
        };
        let display_unchanged = published.last.as_ref().is_some_and(|last| {
            same_display_snapshot(
                &last.snapshot,
                last.settings_open,
                last.sync_open,
                &snapshot,
                settings_open,
                sync_open,
            )
        });
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        let new_delivery = snapshot
            .delivery_telemetry
            .is_some_and(|delivery| delivery.generation != published.last_delivery_generation);
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if new_delivery {
            published.last_delivery_generation = snapshot
                .delivery_telemetry
                .map_or(0, |delivery| delivery.generation);
        }
        if display_unchanged {
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            if new_delivery {
                if let (Some(telemetry), Some(delivery)) =
                    (&self.runtime_telemetry, &mut snapshot.delivery_telemetry)
                {
                    telemetry.record_publish(delivery, false);
                }
            }
            return;
        }
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if !new_delivery {
            snapshot.delivery_telemetry = None;
        }
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if new_delivery {
            if let (Some(telemetry), Some(delivery)) =
                (&self.runtime_telemetry, &mut snapshot.delivery_telemetry)
            {
                telemetry.record_publish(delivery, true);
            }
        }
        let snapshot = Arc::new(snapshot);
        published.latest = Some(snapshot.clone());
        published.last = Some(LastPublishedSnapshot {
            snapshot,
            settings_open,
            sync_open,
        });
        drop(published);

        // A full socket buffer means a wake-up is already pending, so a failed
        // non-blocking write needs no handling.
        let _ = (&*self.wake_writer).write(&[1]);
    }

    pub fn take_runtime_failure(&self) -> Option<String> {
        self.runtime_failure
            .lock()
            .ok()
            .and_then(|mut failure| failure.take())
    }

    pub fn presentation_observation(&self) -> SharedPresentationObservation {
        self.presentation_observation.clone()
    }
}

fn same_display_snapshot(
    previous: &LinuxOverlaySnapshot,
    previous_settings_open: bool,
    previous_sync_open: bool,
    next: &LinuxOverlaySnapshot,
    next_settings_open: bool,
    next_sync_open: bool,
) -> bool {
    previous.state == next.state
        && previous.song_label == next.song_label
        && previous.pattern_tabs == next.pattern_tabs
        && previous.recommendations == next.recommendations
        && previous_settings_open == next_settings_open
        && previous_sync_open == next_sync_open
        && previous.scale == next.scale
        && previous.opacity == next.opacity
        && previous.varchive_upload_needed == next.varchive_upload_needed
        && previous.varchive_upload_progress == next.varchive_upload_progress
        && previous.varchive_account_configured == next.varchive_account_configured
        && previous.lite_mode == next.lite_mode
        && previous.always_visible == next.always_visible
        && previous.snap == next.snap
        && previous.position == next.position
        && Arc::ptr_eq(&previous.record_manager, &next.record_manager)
        && previous.session_initial_record == next.session_initial_record
        && previous.toast == next.toast
        && previous.window_snapshot == next.window_snapshot
        && previous.portal_presentation == next.portal_presentation
        && previous.game_geometry == next.game_geometry
        && previous.capture_fatal == next.capture_fatal
        && previous.overlay_visible_override == next.overlay_visible_override
}

pub type AppRepaintCallback = Arc<dyn Fn() + Send + Sync>;

pub fn spawn(
    game_window_title: String,
    command_tx: Sender<UiCommand>,
    app_repaint: AppRepaintCallback,
    runtime_telemetry: Option<Arc<overmax_detect::RuntimeTelemetry>>,
) -> Result<LinuxLayerOverlayHandle, String> {
    let published = Arc::new(Mutex::new(PublishedSnapshots::default()));
    let runtime_failure = Arc::new(Mutex::new(None));
    let presentation_observation = Arc::new(Mutex::new(None));
    let (wake_reader, wake_writer) = UnixStream::pair().map_err(|error| error.to_string())?;
    wake_reader
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    wake_writer
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let thread_published = published.clone();
    let thread_failure = runtime_failure.clone();
    let thread_telemetry = runtime_telemetry.clone();
    let thread_presentation = presentation_observation.clone();

    std::thread::Builder::new()
        .name("overmax-linux-overlay".to_string())
        .spawn(move || {
            let result = run(
                game_window_title,
                command_tx,
                app_repaint.clone(),
                thread_published,
                wake_reader,
                ready_tx,
                thread_telemetry,
                thread_presentation.clone(),
            );
            if let Ok(mut observation) = thread_presentation.lock() {
                *observation = None;
            }
            if let Err(error) = result {
                eprintln!("[LinuxOverlay] {error}");
                if let Ok(mut failure) = thread_failure.lock() {
                    *failure = Some(error);
                }
                app_repaint();
            }
        })
        .map_err(|error| error.to_string())?;

    ready_rx
        .recv()
        .map_err(|_| "Linux overlay thread exited during startup".to_string())??;
    Ok(LinuxLayerOverlayHandle {
        published,
        wake_writer: Arc::new(wake_writer),
        runtime_failure,
        presentation_observation,
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        runtime_telemetry,
    })
}

#[allow(clippy::too_many_arguments)]
fn run(
    game_window_title: String,
    command_tx: Sender<UiCommand>,
    app_repaint: AppRepaintCallback,
    published: Arc<Mutex<PublishedSnapshots>>,
    wake_reader: UnixStream,
    ready_tx: SyncSender<Result<(), String>>,
    runtime_telemetry: Option<Arc<overmax_detect::RuntimeTelemetry>>,
    presentation_observation: SharedPresentationObservation,
) -> Result<(), String> {
    let initialized = Backend::new(
        game_window_title,
        command_tx,
        app_repaint,
        published,
        runtime_telemetry,
        presentation_observation,
    );
    let (mut event_queue, mut backend) = match initialized {
        Ok(value) => {
            let _ = ready_tx.send(Ok(()));
            value
        }
        Err(error) => {
            let _ = ready_tx.send(Err(error.clone()));
            return Err(error);
        }
    };

    loop {
        event_queue
            .dispatch_pending(&mut backend)
            .map_err(|error| error.to_string())?;
        backend.after_dispatch(&event_queue.handle())?;
        let flush_pending = match event_queue.flush() {
            Ok(()) => false,
            Err(WaylandError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => true,
            Err(error) => return Err(error.to_string()),
        };

        let Some(read_guard) = event_queue.prepare_read() else {
            continue;
        };
        let (wayland_ready, wake_ready) = {
            let wayland_fd = read_guard.connection_fd();
            let mut wayland_interest = PollFlags::IN | PollFlags::ERR;
            if flush_pending {
                wayland_interest |= PollFlags::OUT;
            }
            let mut fds = [
                PollFd::from_borrowed_fd(wayland_fd, wayland_interest),
                PollFd::new(&wake_reader, PollFlags::IN | PollFlags::ERR),
            ];
            loop {
                let timeout = repaint_timeout(backend.next_repaint, Instant::now());
                match poll(&mut fds, timeout.as_ref()) {
                    Ok(_) => break,
                    Err(rustix::io::Errno::INTR) => continue,
                    Err(error) => return Err(error.to_string()),
                }
            }
            (
                fds[0]
                    .revents()
                    .intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
                fds[1]
                    .revents()
                    .intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
            )
        };

        if wayland_ready {
            read_guard.read().map_err(|error| error.to_string())?;
        } else {
            drop(read_guard);
        }
        if wake_ready {
            let connected = drain_wake_socket(&wake_reader);
            backend.consume_published(&event_queue.handle());
            if !connected {
                return Ok(());
            }
        }
    }
}

/// Returns `false` once every `LinuxLayerOverlayHandle` clone has been dropped
/// (EOF on the wake socket), which shuts the overlay thread down.
fn drain_wake_socket(stream: &UnixStream) -> bool {
    let mut stream = stream;
    let mut buffer = [0u8; 64];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return false,
            Ok(_) => {}
            Err(error) => return error.kind() == std::io::ErrorKind::WouldBlock,
        }
    }
}

mod backend;
mod foreign_toplevel;
mod output;
mod panel;
#[cfg(test)]
mod tests;

use backend::*;
use foreign_toplevel::*;
use output::*;
use panel::*;
