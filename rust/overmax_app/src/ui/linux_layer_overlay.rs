//! Native Wayland layer-shell overlay used by the Linux port.

use crate::ui::components::ToastMessage;
use crate::ui::overlay_recommend_ui::PatternTabInfo;
use crate::ui::overlay_ui::{self, OverlayActions, OverlayProps};
use crate::ui::ui_command::UiCommand;
use overmax_core::{GameSessionState, RecordValue};
use overmax_data::{RecommendResult, RecordManager};
use overmax_engine::capture::window_tracker::{
    FocusState, PresentationObservation, SharedPresentationObservation, WindowRect, WindowSnapshot,
};
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
    pub varchive_account_configured: bool,
    pub lite_mode: bool,
    pub always_visible: bool,
    pub snap: String,
    pub position: Option<(i32, i32)>,
    pub record_manager: Arc<RecordManager>,
    pub session_initial_record: Option<RecordValue>,
    pub toast: Option<ToastMessage>,
    pub window_snapshot: Option<WindowSnapshot>,
    pub capture_fatal: Option<String>,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    pub delivery_telemetry: Option<overmax_engine::detector::telemetry::DetectionDeliveryTelemetry>,
}

#[derive(Clone)]
pub struct LinuxLayerOverlayHandle {
    published: Arc<Mutex<PublishedSnapshots>>,
    wake_writer: Arc<UnixStream>,
    runtime_failure: Arc<Mutex<Option<String>>>,
    presentation_observation: SharedPresentationObservation,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    runtime_telemetry: Option<Arc<overmax_engine::detector::telemetry::RuntimeTelemetry>>,
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
        && previous.varchive_account_configured == next.varchive_account_configured
        && previous.lite_mode == next.lite_mode
        && previous.always_visible == next.always_visible
        && previous.snap == next.snap
        && previous.position == next.position
        && Arc::ptr_eq(&previous.record_manager, &next.record_manager)
        && previous.session_initial_record == next.session_initial_record
        && previous.toast == next.toast
        && previous.window_snapshot == next.window_snapshot
        && previous.capture_fatal == next.capture_fatal
}

pub type AppRepaintCallback = Arc<dyn Fn() + Send + Sync>;

pub fn spawn(
    game_window_title: String,
    command_tx: Sender<UiCommand>,
    app_repaint: AppRepaintCallback,
    runtime_telemetry: Option<Arc<overmax_engine::detector::telemetry::RuntimeTelemetry>>,
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
                thread_presentation,
            );
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
    runtime_telemetry: Option<Arc<overmax_engine::detector::telemetry::RuntimeTelemetry>>,
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

#[derive(Clone, Default)]
struct ForeignToplevelSnapshot {
    title: Option<String>,
    activated: bool,
    fullscreen: Option<bool>,
    outputs: Vec<wl_output::WlOutput>,
}

#[derive(Default)]
struct ForeignToplevelState {
    pending: ForeignToplevelSnapshot,
    committed: Option<ForeignToplevelSnapshot>,
}

struct ForeignToplevel {
    handle: ZwlrForeignToplevelHandleV1,
    state: ForeignToplevelState,
}

fn parse_foreign_toplevel_states(raw: &[u8], supports_fullscreen: bool) -> (bool, Option<bool>) {
    let mut activated = false;
    let mut fullscreen = false;
    for bytes in raw.chunks_exact(4) {
        match u32::from_ne_bytes(bytes.try_into().expect("four-byte chunk")) {
            2 => activated = true,
            3 if supports_fullscreen => fullscreen = true,
            _ => {}
        }
    }
    (activated, supports_fullscreen.then_some(fullscreen))
}

fn unique_matching_toplevel<'a>(
    title: &str,
    toplevels: impl Iterator<Item = &'a ForeignToplevelState>,
) -> Option<&'a ForeignToplevelSnapshot> {
    let mut matches = toplevels.filter_map(|state| {
        state
            .committed
            .as_ref()
            .filter(|snapshot| snapshot.title.as_deref() == Some(title))
    });
    let target = matches.next()?;
    matches.next().is_none().then_some(target)
}

fn select_committed_output<'a, T: PartialEq>(
    outputs: &'a [T],
    current: Option<&T>,
) -> Option<&'a T> {
    match outputs {
        [output] => Some(output),
        outputs => current.and_then(|current| outputs.iter().find(|output| *output == current)),
    }
}

fn output_render_scale(
    current: f64,
    estimated: Option<f64>,
    has_viewporter: bool,
    preferred_received: bool,
) -> f64 {
    if preferred_received {
        current
    } else if has_viewporter {
        estimated.unwrap_or(1.0)
    } else {
        estimated.unwrap_or(1.0).round().max(1.0)
    }
}

struct Backend {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    viewporter: Option<WpViewporter>,
    fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    foreign_toplevel_manager: Option<ZwlrForeignToplevelManagerV1>,
    foreign_toplevels: Vec<ForeignToplevel>,
    game_window_title: String,
    presentation_observation: SharedPresentationObservation,
    presentation_generation: u64,
    connection: Connection,
    layer: Option<LayerSurface>,
    viewport: Option<WpViewport>,
    fractional_scale: Option<WpFractionalScaleV1>,
    target_output: Option<wl_output::WlOutput>,
    surface_output: Option<wl_output::WlOutput>,
    output_logical_size: Option<(i32, i32)>,
    preferred_scale_received: bool,
    empty_input_region: Region,
    input_passthrough: bool,
    pointer: Option<wl_pointer::WlPointer>,
    pointer_position: Option<egui::Pos2>,
    recreate_on_output: bool,

    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
    format: wgpu::TextureFormat,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    configured: bool,
    requested_size: (u32, u32),
    logical_size: (u32, u32),
    render_scale: f64,

    egui_ctx: egui::Context,
    events: Vec<egui::Event>,
    start: Instant,
    needs_redraw: bool,
    next_repaint: Option<Instant>,
    snapshot: Option<Arc<LinuxOverlaySnapshot>>,
    published: Arc<Mutex<PublishedSnapshots>>,
    command_tx: Sender<UiCommand>,
    app_repaint: AppRepaintCallback,
    margin: (i32, i32),
    dragging: bool,
    drag_origin_margin: (i32, i32),
    drag_total_delta: egui::Vec2,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    runtime_telemetry: Option<Arc<overmax_engine::detector::telemetry::RuntimeTelemetry>>,
}

