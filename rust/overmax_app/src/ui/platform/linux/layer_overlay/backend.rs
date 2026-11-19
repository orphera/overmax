//! Wayland layer-shell backend: surface/wgpu setup and the render loop state.

use super::*;

mod handlers;

pub(super) struct Backend {
    pub(super) registry_state: RegistryState,
    pub(super) seat_state: SeatState,
    pub(super) output_state: OutputState,
    pub(super) compositor: CompositorState,
    pub(super) layer_shell: LayerShell,
    pub(super) viewporter: Option<WpViewporter>,
    pub(super) fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    pub(super) foreign_toplevel_manager: Option<ZwlrForeignToplevelManagerV1>,
    pub(super) foreign_toplevels: Vec<ForeignToplevel>,
    pub(super) game_window_title: String,
    pub(super) presentation_observation: SharedPresentationObservation,
    pub(super) presentation_generation: u64,
    pub(super) presentation_target: PresentationTarget,
    pub(super) next_toplevel_identity: u64,
    pub(super) presentation_focus: Option<bool>,
    pub(super) connection: Connection,
    pub(super) layer: Option<LayerSurface>,
    pub(super) viewport: Option<WpViewport>,
    pub(super) fractional_scale: Option<WpFractionalScaleV1>,
    pub(super) target_output: Option<wl_output::WlOutput>,
    pub(super) surface_output: Option<wl_output::WlOutput>,
    pub(super) output_logical_size: Option<(i32, i32)>,
    pub(super) preferred_scale_received: bool,
    pub(super) empty_input_region: Region,
    pub(super) input_passthrough: bool,
    pub(super) pointer: Option<wl_pointer::WlPointer>,
    pub(super) pointer_position: Option<egui::Pos2>,
    pub(super) recreate_on_output: bool,

    pub(super) instance: wgpu::Instance,
    pub(super) adapter: wgpu::Adapter,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) renderer: egui_wgpu::Renderer,
    pub(super) format: wgpu::TextureFormat,
    pub(super) surface: Option<wgpu::Surface<'static>>,
    pub(super) surface_config: Option<wgpu::SurfaceConfiguration>,
    pub(super) configured: bool,
    pub(super) requested_size: (u32, u32),
    pub(super) logical_size: (u32, u32),
    pub(super) render_scale: f64,

    pub(super) egui_ctx: egui::Context,
    pub(super) events: Vec<egui::Event>,
    pub(super) start: Instant,
    pub(super) needs_redraw: bool,
    pub(super) next_repaint: Option<Instant>,
    pub(super) snapshot: Option<Arc<LinuxOverlaySnapshot>>,
    pub(super) published: Arc<Mutex<PublishedSnapshots>>,
    pub(super) command_tx: Sender<UiCommand>,
    pub(super) app_repaint: AppRepaintCallback,
    pub(super) margin: (i32, i32),
    pub(super) dragging: bool,
    pub(super) drag_origin_margin: (i32, i32),
    pub(super) drag_total_delta: egui::Vec2,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    pub(super) runtime_telemetry: Option<Arc<overmax_detect::RuntimeTelemetry>>,
}

