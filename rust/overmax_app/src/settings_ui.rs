//! Settings editor: overlay scale/opacity and capture/matcher intervals.

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, RichText, Slider, Stroke, TextEdit, ViewportClass,
};
use overmax_data::{diff_settings, load_merged_settings, normalize_settings, save_user_settings};
use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub struct SettingsUiContext {
    pub current_steam_id: String,
    pub sync_open: Arc<AtomicBool>,
    pub scan_pending: Arc<AtomicBool>,
    pub sync_steam_id: Arc<Mutex<String>>,
    pub fetch_tx: Sender<(String, String, i32)>,
}

pub fn render_settings_form(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    apply_window_style(ui.ctx());
    ui.heading(RichText::new("Overmax 설정").color(Theme::TEXT).strong());
    ui.add_space(10.0);
    let tab = settings_tabs(ui);
    ui.add_space(12.0);
    match tab {
        0 => ui_tab(ui, draft),
        1 => varchive_tab(ui, draft, ctx),
        _ => system_tab(ui, draft),
    }
}

fn settings_tabs(ui: &mut egui::Ui) -> usize {
    let id = ui.id().with("settings_tab");
    let mut active = ui.data(|d| d.get_temp::<usize>(id).unwrap_or(0));
    ui.horizontal(|ui| {
        for (idx, label) in ["UI", "V-Archive", "System"].iter().enumerate() {
            if ui.selectable_label(active == idx, *label).clicked() {
                active = idx;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, active));
    active
}

fn ui_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_frame(ui, "오버레이", |ui| overlay_section(ui, draft));
    ui.add_space(10.0);
    section_frame(ui, "단축키", |ui| hotkey_section(ui, draft));
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

fn hotkey_section(ui: &mut egui::Ui, draft: &mut Value) {
    let Some(Value::Object(overlay)) = draft.get_mut("overlay") else {
        return;
    };
    let mut hotkey = overlay
        .get("toggle_hotkey")
        .and_then(Value::as_str)
        .unwrap_or("F3")
        .to_string();
    ui.horizontal(|ui| {
        ui.label(RichText::new("표시/숨김").color(Theme::TEXT));
        if ui
            .add(TextEdit::singleline(&mut hotkey).desired_width(80.0))
            .changed()
        {
            overlay.insert("toggle_hotkey".into(), json!(hotkey.trim()));
        }
    });
}

fn varchive_tab(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    section_frame(ui, "계정", |ui| {
        ui.label(RichText::new(current_steam_label(ctx)).color(Theme::MUTED));
        auto_refresh_row(ui, draft);
        if ctx.current_steam_id.is_empty() {
            ui.label(RichText::new("발견된 Steam 계정이 없습니다.").color(Theme::MUTED));
            return;
        }
        steam_account_rows(ui, draft, ctx);
    });
}

fn current_steam_label(ctx: &SettingsUiContext) -> String {
    if ctx.current_steam_id.is_empty() {
        "현재 Steam: -".into()
    } else {
        format!("현재 Steam: {}", ctx.current_steam_id)
    }
}

fn auto_refresh_row(ui: &mut egui::Ui, draft: &mut Value) {
    let varchive = object_section_mut(draft, "varchive");
    let mut enabled = varchive
        .get("auto_refresh")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if ui.checkbox(&mut enabled, "시작 시 자동 갱신").changed() {
        varchive.insert("auto_refresh".into(), json!(enabled));
    }
}

fn steam_account_rows(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let entry = user_entry_mut(draft, &ctx.current_steam_id);
    text_row(ui, entry, "V-Archive ID", "v_id", 180.0);
    
    ui.horizontal(|ui| {
        ui.add_space(80.0); // Align with text_row label
        for b in [4, 5, 6, 8] {
            if ui.button(format!("{b}B")).clicked() {
                let v_id = entry.get("v_id").and_then(|v| v.as_str()).unwrap_or("");
                if !v_id.is_empty() {
                    let _ = ctx.fetch_tx.send((ctx.current_steam_id.clone(), v_id.to_string(), b));
                }
            }
        }
        if ui.button("All").clicked() {
            let v_id = entry.get("v_id").and_then(|v| v.as_str()).unwrap_or("");
            if !v_id.is_empty() {
                let _ = ctx.fetch_tx.send((ctx.current_steam_id.clone(), v_id.to_string(), 0));
            }
        }
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("account.txt").color(Theme::TEXT));
        let mut path_str = entry
            .get("account_path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if ui
            .add(TextEdit::singleline(&mut path_str).desired_width(200.0))
            .changed()
        {
            entry.insert("account_path".into(), json!(path_str.trim()));
        }
        if ui.button("찾아보기").clicked() {
            if let Some(file_path) = rfd::FileDialog::new()
                .add_filter("Text Files", &["txt"])
                .pick_file()
            {
                let path_str = file_path.to_string_lossy().to_string();
                entry.insert("account_path".into(), json!(path_str));
            }
        }
        if ui.button("동기화 후보").clicked() {
            if let Ok(mut sid) = ctx.sync_steam_id.lock() {
                *sid = ctx.current_steam_id.clone();
            }
            ctx.sync_open.store(true, Ordering::Relaxed);
            ctx.scan_pending.store(true, Ordering::Relaxed);
        }
    });
}

fn text_row(ui: &mut egui::Ui, entry: &mut Map<String, Value>, label: &str, key: &str, width: f32) {
    let mut text = entry
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(Theme::TEXT));
        if ui
            .add(TextEdit::singleline(&mut text).desired_width(width))
            .changed()
        {
            entry.insert(key.into(), json!(text.trim()));
        }
    });
}

