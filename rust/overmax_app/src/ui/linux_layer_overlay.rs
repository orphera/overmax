//! Native Wayland layer-shell overlay used by the Linux port.

use crate::ui::components::ToastMessage;
use crate::ui::overlay_recommend_ui::PatternTabInfo;
use crate::ui::overlay_ui::{self, OverlayActions, OverlayProps};
use crate::ui::ui_command::UiCommand;
use overmax_core::{GameSessionState, RecordValue};
use overmax_data::{RecommendResult, RecordManager};
use overmax_engine::capture::window_tracker::{WindowRect, WindowSnapshot};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use rustix::event::{poll, PollFd, PollFlags};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat,
    output::{OutputHandler, OutputState},
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
use std::time::Instant;
use wayland_client::{
    backend::WaylandError,
    globals::registry_queue_init,
    protocol::{wl_output, wl_pointer, wl_seat, wl_surface},
    Connection, Proxy, QueueHandle,
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
    pub snap: String,
    pub position: Option<(i32, i32)>,
    pub record_manager: Arc<RecordManager>,
    pub session_initial_record: Option<RecordValue>,
    pub toast: Option<ToastMessage>,
    pub window_snapshot: Option<WindowSnapshot>,
    pub capture_fatal: Option<String>,
}

#[derive(Clone)]
pub struct LinuxLayerOverlayHandle {
    published: Arc<Mutex<PublishedSnapshots>>,
    wake_writer: Arc<UnixStream>,
    runtime_failure: Arc<Mutex<Option<String>>>,
}

#[derive(Default)]
struct PublishedSnapshots {
    latest: Option<Arc<LinuxOverlaySnapshot>>,
    last: Option<LastPublishedSnapshot>,
}

struct LastPublishedSnapshot {
    snapshot: Arc<LinuxOverlaySnapshot>,
    settings_open: bool,
    sync_open: bool,
}

impl LinuxLayerOverlayHandle {
    pub fn publish(&self, snapshot: LinuxOverlaySnapshot) {
        let settings_open = snapshot.settings_open.load(Ordering::Relaxed);
        let sync_open = snapshot.sync_open.load(Ordering::Relaxed);
        let Ok(mut published) = self.published.lock() else {
            return;
        };
        if published.last.as_ref().is_some_and(|last| {
            same_display_snapshot(
                &last.snapshot,
                last.settings_open,
                last.sync_open,
                &snapshot,
                settings_open,
                sync_open,
            )
        }) {
            return;
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
    command_tx: Sender<UiCommand>,
    app_repaint: AppRepaintCallback,
) -> Result<LinuxLayerOverlayHandle, String> {
    let published = Arc::new(Mutex::new(PublishedSnapshots::default()));
    let runtime_failure = Arc::new(Mutex::new(None));
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

    std::thread::Builder::new()
        .name("overmax-linux-overlay".to_string())
        .spawn(move || {
            let result = run(
                command_tx,
                app_repaint.clone(),
                thread_published,
                wake_reader,
                ready_tx,
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
    })
}

fn run(
    command_tx: Sender<UiCommand>,
    app_repaint: AppRepaintCallback,
    published: Arc<Mutex<PublishedSnapshots>>,
    wake_reader: UnixStream,
    ready_tx: SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let initialized = Backend::new(command_tx, app_repaint, published);
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
                match poll(&mut fds, None) {
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
            backend.consume_published();
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

struct Backend {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    connection: Connection,
    layer: Option<LayerSurface>,
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
    output_scale: i32,

    egui_ctx: egui::Context,
    events: Vec<egui::Event>,
    start: Instant,
    needs_redraw: bool,
    snapshot: Option<Arc<LinuxOverlaySnapshot>>,
    published: Arc<Mutex<PublishedSnapshots>>,
    command_tx: Sender<UiCommand>,
    app_repaint: AppRepaintCallback,
    margin: (i32, i32),
    dragging: bool,
    drag_origin_margin: (i32, i32),
    drag_total_delta: egui::Vec2,
}

impl Backend {
    fn new(
        command_tx: Sender<UiCommand>,
        app_repaint: AppRepaintCallback,
        published: Arc<Mutex<PublishedSnapshots>>,
    ) -> Result<(wayland_client::EventQueue<Self>, Self), String> {
        let connection = Connection::connect_to_env().map_err(|error| error.to_string())?;
        let (globals, event_queue) =
            registry_queue_init(&connection).map_err(|error| error.to_string())?;
        let qh = event_queue.handle();
        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|_| "wl_compositor is unavailable".to_string())?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|_| "zwlr_layer_shell_v1 is unavailable".to_string())?;
        let requested_size = panel_size(None);
        let margin = (DEFAULT_MARGIN, DEFAULT_MARGIN);
        let layer = create_layer(&compositor, &layer_shell, &qh, requested_size, margin, 1);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
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
        let (format, surface_config) = surface_settings(&surface, &adapter, requested_size, 1)?;
        let renderer =
            egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
        let egui_ctx = egui::Context::default();
        egui_ctx.set_visuals(egui::Visuals::dark());
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
                connection,
                layer: Some(layer),
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
                output_scale: 1,
                egui_ctx,
                events: Vec::new(),
                start: Instant::now(),
                needs_redraw: false,
                snapshot: None,
                published,
                command_tx,
                app_repaint,
                margin,
                dragging: false,
                drag_origin_margin: margin,
                drag_total_delta: egui::Vec2::ZERO,
            },
        ))
    }

