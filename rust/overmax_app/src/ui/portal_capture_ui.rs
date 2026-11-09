use egui::{ColorImage, TextureHandle, TextureOptions};
use overmax_engine::capture::portal::connection::{Confirmation, ConnectionStatus, PortalControl};
use std::time::Duration;

pub(super) fn render(ui: &mut egui::Ui, control: &PortalControl) {
    let view = control.view();
    ui.label(crate::t!("portal-native-hint"));
    let status = match &view.status {
        ConnectionStatus::Idle => crate::t!("portal-native-idle"),
        ConnectionStatus::Selecting => crate::t!("portal-native-selecting"),
        ConnectionStatus::Preview => crate::t!("portal-native-preview"),
        ConnectionStatus::WaitingForFrame => crate::t!("portal-native-waiting"),
        ConnectionStatus::Connected => crate::t!("portal-native-connected"),
        ConnectionStatus::Paused => crate::t!("portal-native-paused"),
        ConnectionStatus::XWayland => crate::t!("portal-native-xwayland"),
        ConnectionStatus::TargetUnavailable => crate::t!("portal-native-target-unavailable"),
        ConnectionStatus::TargetChanged => crate::t!("portal-native-target-changed"),
        ConnectionStatus::Stopped => crate::t!("portal-native-stopped"),
        ConnectionStatus::Failed(error) => error.as_str(),
    };
    ui.label(status);
    let texture_key = ui.id().with("native_wayland_preview");
    if let (Some(preview), Some(ticket)) = (&view.preview, view.confirmation) {
        let texture = ui
            .ctx()
            .data_mut(|data| {
                data.get_temp::<(Confirmation, TextureHandle)>(texture_key)
                    .filter(|(previous, _)| *previous == ticket)
                    .map(|(_, texture)| texture)
            })
            .unwrap_or_else(|| {
                let texture = ui.ctx().load_texture(
                    "native Wayland preview",
                    ColorImage::from_rgba_unmultiplied(
                        [preview.width, preview.height],
                        &preview.rgba,
                    ),
                    TextureOptions::LINEAR,
                );
                ui.ctx()
                    .data_mut(|data| data.insert_temp(texture_key, (ticket, texture.clone())));
                texture
            });
        let scale = (ui.available_width() / preview.width as f32).min(1.0);
        ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(
            preview.width as f32 * scale,
            preview.height as f32 * scale,
        )));
        if ui.button(crate::t!("portal-native-confirm")).clicked() {
            control.confirm(ticket);
        }
    } else {
        ui.ctx()
            .data_mut(|data| data.remove::<(Confirmation, TextureHandle)>(texture_key));
    }
    ui.horizontal(|ui| {
        let selecting = view.status == ConnectionStatus::Selecting;
        if ui
            .add_enabled(
                !selecting,
                egui::Button::new(crate::t!("portal-native-connect")),
            )
            .clicked()
        {
            control.connect();
        }
        if matches!(
            view.status,
            ConnectionStatus::Selecting
                | ConnectionStatus::Preview
                | ConnectionStatus::WaitingForFrame
                | ConnectionStatus::Connected
                | ConnectionStatus::Paused
        ) && ui.button(crate::t!("portal-native-disconnect")).clicked()
        {
            control.disconnect();
        }
    });
    ui.ctx().request_repaint_after(Duration::from_millis(200));
    ui.add_space(super::dialog_theme::DialogTheme::GAP_MD);
}