impl Backend {
    fn new(
        game_window_title: String,
        command_tx: Sender<UiCommand>,
        app_repaint: AppRepaintCallback,
        published: Arc<Mutex<PublishedSnapshots>>,
        runtime_telemetry: Option<Arc<overmax_engine::detector::telemetry::RuntimeTelemetry>>,
        presentation_observation: SharedPresentationObservation,
    ) -> Result<(wayland_client::EventQueue<Self>, Self), String> {
        #[cfg(not(any(debug_assertions, feature = "telemetry")))]
        let _ = &runtime_telemetry;
        let connection = Connection::connect_to_env().map_err(|error| error.to_string())?;
        let (globals, event_queue) =
            registry_queue_init(&connection).map_err(|error| error.to_string())?;
        let qh = event_queue.handle();
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if let Some(telemetry) = &runtime_telemetry {
            let foreign_toplevel_version = globals.contents().with_list(|list| {
                list.iter()
                    .filter(|global| global.interface == "zwlr_foreign_toplevel_manager_v1")
                    .map(|global| global.version)
                    .max()
            });
            telemetry.record_wayland_capabilities(foreign_toplevel_version);
        }
        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|_| "wl_compositor is unavailable".to_string())?;
        let empty_input_region = Region::new(&compositor).map_err(|error| error.to_string())?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|_| "zwlr_layer_shell_v1 is unavailable".to_string())?;
        let viewporter = globals.bind::<WpViewporter, _, _>(&qh, 1..=1, ()).ok();
        let fractional_scale_manager = viewporter.as_ref().and_then(|_| {
            globals
                .bind::<WpFractionalScaleManagerV1, _, _>(&qh, 1..=1, ())
                .ok()
        });
        let foreign_toplevel_manager = globals
            .bind::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ())
            .ok();
        let requested_size = panel_size(None);
        let margin = (DEFAULT_MARGIN, DEFAULT_MARGIN);
        let layer = create_layer(
            &compositor,
            &layer_shell,
            &qh,
            requested_size,
            margin,
            1,
            None,
        );
        layer.commit();
        let viewport = viewporter
            .as_ref()
            .map(|manager| create_viewport(manager, &layer, &qh, requested_size));
        let fractional_scale = fractional_scale_manager
            .as_ref()
            .map(|manager| manager.get_fractional_scale(layer.wl_surface(), &qh, ()));

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
            memory_budget_thresholds: Default::default(),
        });
        let surface = create_wgpu_surface(&connection, &instance, &layer)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|error| error.to_string())?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|error| error.to_string())?;
        let (format, surface_config) = surface_settings(&surface, &adapter, requested_size, 1.0)?;
        let renderer =
            egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
        let egui_ctx = egui::Context::default();
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::TRANSPARENT;
        visuals.window_fill = egui::Color32::TRANSPARENT;
        visuals.window_stroke = egui::Stroke::NONE;
        visuals.window_shadow = egui::Shadow::NONE;
        egui_ctx.set_visuals(visuals);
        if !overlay_ui::install_cjk_fonts(&egui_ctx) {
            return Err("No Korean font was found through fontconfig".to_string());
        }

        Ok((
            event_queue,
            Self {
                registry_state: RegistryState::new(&globals),
                seat_state: SeatState::new(&globals, &qh),
                output_state: OutputState::new(&globals, &qh),
                compositor,
                layer_shell,
                viewporter,
                fractional_scale_manager,
                foreign_toplevel_manager,
                foreign_toplevels: Vec::new(),
                game_window_title,
                presentation_observation,
                presentation_generation: 0,
                connection,
                layer: Some(layer),
                viewport,
                fractional_scale,
                target_output: None,
                surface_output: None,
                output_logical_size: None,
                preferred_scale_received: false,
                empty_input_region,
                input_passthrough: false,
                pointer: None,
                pointer_position: None,
                recreate_on_output: false,
                instance,
                adapter,
                device,
                queue,
                renderer,
                format,
                surface: Some(surface),
                surface_config: Some(surface_config),
                configured: false,
                requested_size,
                logical_size: requested_size,
                render_scale: 1.0,
                egui_ctx,
                events: Vec::new(),
                start: Instant::now(),
                needs_redraw: false,
                next_repaint: None,
                snapshot: None,
                published,
                command_tx,
                app_repaint,
                margin,
                dragging: false,
                drag_origin_margin: margin,
                drag_total_delta: egui::Vec2::ZERO,
                #[cfg(any(debug_assertions, feature = "telemetry"))]
                runtime_telemetry,
            },
        ))
    }

    fn publish_presentation_observation(&mut self) {
        let target = unique_matching_toplevel(
            &self.game_window_title,
            self.foreign_toplevels
                .iter()
                .map(|toplevel| &toplevel.state),
        )
        .map(|target| (target.activated, target.fullscreen));
        self.presentation_generation = self.presentation_generation.wrapping_add(1);
        let observation = target.map(|(activated, fullscreen)| PresentationObservation {
            focus: if activated {
                FocusState::Focused
            } else {
                FocusState::Background
            },
            fullscreen,
            generation: self.presentation_generation,
            committed_at: Instant::now(),
        });
        if let Ok(mut shared) = self.presentation_observation.lock() {
            *shared = observation;
        }
    }

    fn remove_toplevel_output(&mut self, output: &wl_output::WlOutput) {
        for toplevel in &mut self.foreign_toplevels {
            toplevel
                .state
                .pending
                .outputs
                .retain(|candidate| candidate != output);
            if let Some(committed) = &mut toplevel.state.committed {
                committed.outputs.retain(|candidate| candidate != output);
            }
        }
    }

    fn after_dispatch(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        if self
            .next_repaint
            .is_some_and(|deadline| deadline <= Instant::now())
        {
            self.next_repaint = None;
            self.needs_redraw = true;
        }
        if self.recreate_on_output
            && self.layer.is_none()
            && self.output_state.outputs().next().is_some()
        {
            self.recreate_on_output = false;
            self.create_surface(qh)?;
        }
        if self.configured && self.needs_redraw {
            self.render(qh)?;
        }
        Ok(())
    }

    fn consume_published(&mut self, qh: &QueueHandle<Self>) {
        let snapshot = self
            .published
            .lock()
            .ok()
            .and_then(|mut published| published.latest.take());
        if let Some(snapshot) = snapshot {
            self.apply_snapshot(snapshot, qh);
        }
    }

    fn apply_snapshot(&mut self, snapshot: Arc<LinuxOverlaySnapshot>, qh: &QueueHandle<Self>) {
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if let (Some(telemetry), Some(delivery)) =
            (&self.runtime_telemetry, &snapshot.delivery_telemetry)
        {
            telemetry.record_snapshot_applied(delivery);
        }
        let hidden = is_hidden(&snapshot);
        let size = retained_panel_size(self.requested_size, &snapshot);
        let size_changed = self.requested_size != size;
        let reposition = self.snapshot.as_ref().is_none_or(|previous| {
            previous.snap != snapshot.snap
                || previous.position != snapshot.position
                || previous.window_snapshot != snapshot.window_snapshot
                || size_changed
        });
        let margin = if reposition {
            panel_margin(&snapshot, size, self.output_logical_size)
        } else {
            self.margin
        };
        self.snapshot = Some(snapshot);
        self.requested_size = size;
        self.margin = margin;
        if reposition && self.dragging {
            self.reset_pointer_state();
        }
        if self.select_target_output() {
            self.drop_surface();
            if let Err(error) = self.create_surface(qh) {
                self.recreate_on_output = true;
                eprintln!("[LinuxOverlay] output switch failed: {error}");
            }
            return;
        }
        self.set_input_passthrough(hidden);
        if size_changed {
            self.logical_size = size;
            self.configure_surface();
        }
        if let Some(layer) = &self.layer {
            layer.set_size(size.0, size.1);
            if let Some(viewport) = &self.viewport {
                viewport.set_destination(size.0 as i32, size.1 as i32);
            }
            layer.set_margin(margin.1, 0, 0, margin.0);
            layer.commit();
        }
        self.needs_redraw = true;
    }

    fn select_target_output(&mut self) -> bool {
        let target = unique_matching_toplevel(
            &self.game_window_title,
            self.foreign_toplevels
                .iter()
                .map(|toplevel| &toplevel.state),
        )
        .and_then(|target| {
            select_committed_output(&target.outputs, self.target_output.as_ref()).cloned()
        })
        .or_else(|| {
            let rect = self.snapshot.as_ref()?.window_snapshot?.rect;
            select_output_by_overlap(
                rect,
                self.output_state.outputs().filter_map(|output| {
                    let geometry = self
                        .output_state
                        .info(&output)
                        .and_then(|info| output_geometry(&info))?;
                    Some((output, geometry.rect))
                }),
            )
        });
        if self.target_output == target {
            return false;
        }
        self.target_output = target;
        true
    }

    fn refresh_output(&mut self, qh: &QueueHandle<Self>) {
        if self.select_target_output() {
            self.drop_surface();
            if let Err(error) = self.create_surface(qh) {
                self.recreate_on_output = true;
                eprintln!("[LinuxOverlay] output switch failed: {error}");
            }
            return;
        }
        if !self.update_output_geometry() {
            return;
        }
        if let Some(snapshot) = &self.snapshot {
            self.margin = panel_margin(snapshot, self.requested_size, self.output_logical_size);
        }
        if let Some(layer) = &self.layer {
            layer.wl_surface().set_buffer_scale(fallback_buffer_scale(
                self.render_scale,
                self.viewport.is_some(),
            ));
            layer.set_margin(self.margin.1, 0, 0, self.margin.0);
            layer.commit();
        }
        self.needs_redraw = true;
    }

    fn update_output_geometry(&mut self) -> bool {
        let output = self.target_output.as_ref().or(self.surface_output.as_ref());
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if let Some(output) = output {
            self.record_output_environment(output);
        }
        let geometry = output
            .and_then(|output| self.output_state.info(output))
            .and_then(|info| output_geometry(&info));
        let logical_size = geometry.map(|geometry| (geometry.rect.width, geometry.rect.height));
        let mut changed = self.output_logical_size != logical_size;
        self.output_logical_size = logical_size;
        let scale = output_render_scale(
            self.render_scale,
            geometry.map(|geometry| geometry.scale),
            self.viewporter.is_some(),
            self.preferred_scale_received,
        );
        if (self.render_scale - scale).abs() > f64::EPSILON {
            self.render_scale = scale;
            self.configure_surface();
            changed = true;
        }
        changed
    }

    #[cfg(any(debug_assertions, feature = "telemetry"))]
    fn record_output_environment(&self, output: &wl_output::WlOutput) {
        let (Some(telemetry), Some(info)) =
            (&self.runtime_telemetry, self.output_state.info(output))
        else {
            return;
        };
        let Some(geometry) = output_geometry(&info) else {
            return;
        };
        let physical_size = info
            .modes
            .iter()
            .find(|mode| mode.current)
            .map(|mode| mode.dimensions);
        telemetry.record_output_environment(
            info.name.as_deref(),
            (geometry.rect.width, geometry.rect.height),
            physical_size,
            geometry.scale,
        );
    }

    fn create_surface(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        self.update_output_geometry();
        if let Some(snapshot) = &self.snapshot {
            self.margin = panel_margin(snapshot, self.requested_size, self.output_logical_size);
        }
        let input_passthrough = self.snapshot.as_deref().is_some_and(is_hidden);
        let layer = create_layer(
            &self.compositor,
            &self.layer_shell,
            qh,
            self.requested_size,
            self.margin,
            fallback_buffer_scale(self.render_scale, self.viewporter.is_some()),
            self.target_output.as_ref(),
        );
        layer
            .wl_surface()
            .set_input_region(input_passthrough.then_some(self.empty_input_region.wl_region()));
        layer.commit();
        self.viewport = self
            .viewporter
            .as_ref()
            .map(|manager| create_viewport(manager, &layer, qh, self.requested_size));
        self.fractional_scale = self
            .fractional_scale_manager
            .as_ref()
            .map(|manager| manager.get_fractional_scale(layer.wl_surface(), qh, ()));
        let surface = create_wgpu_surface(&self.connection, &self.instance, &layer)?;
        let capabilities = surface.get_capabilities(&self.adapter);
        if !capabilities.formats.contains(&self.format) {
            return Err("The recreated output does not support the selected texture format".into());
        }
        let (_, mut config) = surface_settings(
            &surface,
            &self.adapter,
            self.requested_size,
            self.render_scale,
        )?;
        config.format = self.format;
        self.layer = Some(layer);
        self.surface = Some(surface);
        self.surface_config = Some(config);
        self.logical_size = self.requested_size;
        self.configured = false;
        self.needs_redraw = true;
        self.recreate_on_output = false;
        self.input_passthrough = input_passthrough;
        Ok(())
    }

    fn drop_surface(&mut self) {
        self.reset_pointer_state();
        if let Some(viewport) = self.viewport.take() {
            viewport.destroy();
        }
        if let Some(fractional_scale) = self.fractional_scale.take() {
            fractional_scale.destroy();
        }
        self.surface = None;
        self.surface_config = None;
        self.layer = None;
        self.surface_output = None;
        self.output_logical_size = None;
        self.preferred_scale_received = false;
        self.input_passthrough = false;
        self.configured = false;
        self.needs_redraw = false;
        self.next_repaint = None;
    }

    fn reset_pointer_state(&mut self) {
        self.dragging = false;
        self.drag_total_delta = egui::Vec2::ZERO;
        if let Some(pos) = self.pointer_position.take() {
            for button in [
                egui::PointerButton::Primary,
                egui::PointerButton::Secondary,
                egui::PointerButton::Middle,
            ] {
                self.events.push(egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                });
            }
        }
        self.events.push(egui::Event::PointerGone);
    }

    fn set_input_passthrough(&mut self, passthrough: bool) {
        if self.input_passthrough == passthrough {
            return;
        }
        if passthrough {
            self.reset_pointer_state();
        }
        if let Some(layer) = &self.layer {
            layer
                .wl_surface()
                .set_input_region(passthrough.then_some(self.empty_input_region.wl_region()));
            layer.commit();
        }
        self.input_passthrough = passthrough;
    }

    fn configure_surface(&mut self) {
        let Some(surface) = &self.surface else {
            return;
        };
        let Some(config) = &mut self.surface_config else {
            return;
        };
        (config.width, config.height) = physical_size(self.logical_size, self.render_scale);
        surface.configure(&self.device, config);
    }

    fn render(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        self.needs_redraw = false;
        let frame = match self.acquire_frame()? {
            Some(frame) => frame,
            None => {
                if self.needs_redraw {
                    self.request_frame(qh);
                }
                return Ok(());
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(self.logical_size.0 as f32, self.logical_size.1 as f32),
            )),
            time: Some(self.start.elapsed().as_secs_f64()),
            events: std::mem::take(&mut self.events),
            ..Default::default()
        };
        if let Some(viewport) = raw_input.viewports.get_mut(&egui::ViewportId::ROOT) {
            viewport.native_pixels_per_point = Some(self.render_scale as f32);
        }

        let mut actions = OverlayActions::default();
        let mut control_command = None;
        let ctx = self.egui_ctx.clone();
        let mut full_output = ctx.run_ui(raw_input, |ctx| {
            if let Some(snapshot) = &self.snapshot {
                if !is_hidden(snapshot) && !is_degraded(snapshot) {
                    egui::Panel::bottom("linux_overlay_controls")
                        .exact_size(CONTROLS_HEIGHT * snapshot.scale)
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            ui.set_opacity(snapshot.opacity.clamp(0.0, 1.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Exit").clicked() {
                                        control_command = Some(UiCommand::Exit);
                                    }
                                    if ui.small_button("Debug").clicked() {
                                        control_command = Some(UiCommand::OpenDebug);
                                    }
                                },
                            );
                        });
                }
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    if let Some(snapshot) = &self.snapshot {
                        if !is_hidden(snapshot) {
                            ui.set_opacity(snapshot.opacity.clamp(0.0, 1.0));
                            actions = if is_degraded(snapshot) {
                                draw_degraded(ui, snapshot)
                            } else {
                                overlay_ui::draw_overlay_panel(ui, &overlay_props(snapshot))
                            };
                        }
                    }
                });
        });
        if control_command.is_some() {
            actions.command = control_command;
        }
        self.apply_actions(actions);

        let clipped = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let physical = physical_size(self.logical_size, self.render_scale);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [physical.0, physical.1],
            pixels_per_point: full_output.pixels_per_point,
        };
        for (id, deltas) in &full_output.textures_delta.set {
            for delta in deltas {
                self.renderer
                    .update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let user_buffers = self.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &clipped,
            &screen,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("overmax-linux-overlay"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &clipped, &screen);
        }
        for id in &full_output.textures_delta.free {
            self.renderer.free_texture(id);
        }
        full_output.textures_delta.clear();

        let repaint_delay = full_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |output| output.repaint_delay);
        if repaint_delay.is_zero() {
            self.next_repaint = None;
            if let Some(layer) = &self.layer {
                layer.wl_surface().frame(qh, layer.wl_surface().clone());
            }
        } else {
            self.next_repaint = Instant::now().checked_add(repaint_delay);
        }
        self.queue.submit(
            user_buffers
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        self.queue.present(frame);
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if let (Some(telemetry), Some(delivery)) = (
            &self.runtime_telemetry,
            self.snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.delivery_telemetry.as_ref()),
        ) {
            telemetry.record_snapshot_presented(delivery);
        }
        Ok(())
    }

    fn request_frame(&self, qh: &QueueHandle<Self>) {
        if let Some(layer) = &self.layer {
            layer.wl_surface().frame(qh, layer.wl_surface().clone());
            layer.commit();
        }
    }

    fn acquire_frame(&mut self) -> Result<Option<wgpu::SurfaceTexture>, String> {
        for _ in 0..2 {
            let Some(surface) = &self.surface else {
                return Ok(None);
            };
            match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame)
                | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => return Ok(Some(frame)),
                wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                    self.configure_surface();
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    self.needs_redraw = true;
                    return Ok(None);
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err("Validation error getting current texture".to_string());
                }
            }
        }
        self.needs_redraw = true;
        Ok(None)
    }

    fn apply_actions(&mut self, actions: OverlayActions) {
        if let Some(command) = actions.command {
            self.send_command(command);
        }
        if actions.start_drag {
            self.dragging = true;
            self.drag_origin_margin = self.margin;
            self.drag_total_delta = egui::Vec2::ZERO;
        }
        if let Some(delta) = actions.drag_delta {
            if !self.dragging {
                self.dragging = true;
                self.drag_origin_margin = self.margin;
                self.drag_total_delta = egui::Vec2::ZERO;
            }
            self.drag_total_delta += delta;
            self.margin.0 =
                (self.drag_origin_margin.0 + self.drag_total_delta.x.round() as i32).max(0);
            self.margin.1 =
                (self.drag_origin_margin.1 + self.drag_total_delta.y.round() as i32).max(0);
            if let Some(layer) = &self.layer {
                layer.set_margin(self.margin.1, 0, 0, self.margin.0);
                layer.commit();
            }
        }
        if actions.restore_game_focus && self.dragging {
            self.dragging = false;
            self.drag_total_delta = egui::Vec2::ZERO;
            self.send_command(UiCommand::SetOverlayPosition {
                x: self.margin.0,
                y: self.margin.1,
            });
        }
    }

    fn send_command(&self, command: UiCommand) {
        if self.command_tx.send(command).is_ok() {
            (self.app_repaint)();
        }
    }
}

