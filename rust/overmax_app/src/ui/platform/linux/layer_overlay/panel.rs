//! Overlay panel size, margins, visibility decisions and degraded drawing.

use super::*;

pub(super) fn overlay_props(snapshot: &LinuxOverlaySnapshot) -> OverlayProps<'_> {
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
        varchive_upload_progress: snapshot.varchive_upload_progress,
        varchive_account_configured: snapshot.varchive_account_configured,
        lite_mode: snapshot.lite_mode,
        is_snap_manual: uses_manual_position(snapshot),
        record_manager: snapshot.record_manager.as_ref(),
        session_initial_record: snapshot.session_initial_record,
        toast: snapshot.toast.as_ref(),
    }
}

pub(super) fn draw_degraded(ui: &mut egui::Ui, snapshot: &LinuxOverlaySnapshot) -> OverlayActions {
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

pub(super) fn degraded_message(snapshot: &LinuxOverlaySnapshot) -> &str {
    if let Some(error) = snapshot.capture_fatal.as_deref() {
        return error;
    }
    "DJMAX RESPECT V window not found"
}

pub(super) fn is_degraded(snapshot: &LinuxOverlaySnapshot) -> bool {
    snapshot.capture_fatal.is_some() || capture_presentation(snapshot).is_none()
}

pub(super) fn capture_presentation(snapshot: &LinuxOverlaySnapshot) -> Option<(bool, bool)> {
    snapshot
        .window_snapshot
        .map(|window| (window.foreground, window.fullscreen))
        .or_else(|| {
            snapshot.portal_presentation.map(|value| {
                (
                    value.focus == FocusState::Focused,
                    value.fullscreen.unwrap_or(false),
                )
            })
        })
}

pub(super) fn is_hidden(snapshot: &LinuxOverlaySnapshot, presentation_focus: Option<bool>) -> bool {
    if snapshot.overlay_visible_override == Some(false) || presentation_focus == Some(false) {
        return true;
    }
    snapshot.capture_fatal.is_none()
        && capture_presentation(snapshot).is_some_and(|(foreground, _)| {
            !foreground
                || (!snapshot.always_visible
                    && (snapshot.state.scene == overmax_core::SceneType::Unknown
                        || snapshot.state.scene.is_ingame()))
        })
}

pub(super) fn panel_size(snapshot: Option<&LinuxOverlaySnapshot>) -> (u32, u32) {
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

pub(super) fn retained_panel_size(
    current: (u32, u32),
    snapshot: &LinuxOverlaySnapshot,
    presentation_focus: Option<bool>,
) -> (u32, u32) {
    if is_hidden(snapshot, presentation_focus) {
        current
    } else {
        panel_size(Some(snapshot))
    }
}

pub(super) fn uses_manual_position(snapshot: &LinuxOverlaySnapshot) -> bool {
    snapshot.snap == "manual"
        || capture_presentation(snapshot).is_some_and(|(_, fullscreen)| !fullscreen)
}

pub(super) fn should_switch_drag_to_manual_snap(snapshot: &LinuxOverlaySnapshot) -> bool {
    snapshot.snap != "manual"
        && capture_presentation(snapshot).is_some_and(|(_, fullscreen)| !fullscreen)
}

pub(super) fn game_anchor_on_output(
    geometry: &GameGeometry,
    output_name: &str,
    output_rect: WindowRect,
) -> Option<WindowRect> {
    (geometry.output == output_name && geometry.rect.is_valid()).then_some(WindowRect {
        left: geometry.rect.left - output_rect.left,
        top: geometry.rect.top - output_rect.top,
        width: geometry.rect.width,
        height: geometry.rect.height,
    })
}

pub(super) fn panel_margin(
    snapshot: &LinuxOverlaySnapshot,
    size: (u32, u32),
    output_logical_size: Option<(i32, i32)>,
    game_anchor: Option<WindowRect>,
) -> (i32, i32) {
    let windowed = capture_presentation(snapshot).is_some_and(|(_, fullscreen)| !fullscreen);
    if windowed && snapshot.snap != "manual" {
        if let Some(rect) = game_anchor {
            return calculate_margin(&snapshot.snap, snapshot.position, Some(rect), size);
        }
    }
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

pub(super) fn calculate_margin(
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
