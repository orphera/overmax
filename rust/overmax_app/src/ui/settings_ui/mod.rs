//! Settings editor: overlay scale/opacity and capture/matcher intervals.

use crate::ui::dialog_theme::{apply_dialog_style, render_dialog_tabs, DialogTheme};
use eframe::egui::{self, Frame, Margin, RichText, ViewportClass};
use overmax_data::{
    diff_settings, load_merged_settings_from_paths, normalize_settings, save_user_settings_to_path,
    SettingsPaths,
};

use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

mod advanced;
mod general;
mod recommend;
mod varchive;

use advanced::advanced_tab;
use general::general_tab;
use recommend::recommend_tab;
use varchive::varchive_tab;

pub struct SettingsUiContext {
    #[cfg(all(target_os = "linux", feature = "portal-capture"))]
    pub portal: overmax_capture::PortalControl,
    pub root: Arc<std::path::PathBuf>,
    pub paths: Arc<overmax_data::AppPaths>,
    pub current_steam_id: String,
    pub sync_open: Arc<AtomicBool>,
    pub debug_open: Arc<AtomicBool>,
    pub scan_pending: Arc<AtomicBool>,
    pub sync_steam_id: Arc<Mutex<String>>,
    pub fetch_tx: Sender<(String, String, i32)>,
    pub steam_users:
        Arc<Mutex<std::collections::HashMap<String, crate::system::steam_session::SteamUser>>>,
    /// IPC 서버 실제 바인딩 포트 (None = 비활성)
    pub ipc_bound_port: crate::system::ipc_server::BoundPortSlot,
}

pub fn render_settings_form(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    apply_dialog_style(ui.ctx());

    ui.add_space(DialogTheme::GAP_SM);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Overmax")
                .color(DialogTheme::TEXT_ACCENT)
                .size(DialogTheme::FONT_TITLE)
                .strong(),
        );
        ui.label(
            RichText::new(crate::t!("settings-title"))
                .color(DialogTheme::TEXT_PRIMARY)
                .size(DialogTheme::FONT_TITLE)
                .strong(),
        );
    });

    ui.add_space(DialogTheme::GAP_LG);

    let id = ui.id().with("settings_tab");
    let mut active = ui.data(|d| d.get_temp::<usize>(id).unwrap_or(0));
    render_dialog_tabs(
        ui,
        "settings_tabs",
        &[
            crate::t!("settings-tab-general"),
            crate::t!("settings-tab-recommend"),
            crate::t!("settings-tab-varchive"),
            crate::t!("settings-tab-advanced"),
        ],
        &mut active,
    );
    ui.data_mut(|d| d.insert_temp(id, active));

    ui.add_space(DialogTheme::GAP_XL);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match active {
            0 => general_tab(ui, draft),
            1 => recommend_tab(ui, draft),
            2 => varchive_tab(ui, draft, ctx),
            _ => advanced_tab(ui, draft, ctx),
        });
}

pub fn render_settings_deferred(
    ui: &mut egui::Ui,
    class: ViewportClass,
    title: &str,
    draft: &mut Value,
    settings_ctx: &SettingsUiContext,
) {
    if class == ViewportClass::EmbeddedWindow {
        egui::Window::new(title).show(ui.ctx(), |ui| render_settings_form(ui, draft, settings_ctx));
    } else {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(DialogTheme::BG_WINDOW)
                    .inner_margin(Margin::same(DialogTheme::PANEL_PADDING as i8)),
            )
            .show(ui, |ui| render_settings_form(ui, draft, settings_ctx));
    }
}

/// Applies normalize + delta save vs `base`, reloads merged into `merged_out` using `SettingsPaths`.
pub fn save_settings_to_paths(
    paths: &SettingsPaths,
    defaults: &Value,
    base: &Value,
    draft: &mut Value,
    merged_out: &mut Value,
) -> Result<(), String> {
    normalize_settings(draft);
    let diff = diff_settings(base, draft);
    save_user_settings_to_path(&paths.settings_user_json, &diff).map_err(|e| e.to_string())?;
    *merged_out = load_merged_settings_from_paths(paths, defaults.clone());
    Ok(())
}

/// Applies normalize + delta save vs `base`, reloads merged into `merged_out`.
pub fn save_settings_to_disk(
    root: &Path,
    defaults: &Value,
    base: &Value,
    draft: &mut Value,
    merged_out: &mut Value,
) -> Result<(), String> {
    save_settings_to_paths(
        &SettingsPaths::in_dir(root),
        defaults,
        base,
        draft,
        merged_out,
    )
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

fn root_map_mut(draft: &mut Value) -> &mut Map<String, Value> {
    if !draft.is_object() {
        *draft = Value::Object(Map::new());
    }
    draft
        .as_object_mut()
        .expect("draft must be verified as a JSON Object")
}

fn object_section_mut<'a>(draft: &'a mut Value, section: &str) -> &'a mut Map<String, Value> {
    if !draft.is_object() {
        *draft = Value::Object(Map::new());
    }
    let root = draft
        .as_object_mut()
        .expect("draft must be verified as a JSON Object");
    let entry = root
        .entry(section)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry
        .as_object_mut()
        .expect("entry must be verified as a JSON Object")
}

fn user_entry_mut<'a>(draft: &'a mut Value, steam_id: &str) -> &'a mut Map<String, Value> {
    let varchive = object_section_mut(draft, "varchive");
    let user_map_value = varchive
        .entry("user_map")
        .or_insert_with(|| Value::Object(Map::new()));
    if !user_map_value.is_object() {
        *user_map_value = Value::Object(Map::new());
    }
    let user_map = user_map_value
        .as_object_mut()
        .expect("user_map must be verified as a JSON Object");
    let entry = user_map
        .entry(steam_id)
        .or_insert_with(|| json!({"v_id": "", "account_path": ""}));
    if let Some(v_id) = entry.as_str().map(str::to_string) {
        *entry = json!({"v_id": v_id, "account_path": ""});
    }
    if !entry.is_object() {
        *entry = json!({"v_id": "", "account_path": ""});
    }
    entry
        .as_object_mut()
        .expect("entry must be verified as a JSON Object")
}

#[cfg(test)]
mod tests;