fn repaint_timeout(deadline: Option<Instant>, now: Instant) -> Option<Timespec> {
    deadline
        .map(|deadline| deadline.saturating_duration_since(now))
        .and_then(|duration| duration.try_into().ok())
}

impl Drop for Backend {
    fn drop(&mut self) {
        // The unsafe raw Wayland handles held by wgpu must outlive the wgpu surface.
        self.drop_surface();
    }
}

fn create_layer(
    compositor: &CompositorState,
    layer_shell: &LayerShell,
    qh: &QueueHandle<Backend>,
    size: (u32, u32),
    margin: (i32, i32),
    output_scale: i32,
    output: Option<&wl_output::WlOutput>,
) -> LayerSurface {
    let surface = compositor.create_surface(qh);
    surface.set_buffer_scale(output_scale.max(1));
    let layer =
        layer_shell.create_layer_surface(qh, surface, Layer::Overlay, Some("overmax"), output);
    layer.set_anchor(Anchor::TOP | Anchor::LEFT);
    layer.set_margin(margin.1, 0, 0, margin.0);
    layer.set_size(size.0, size.1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_exclusive_zone(-1);
    layer
}

fn create_viewport(
    manager: &WpViewporter,
    layer: &LayerSurface,
    qh: &QueueHandle<Backend>,
    logical_size: (u32, u32),
) -> WpViewport {
    layer.wl_surface().set_buffer_scale(1);
    let viewport = manager.get_viewport(layer.wl_surface(), qh, ());
    viewport.set_destination(logical_size.0 as i32, logical_size.1 as i32);
    viewport
}

fn create_wgpu_surface(
    connection: &Connection,
    instance: &wgpu::Instance,
    layer: &LayerSurface,
) -> Result<wgpu::Surface<'static>, String> {
    let display = NonNull::new(connection.backend().display_ptr() as *mut _)
        .ok_or_else(|| "Wayland display pointer is null".to_string())?;
    let surface = NonNull::new(layer.wl_surface().id().as_ptr() as *mut _)
        .ok_or_else(|| "Wayland surface pointer is null".to_string())?;
    let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display));
    let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(surface));
    unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display_handle),
                raw_window_handle,
            })
            .map_err(|error| error.to_string())
    }
}

