use crate::overlay_recommend_ui::{
    avg_rate_text, draw_diff_tabs, draw_recommendations, pattern_count_text, PatternTabInfo,
};
use crate::overlay_theme::Theme;
use crate::ui_command::UiCommand;
use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Frame, Label, Layout, Margin, Rect, RichText, Sense, Vec2,
};
use overmax_core::GameSessionState;
use overmax_data::RecommendResult;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const BASE_WIDTH: f32 = 360.0;
pub const BASE_HEIGHT: f32 = 380.0;

#[derive(Default, Clone, Copy)]
pub struct OverlayActions {
    pub start_drag: bool,
    pub restore_game_focus: bool,
    pub command: Option<UiCommand>,
}

struct Px {
    scale: f32,
}

impl Px {
    fn new(scale: f32) -> Self {
        Self { scale }
    }
    fn panel_margin(&self) -> f32 { 8.0 * self.scale }
    fn panel_gap(&self) -> f32 { 1.5 * self.scale }
    fn header_radius(&self) -> f32 { 10.0 * self.scale }
    fn header_margin_x(&self) -> f32 { 12.0 * self.scale }
    fn header_margin_y(&self) -> f32 { 8.0 * self.scale }
    fn header_row_gap(&self) -> f32 { 8.0 * self.scale }
    fn header_meta_gap(&self) -> f32 { 4.0 * self.scale }
    fn status_dot(&self) -> f32 { 7.0 * self.scale }
    fn mode_badge_w(&self) -> f32 { 28.0 * self.scale }
    fn mode_badge_h(&self) -> f32 { 22.0 * self.scale }
    fn settings_btn(&self) -> f32 { 24.0 * self.scale }
    fn body_gap(&self) -> f32 { 6.0 * self.scale }
    fn footer_margin_x(&self) -> f32 { 10.0 * self.scale }
    fn footer_margin_y(&self) -> f32 { 5.0 * self.scale }
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
                fonts.font_data.insert(
                    name.to_string(),
                    std::sync::Arc::new(font_data),
                );
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
}

pub fn draw_overlay_panel(
    ui: &mut egui::Ui,
    props: &OverlayProps,
) -> OverlayActions {
    // 레이아웃 경고(노란 선) 강제 비활성화
    #[cfg(debug_assertions)]
    {
        ui.ctx().style_mut(|s| {
            s.debug.show_expand_width = false;
            s.debug.show_expand_height = false;
        });
        ui.ctx().set_debug_on_hover(false);
    }

    let px = Px::new(props.scale);
    let mut actions = OverlayActions::default();
    Frame::new()
        .fill(Theme::PANEL_BG)
        .corner_radius(CornerRadius::same((14.0 * props.scale) as u8))
        .inner_margin(Margin::same(px.panel_margin() as i8))
        .stroke(egui::Stroke::new(1.0, Theme::PANEL_STROKE))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            draw_header(
                ui,
                props.state,
                props.song_label,
                props.pattern_tabs,
                &props.settings_open,
                &mut actions,
                &px,
            );
            ui.add_space(px.panel_gap());
            draw_body(ui, props.state, props.pattern_tabs, props.recommendations, &px);
            ui.add_space(px.panel_gap());
            draw_footer(
                ui,
                props.recommendations,
                &props.sync_open,
                &mut actions,
                &px,
            );
        });

    actions
}

