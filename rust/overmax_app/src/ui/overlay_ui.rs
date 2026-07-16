use crate::ui::components::{LitePanel, OverlayHeader};
use crate::ui::overlay_recommend_ui::{
    avg_rate_text, draw_diff_tabs, draw_recommendations, pattern_count_text, PatternTabInfo,
};
use crate::ui::overlay_theme::Theme;
use crate::ui::ui_command::UiCommand;
use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Frame, Layout, Margin, Rect, RichText, Vec2,
};
use overmax_core::GameSessionState;
use overmax_data::RecommendResult;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const BASE_WIDTH: f32 = 360.0;
pub const BASE_HEIGHT: f32 = 380.0;
pub const LITE_BASE_HEIGHT: f32 = 60.0;

#[derive(Default, Clone, Copy)]
pub struct OverlayActions {
    pub start_drag: bool,
    pub restore_game_focus: bool,
    pub command: Option<UiCommand>,
    pub response_rect: Option<Rect>,
    pub drag_delta: Option<Vec2>,
}

pub(crate) struct Px {
    pub(crate) scale: f32,
}

impl Px {
    pub(crate) fn new(scale: f32) -> Self {
        Self { scale }
    }
    pub(crate) fn panel_margin(&self) -> f32 {
        8.0 * self.scale
    }
    pub(crate) fn panel_gap(&self) -> f32 {
        1.5 * self.scale
    }
    pub(crate) fn header_radius(&self) -> f32 {
        10.0 * self.scale
    }
    pub(crate) fn header_margin_x(&self) -> f32 {
        12.0 * self.scale
    }
    pub(crate) fn header_margin_y(&self) -> f32 {
        8.0 * self.scale
    }
    pub(crate) fn header_row_gap(&self) -> f32 {
        8.0 * self.scale
    }
    pub(crate) fn header_meta_gap(&self) -> f32 {
        4.0 * self.scale
    }
    pub(crate) fn mode_badge_w(&self) -> f32 {
        24.0 * self.scale
    }
    pub(crate) fn mode_badge_h(&self) -> f32 {
        18.0 * self.scale
    }
    pub(crate) fn settings_btn(&self) -> f32 {
        24.0 * self.scale
    }
    pub(crate) fn body_gap(&self) -> f32 {
        6.0 * self.scale
    }
    pub(crate) fn footer_margin_x(&self) -> f32 {
        10.0 * self.scale
    }
    pub(crate) fn footer_margin_y(&self) -> f32 {
        5.0 * self.scale
    }
}

pub fn install_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    let font_names = [
        ("malgun", "malgun.ttf"),
        ("msgothic", "msgothic.ttc"),
        ("msyh", "msyh.ttc"),
        ("meiryo", "meiryo.ttc"),
        ("gulim", "gulim.ttc"),
    ];

    let font_dirs = get_platform_font_dirs();
    let mut loaded_fonts = Vec::new();

    for (name, filename) in font_names {
        for dir in &font_dirs {
            let path = dir.join(filename);
            if let Ok(bytes) = std::fs::read(&path) {
                let mut font_data = FontData::from_owned(bytes);
                if filename.ends_with(".ttc") {
                    font_data.index = 0;
                }
                fonts
                    .font_data
                    .insert(name.to_string(), std::sync::Arc::new(font_data));
                loaded_fonts.push(name.to_string());
                break; // Found this font, move to the next name
            }
        }
    }

    if loaded_fonts.is_empty() {
        return;
    }

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let family_fonts = fonts.families.entry(family).or_default();
        for name in &loaded_fonts {
            family_fonts.push(name.clone());
        }
    }

    ctx.set_fonts(fonts);
}