fn surface_settings(
    surface: &wgpu::Surface<'static>,
    adapter: &wgpu::Adapter,
    logical_size: (u32, u32),
    render_scale: f64,
) -> Result<(wgpu::TextureFormat, wgpu::SurfaceConfiguration), String> {
    let capabilities = surface.get_capabilities(adapter);
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(|format| {
            matches!(
                format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
            )
        })
        .or_else(|| capabilities.formats.first().copied())
        .ok_or_else(|| "Wayland surface exposes no texture format".to_string())?;
    let physical = physical_size(logical_size, render_scale);
    let mut config = surface
        .get_default_config(adapter, physical.0, physical.1)
        .ok_or_else(|| "GPU adapter cannot render to the Wayland surface".to_string())?;
    config.format = format;
    if !capabilities
        .alpha_modes
        .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
    {
        return Err("Wayland surface does not support premultiplied transparency".to_string());
    }
    config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
    Ok((format, config))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OutputGeometry {
    rect: WindowRect,
    scale: f64,
}

fn output_geometry(info: &OutputInfo) -> Option<OutputGeometry> {
    let current_mode = info.modes.iter().find(|mode| mode.current);
    let (position, logical_size) = match (info.logical_position, info.logical_size) {
        (Some(position), Some(size)) if size.0 > 0 && size.1 > 0 => (position, size),
        _ => {
            let mode = current_mode?;
            let scale = info.scale_factor.max(1);
            (
                info.location,
                (mode.dimensions.0 / scale, mode.dimensions.1 / scale),
            )
        }
    };
    let scale = current_mode.map_or(info.scale_factor.max(1) as f64, |mode| {
        let physical_area = f64::from(mode.dimensions.0) * f64::from(mode.dimensions.1);
        let logical_area = f64::from(logical_size.0) * f64::from(logical_size.1);
        (physical_area / logical_area).sqrt()
    });
    Some(OutputGeometry {
        rect: WindowRect {
            left: position.0,
            top: position.1,
            width: logical_size.0,
            height: logical_size.1,
        },
        scale: scale.max(1.0),
    })
}

fn select_output_by_overlap<T>(
    game_rect: WindowRect,
    outputs: impl Iterator<Item = (T, WindowRect)>,
) -> Option<T> {
    outputs
        .map(|(output, rect)| {
            let width = (game_rect.left + game_rect.width).min(rect.left + rect.width)
                - game_rect.left.max(rect.left);
            let height = (game_rect.top + game_rect.height).min(rect.top + rect.height)
                - game_rect.top.max(rect.top);
            (output, i64::from(width.max(0)) * i64::from(height.max(0)))
        })
        .max_by_key(|(_, overlap)| *overlap)
        .filter(|(_, overlap)| *overlap > 0)
        .map(|(output, _)| output)
}

fn physical_size(logical: (u32, u32), scale: f64) -> (u32, u32) {
    (
        (f64::from(logical.0) * scale).ceil().max(1.0) as u32,
        (f64::from(logical.1) * scale).ceil().max(1.0) as u32,
    )
}

fn fallback_buffer_scale(render_scale: f64, has_viewporter: bool) -> i32 {
    if has_viewporter {
        1
    } else {
        render_scale.round().max(1.0) as i32
    }
}

fn overlay_props(snapshot: &LinuxOverlaySnapshot) -> OverlayProps<'_> {
    OverlayProps {
        state: &snapshot.state,
        song_label: &snapshot.song_label,
        pattern_tabs: &snapshot.pattern_tabs,
        recommendations: &snapshot.recommendations,
        settings_open: snapshot.settings_open.clone(),
        sync_open: snapshot.sync_open.clone(),
        scale: snapshot.scale,
        opacity: snapshot.opacity,
        varchive_upload_needed: snapshot.varchive_upload_needed,
        varchive_account_configured: snapshot.varchive_account_configured,
        lite_mode: snapshot.lite_mode,
        is_snap_manual: uses_manual_position(snapshot),
        record_manager: snapshot.record_manager.as_ref(),
        session_initial_record: snapshot.session_initial_record,
        toast: snapshot.toast.as_ref(),
    }
}

