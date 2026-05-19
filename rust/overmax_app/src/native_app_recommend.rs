use crate::debug_ui;
use crate::native_app::NativeApp;
use crate::overlay_recommend_ui::PatternTabInfo;
use overmax_core::GameSessionState;
use overmax_data::{RecommendResult, Recommender};

const DIFFICULTIES: [&str; 4] = ["NM", "HD", "MX", "SC"];

impl NativeApp {
    pub(crate) fn drain_detection_results(&mut self) {
        let mut changed = false;
        while let Ok(output) = self.detection_rx.try_recv() {
            self.confidence = output.confidence;
            if self.session != output.state {
                changed = true;
                self.session = output.state;
            }
        }
        if changed {
            self.refresh_overlay_data();
            self.log_overlay_state();
        }
    }

    pub(crate) fn current_song_label(&self) -> String {
        let Some(song_id) = self.session.song_id else {
            return "곡을 선택하세요".into();
        };
        let Some(song) = self.varchive_db.search_by_id(song_id as i32) else {
            return format!("Song #{song_id}");
        };
        song.name
    }

    pub(crate) fn refresh_overlay_data(&mut self) {
        self.pattern_tabs = self.pattern_tabs_for_state(&self.session);
        self.recommendations = self.recommend_for_state(&self.session);
    }

    fn recommend_for_state(&self, state: &GameSessionState) -> RecommendResult {
        if !state.is_valid() {
            return RecommendResult::empty();
        }
        let recommender = Recommender::new(self.varchive_db.as_ref(), self.record_manager.as_ref());
        recommender.recommend(
            state.song_id.unwrap_or_default() as i32,
            state.mode.as_deref().unwrap_or_default(),
            state.diff.as_deref().unwrap_or_default(),
            0.0,
            6,
            true,
        )
    }

    fn pattern_tabs_for_state(&self, state: &GameSessionState) -> Vec<PatternTabInfo> {
        let Some(song_id) = state.song_id else {
            return Vec::new();
        };
        let Some(song) = self.varchive_db.search_by_id(song_id as i32) else {
            return Vec::new();
        };
        let mode = state.mode.as_deref().unwrap_or_default();
        let Some(patterns) = song.patterns.get(mode) else {
            return Vec::new();
        };
        DIFFICULTIES
            .iter()
            .filter_map(|diff| {
                let pattern = patterns.get(*diff)?;
                let meta = self.sheet_meta.get(&song.name, mode, diff);
                Some(PatternTabInfo {
                    diff: (*diff).to_string(),
                    level: pattern.level,
                    floor_name: pattern.floor_name.clone(),
                    gold: meta.gold,
                    note: meta.note,
                    assist_key: meta.assist_key,
                })
            })
            .collect()
    }

    fn log_overlay_state(&self) {
        debug_ui::push_log(
            &self.log_lines,
            self.max_log_lines(),
            format!(
                "[UI] overlay state <- {} / recs={}",
                self.session,
                self.recommendations.entries.len()
            ),
        );
    }
}
