use crate::t;
use crate::ui::overlay_recommend_ui::PatternTabInfo;
use crate::ui::overlay_theme::Theme;
use eframe::egui::{self, Color32, FontId, Rect, Vec2};
use overmax_core::{GameSessionState, RecordValue};

#[derive(Clone, Debug, PartialEq)]
pub struct ToastMessage {
    pub text: String,
    pub is_success: bool,
    pub expires_at: std::time::Instant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextSegment {
    pub text: String,
    pub color: Color32,
}

pub struct OverlayHeaderDetail<'a> {
    state: &'a GameSessionState,
    pattern_tabs: &'a [PatternTabInfo],
    is_result: bool,
    session_initial_record: Option<RecordValue>,
    scale: f32,
    height: Option<f32>,
    toast: Option<&'a ToastMessage>,
}

impl<'a> OverlayHeaderDetail<'a> {
    pub fn new(state: &'a GameSessionState, pattern_tabs: &'a [PatternTabInfo]) -> Self {
        Self {
            state,
            pattern_tabs,
            is_result: false,
            session_initial_record: None,
            scale: 1.0,
            height: None,
            toast: None,
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

    pub fn toast(mut self, toast: Option<&'a ToastMessage>) -> Self {
        self.toast = toast;
        self
    }

    // 1. RecordValue 관련 정보 (플레이 기록 정보) 세그먼트들을 수집
    pub fn collect_record_segments(&self) -> Vec<TextSegment> {
        let mut segments = Vec::new();

        if self.is_result {
            if let Some(ctx) = &self.state.context {
                // 결과창 정보 추출
                // 현재 판정 (Theme::OK)
                segments.push(TextSegment {
                    text: format!("{:.2}%", ctx.rate),
                    color: Theme::OK,
                });

                // 현재 MaxCombo (P/M - Accent)
                if ctx.is_max_combo {
                    let sym = if ctx.rate >= 100.0 { "P" } else { "M" };
                    segments.push(TextSegment {
                        text: format!(" {}", sym),
                        color: Theme::TEXT_ACCENT,
                    });
                }

                // 기록 비교 (이전 기록 + diff)
                let mut prev_rate = None;
                let mut prev_mc = false;

                if let Some((r, mc)) = self.session_initial_record {
                    if r >= overmax_engine::detector::play_state::MIN_VALID_RATE {
                        prev_rate = Some(r);
                        prev_mc = mc;
                    }
                }

                if let Some(p_rate) = prev_rate {
                    let mc_symbol = if prev_mc {
                        if p_rate >= 100.0 {
                            " P"
                        } else {
                            " M"
                        }
                    } else {
                        ""
                    };
                    segments.push(TextSegment {
                        text: format!(" ({:.2}%{}", p_rate, mc_symbol),
                        color: Theme::TEXT_SECONDARY,
                    });

                    // 상승치(diff) 추가 (빨간색 - Theme::RED)
                    if ctx.rate > p_rate {
                        let diff = ctx.rate - p_rate;
                        segments.push(TextSegment {
                            text: format!(" +{:.2}%", diff),
                            color: Theme::RED,
                        });
                    }

                    segments.push(TextSegment {
                        text: ")".to_string(),
                        color: Theme::TEXT_SECONDARY,
                    });
                } else {
                    segments.push(TextSegment {
                        text: " (NEW!)".to_string(),
                        color: Theme::RED,
                    });
                }
            }
        } else {
            // 일반 선곡창 상태
            if let Some(ctx) = &self.state.context {
                if ctx.rate > 0.0 {
                    segments.push(TextSegment {
                        text: format!("{:.2}%", ctx.rate),
                        color: Theme::OK,
                    });
                    if ctx.is_max_combo {
                        let sym = if ctx.rate >= 100.0 { "P" } else { "M" };
                        segments.push(TextSegment {
                            text: format!(" {}", sym),
                            color: Theme::TEXT_ACCENT,
                        });
                    }
                }
            }
        }

        segments
    }

    // 2. 패턴 메타 정보 목록을 수집
    pub fn collect_pattern_meta(&self) -> Vec<String> {
        let mut meta_list = Vec::new();

        let Some(ctx) = &self.state.context else {
            return meta_list;
        };
        let diff = &ctx.diff;
        let Some(pattern) = self
            .pattern_tabs
            .iter()
            .find(|pattern| &pattern.diff == diff)
        else {
            return meta_list;
        };

        if !pattern.gold.is_none() {
            meta_list.push(t!("overlay-gold-rec-meta", val = t!(gold = pattern.gold)));
        }
        if !pattern.assist_key.is_none() {
            meta_list.push(t!(
                "overlay-assist-key-meta",
                val = t!(assist = pattern.assist_key)
            ));
        }
        if pattern.keypart {
            meta_list.push(t!("overlay-keypart-focused").to_string());
        }
        // `note`는 한국어 커뮤니티 서열표 원문 텍스트(예: "무리배치", "개인차")이므로,
        // 번역이 제공되지 않고 영미/일본 플레이어에게는 가독성이 없어 노이즈가 되므로 한국어(Ko) 환경에서만 표출합니다.
        if crate::ui::i18n::current_locale() == crate::ui::i18n::Locale::Ko
            && !pattern.note.is_empty()
        {
            meta_list.push(pattern.note.clone());
        }

        meta_list
    }

    // 3. 수집된 세그먼트와 메타 목록을 레이아웃에 맞춰 그림
    fn draw_layout(
        &self,
        ui: &mut egui::Ui,
        row_rect: Rect,
        record_segments: &[TextSegment],
        meta_list: &[String],
    ) {
        let font_meta = FontId::proportional(9.0 * self.scale);
        let mut total_width = 0.0f32;

        // record_segments 가로 폭 미리 계산
        let mut record_widths = Vec::new();
        for seg in record_segments {
            let w = ui
                .painter()
                .layout_no_wrap(seg.text.clone(), font_meta.clone(), seg.color)
                .size()
                .x;
            total_width += w;
            record_widths.push(w);
        }

        // separator 가로 폭
        let has_records = !record_segments.is_empty();
        let has_meta = !meta_list.is_empty();
        let use_separator = has_records && has_meta;
        if use_separator {
            total_width += 14.0 * self.scale;
        }

        // meta_list 가로 폭
        let meta_str = meta_list.join(" | ");
        if has_meta {
            let meta_width = ui
                .painter()
                .layout_no_wrap(meta_str.clone(), font_meta.clone(), Theme::TEXT_SECONDARY)
                .size()
                .x;
            total_width += meta_width;
        }

        // 시작 X 좌표 (가운데 정렬)
        let mut current_x = row_rect.left() + (row_rect.width() - total_width) / 2.0;
        let center_y = row_rect.center().y;

        // record_segments 그리기
        for (i, seg) in record_segments.iter().enumerate() {
            ui.painter().text(
                egui::pos2(current_x, center_y),
                egui::Align2::LEFT_CENTER,
                &seg.text,
                font_meta.clone(),
                seg.color,
            );
            current_x += record_widths[i];
        }

        // separator 그리기
        if use_separator {
            current_x += 4.0 * self.scale;
            ui.painter().text(
                egui::pos2(current_x, center_y),
                egui::Align2::LEFT_CENTER,
                "|",
                FontId::proportional(10.0 * self.scale),
                Theme::TEXT_MUTED,
            );
            current_x += 10.0 * self.scale;
        }

        // meta_list 그리기
        if has_meta {
            ui.painter().text(
                egui::pos2(current_x, center_y),
                egui::Align2::LEFT_CENTER,
                &meta_str,
                font_meta,
                Theme::TEXT_SECONDARY,
            );
        }
    }
}

impl<'a> egui::Widget for OverlayHeaderDetail<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let height = self.height.unwrap_or(14.0 * self.scale);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), height),
            egui::Sense::hover(),
        );