fn draw_degraded(ui: &mut egui::Ui, snapshot: &LinuxOverlaySnapshot) -> OverlayActions {
    let mut actions = OverlayActions::default();
    let response = egui::Frame::NONE
        .fill(egui::Color32::from_rgba_unmultiplied(14, 14, 20, 235))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            let drag = ui.add(egui::Label::new("OVERMAX").sense(egui::Sense::drag()));
            if drag.drag_started() {
                actions.start_drag = true;
            }
            if drag.dragged() {
                actions.drag_delta = Some(drag.drag_delta());
            }
            if drag.drag_stopped() {
                actions.restore_game_focus = true;
            }
            let message = degraded_message(snapshot);
            ui.add(egui::Label::new(message).truncate())
                .on_hover_text(message);
            ui.horizontal(|ui| {
                if ui.button("Settings").clicked() {
                    snapshot
                        .settings_open
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    actions.command = Some(UiCommand::OpenSettings);
                }
                if ui.button("Debug").clicked() {
                    actions.command = Some(UiCommand::OpenDebug);
                }
                if ui.button("Exit").clicked() {
                    actions.command = Some(UiCommand::Exit);
                }
            });
        });
    actions.response_rect = Some(response.response.rect);
    actions
}

fn degraded_message(snapshot: &LinuxOverlaySnapshot) -> &str {
    if let Some(error) = snapshot.capture_fatal.as_deref() {
        return error;
    }
    "DJMAX RESPECT V window not found"
}