fn get_platform_font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // 1. System Font Folder (usually C:\Windows\Fonts)
        if let Ok(windir) = std::env::var("SystemRoot") {
            dirs.push(std::path::PathBuf::from(windir).join("Fonts"));
        } else if let Ok(windir) = std::env::var("WINDIR") {
            dirs.push(std::path::PathBuf::from(windir).join("Fonts"));
        } else {
            dirs.push(std::path::PathBuf::from(r"C:\Windows\Fonts"));
        }

        // 2. User Font Folder (introduced in Windows 10)
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            dirs.push(std::path::PathBuf::from(localappdata).join(r"Microsoft\Windows\Fonts"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        dirs.push(std::path::PathBuf::from("/usr/share/fonts"));
        dirs.push(std::path::PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(std::path::PathBuf::from(home).join(".local/share/fonts"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        dirs.push(std::path::PathBuf::from("/Library/Fonts"));
        dirs.push(std::path::PathBuf::from("/System/Library/Fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(std::path::PathBuf::from(home).join("Library/Fonts"));
        }
    }

    dirs
}

pub struct OverlayProps<'a> {
    pub state: &'a GameSessionState,
    pub song_label: &'a str,
    pub pattern_tabs: &'a [PatternTabInfo],
    pub recommendations: &'a RecommendResult,
    pub settings_open: Arc<AtomicBool>,
    pub sync_open: Arc<AtomicBool>,
    pub scale: f32,
    pub varchive_upload_needed: bool,
    pub varchive_account_configured: bool,
    pub lite_mode: bool,
    pub is_snap_manual: bool,
    pub record_manager: &'a overmax_data::RecordManager,
    pub session_initial_record: Option<overmax_data::RecordValue>,
    pub toast: Option<&'a crate::ui::components::ToastMessage>,
}

pub fn draw_overlay_panel(ui: &mut egui::Ui, props: &OverlayProps) -> OverlayActions {
    // 레이아웃 경고(노란 선) 강제 비활성화
    #[cfg(debug_assertions)]
    {
        ui.ctx().style_mut(|s| {
            s.debug.show_expand_width = false;
            s.debug.show_expand_height = false;
        });
        ui.ctx().set_debug_on_hover(false);
    }

    if props.lite_mode {
        return LitePanel::show(ui, props);
    }

    let px = Px::new(props.scale);
    let mut actions = OverlayActions::default();
    let response = Frame::new()
        .fill(Theme::PANEL_BG)
        .corner_radius(CornerRadius::same((14.0 * props.scale) as u8))
        .inner_margin(Margin::same(px.panel_margin() as i8))
        .stroke(egui::Stroke::new(1.0, Theme::PANEL_STROKE))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            OverlayHeader::new(
                props.state,
                props.song_label,
                props.pattern_tabs,
                &props.settings_open,
                &px,
            )
            .varchive_upload_needed(props.varchive_upload_needed)
            .varchive_account_configured(props.varchive_account_configured)
            .is_snap_manual(props.is_snap_manual)
            .session_initial_record(props.session_initial_record)
            .toast(props.toast)
            .show(ui, &mut actions);
            ui.add_space(px.panel_gap());
            draw_body(
                ui,
                props.state,
                props.pattern_tabs,
                props.recommendations,
                &px,
            );
            ui.add_space(px.panel_gap());
            draw_footer(
                ui,
                props.recommendations,
                &props.sync_open,
                &mut actions,
                &px,
            );
        });

    actions.response_rect = Some(response.response.rect);
    actions
}

fn draw_body(
    ui: &mut egui::Ui,
    state: &GameSessionState,
    pattern_tabs: &[PatternTabInfo],
    recommendations: &RecommendResult,
    px: &Px,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = px.body_gap();
        draw_diff_tabs(
            ui,
            state.context.as_ref().map(|ctx| ctx.diff.as_str()),
            pattern_tabs,
            px.scale,
        );
        draw_recommendations(ui, state, recommendations, px.scale);
    });
}

fn draw_footer(
    ui: &mut egui::Ui,
    recommendations: &RecommendResult,
    sync_open: &Arc<AtomicBool>,
    actions: &mut OverlayActions,
    px: &Px,
) {
    Frame::new()
        .fill(Theme::SECTION_BG)
        .corner_radius(CornerRadius::same((8.0 * px.scale) as u8))
        .inner_margin(Margin::symmetric(
            px.footer_margin_x() as i8,
            px.footer_margin_y() as i8,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(2.0 * px.scale, 1.0 * px.scale);

                let sync_btn =
                    Button::new(RichText::new("sync").font(FontId::proportional(11.0 * px.scale)))
                        .wrap();
                if ui
                    .add_sized(Vec2::new(42.0 * px.scale, 18.0 * px.scale), sync_btn)
                    .clicked()
                {
                    sync_open.store(true, Ordering::Relaxed);
                    actions.command = Some(UiCommand::OpenSync);
                }
                ui.label(
                    RichText::new("유사 구간 평균")
                        .color(Theme::TEXT_SECONDARY)
                        .font(FontId::proportional(11.0 * px.scale)),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(pattern_count_text(recommendations))
                            .color(Theme::TEXT_SECONDARY)
                            .font(FontId::proportional(11.0 * px.scale)),
                    );
                    ui.label(
                        RichText::new(avg_rate_text(recommendations))
                            .color(Theme::TEXT_SECONDARY)
                            .font(FontId::proportional(11.0 * px.scale))
                            .strong(),
                    );
                });
            });
        });
}

pub(crate) fn diff_color(diff: &str) -> Color32 {
    match diff {
        "NM" => Color32::from_rgb(0x4A, 0x90, 0xD9),
        "HD" => Color32::from_rgb(0xF5, 0xA6, 0x23),
        "MX" => Color32::from_rgb(0xD0, 0x02, 0x1B),
        "SC" => Color32::from_rgb(0x9B, 0x59, 0xB6),
        _ => Color32::WHITE,
    }
}

#[cfg(test)]
mod tests {
    use super::{diff_color, install_cjk_fonts};
    use crate::ui::overlay_recommend_ui::PatternTabInfo;
    use eframe::egui::{self, Color32, Context};
    use overmax_core::GameSessionState;

    #[test]
    fn uses_existing_diff_colors() {
        assert_eq!(diff_color("SC"), Color32::from_rgb(0x9B, 0x59, 0xB6));
    }

