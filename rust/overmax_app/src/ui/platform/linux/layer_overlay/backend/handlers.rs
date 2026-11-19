//! smithay-client-toolkit handler and wayland Dispatch impls for `Backend`.

use super::*;

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
                state.next_toplevel_identity += 1;
                state.foreign_toplevels.push(ForeignToplevel {
                    handle: toplevel,
                    state: ForeignToplevelState {
                        pending: ForeignToplevelSnapshot {
                            identity: state.next_toplevel_identity,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
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