fn is_degraded(snapshot: &LinuxOverlaySnapshot) -> bool {
    snapshot.capture_fatal.is_some() || snapshot.window_snapshot.is_none()
}

fn is_hidden(snapshot: &LinuxOverlaySnapshot) -> bool {
    snapshot.capture_fatal.is_none()
        && snapshot.window_snapshot.is_some_and(|window| {
            !window.foreground
                || (!snapshot.always_visible
                    && (snapshot.state.scene == overmax_core::SceneType::Unknown
                        || snapshot.state.scene.is_ingame()))
        })
}

fn panel_size(snapshot: Option<&LinuxOverlaySnapshot>) -> (u32, u32) {
    let Some(snapshot) = snapshot else {
        return (DEGRADED_WIDTH as u32, DEGRADED_HEIGHT as u32);
    };
    if is_degraded(snapshot) {
        return (DEGRADED_WIDTH as u32, DEGRADED_HEIGHT as u32);
    }
    let (width, height) = if snapshot.lite_mode {
        (overlay_ui::BASE_WIDTH, overlay_ui::LITE_BASE_HEIGHT)
    } else {
        (overlay_ui::BASE_WIDTH, overlay_ui::BASE_HEIGHT)
    };
    (
        (width * snapshot.scale).ceil().max(1.0) as u32,
        ((height + CONTROLS_HEIGHT) * snapshot.scale)
            .ceil()
            .max(1.0) as u32,
    )
}

fn retained_panel_size(current: (u32, u32), snapshot: &LinuxOverlaySnapshot) -> (u32, u32) {
    if is_hidden(snapshot) {
        current
    } else {
        panel_size(Some(snapshot))
    }
}

fn uses_manual_position(snapshot: &LinuxOverlaySnapshot) -> bool {
    snapshot.snap == "manual"
        || snapshot
            .window_snapshot
            .is_some_and(|window| !window.fullscreen)
}

fn panel_margin(
    snapshot: &LinuxOverlaySnapshot,
    size: (u32, u32),
    output_logical_size: Option<(i32, i32)>,
) -> (i32, i32) {
    let snap = if uses_manual_position(snapshot) {
        "manual"
    } else {
        &snapshot.snap
    };
    calculate_margin(
        snap,
        snapshot.position,
        output_logical_size.map(|(width, height)| WindowRect {
            left: 0,
            top: 0,
            width,
            height,
        }),
        size,
    )
}

fn calculate_margin(
    snap: &str,
    position: Option<(i32, i32)>,
    anchor_rect: Option<WindowRect>,
    size: (u32, u32),
) -> (i32, i32) {
    let manual = || {
        let (x, y) = position.unwrap_or((DEFAULT_MARGIN, DEFAULT_MARGIN));
        (x.max(0), y.max(0))
    };
    if snap == "manual" {
        return manual();
    }
    let Some(rect) = anchor_rect else {
        return manual();
    };
    let right = (rect.left + rect.width - size.0 as i32 - SNAP_MARGIN).max(0);
    let bottom = (rect.top + rect.height - size.1 as i32 - SNAP_MARGIN).max(0);
    match snap {
        "top_left" => (
            (rect.left + SNAP_MARGIN).max(0),
            (rect.top + SNAP_MARGIN).max(0),
        ),
        "top_right" => (right, (rect.top + SNAP_MARGIN).max(0)),
        "bottom_left" => ((rect.left + SNAP_MARGIN).max(0), bottom),
        "bottom_right" => (right, bottom),
        _ => manual(),
    }
}

impl CompositorHandler for Backend {
    fn scale_factor_changed(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if !self
            .layer
            .as_ref()
            .is_some_and(|layer| layer.wl_surface() == surface)
        {
            return;
        }
        if self.viewport.is_none() {
            self.render_scale = f64::from(factor.max(1));
        }
        surface.set_buffer_scale(fallback_buffer_scale(
            self.render_scale,
            self.viewport.is_some(),
        ));
        self.configure_surface();
        self.needs_redraw = true;
    }

    fn transform_changed(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        if self
            .layer
            .as_ref()
            .is_some_and(|layer| layer.wl_surface() == surface)
        {
            self.needs_redraw = true;
        }
    }

    fn surface_enter(
        &mut self,
        _connection: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if !self
            .layer
            .as_ref()
            .is_some_and(|layer| layer.wl_surface() == surface)
        {
            return;
        }
        self.surface_output = Some(output.clone());
        self.refresh_output(qh);
    }

    fn surface_leave(
        &mut self,
        _connection: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if self
            .layer
            .as_ref()
            .is_some_and(|layer| layer.wl_surface() == surface)
            && self.surface_output.as_ref() == Some(output)
        {
            self.surface_output = None;
            self.refresh_output(qh);
        }
    }
}

impl LayerShellHandler for Backend {
    fn closed(&mut self, _connection: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        if self.layer.as_ref() == Some(layer) {
            self.drop_surface();
            self.recreate_on_output = true;
        }
    }

    fn configure(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if self.layer.as_ref() != Some(layer) {
            return;
        }
        self.logical_size = (
            if configure.new_size.0 == 0 {
                self.requested_size.0
            } else {
                configure.new_size.0
            },
            if configure.new_size.1 == 0 {
                self.requested_size.1
            } else {
                configure.new_size.1
            },
        );
        if let Some(viewport) = &self.viewport {
            viewport.set_destination(self.logical_size.0 as i32, self.logical_size.1 as i32);
        }
        self.configure_surface();
        self.configured = true;
        self.needs_redraw = true;
    }
}

impl SeatHandler for Backend {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _connection: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }

    fn remove_capability(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for Backend {
    fn pointer_frame(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(layer) = &self.layer else {
            return;
        };
        for event in events {
            if &event.surface != layer.wl_surface() {
                continue;
            }
            let position = egui::pos2(event.position.0 as f32, event.position.1 as f32);
            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.pointer_position = Some(position);
                    self.events.push(egui::Event::PointerMoved(position));
                }
                PointerEventKind::Leave { .. } => {
                    self.pointer_position = None;
                    self.events.push(egui::Event::PointerGone);
                }
                PointerEventKind::Press { button, .. }
                | PointerEventKind::Release { button, .. } => {
                    let button = match button {
                        272 => egui::PointerButton::Primary,
                        273 => egui::PointerButton::Secondary,
                        274 => egui::PointerButton::Middle,
                        _ => continue,
                    };
                    self.events.push(egui::Event::PointerButton {
                        pos: position,
                        button,
                        pressed: matches!(event.kind, PointerEventKind::Press { .. }),
                        modifiers: egui::Modifiers::default(),
                    });
                }
                PointerEventKind::Axis { .. } => {}
            }
        }
        self.needs_redraw = true;
    }
}