    #[test]
    fn finds_and_installs_cjk_fonts_without_panic() {
        let ctx = Context::default();
        install_cjk_fonts(&ctx);
        // If it reaches here without panicking, the logic is sound.
    }

    #[test]
    fn test_header_height_constancy() {
        let ctx = Context::default();

        let scales = vec![0.8, 1.0, 1.2, 1.25, 1.5, 1.75, 2.0];
        let state_detecting = GameSessionState::detecting();

        let state_no_badge = GameSessionState {
            scene: overmax_core::SceneType::Unknown,
            context: Some(overmax_core::PlayContext {
                song_id: 1,
                mode: "5B".into(),
                diff: "SC".into(),
                rate: 0.0,
                is_max_combo: false,
            }),
            is_stable: true,
            is_fullscreen: false,
        };

        let state_normal_badge = GameSessionState {
            scene: overmax_core::SceneType::Unknown,
            context: Some(overmax_core::PlayContext {
                song_id: 1,
                mode: "5B".into(),
                diff: "SC".into(),
                rate: 99.50,
                is_max_combo: true,
            }),
            is_stable: true,
            is_fullscreen: false,
        };

        let state_perfect_badge = GameSessionState {
            scene: overmax_core::SceneType::Unknown,
            context: Some(overmax_core::PlayContext {
                song_id: 1,
                mode: "5B".into(),
                diff: "SC".into(),
                rate: 100.00,
                is_max_combo: true,
            }),
            is_stable: true,
            is_fullscreen: false,
        };

        let pattern_tabs = vec![PatternTabInfo {
            diff: "SC".into(),
            level: Some(12),
            floor_name: Some("12.3".into()),
            gold: "O".into(),
            note: "개인차".into(),
            assist_key: "Y".into(),
            keypart: false,
        }];

        for scale in scales {
            let mut h_detecting = 0.0;
            let mut h_no_badge = 0.0;
            let mut h_normal = 0.0;
            let mut h_perfect = 0.0;

            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let px = super::Px::new(scale);
                    let settings_open =
                        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let mut actions = super::OverlayActions::default();

                    let start_y = ui.cursor().top();
                    crate::ui::components::OverlayHeader::new(
                        &state_detecting,
                        "Test Song Name",
                        &pattern_tabs,
                        &settings_open,
                        &px,
                    )
                    .varchive_upload_needed(false)
                    .varchive_account_configured(false)
                    .is_snap_manual(true)
                    .session_initial_record(None)
                    .show(ui, &mut actions);
                    h_detecting = ui.cursor().top() - start_y;
                });
            });

            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let px = super::Px::new(scale);
                    let settings_open =
                        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let mut actions = super::OverlayActions::default();

                    let start_y = ui.cursor().top();
                    crate::ui::components::OverlayHeader::new(
                        &state_no_badge,
                        "Test Song Name",
                        &pattern_tabs,
                        &settings_open,
                        &px,
                    )
                    .varchive_upload_needed(false)
                    .varchive_account_configured(false)
                    .is_snap_manual(true)
                    .session_initial_record(None)
                    .show(ui, &mut actions);
                    h_no_badge = ui.cursor().top() - start_y;
                });
            });

            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let px = super::Px::new(scale);
                    let settings_open =
                        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let mut actions = super::OverlayActions::default();

                    let start_y = ui.cursor().top();
                    crate::ui::components::OverlayHeader::new(
                        &state_normal_badge,
                        "Test Song Name",
                        &pattern_tabs,
                        &settings_open,
                        &px,
                    )
                    .varchive_upload_needed(false)
                    .varchive_account_configured(false)
                    .is_snap_manual(true)
                    .session_initial_record(None)
                    .show(ui, &mut actions);
                    h_normal = ui.cursor().top() - start_y;
                });
            });

            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let px = super::Px::new(scale);
                    let settings_open =
                        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let mut actions = super::OverlayActions::default();

                    let start_y = ui.cursor().top();
                    crate::ui::components::OverlayHeader::new(
                        &state_perfect_badge,
                        "Test Song Name",
                        &pattern_tabs,
                        &settings_open,
                        &px,
                    )
                    .varchive_upload_needed(false)
                    .varchive_account_configured(false)
                    .is_snap_manual(true)
                    .session_initial_record(None)
                    .show(ui, &mut actions);
                    h_perfect = ui.cursor().top() - start_y;
                });
            });

            println!(
                "Scale: {:.2} -> detecting: {}, no_badge: {}, normal: {}, perfect: {}",
                scale, h_detecting, h_no_badge, h_normal, h_perfect
            );

            assert_eq!(
                h_detecting, h_no_badge,
                "Height mismatch at scale {:.2} between detecting and no_badge",
                scale
            );
            assert_eq!(
                h_no_badge, h_normal,
                "Height mismatch at scale {:.2} between no_badge and normal",
                scale
            );
            assert_eq!(
                h_normal, h_perfect,
                "Height mismatch at scale {:.2} between normal and perfect",
                scale
            );
        }
    }
}
