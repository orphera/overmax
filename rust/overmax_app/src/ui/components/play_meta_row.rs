use crate::ui::overlay_recommend_ui::PatternTabInfo;
use crate::ui::overlay_theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, FontId, Rect, Vec2};
use overmax_core::{GameSessionState, RecordValue};

pub struct PlayMetaRow<'a> {
    state: &'a GameSessionState,
    pattern_tabs: &'a [PatternTabInfo],
    is_result: bool,
    session_initial_record: Option<RecordValue>,
    scale: f32,
    height: Option<f32>,
}

impl<'a> PlayMetaRow<'a> {
    pub fn new(state: &'a GameSessionState, pattern_tabs: &'a [PatternTabInfo]) -> Self {
        Self {
            state,
            pattern_tabs,
            is_result: false,
            session_initial_record: None,
            scale: 1.0,
            height: None,
        }
    }

    pub fn is_result(mut self, is_result: bool) -> Self {
        self.is_result = is_result;
        self
    }

    pub fn session_initial_record(mut self, record: Option<RecordValue>) -> Self {
        self.session_initial_record = record;
        self
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    fn badge_width(text: &str, scale: f32) -> f32 {
        (text.len() as f32 * 5.2 + 8.0) * scale
    }

    fn draw_meta_badge(
        &self,
        painter: &egui::Painter,
        text: &str,
        current_x: f32,
        center_y: f32,
        color: egui::Color32,
    ) -> f32 {
        let badge_w = Self::badge_width(text, self.scale);
        let badge_rect = Rect::from_center_size(
            egui::pos2(current_x + badge_w / 2.0, center_y),
            Vec2::new(badge_w, 13.0 * self.scale),
        );
        painter.rect_filled(
            badge_rect,
            CornerRadius::same((3.0 * self.scale) as u8),
            Theme::TAB_INACTIVE_BG,
        );
        painter.text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            FontId::proportional(9.0 * self.scale),
            color,
        );
        current_x + badge_w
    }

    fn meta_badges(&self) -> (Vec<(String, Color32)>, String, bool) {
        let mut badges: Vec<(String, Color32)> = Vec::new();
        let mut trailing = String::new();
        let mut separator = false;

        if self.is_result {
            if let Some(ctx) = &self.state.context {
                let (curr_rate, curr_mc, comp_str) =
                    get_result_rate_comparison(ctx, self.session_initial_record);
                badges.push((curr_rate, Theme::OK));
                if let Some(mc) = curr_mc {
                    badges.push((mc.to_string(), Theme::TEXT_ACCENT));
                }
                trailing = comp_str;
            } else {
                trailing = "—".to_string();
            }
        } else {
            let mut has_badge = false;
            if let Some(ctx) = &self.state.context {
                if ctx.rate > 0.0 {
                    badges.push((format!("{:.2}%", ctx.rate), Theme::OK));
                    has_badge = true;
                    if ctx.is_max_combo {
                        let sym = if ctx.rate >= 100.0 { "P" } else { "M" };
                        badges.push((sym.to_string(), Theme::TEXT_ACCENT));
                    }
                }
            }
            let meta = meta_text(self.state, self.pattern_tabs);
            if meta != "—" && !meta.is_empty() {
                trailing = meta;
                separator = has_badge;
            }
        }

        (badges, trailing, separator)
    }