impl OutputHandler for Backend {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        if self.layer.is_none() {
            self.recreate_on_output = true;
        }
    }

    fn update_output(
        &mut self,
        _connection: &Connection,
        qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        self.refresh_output(qh);
    }

    fn output_destroyed(
        &mut self,
        _connection: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.remove_toplevel_output(&output);
        if self.surface_output.as_ref() == Some(&output) {
            self.surface_output = None;
        }
        self.refresh_output(qh);
    }
}

impl Dispatch<WpViewporter, ()> for Backend {
    fn event(
        _: &mut Self,
        _: &WpViewporter,
        _: wp_viewporter::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("wp_viewporter has no events")
    }
}

impl Dispatch<WpFractionalScaleManagerV1, ()> for Backend {
    fn event(
        _: &mut Self,
        _: &WpFractionalScaleManagerV1,
        _: wp_fractional_scale_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("wp_fractional_scale_manager_v1 has no events")
    }
}

impl Dispatch<WpFractionalScaleV1, ()> for Backend {
    fn event(
        state: &mut Self,
        scale_object: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if state.fractional_scale.as_ref() != Some(scale_object) {
            return;
        }
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else {
            return;
        };
        let scale = f64::from(scale) / 120.0;
        if (state.render_scale - scale).abs() <= f64::EPSILON {
            state.preferred_scale_received = true;
            return;
        }
        state.preferred_scale_received = true;
        state.render_scale = scale;
        state.configure_surface();
        state.needs_redraw = true;
        eprintln!("[LinuxOverlay] preferred fractional scale={scale:.3}");
    }
}