fn draw_header(
    ui: &mut egui::Ui,
    state: &GameSessionState,
    song_label: &str,
    pattern_tabs: &[PatternTabInfo],
    settings_open: &Arc<AtomicBool>,
    actions: &mut OverlayActions,
    px: &Px,
) {
    let mut settings_button_rect = None;
    let header = Frame::new()
        .fill(Theme::HEADER_BG)
        .corner_radius(CornerRadius::same(px.header_radius() as u8))
        .inner_margin(Margin::symmetric(px.header_margin_x() as i8, px.header_margin_y() as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = px.header_row_gap();
                draw_status_lamp(ui, state.is_stable, px);
                draw_mode_badge(ui, state.context.as_ref().map(|ctx| ctx.mode.as_str()), px);
                ui.add(
                    Label::new(
                        RichText::new(song_label)
                            .color(Theme::TEXT_PRIMARY)
                            .font(FontId::proportional(14.0 * px.scale))
                            .strong(),
                    )
                    .selectable(false),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().button_padding = Vec2::ZERO;
                    let text = RichText::new("⚙")
                        .color(Theme::TEXT_PRIMARY)
                        .font(FontId::proportional(15.0 * px.scale));
                    let btn = Button::new(text)
                        .fill(Theme::SECTION_BG)
                        .corner_radius(CornerRadius::same((6.0 * px.scale) as u8))
                        .wrap();
                    let response = ui.add_sized(Vec2::splat(px.settings_btn()), btn.sense(Sense::click())).on_hover_text("설정");
                    settings_button_rect = Some(response.rect);
                    if response.clicked() {
                        settings_open.store(true, Ordering::Relaxed);
                        actions.command = Some(UiCommand::OpenSettings);
                    }
                });
            });
            let mut rate = 0.0;
            let mut is_perfect = false;
            let mut is_max_combo = false;
            let mut has_badge = false;
            if let Some(ctx) = &state.context {
                rate = ctx.rate;
                is_perfect = rate >= 100.0;
                is_max_combo = ctx.is_max_combo;
                has_badge = rate > 0.0 || is_max_combo;
            }

            ui.add_space(px.header_meta_gap());

            let text_str = meta_text(state, pattern_tabs);

            if has_badge {
                let scale = px.scale;
                let mut total_width = 0.0;

                let mut has_rate = false;
                let rate_text = if rate > 0.0 {
                    let s = format!("{:.2}%", rate);
                    total_width += (s.len() as f32 * 5.2 + 8.0) * scale;
                    has_rate = true;
                    Some(s)
                } else {
                    None
                };

                let combo_text = if is_max_combo {
                    let s = if is_perfect { "P" } else { "M" };
                    if has_rate {
                        total_width += 3.0 * scale;
                    }
                    total_width += (s.len() as f32 * 5.2 + 8.0) * scale;
                    Some(s)
                } else {
                    None
                };

                total_width += 10.0 * scale;

                let font_meta = FontId::proportional(10.0 * scale);
                let galley_meta = ui.painter().layout_no_wrap(text_str.clone(), font_meta, Theme::TEXT_ACCENT);
                total_width += galley_meta.size().x;

                let parent_width = ui.available_width();
                let start_space = ((parent_width - total_width) / 2.0).max(0.0);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.spacing_mut().interact_size.y = 0.0;

                    ui.add_space(start_space);

                    if let Some(r_txt) = &rate_text {
                        draw_mini_badge(
                            ui,
                            r_txt,
                            Theme::TAB_INACTIVE_BG,
                            Theme::OK,
                            scale,
                        );
                    }

                    if let Some(c_txt) = &combo_text {
                        if has_rate {
                            ui.add_space(3.0 * scale);
                        }
                        draw_mini_badge(
                            ui,
                            c_txt,
                            Theme::TAB_INACTIVE_BG,
                            Theme::TEXT_ACCENT,
                            scale,
                        );
                    }

                    ui.add_space(4.0 * scale);
                    ui.label(
                        RichText::new("|")
                            .color(Theme::TEXT_MUTED)
                            .font(FontId::proportional(10.0 * scale)),
                    );
                    ui.add_space(4.0 * scale);

                    ui.add(
                        Label::new(
                            RichText::new(text_str)
                                .color(Theme::TEXT_ACCENT)
                                .font(FontId::proportional(10.0 * scale))
                                .strong(),
                        )
                        .selectable(false),
                    );
                });
            } else {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(text_str)
                                .color(Theme::TEXT_ACCENT)
                                .font(FontId::proportional(10.0 * px.scale))
                                .strong(),
                        )
                        .selectable(false),
                    );
                });
            }
        });

    let drag_rect = drag_rect_excluding_button(header.response.rect, settings_button_rect);
    let drag_response = ui.interact(
        drag_rect,
        ui.id().with("overlay_header_drag"),
        Sense::drag(),
    );
    if drag_response.drag_started() {
        actions.start_drag = true;
    }
    if drag_response.drag_stopped() {
        actions.restore_game_focus = true;
    }
}

fn draw_mini_badge(
    ui: &mut egui::Ui,
    text: &str,
    bg_color: Color32,
    text_color: Color32,
    scale: f32,
) {
    let width = (text.len() as f32 * 5.2 + 8.0) * scale;
    let height = 13.0 * scale;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());

    ui.painter().rect_filled(
        rect,
        CornerRadius::same((3.0 * scale) as u8),
        bg_color,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(9.0 * scale),
        text_color,
    );
}

