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
pub const LITE_BASE_HEIGHT: f32 = 60.0;

#[derive(Default, Clone, Copy)]
pub struct OverlayActions {
    pub start_drag: bool,
    pub restore_game_focus: bool,
    pub command: Option<UiCommand>,
    pub response_rect: Option<Rect>,
    pub drag_delta: Option<Vec2>,
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
    pub varchive_upload_needed: bool,
    pub varchive_account_configured: bool,
    pub lite_mode: bool,
    pub is_snap_manual: bool,
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

    if props.lite_mode {
        return draw_lite_panel(ui, props);
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
            draw_header(
                ui,
                props.state,
                props.song_label,
                props.pattern_tabs,
                &props.settings_open,
                &mut actions,
                &px,
                props.varchive_upload_needed,
                props.varchive_account_configured,
                props.is_snap_manual,
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

    actions.response_rect = Some(response.response.rect);
    actions
}

fn draw_lite_panel(
    ui: &mut egui::Ui,
    props: &OverlayProps,
) -> OverlayActions {
    let px = Px::new(props.scale);
    let mut actions = OverlayActions::default();
    
    // 라이트모드 2열 켜짐/꺼짐 시 콘텐츠 크기 변동에 의해 창 높이가 출렁이는 현상을 방지하기 위해 높이를 강제 고정
    let target_height = LITE_BASE_HEIGHT * props.scale;
    ui.set_min_height(target_height);
    ui.set_max_height(target_height);
    
    let response = Frame::new()
        .fill(Theme::PANEL_BG)
        .corner_radius(CornerRadius::same((8.0 * props.scale) as u8))
        .inner_margin(Margin::symmetric((10.0 * props.scale) as i8, (6.0 * props.scale) as i8))
        .stroke(egui::Stroke::new(1.0, Theme::PANEL_STROKE))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0 * props.scale;
            
            // 1열: 상태 표시등 + [버튼모드] [난이도] [비공식 난이도] + 곡명 + 업로드 버튼 + 설정 버튼
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0 * props.scale;
                
                draw_status_lamp(ui, props.state.is_stable, &px);
                
                if let Some(ctx) = &props.state.context {
                    // 1. Mode 뱃지
                    draw_mode_badge(ui, Some(&ctx.mode), &px);
                    
                    // 2. Diff 뱃지
                    let color = diff_color(&ctx.diff);
                    let (d_rect, _) = ui.allocate_exact_size(
                        Vec2::new(px.mode_badge_w(), px.mode_badge_h()),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(d_rect, CornerRadius::same((3.0 * props.scale) as u8), color);
                    ui.painter().text(
                        d_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &ctx.diff,
                        FontId::proportional(11.0 * props.scale),
                        Theme::TEXT_PRIMARY,
                    );
                    
                    // 3. 비공식 난이도 (또는 공식 레벨)
                    if let Some(pattern) = props.pattern_tabs.iter().find(|p| p.diff == ctx.diff) {
                        if let Some(floor) = &pattern.floor_name {
                            ui.label(
                                RichText::new(format!("★{}", floor))
                                    .color(Theme::TEXT_ACCENT)
                                    .font(FontId::proportional(12.0 * props.scale))
                                    .strong()
                            );
                        } else if let Some(level) = pattern.level {
                            ui.label(
                                RichText::new(format!("Lv.{}", level))
                                    .color(Theme::TEXT_SECONDARY)
                                    .font(FontId::proportional(11.0 * props.scale))
                            );
                        }
                    }
                }
                
                // 우측 배치: 설정/업로드 버튼만 우측 정렬로 묶음
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().button_padding = Vec2::ZERO;
                    
                    // 설정 버튼: 투명도 없이 불투명 fill을 적용해 버튼 영역 전체가 클릭 이벤트를 정확하게 받게 함
                    let text = RichText::new("⚙")
                        .color(Theme::TEXT_PRIMARY)
                        .font(FontId::proportional(14.0 * props.scale));
                    let btn = Button::new(text)
                        .fill(Theme::SECTION_BG)
                        .corner_radius(CornerRadius::same((5.0 * props.scale) as u8))
                        .wrap();
                    let response = ui.add_sized(Vec2::splat(18.0 * props.scale), btn.sense(Sense::click())).on_hover_text("설정");
                    if response.clicked() {
                        props.settings_open.store(true, Ordering::Relaxed);
                        actions.command = Some(UiCommand::OpenSettings);
                    }
                    
                    // 업로드 버튼: 마찬가지로 불투명 fill 적용
                    if props.varchive_upload_needed {
                        ui.add_space(4.0 * props.scale);
                        let upload_text = RichText::new("⬆")
                            .color(if props.varchive_account_configured { Theme::TEXT_PRIMARY } else { Theme::TEXT_MUTED })
                            .font(FontId::proportional(11.0 * props.scale));
                        
                        let upload_btn = Button::new(upload_text)
                            .fill(if props.varchive_account_configured { Theme::PRIMARY } else { Theme::SECTION_BG })
                            .corner_radius(CornerRadius::same((4.0 * props.scale) as u8))
                            .wrap();
                            
                        let btn_size = Vec2::splat(18.0 * props.scale);
                        let response_upload = ui.add_sized(btn_size, upload_btn.sense(Sense::click()));
                        let response_upload = if props.varchive_account_configured {
                            response_upload.on_hover_text("V-Archive 업로드 필요 (클릭하여 즉시 업로드)")
                        } else {
                            response_upload.on_hover_text("V-Archive 계정 연동 필요 (설정에서 account.txt 경로를 지정해주세요)")
                        };
                        
                        if response_upload.clicked() {
                            if props.varchive_account_configured {
                                actions.command = Some(UiCommand::UploadCurrentPattern);
                            }
                        }
                    }
                    
                    // [CRITICAL LAYOUT NOTE]
                    // 1. right_to_left 스코프 내부에서는 남은 available_width() 전체를 라벨 크기(title_w)로
                    //    명시적으로 할당해주어야 라벨 너비가 0으로 찌그러져 텍스트가 소멸되는 현상을 방지합니다.
                    // 2. 동시에 right_to_left 내부에 배치되는 요소는 기본 우측 정렬 대상이 되므로,
                    //    반드시 left_to_right 레이아웃으로 래핑하여 텍스트의 흐름 방향을 좌->우로 강제 리셋해 주어야
                    //    라벨 내부 텍스트가 오른쪽이 아닌 왼쪽 정렬 밀착 상태를 유지합니다.
                    let title_w = ui.available_width();
                    ui.with_layout(Layout::left_to_right(Align::Center).with_main_align(egui::Align::Min), |ui| {
                        ui.set_max_width(title_w);
                        ui.add(
                            Label::new(
                                RichText::new(props.song_label)
                                    .color(Theme::TEXT_PRIMARY)
                                    .font(FontId::proportional(13.0 * props.scale))
                                    .strong(),
                            )
                            .selectable(false)
                            .truncate(),
                        );
                    });
                });
            });
            
            // 2열: Rate + 콤보상태 + sheet_meta 정보 (황배 | 보조 | 메모 등) - 가운데 정렬
            let mut meta_parts = Vec::new();
            if let Some(ctx) = &props.state.context {
                if ctx.rate > 0.0 {
                    let mut rate_str = format!("{:.2}%", ctx.rate);
                    if ctx.is_max_combo {
                        let combo_symbol = if ctx.rate >= 100.0 { "[P]" } else { "[M]" };
                        rate_str = format!("{} {}", rate_str, combo_symbol);
                    }
                    meta_parts.push(rate_str);
                }
            }
            
            let meta_text_str = meta_text(props.state, props.pattern_tabs);
            if meta_text_str != "—" && !meta_text_str.is_empty() {
                meta_parts.push(meta_text_str);
            }
            
            let final_meta = if meta_parts.is_empty() {
                "—".to_string()
            } else {
                meta_parts.join(" | ")
            };

            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(
                    RichText::new(final_meta)
                        .color(Theme::TEXT_SECONDARY)
                        .font(FontId::proportional(10.5 * props.scale))
                );
            });
        });

    let is_snap_manual = props.is_snap_manual;

    if is_snap_manual {
        // Exclude the right settings/upload buttons area (approx 45px * scale) from the drag target
        let mut drag_rect = response.response.rect;
        drag_rect.max.x -= 45.0 * props.scale;
        
        let drag_response = ui.interact(
            drag_rect,
            ui.id().with("lite_overlay_drag"),
            Sense::drag(),
        );
        if drag_response.drag_started() {
            actions.start_drag = true;
        }
        if drag_response.dragged() {
            actions.drag_delta = Some(drag_response.drag_delta());
        }
        if drag_response.drag_stopped() {
            actions.restore_game_focus = true;
        }
    }

    actions.response_rect = Some(response.response.rect);
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
    varchive_upload_needed: bool,
    varchive_account_configured: bool,
    is_snap_manual: bool,
) {
    let mut buttons_left_x = None;
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
                    buttons_left_x = Some(response.rect.min.x);
                    if response.clicked() {
                        settings_open.store(true, Ordering::Relaxed);
                        actions.command = Some(UiCommand::OpenSettings);
                    }

                    if varchive_upload_needed {
                        ui.add_space(4.0 * px.scale);
                        let upload_text = RichText::new("⬆")
                            .color(if varchive_account_configured { Theme::TEXT_PRIMARY } else { Theme::TEXT_MUTED })
                            .font(FontId::proportional(11.0 * px.scale));
                        
                        let upload_btn = Button::new(upload_text)
                            .fill(if varchive_account_configured { Theme::PRIMARY } else { Theme::SECTION_BG })
                            .corner_radius(CornerRadius::same((4.0 * px.scale) as u8))
                            .wrap();
                            
                        let btn_size = Vec2::splat(18.0 * px.scale);
                        let response = ui.add_sized(btn_size, upload_btn.sense(Sense::click()));
                        let response = if varchive_account_configured {
                            response.on_hover_text("V-Archive 업로드 필요 (클릭하여 즉시 업로드)")
                        } else {
                            response.on_hover_text("V-Archive 계정 연동 필요 (설정에서 account.txt 경로를 지정해주세요)")
                        };
                        buttons_left_x = Some(
                            buttons_left_x
                                .map(|x| x.min(response.rect.min.x))
                                .unwrap_or(response.rect.min.x),
                        );
                        
                        if response.clicked() {
                            if varchive_account_configured {
                                actions.command = Some(UiCommand::UploadCurrentPattern);
                            }
                        }
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
                has_badge = rate > 0.0;
            }

            ui.add_space(px.header_meta_gap());

            let text_str = meta_text(state, pattern_tabs);
            let scale = px.scale;
            let second_row_height = 15.0 * scale;

            // 세로 높이를 정확히 15.0 * scale 로 고정하여 할당받음 (높이 흔들림 원천 방지)
            let (row_rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), second_row_height),
                egui::Sense::hover(),
            );

            let mut total_width = 0.0;
            let mut has_rate = false;
            let rate_text = if has_badge && rate > 0.0 {
                let s = format!("{:.2}%", rate);
                total_width += (s.len() as f32 * 5.2 + 8.0) * scale;
                has_rate = true;
                Some(s)
            } else {
                None
            };

            let combo_text = if has_badge && is_max_combo {
                let s = if is_perfect { "P" } else { "M" };
                if has_rate {
                    total_width += 3.0 * scale;
                }
                total_width += (s.len() as f32 * 5.2 + 8.0) * scale;
                Some(s)
            } else {
                None
            };

            if has_badge {
                total_width += 10.0 * scale; // 구분선 `|` 가로폭 (여백 포함)
            }

            let font_meta = FontId::proportional(10.0 * scale);
            let galley_meta = ui.painter().layout_no_wrap(text_str.clone(), font_meta.clone(), Theme::TEXT_ACCENT);
            total_width += galley_meta.size().x;

            let mut current_x = row_rect.left() + (row_rect.width() - total_width) / 2.0;
            let center_y = row_rect.center().y;

            if let Some(r_txt) = &rate_text {
                let badge_w = (r_txt.len() as f32 * 5.2 + 8.0) * scale;
                let badge_rect = Rect::from_center_size(
                    egui::pos2(current_x + badge_w / 2.0, center_y),
                    Vec2::new(badge_w, 13.0 * scale),
                );
                ui.painter().rect_filled(
                    badge_rect,
                    CornerRadius::same((3.0 * scale) as u8),
                    Theme::TAB_INACTIVE_BG,
                );
                ui.painter().text(
                    badge_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    r_txt,
                    FontId::proportional(9.0 * scale),
                    Theme::OK,
                );
                current_x += badge_w;
            }

            if let Some(c_txt) = &combo_text {
                if has_rate {
                    current_x += 3.0 * scale;
                }
                let badge_w = (c_txt.len() as f32 * 5.2 + 8.0) * scale;
                let badge_rect = Rect::from_center_size(
                    egui::pos2(current_x + badge_w / 2.0, center_y),
                    Vec2::new(badge_w, 13.0 * scale),
                );
                ui.painter().rect_filled(
                    badge_rect,
                    CornerRadius::same((3.0 * scale) as u8),
                    Theme::TAB_INACTIVE_BG,
                );
                ui.painter().text(
                    badge_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    c_txt,
                    FontId::proportional(9.0 * scale),
                    Theme::TEXT_ACCENT,
                );
                current_x += badge_w;
            }

            if has_badge {
                current_x += 4.0 * scale;
                ui.painter().text(
                    egui::pos2(current_x, center_y),
                    egui::Align2::LEFT_CENTER,
                    "|",
                    FontId::proportional(10.0 * scale),
                    Theme::TEXT_MUTED,
                );
                current_x += 2.0 * scale;
                current_x += 4.0 * scale;
            }

            ui.painter().text(
                egui::pos2(current_x, center_y),
                egui::Align2::LEFT_CENTER,
                &text_str,
                font_meta,
                Theme::TEXT_ACCENT,
            );
        });

    if is_snap_manual {
        let drag_rect = drag_rect_excluding_buttons(header.response.rect, buttons_left_x);
        let drag_response = ui.interact(
            drag_rect,
            ui.id().with("overlay_header_drag"),
            Sense::drag(),
        );
        if drag_response.drag_started() {
            actions.start_drag = true;
        }
        if drag_response.dragged() {
            actions.drag_delta = Some(drag_response.drag_delta());
        }
        if drag_response.drag_stopped() {
            actions.restore_game_focus = true;
        }
    }
}