impl Dispatch<WpViewport, ()> for Backend {
    fn event(
        _: &mut Self,
        _: &WpViewport,
        _: wp_viewport::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("wp_viewport has no events")
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for Backend {
    fn event(
        state: &mut Self,
        manager: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } => {
                state.foreign_toplevels.push(ForeignToplevel {
                    handle: toplevel,
                    state: ForeignToplevelState::default(),
                });
            }
            zwlr_foreign_toplevel_manager_v1::Event::Finished => {
                if state.foreign_toplevel_manager.as_ref() == Some(manager) {
                    state.foreign_toplevel_manager = None;
                }
                state.foreign_toplevels.clear();
                state.publish_presentation_observation();
                state.refresh_output(qh);
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(Backend, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for Backend {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(index) = state
            .foreign_toplevels
            .iter()
            .position(|toplevel| &toplevel.handle == handle)
        else {
            return;
        };
        let mut publish = false;
        let mut closed = false;
        let toplevel = &mut state.foreign_toplevels[index];
        match event {
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                toplevel.state.pending.title = Some(title);
            }
            zwlr_foreign_toplevel_handle_v1::Event::State { state: raw } => {
                let (activated, fullscreen) =
                    parse_foreign_toplevel_states(&raw, handle.version() >= 2);
                toplevel.state.pending.activated = activated;
                toplevel.state.pending.fullscreen = fullscreen;
            }
            zwlr_foreign_toplevel_handle_v1::Event::OutputEnter { output } => {
                if !toplevel.state.pending.outputs.contains(&output) {
                    toplevel.state.pending.outputs.push(output);
                }
            }
            zwlr_foreign_toplevel_handle_v1::Event::OutputLeave { output } => {
                toplevel
                    .state
                    .pending
                    .outputs
                    .retain(|candidate| candidate != &output);
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                toplevel.state.committed = Some(toplevel.state.pending.clone());
                publish = true;
            }
            zwlr_foreign_toplevel_handle_v1::Event::Closed => closed = true,
            _ => {}
        }
        if closed {
            let toplevel = state.foreign_toplevels.remove(index);
            toplevel.handle.destroy();
            state.publish_presentation_observation();
            state.refresh_output(qh);
        } else if publish {
            state.publish_presentation_observation();
            state.refresh_output(qh);
        }
    }
}

delegate_compositor!(Backend);
delegate_output!(Backend);
delegate_seat!(Backend);
delegate_pointer!(Backend);
delegate_layer!(Backend);
delegate_registry!(Backend);

impl ProvidesRegistryState for Backend {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_margin, output_render_scale, overlay_props, panel_margin, panel_size,
        parse_foreign_toplevel_states, physical_size, retained_panel_size, select_committed_output,
        select_output_by_overlap, unique_matching_toplevel, uses_manual_position,
        ForeignToplevelState, LinuxLayerOverlayHandle, LinuxOverlaySnapshot, PublishedSnapshots,
    };
    use overmax_core::{GameSessionState, SceneType};
    use overmax_data::{RecommendResult, RecordDB, RecordManager};
    use overmax_engine::capture::window_tracker::{WindowRect, WindowSnapshot};
    use std::io::Read as _;
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[test]
    fn calculates_snap_and_clamps_manual_position() {
        let rect = WindowRect {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        };
        assert_eq!(
            calculate_margin("bottom_right", None, Some(rect), (360, 380)),
            (1544, 684)
        );
        assert_eq!(
            calculate_margin(
                "bottom_right",
                None,
                Some(WindowRect {
                    left: 0,
                    top: 0,
                    width: 2560,
                    height: 1440,
                }),
                (360, 380),
            ),
            (2184, 1044)
        );
        assert_eq!(
            calculate_margin("manual", Some((-20, 30)), Some(rect), (360, 380)),
            (0, 30)
        );
        assert_eq!(panel_size(None), (320, 116));
    }

    #[test]
    fn scales_fractional_buffers() {
        assert_eq!(physical_size((360, 380), 1.25), (450, 475));
        assert_eq!(output_render_scale(1.0, Some(1.25), true, false), 1.25);
        assert_eq!(output_render_scale(1.5, Some(1.25), true, true), 1.5);
    }

    #[test]
    fn selects_only_unambiguous_committed_output() {
        assert_eq!(select_committed_output(&[1], None), Some(&1));
        assert_eq!(select_committed_output(&[1, 2], Some(&2)), Some(&2));
        assert_eq!(select_committed_output(&[1, 2], Some(&3)), None);
        assert_eq!(select_committed_output::<i32>(&[], Some(&1)), None);
    }

    #[test]
    fn falls_back_to_the_output_with_the_largest_game_overlap() {
        let left = WindowRect {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        };
        let right = WindowRect {
            left: 1920,
            top: 0,
            width: 2560,
            height: 1440,
        };
        assert_eq!(
            select_output_by_overlap(right, [("left", left), ("right", right)].into_iter()),
            Some("right")
        );
        assert_eq!(
            select_output_by_overlap(
                WindowRect {
                    left: -500,
                    top: -500,
                    width: 100,
                    height: 100,
                },
                [("left", left), ("right", right)].into_iter(),
            ),
            None
        );
    }

    #[test]
    fn delayed_repaint_becomes_a_poll_timeout() {
        let now = Instant::now();
        let timeout = super::repaint_timeout(Some(now + Duration::from_millis(25)), now)
            .expect("scheduled repaint must set a timeout");
        assert_eq!(
            Duration::try_from(timeout).unwrap(),
            Duration::from_millis(25)
        );

        let due = super::repaint_timeout(Some(now), now).expect("due repaint uses zero timeout");
        assert!(Duration::try_from(due).unwrap().is_zero());
        assert!(super::repaint_timeout(None, now).is_none());
    }

    #[test]
    fn commits_exact_unique_foreign_toplevel_state() {
        let mut target = ForeignToplevelState::default();
        target.pending.title = Some("DJMAX RESPECT V".into());
        target.pending.activated = true;

        assert!(unique_matching_toplevel("DJMAX RESPECT V", [&target].into_iter()).is_none());
        target.committed = Some(target.pending.clone());
        assert!(
            unique_matching_toplevel("DJMAX RESPECT V", [&target].into_iter())
                .is_some_and(|snapshot| snapshot.activated)
        );

        let duplicate = ForeignToplevelState {
            committed: target.committed.clone(),
            ..Default::default()
        };
        assert!(
            unique_matching_toplevel("DJMAX RESPECT V", [&target, &duplicate].into_iter())
                .is_none()
        );
    }

    #[test]
    fn parses_v1_focus_and_v2_fullscreen_states() {
        let raw = [2u32.to_ne_bytes(), 3u32.to_ne_bytes(), 99u32.to_ne_bytes()].concat();
        assert_eq!(parse_foreign_toplevel_states(&raw, false), (true, None));
        assert_eq!(
            parse_foreign_toplevel_states(&raw, true),
            (true, Some(true))
        );
    }

    #[test]
    fn publish_skips_equal_display_state_and_tracks_atomic_values() {
        let settings_open = Arc::new(AtomicBool::new(false));
        let sync_open = Arc::new(AtomicBool::new(false));
        let record_db = Arc::new(RecordDB::new("unused-record.db", None));
        let record_manager = Arc::new(RecordManager::new(record_db));
        let snapshot = LinuxOverlaySnapshot {
            state: GameSessionState::detecting(),
            song_label: String::new(),
            pattern_tabs: Vec::new(),
            recommendations: RecommendResult::empty(),
            settings_open: settings_open.clone(),
            sync_open,
            scale: 1.0,
            opacity: 0.8,
            varchive_upload_needed: false,
            varchive_account_configured: false,
            lite_mode: false,
            always_visible: false,
            snap: "manual".to_string(),
            position: None,
            record_manager,
            session_initial_record: None,
            toast: None,
            window_snapshot: None,
            capture_fatal: None,
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            delivery_telemetry: None,
        };
        assert_eq!(panel_size(Some(&snapshot)), (320, 116));
        let mut background = snapshot.clone();
        background.window_snapshot = Some(WindowSnapshot {
            window: 7,
            rect: WindowRect {
                left: -1920,
                top: -200,
                width: 1920,
                height: 1080,
            },
            foreground: false,
            fullscreen: true,
        });
        assert_eq!(panel_size(Some(&background)), (360, 406));
        assert_eq!(retained_panel_size((420, 240), &background), (420, 240));
        background.window_snapshot.as_mut().unwrap().foreground = true;
        assert_eq!(retained_panel_size((420, 240), &background), (420, 240));
        background.state.scene = SceneType::Freestyle;
        assert_eq!(panel_size(Some(&background)), (360, 406));
        assert_eq!(retained_panel_size((420, 240), &background), (360, 406));
        for scene in [SceneType::Gameplay, SceneType::Paused] {
            background.state.scene = scene;
            assert!(super::is_hidden(&background));
            background.always_visible = true;
            assert!(!super::is_hidden(&background));
            background.always_visible = false;
        }
        background.state.scene = SceneType::Freestyle;
        background.window_snapshot.as_mut().unwrap().fullscreen = false;
        background.snap = "bottom_right".to_string();
        background.position = Some((25, 35));
        assert_eq!(panel_size(Some(&background)), (360, 406));
        assert!(uses_manual_position(&background));
        assert!(overlay_props(&background).is_snap_manual);
        assert_eq!(panel_margin(&background, (360, 406), None), (25, 35));
        background.window_snapshot.as_mut().unwrap().fullscreen = true;
        assert!(!uses_manual_position(&background));
        assert_eq!(
            panel_margin(&background, (360, 406), Some((1920, 1080))),
            (1544, 658)
        );

        let (mut reader, writer) = UnixStream::pair().expect("UnixStream pair");
        reader
            .set_nonblocking(true)
            .expect("nonblocking wake reader");
        let handle = LinuxLayerOverlayHandle {
            published: Arc::new(Mutex::new(PublishedSnapshots::default())),
            wake_writer: Arc::new(writer),
            runtime_failure: Arc::new(Mutex::new(None)),
            presentation_observation: Arc::new(Mutex::new(None)),
            runtime_telemetry: None,
        };
        let mut wake = [0u8; 8];

        handle.publish(snapshot.clone());
        assert!(
            reader
                .read(&mut wake)
                .expect("first snapshot wakes backend")
                > 0
        );
        handle.publish(snapshot.clone());
        assert_eq!(
            reader
                .read(&mut wake)
                .expect_err("duplicate display state must not wake backend")
                .kind(),
            std::io::ErrorKind::WouldBlock
        );

        let mut changed_opacity = snapshot.clone();
        changed_opacity.opacity = 0.7;
        handle.publish(changed_opacity);
        assert!(
            reader
                .read(&mut wake)
                .expect("changed opacity wakes backend")
                > 0
        );

        settings_open.store(true, Ordering::Relaxed);
        handle.publish(snapshot);
        assert!(
            reader
                .read(&mut wake)
                .expect("changed atomic display value wakes backend")
                > 0
        );

        assert!(
            super::drain_wake_socket(&reader),
            "no wake pending, handle still connected"
        );
        drop(handle);
        assert!(
            wait_for_eof(&reader, Duration::from_millis(200)),
            "dropping every handle signals shutdown via EOF"
        );
    }

    fn wait_for_eof(reader: &UnixStream, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if !super::drain_wake_socket(reader) {
                return true; // EOF 관측됨
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
