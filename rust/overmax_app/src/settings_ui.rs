//! Settings editor: overlay scale/opacity and capture/matcher intervals.

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, RichText, Slider, Stroke, ViewportClass,
};
use overmax_data::{diff_settings, load_merged_settings, normalize_settings, save_user_settings};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn render_settings_form(ui: &mut egui::Ui, draft: &mut Value) {
    apply_window_style(ui.ctx());
    ui.heading(RichText::new("Overmax 설정").color(Theme::TEXT).strong());
    ui.label(RichText::new("오버레이 표시와 인식 주기를 조정합니다.").color(Theme::MUTED));
    ui.add_space(14.0);
    section_frame(ui, "오버레이", |ui| overlay_section(ui, draft));
    ui.add_space(10.0);
    section_frame(ui, "처리 주기", |ui| intervals_section(ui, draft));
}

fn overlay_section(ui: &mut egui::Ui, draft: &mut Value) {
    let Some(Value::Object(overlay)) = draft.get_mut("overlay") else {
        return;
    };
    let mut scale = overlay.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
    if ui
        .add(Slider::new(&mut scale, 0.75..=1.5).text("크기"))
        .changed()
    {
        overlay.insert("scale".into(), serde_json::json!(scale));
    }
    let mut opacity = overlay
        .get("base_opacity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.8);
    if ui
        .add(Slider::new(&mut opacity, 0.1..=1.0).text("기본 투명도"))
        .changed()
    {
        overlay.insert("base_opacity".into(), serde_json::json!(opacity));
    }
}

fn intervals_section(ui: &mut egui::Ui, draft: &mut Value) {
    interval_row(
        ui,
        draft,
        "window_tracker",
        "poll_interval_sec",
        "게임 창 추적",
    );
    interval_row(ui, draft, "screen_capture", "ocr_interval_sec", "OCR");
    interval_row(
        ui,
        draft,
        "jacket_matcher",
        "match_interval_sec",
        "자켓 매칭",
    );
}

fn interval_row(ui: &mut egui::Ui, draft: &mut Value, section: &str, key: &str, label: &str) {
    let Some(Value::Object(sec)) = draft.get_mut(section) else {
        return;
    };
    let mut v = sec.get(key).and_then(|x| x.as_f64()).unwrap_or(0.5);
    if ui
        .add(Slider::new(&mut v, 0.05..=5.0).text(format!("{label} (초)")))
        .changed()
    {
        sec.insert(key.to_string(), serde_json::json!(v));
    }
}

pub fn render_settings_deferred(
    ctx: &egui::Context,
    class: ViewportClass,
    title: &str,
    draft: &mut Value,
) {
    if class == ViewportClass::Embedded {
        egui::Window::new(title).show(ctx, |ui| render_settings_form(ui, draft));
    } else {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(Theme::BG).inner_margin(Margin::same(18)))
            .show(ctx, |ui| render_settings_form(ui, draft));
    }
}

/// Applies normalize + delta save vs `base`, reloads merged into `merged_out`.
pub fn save_settings_to_disk(
    root: &Path,
    defaults: &Value,
    base: &Value,
    draft: &mut Value,
    merged_out: &mut Value,
) -> Result<(), String> {
    normalize_settings(draft);
    let diff = diff_settings(base, draft);
    save_user_settings(root, &diff).map_err(|e| e.to_string())?;
    *merged_out = load_merged_settings(root, defaults.clone());
    Ok(())
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
    }
}

struct Theme;

impl Theme {
    const BG: Color32 = Color32::from_rgb(17, 20, 27);
    const CARD: Color32 = Color32::from_rgb(27, 32, 42);
    const STROKE: Color32 = Color32::from_rgb(55, 64, 80);
    const TEXT: Color32 = Color32::from_rgb(235, 239, 247);
    const MUTED: Color32 = Color32::from_rgb(145, 154, 170);
}

fn apply_window_style(ctx: &egui::Context) {
    ctx.style_mut(|s| {
        s.visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 45, 58);
        s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(50, 59, 76);
        s.visuals.selection.bg_fill = Color32::from_rgb(70, 105, 150);
    });
}

fn section_frame(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(Theme::CARD)
        .stroke(Stroke::new(1.0, Theme::STROKE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.label(RichText::new(title).color(Theme::TEXT).strong());
            ui.add_space(8.0);
            add(ui);
        });
}

#[cfg(test)]
mod tests {
    use super::save_settings_to_disk;
    use overmax_data::load_merged_settings;
    use serde_json::json;
    use std::fs;

    #[test]
    fn save_user_roundtrip_matches_python_delta_policy() {
        let root =
            std::env::temp_dir().join(format!("overmax-app-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("settings.json"),
            r#"{"overlay":{"scale":1.0,"base_opacity":0.8},"window_tracker":{"poll_interval_sec":0.5}}"#,
        )
        .unwrap();
        fs::write(root.join("settings.user.json"), "{}").unwrap();

        let defaults = json!({
            "overlay": {"scale": 1.0, "base_opacity": 0.8},
            "window_tracker": {"poll_interval_sec": 0.5}
        });
        let base = overmax_data::load_base_settings(&root, defaults.clone());
        let mut merged = load_merged_settings(&root, defaults.clone());
        let mut draft = merged.clone();
        draft["overlay"]["base_opacity"] = json!(0.55);

        save_settings_to_disk(&root, &defaults, &base, &mut draft, &mut merged).unwrap();

        let reloaded = load_merged_settings(&root, defaults);
        assert_eq!(reloaded["overlay"]["base_opacity"], json!(0.55));
        assert_eq!(reloaded["overlay"]["scale"], json!(1.0));

        let user_text = fs::read_to_string(root.join("settings.user.json")).unwrap();
        assert!(user_text.contains("base_opacity"));

        let _ = fs::remove_dir_all(root);
    }
}