fn system_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_frame(ui, "업데이트", |ui| update_section(ui, draft));
    ui.add_space(10.0);
    section_frame(ui, "디버그", |ui| debug_section(ui, draft));
    ui.add_space(10.0);
    section_frame(ui, "처리 주기", |ui| intervals_section(ui, draft));
}

fn update_section(ui: &mut egui::Ui, draft: &mut Value) {
    let app_update = object_section_mut(draft, "app_update");
    let mut enabled = app_update
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if ui.checkbox(&mut enabled, "자동 업데이트").changed() {
        app_update.insert("enabled".into(), json!(enabled));
    }
    ui.label(RichText::new(format!("현재 버전: {}", env!("CARGO_PKG_VERSION"))).color(Theme::TEXT));
}

fn debug_section(ui: &mut egui::Ui, draft: &mut Value) {
    let debug = object_section_mut(draft, "debug_window");
    let mut title = debug
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Overmax Debug Log")
        .to_string();
    if ui.add(TextEdit::singleline(&mut title)).changed() {
        debug.insert("title".into(), json!(title.trim()));
    }
    let mut max_lines = debug
        .get("max_lines")
        .and_then(Value::as_u64)
        .unwrap_or(500) as f64;
    if ui
        .add(Slider::new(&mut max_lines, 100.0..=2000.0).text("로그 줄 수"))
        .changed()
    {
        debug.insert("max_lines".into(), json!(max_lines.round() as u64));
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
    settings_ctx: &SettingsUiContext,
) {
    if class == ViewportClass::Embedded {
        egui::Window::new(title).show(ctx, |ui| render_settings_form(ui, draft, settings_ctx));
    } else {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(Theme::BG).inner_margin(Margin::same(18)))
            .show(ctx, |ui| render_settings_form(ui, draft, settings_ctx));
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
        ctx.request_repaint_of(ctx.parent_viewport_id());
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

fn object_section_mut<'a>(draft: &'a mut Value, section: &str) -> &'a mut Map<String, Value> {
    if !draft.is_object() {
        *draft = Value::Object(Map::new());
    }
    let root = draft.as_object_mut().expect("draft object initialized");
    let entry = root
        .entry(section)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry.as_object_mut().expect("settings section object")
}

fn user_entry_mut<'a>(draft: &'a mut Value, steam_id: &str) -> &'a mut Map<String, Value> {
    let varchive = object_section_mut(draft, "varchive");
    let user_map_value = varchive
        .entry("user_map")
        .or_insert_with(|| Value::Object(Map::new()));
    if !user_map_value.is_object() {
        *user_map_value = Value::Object(Map::new());
    }
    let user_map = user_map_value.as_object_mut().expect("user_map object");
    let entry = user_map
        .entry(steam_id)
        .or_insert_with(|| json!({"v_id": "", "account_path": ""}));
    if let Some(v_id) = entry.as_str().map(str::to_string) {
        *entry = json!({"v_id": v_id, "account_path": ""});
    }
    entry.as_object_mut().expect("user_map entry object")
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