    fn after_dispatch(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
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

    fn consume_published(&mut self) {
        let snapshot = self
            .published
            .lock()
            .ok()
            .and_then(|mut published| published.latest.take());
        if let Some(snapshot) = snapshot {
            self.apply_snapshot(snapshot);
        }
    }

    fn apply_snapshot(&mut self, snapshot: Arc<LinuxOverlaySnapshot>) {
        let size = panel_size(Some(&snapshot));
        let size_changed = self.requested_size != size;
        let reposition = self.snapshot.as_ref().is_none_or(|previous| {
            previous.snap != snapshot.snap
                || previous.position != snapshot.position
                || previous.window_snapshot != snapshot.window_snapshot
                || size_changed
        });
        let margin = if reposition {
            panel_margin(&snapshot, size)
        } else {
            self.margin
        };
        self.snapshot = Some(snapshot);
        self.requested_size = size;
        self.margin = margin;
        if reposition && self.dragging {
            self.reset_pointer_state();
        }
        if let Some(layer) = &self.layer {
            if size_changed {
                self.configured = false;
            }
            layer.set_size(size.0, size.1);
            layer.set_margin(margin.1, 0, 0, margin.0);
            layer.commit();
        }
        self.needs_redraw = true;
    }

    fn create_surface(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        let layer = create_layer(
            &self.compositor,
            &self.layer_shell,
            qh,
            self.requested_size,
            self.margin,
            self.output_scale,
        );
        let surface = create_wgpu_surface(&self.connection, &self.instance, &layer)?;
        let capabilities = surface.get_capabilities(&self.adapter);
        if !capabilities.formats.contains(&self.format) {
            return Err("The recreated output does not support the selected texture format".into());
        }
        let (_, mut config) = surface_settings(
            &surface,
            &self.adapter,
            self.requested_size,
            self.output_scale,
        )?;
        config.format = self.format;
        self.layer = Some(layer);
        self.surface = Some(surface);
        self.surface_config = Some(config);
        self.logical_size = self.requested_size;
        self.configured = false;
        self.needs_redraw = true;
        Ok(())
    }

    fn drop_surface(&mut self) {
        self.reset_pointer_state();
        self.surface = None;
        self.surface_config = None;
        self.layer = None;
        self.configured = false;
        self.needs_redraw = false;
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

    fn configure_surface(&mut self) {
        let Some(surface) = &self.surface else {
            return;
        };
        let Some(config) = &mut self.surface_config else {
            return;
        };
        config.width = self
            .logical_size
            .0
            .saturating_mul(self.output_scale as u32)
            .max(1);
        config.height = self
            .logical_size
            .1
            .saturating_mul(self.output_scale as u32)
            .max(1);
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
            viewport.native_pixels_per_point = Some(self.output_scale as f32);
        }

        let mut actions = OverlayActions::default();
        let mut control_command = None;
        let ctx = self.egui_ctx.clone();
        let full_output = ctx.run(raw_input, |ctx| {
            if let Some(snapshot) = &self.snapshot {
                if !is_hidden(snapshot) && !is_degraded(snapshot) {
                    egui::TopBottomPanel::bottom("linux_overlay_controls")
                        .exact_height(CONTROLS_HEIGHT * snapshot.scale)
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
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [
                self.logical_size.0.saturating_mul(self.output_scale as u32),
                self.logical_size.1.saturating_mul(self.output_scale as u32),
            ],
            pixels_per_point: full_output.pixels_per_point,
        };
        for (id, delta) in &full_output.textures_delta.set {
            self.renderer
                .update_texture(&self.device, &self.queue, *id, delta);
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

        let repaint_now = full_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|output| output.repaint_delay.is_zero());
        if repaint_now {
            if let Some(layer) = &self.layer {
                layer.wl_surface().frame(qh, layer.wl_surface().clone());
            }
        }
        self.queue.submit(
            user_buffers
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        frame.present();
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
                Ok(frame) => return Ok(Some(frame)),
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    self.configure_surface();
                }
                Err(wgpu::SurfaceError::Timeout) => {
                    self.needs_redraw = true;
                    return Ok(None);
                }
                Err(error) => return Err(error.to_string()),
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
) -> LayerSurface {
    let surface = compositor.create_surface(qh);
    surface.set_buffer_scale(output_scale.max(1));
    let layer =
        layer_shell.create_layer_surface(qh, surface, Layer::Overlay, Some("overmax"), None);
    layer.set_anchor(Anchor::TOP | Anchor::LEFT);
    layer.set_margin(margin.1, 0, 0, margin.0);
    layer.set_size(size.0, size.1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_exclusive_zone(-1);
    layer.commit();
    layer
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
                raw_display_handle,
                raw_window_handle,
            })
            .map_err(|error| error.to_string())
    }
}

fn surface_settings(
    surface: &wgpu::Surface<'static>,
    adapter: &wgpu::Adapter,
    logical_size: (u32, u32),
    output_scale: i32,
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
    let mut config = surface
        .get_default_config(
            adapter,
            logical_size.0.saturating_mul(output_scale as u32).max(1),
            logical_size.1.saturating_mul(output_scale as u32).max(1),
        )
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

fn overlay_props(snapshot: &LinuxOverlaySnapshot) -> OverlayProps<'_> {
    OverlayProps {
        state: &snapshot.state,
        song_label: &snapshot.song_label,
        pattern_tabs: &snapshot.pattern_tabs,
        recommendations: &snapshot.recommendations,
        settings_open: snapshot.settings_open.clone(),
        sync_open: snapshot.sync_open.clone(),
        scale: snapshot.scale,
        varchive_upload_needed: snapshot.varchive_upload_needed,
        varchive_account_configured: snapshot.varchive_account_configured,
        lite_mode: snapshot.lite_mode,
        is_snap_manual: snapshot.snap == "manual",
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
    match snapshot.window_snapshot {
        Some(window) if !window.fullscreen => "Borderless fullscreen is required",
        _ => "DJMAX RESPECT V window not found",
    }
}

fn is_degraded(snapshot: &LinuxOverlaySnapshot) -> bool {
    snapshot.capture_fatal.is_some()
        || snapshot
            .window_snapshot
            .is_none_or(|window| !window.fullscreen)
}

fn is_hidden(snapshot: &LinuxOverlaySnapshot) -> bool {
    snapshot.capture_fatal.is_none()
        && snapshot
            .window_snapshot
            .is_some_and(|window| !window.foreground)
}

fn panel_size(snapshot: Option<&LinuxOverlaySnapshot>) -> (u32, u32) {
    let Some(snapshot) = snapshot else {
        return (DEGRADED_WIDTH as u32, DEGRADED_HEIGHT as u32);
    };
    if is_hidden(snapshot) {
        return (1, 1);
    }
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

fn panel_margin(snapshot: &LinuxOverlaySnapshot, size: (u32, u32)) -> (i32, i32) {
    calculate_margin(
        &snapshot.snap,
        snapshot.position,
        snapshot.window_snapshot.map(|window| window.rect),
        size,
    )
}

fn calculate_margin(
    snap: &str,
    position: Option<(i32, i32)>,
    game_rect: Option<WindowRect>,
    size: (u32, u32),
) -> (i32, i32) {
    let manual = || {
        let (x, y) = position.unwrap_or((DEFAULT_MARGIN, DEFAULT_MARGIN));
        (x.max(0), y.max(0))
    };
    if snap == "manual" {
        return manual();
    }
    let Some(rect) = game_rect else {
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
        self.output_scale = factor.max(1);
        surface.set_buffer_scale(self.output_scale);
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
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
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
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        self.recreate_on_output = false;
        self.drop_surface();
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
        calculate_margin, panel_size, LinuxLayerOverlayHandle, LinuxOverlaySnapshot,
        PublishedSnapshots,
    };
    use overmax_core::GameSessionState;
    use overmax_data::{RecommendResult, RecordDB, RecordManager};
    use overmax_engine::capture::window_tracker::{WindowRect, WindowSnapshot};
    use std::io::Read as _;
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

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
            calculate_margin("manual", Some((-20, 30)), Some(rect), (360, 380)),
            (0, 30)
        );
        assert_eq!(panel_size(None), (320, 116));
    }

    #[test]
    fn publish_skips_equal_display_state_and_tracks_atomic_values() {
        let settings_open = Arc::new(AtomicBool::new(false));
        let sync_open = Arc::new(AtomicBool::new(false));
        let record_db = Arc::new(RecordDB::new("unused-record.db", None));
        let record_manager = Arc::new(RecordManager::new(record_db, "."));
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
            snap: "manual".to_string(),
            position: None,
            record_manager,
            session_initial_record: None,
            toast: None,
            window_snapshot: None,
            capture_fatal: None,
        };
        assert_eq!(panel_size(Some(&snapshot)), (320, 116));
        let mut background = snapshot.clone();
        background.window_snapshot = Some(WindowSnapshot {
            window: 7,
            rect: WindowRect {
                left: 0,
                top: 0,
                width: 1920,
                height: 1080,
            },
            foreground: false,
            fullscreen: true,
        });
        assert_eq!(panel_size(Some(&background)), (1, 1));
        background.window_snapshot.as_mut().unwrap().foreground = true;
        assert_eq!(panel_size(Some(&background)), (360, 406));
        background.window_snapshot.as_mut().unwrap().fullscreen = false;
        assert_eq!(panel_size(Some(&background)), (320, 116));

        let (mut reader, writer) = UnixStream::pair().expect("UnixStream pair");
        reader
            .set_nonblocking(true)
            .expect("nonblocking wake reader");
        let handle = LinuxLayerOverlayHandle {
            published: Arc::new(Mutex::new(PublishedSnapshots::default())),
            wake_writer: Arc::new(writer),
            runtime_failure: Arc::new(Mutex::new(None)),
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
            !super::drain_wake_socket(&reader),
            "dropping every handle signals shutdown via EOF"
        );
    }
}