        if ui.is_rect_visible(rect) {
            if let Some(toast) = self.toast {
                let font_toast = FontId::proportional(10.0 * self.scale);
                let text_color = if toast.is_success {
                    Theme::OK
                } else {
                    Theme::RED
                };
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &toast.text,
                    font_toast,
                    text_color,
                );
            } else {
                let mut record_segments = self.collect_record_segments();
                let meta_list = self.collect_pattern_meta();

                // [가드레일] 결과가 존재하지 않으면 "기록 없음" 이라고 Theme::OK로 표시
                if record_segments.is_empty() {
                    record_segments.push(TextSegment {
                        text: crate::t!("overlay-no-record").to_string(),
                        color: Theme::OK,
                    });
                }

                self.draw_layout(ui, rect, &record_segments, &meta_list);
            }
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::OverlayHeaderDetail;
    use crate::ui::overlay_recommend_ui::PatternTabInfo;
    use overmax_core::{GameSessionState, PlayContext};

    #[test]
    fn collects_empty_meta_and_records_correctly() {
        let state = GameSessionState::detecting();
        let sub = OverlayHeaderDetail::new(&state, &[]);
        let records = sub.collect_record_segments();
        let meta = sub.collect_pattern_meta();

        assert!(records.is_empty());
        assert!(meta.is_empty());
    }

    #[test]
    fn collects_sheet_meta_correctly() {
        use crate::ui::i18n::{set_locale, Locale, LOCALE_TEST_LOCK};

        let _guard = LOCALE_TEST_LOCK.lock().unwrap();

        let state = GameSessionState {
            scene: overmax_core::SceneType::Unknown,
            context: Some(PlayContext {
                song_id: 1,
                mode: overmax_core::Mode::B5,
                diff: overmax_core::Difficulty::SC,
                rate: 0.0,
                is_max_combo: false,
            }),
            is_stable: true,
            is_fullscreen: false,
        };
        let patterns = vec![PatternTabInfo {
            diff: overmax_core::Difficulty::SC,
            level: Some(12),
            floor_name: Some("12.3".into()),
            gold: overmax_data::community::sheet_meta::GoldMeta::Random,
            note: "개인차".into(),
            assist_key: overmax_data::community::sheet_meta::AssistMeta::Used,
            keypart: false,
        }];

        let sub = OverlayHeaderDetail::new(&state, &patterns);

        // Ko locale: note is displayed
        set_locale(Locale::Ko);
        assert_eq!(
            sub.collect_pattern_meta(),
            vec!["황배:랜덤", "보조:사용", "개인차"]
        );

        // En locale: note is omitted because it is unlocalized Korean text
        set_locale(Locale::En);
        assert_eq!(
            sub.collect_pattern_meta(),
            vec!["Recommendation:Random", "Assist Key:Used"]
        );

        // Ja locale: note is omitted
        set_locale(Locale::Ja);
        assert_eq!(
            sub.collect_pattern_meta(),
            vec!["推奨:ランダム", "アシスト:使用"]
        );

        // Reset to Ko
        set_locale(Locale::Ko);
    }
}