    fn draw_meta_badge_row(
        &self,
        ui: &mut egui::Ui,
        row_rect: Rect,
        badges: &[(String, Color32)],
        trailing: &str,
        use_separator: bool,
    ) {
        let mut total_width = 0.0f32;
        for (i, (t, _)) in badges.iter().enumerate() {
            total_width += Self::badge_width(t, self.scale);
            if i + 1 < badges.len() {
                total_width += 3.0 * self.scale;
            }
        }

        let font_meta = FontId::proportional(9.0 * self.scale);
        let galley = ui.painter().layout_no_wrap(
            trailing.to_string(),
            font_meta.clone(),
            Theme::TEXT_ACCENT,
        );
        if !trailing.is_empty() {
            if !badges.is_empty() {
                total_width += if use_separator {
                    10.0 * self.scale
                } else {
                    6.0 * self.scale
                };
            }
            total_width += galley.size().x;
        }

        let mut current_x = row_rect.left() + (row_rect.width() - total_width) / 2.0;
        let center_y = row_rect.center().y;

        for (i, (t, c)) in badges.iter().enumerate() {
            current_x = self.draw_meta_badge(ui.painter(), t, current_x, center_y, *c);
            if i + 1 < badges.len() {
                current_x += 3.0 * self.scale;
            }
        }

        if !badges.is_empty() && !trailing.is_empty() {
            if use_separator {
                current_x += 4.0 * self.scale;
                ui.painter().text(
                    egui::pos2(current_x, center_y),
                    egui::Align2::LEFT_CENTER,
                    "|",
                    FontId::proportional(10.0 * self.scale),
                    Theme::TEXT_MUTED,
                );
                current_x += 2.0 * self.scale;
                current_x += 4.0 * self.scale;
            } else {
                current_x += 6.0 * self.scale;
            }
        }

        if !trailing.is_empty() {
            ui.painter().text(
                egui::pos2(current_x, center_y),
                egui::Align2::LEFT_CENTER,
                trailing,
                font_meta,
                Theme::TEXT_ACCENT,
            );
        }
    }
}

impl<'a> egui::Widget for PlayMetaRow<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let (badges, trailing, separator) = self.meta_badges();
        let height = self.height.unwrap_or(14.0 * self.scale);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), height),
            egui::Sense::hover(),
        );

        if ui.is_rect_visible(rect) {
            self.draw_meta_badge_row(ui, rect, &badges, &trailing, separator);
        }

        response
    }
}

fn get_result_rate_comparison(
    ctx: &overmax_core::PlayContext,
    session_initial_record: Option<RecordValue>,
) -> (String, Option<&'static str>, String) {
    let current_rate = ctx.rate;
    let current_rate_str = format!("{:.2}%", current_rate);
    let current_mc = if ctx.is_max_combo {
        Some(if current_rate >= 100.0 { "P" } else { "M" })
    } else {
        None
    };

    let mut prev_rate = None;
    let mut prev_mc = false;

    if let Some((r, mc)) = session_initial_record {
        if r >= overmax_engine::detector::play_state::MIN_VALID_RATE {
            prev_rate = Some(r);
            prev_mc = mc;
        }
    }

    let format_prev = |rate: f32, is_mc: bool| -> String {
        let mc_symbol = if is_mc {
            if rate >= 100.0 {
                " P"
            } else {
                " M"
            }
        } else {
            ""
        };
        format!("{:.2}%{}", rate, mc_symbol)
    };

    let comparison_str = if let Some(p_rate) = prev_rate {
        if current_rate > p_rate {
            let diff = current_rate - p_rate;
            format!("({} +{:.2}%)", format_prev(p_rate, prev_mc), diff)
        } else {
            format!("({})", format_prev(p_rate, prev_mc))
        }
    } else {
        "(NEW!)".to_string()
    };

    (current_rate_str, current_mc, comparison_str)
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

#[cfg(test)]
mod tests {
    use crate::ui::components::play_meta_row::meta_text;
    use crate::ui::overlay_recommend_ui::PatternTabInfo;
    use overmax_core::{GameSessionState, PlayContext};

    #[test]
    fn formats_empty_meta_like_pyqt_header() {
        assert_eq!(meta_text(&GameSessionState::detecting(), &[]), "—");
    }

    #[test]
    fn formats_sheet_meta_like_pyqt_header() {
        let state = GameSessionState {
            scene: overmax_core::SceneType::Unknown,
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
}