fn drag_rect_excluding_buttons(header: Rect, buttons_left_x: Option<f32>) -> Rect {
    let Some(left_x) = buttons_left_x else {
        return header;
    };
    let mut rect = header;
    rect.max.x = (left_x - 4.0).max(rect.min.x);
    rect
}

fn draw_status_lamp(ui: &mut egui::Ui, stable: bool, px: &Px) {
    let color = if stable { Theme::OK } else { Theme::WARN };
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(px.status_dot(), px.mode_badge_h()),
        egui::Sense::hover(),
    );
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
    if pattern.keypart {
        badges.push("키파트 위주 패턴".to_string());
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
    use eframe::egui::{self, Color32, Context};
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
            is_fullscreen: false,
        };
        let patterns = vec![PatternTabInfo {
            diff: "SC".into(),
            level: Some(12),
            floor_name: Some("12.3".into()),
            gold: "O".into(),
            note: "개인차".into(),
            assist_key: "Y".into(),
            keypart: false,
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

    #[test]
    fn test_header_height_constancy() {
        let ctx = Context::default();

        let scales = vec![0.8, 1.0, 1.2, 1.25, 1.5, 1.75, 2.0];
        let state_detecting = GameSessionState::detecting();

        let state_no_badge = GameSessionState {
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
                    let settings_open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let mut actions = super::OverlayActions::default();

                    let start_y = ui.cursor().top();
                    super::draw_header(ui, &state_detecting, "Test Song Name", &pattern_tabs, &settings_open, &mut actions, &px, false, false, true);
                    h_detecting = ui.cursor().top() - start_y;
                });
            });

            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let px = super::Px::new(scale);
                    let settings_open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let mut actions = super::OverlayActions::default();

                    let start_y = ui.cursor().top();
                    super::draw_header(ui, &state_no_badge, "Test Song Name", &pattern_tabs, &settings_open, &mut actions, &px, false, false, true);
                    h_no_badge = ui.cursor().top() - start_y;
                });
            });

            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let px = super::Px::new(scale);
                    let settings_open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let mut actions = super::OverlayActions::default();

                    let start_y = ui.cursor().top();
                    super::draw_header(ui, &state_normal_badge, "Test Song Name", &pattern_tabs, &settings_open, &mut actions, &px, false, false, true);
                    h_normal = ui.cursor().top() - start_y;
                });
            });

            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let px = super::Px::new(scale);
                    let settings_open = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let mut actions = super::OverlayActions::default();

                    let start_y = ui.cursor().top();
                    super::draw_header(ui, &state_perfect_badge, "Test Song Name", &pattern_tabs, &settings_open, &mut actions, &px, false, false, true);
                    h_perfect = ui.cursor().top() - start_y;
                });
            });

            println!(
                "Scale: {:.2} -> detecting: {}, no_badge: {}, normal: {}, perfect: {}",
                scale, h_detecting, h_no_badge, h_normal, h_perfect
            );

            assert_eq!(h_detecting, h_no_badge, "Height mismatch at scale {:.2} between detecting and no_badge", scale);
            assert_eq!(h_no_badge, h_normal, "Height mismatch at scale {:.2} between no_badge and normal", scale);
            assert_eq!(h_normal, h_perfect, "Height mismatch at scale {:.2} between normal and perfect", scale);
        }
    }
}