impl Backend {
    pub(super) fn new(
        game_window_title: String,
        command_tx: Sender<UiCommand>,
        app_repaint: AppRepaintCallback,
        published: Arc<Mutex<PublishedSnapshots>>,
        runtime_telemetry: Option<Arc<overmax_detect::RuntimeTelemetry>>,
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
                presentation_target: PresentationTarget::default(),
                next_toplevel_identity: 0,
                presentation_focus: None,
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

    pub(super) fn publish_presentation_observation(&mut self) {
        let target = unique_matching_toplevel(
            &self.game_window_title,
            self.foreign_toplevels
                .iter()
                .map(|toplevel| &toplevel.state),
        )
        .map(|target| (target.identity, target.activated, target.fullscreen));
        let target_epoch = self
            .presentation_target
            .observe(target.map(|value| value.0));
        self.presentation_generation = self.presentation_generation.wrapping_add(1);
        let observation = target.map(|(_, activated, fullscreen)| PresentationObservation {
            target_epoch,
            focus: if activated {
                FocusState::Focused
            } else {
                FocusState::Background
            },
            fullscreen,
            generation: self.presentation_generation,
            committed_at: Instant::now(),
        });
        let changed = if let Ok(mut shared) = self.presentation_observation.lock() {
            let state = |value: Option<PresentationObservation>| {
                value.map(|value| (value.target_epoch, value.focus, value.fullscreen))
            };
            let changed = state(*shared) != state(observation);
            *shared = observation;
            changed
        } else {
            false
        };
        if changed {
            (self.app_repaint)();
        }
        let focus = target.map(|(_, activated, _)| activated);
        if self.presentation_focus != focus {
            self.presentation_focus = focus;
            let hidden = self
                .snapshot
                .as_deref()
                .is_some_and(|snapshot| is_hidden(snapshot, self.presentation_focus));
            self.set_input_passthrough(hidden);
            self.needs_redraw = true;
        }
    }

    pub(super) fn remove_toplevel_output(&mut self, output: &wl_output::WlOutput) {
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

    pub(super) fn after_dispatch(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
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

    pub(super) fn consume_published(&mut self, qh: &QueueHandle<Self>) {
        let snapshot = self
            .published
            .lock()
            .ok()
            .and_then(|mut published| published.latest.take());
        if let Some(snapshot) = snapshot {
            self.apply_snapshot(snapshot, qh);
        }
    }

    pub(super) fn apply_snapshot(
        &mut self,
        snapshot: Arc<LinuxOverlaySnapshot>,
        qh: &QueueHandle<Self>,
    ) {
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        if let (Some(telemetry), Some(delivery)) =
            (&self.runtime_telemetry, &snapshot.delivery_telemetry)
        {
            telemetry.record_snapshot_applied(delivery);
        }
        let hidden = is_hidden(&snapshot, self.presentation_focus);
        let size = retained_panel_size(self.requested_size, &snapshot, self.presentation_focus);
        let size_changed = self.requested_size != size;
        let reposition = self.snapshot.as_ref().is_none_or(|previous| {
            previous.snap != snapshot.snap
                || previous.position != snapshot.position
                || previous.window_snapshot != snapshot.window_snapshot
                || capture_presentation(previous) != capture_presentation(&snapshot)
                || previous.game_geometry != snapshot.game_geometry
                || size_changed
        });
        let margin = if reposition && !hidden {
            panel_margin(
                &snapshot,
                size,
                self.output_logical_size,
                self.game_anchor(&snapshot),
            )
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

    pub(super) fn select_target_output(&mut self) -> bool {
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

    pub(super) fn refresh_output(&mut self, qh: &QueueHandle<Self>) {
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
        if let Some(snapshot) = self
            .snapshot
            .as_ref()
            .filter(|snapshot| !is_hidden(snapshot, self.presentation_focus))
        {
            self.margin = panel_margin(
                snapshot,
                self.requested_size,
                self.output_logical_size,
                self.game_anchor(snapshot),
            );
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

    /// Game window rect in target-output-local logical coordinates, when a geometry source
    /// reported it for the output the overlay is on.
    pub(super) fn game_anchor(&self, snapshot: &LinuxOverlaySnapshot) -> Option<WindowRect> {
        let geometry = snapshot.game_geometry.as_ref()?;
        let output = self
            .target_output
            .as_ref()
            .or(self.surface_output.as_ref())?;
        let info = self.output_state.info(output)?;
        game_anchor_on_output(
            geometry,
            info.name.as_deref()?,
            output_geometry(&info)?.rect,
        )
    }

    pub(super) fn update_output_geometry(&mut self) -> bool {
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
    pub(super) fn record_output_environment(&self, output: &wl_output::WlOutput) {
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

    pub(super) fn create_surface(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        self.update_output_geometry();
        if let Some(snapshot) = self
            .snapshot
            .as_ref()
            .filter(|snapshot| !is_hidden(snapshot, self.presentation_focus))
        {
            self.margin = panel_margin(
                snapshot,
                self.requested_size,
                self.output_logical_size,
                self.game_anchor(snapshot),
            );
        }
        let input_passthrough = self
            .snapshot
            .as_deref()
            .is_some_and(|snapshot| is_hidden(snapshot, self.presentation_focus));
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

    pub(super) fn drop_surface(&mut self) {
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

    pub(super) fn reset_pointer_state(&mut self) {
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

    pub(super) fn set_input_passthrough(&mut self, passthrough: bool) {
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

    pub(super) fn configure_surface(&mut self) {
        let Some(surface) = &self.surface else {
            return;
        };
        let Some(config) = &mut self.surface_config else {
            return;
        };
        (config.width, config.height) = physical_size(self.logical_size, self.render_scale);
        surface.configure(&self.device, config);
    }

    pub(super) fn render(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
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
                if !is_hidden(snapshot, self.presentation_focus) && !is_degraded(snapshot) {
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
                        if !is_hidden(snapshot, self.presentation_focus) {
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

    pub(super) fn request_frame(&self, qh: &QueueHandle<Self>) {
        if let Some(layer) = &self.layer {
            layer.wl_surface().frame(qh, layer.wl_surface().clone());
            layer.commit();
        }
    }

    pub(super) fn acquire_frame(&mut self) -> Result<Option<wgpu::SurfaceTexture>, String> {
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

    pub(super) fn apply_actions(&mut self, actions: OverlayActions) {
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
            let position_command = if self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| should_switch_drag_to_manual_snap(snapshot))
            {
                UiCommand::SetOverlayPositionAndManualSnap {
                    x: self.margin.0,
                    y: self.margin.1,
                }
            } else {
                UiCommand::SetOverlayPosition {
                    x: self.margin.0,
                    y: self.margin.1,
                }
            };
            self.send_command(position_command);
        }
    }

    pub(super) fn send_command(&self, command: UiCommand) {
        if self.command_tx.send(command).is_ok() {
            (self.app_repaint)();
        }
    }
}

pub(super) fn repaint_timeout(deadline: Option<Instant>, now: Instant) -> Option<Timespec> {
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

pub(super) fn create_layer(
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

pub(super) fn create_viewport(
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

pub(super) fn create_wgpu_surface(
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

pub(super) fn surface_settings(
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