fn drag_rect_excluding_button(header: Rect, button: Option<Rect>) -> Rect {
    let Some(button) = button else {
        return header;
    };
    let mut rect = header;
    rect.max.x = (button.min.x - 4.0).max(rect.min.x);
    rect
}

fn draw_status_lamp(ui: &mut egui::Ui, stable: bool, px: &Px) {
    let color = if stable { Theme::OK } else { Theme::WARN };
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(px.status_dot()), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5 * px.scale, color);
}

pub(crate) fn mode_color(mode: &str) -> Color32 {
    match mode {
        "4B" => Color32::from_rgb(0x2D, 0x4F, 0x55),
        "5B" => Color32::from_rgb(0x44, 0xA9, 0xC6),
        "6B" => Color32::from_rgb(0xED, 0x94, 0x30),
        "8B" => Color32::from_rgb(0x1D, 0x14, 0x31),
        _ => Color32::from_rgb(0x6A, 0x4D, 0x3D),
    }
}

fn draw_mode_badge(ui: &mut egui::Ui, mode: Option<&str>, px: &Px) {
    let text = mode.unwrap_or("—");
    let color = mode.map_or(Color32::from_rgb(0x6A, 0x4D, 0x3D), mode_color);

    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(px.mode_badge_w(), px.mode_badge_h()),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, CornerRadius::same((3.0 * px.scale) as u8), color);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.0 * px.scale),
        Theme::TEXT_PRIMARY,
    );
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
        draw_diff_tabs(ui, state.context.as_ref().map(|ctx| ctx.diff.as_str()), pattern_tabs, px.scale);
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
        .inner_margin(Margin::symmetric(px.footer_margin_x() as i8, px.footer_margin_y() as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(2.0 * px.scale, 1.0 * px.scale);
                
                let sync_btn = Button::new(RichText::new("sync").font(FontId::proportional(11.0 * px.scale)))
                    .wrap();
                if ui.add_sized(Vec2::new(42.0 * px.scale, 18.0 * px.scale), sync_btn).clicked() {
                    sync_open.store(true, Ordering::Relaxed);
                    actions.command = Some(UiCommand::OpenSync);
                }
                ui.label(RichText::new("유사 구간 평균").color(Theme::TEXT_SECONDARY).font(FontId::proportional(11.0 * px.scale)));
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

fn meta_text(state: &GameSessionState, pattern_tabs: &[PatternTabInfo]) -> String {
    let Some(ctx) = &state.context else {
        return "—".to_string();
    };
    let diff = &ctx.diff;
    let Some(pattern) = pattern_tabs.iter().find(|pattern| &pattern.diff == diff) else {
        return "—".to_string();
    };
    let mut badges = Vec::new();
    if !pattern.gold.is_empty() {
        badges.push(format!("황배:{}", pattern.gold));
    }
    if !pattern.assist_key.is_empty() {
        badges.push(format!("보조:{}", pattern.assist_key));
    }
    if !pattern.note.is_empty() {
        badges.push(pattern.note.clone());
    }
    if badges.is_empty() {
        "—".to_string()
    } else {
        badges.join(" | ")
    }
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
    use super::{diff_color, install_cjk_fonts, meta_text};
    use crate::overlay_recommend_ui::PatternTabInfo;
    use eframe::egui::{Color32, Context};
    use overmax_core::{GameSessionState, PlayContext};

    #[test]
    fn formats_empty_meta_like_pyqt_header() {
        assert_eq!(meta_text(&GameSessionState::detecting(), &[]), "—");
    }

    #[test]
    fn formats_sheet_meta_like_pyqt_header() {
        let state = GameSessionState {
            context: Some(PlayContext {
                song_id: 1,
                mode: "5B".into(),
                diff: "SC".into(),
                rate: 0.0,
                is_max_combo: false,
            }),
            is_stable: true,
        };
        let patterns = vec![PatternTabInfo {
            diff: "SC".into(),
            level: Some(12),
            floor_name: Some("12.3".into()),
            gold: "O".into(),
            note: "개인차".into(),
            assist_key: "Y".into(),
        }];

        assert_eq!(meta_text(&state, &patterns), "황배:O | 보조:Y | 개인차");
    }

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
}
