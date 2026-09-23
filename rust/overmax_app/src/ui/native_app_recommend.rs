use crate::ui::debug_ui;
use crate::ui::native_app::NativeApp;
use crate::ui::overlay_recommend_ui::PatternTabInfo;
use overmax_core::{Difficulty, GameSessionState};
use overmax_data::{RecommendContext, RecommendResult, RecordSource};

impl NativeApp {
    pub(crate) fn drain_detection_results(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        while let Ok(output) = self.detection_rx.try_recv() {
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            let mut output = output;
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            if let (Some(telemetry), Some(delivery)) =
                (&self.runtime_telemetry, &mut output.delivery_telemetry)
            {
                telemetry.record_output_drained(delivery);
            }
            if let Some(snap) = output.telemetry_snapshot {
                self.last_telemetry_snapshot = Some(snap);
            }
            self.last_detection_output = Some(output.clone());
            if let Ok(mut r) = self.game_rect.lock() {
                *r = output.game_rect;
            }
            self.window_snapshot = output.window_snapshot;
            self.capture_fatal = output.capture_fatal.clone();

            if output.state.scene.is_result() {
                if let Some(ctx_val) = &output.state.context {
                    if self.session_initial_record.is_none() {
                        let song_id = ctx_val.song_id;
                        let rate_map = self.record_manager.get_rate_map(&[song_id]);
                        if let Some(&(r, mc)) = rate_map.get(&(song_id, ctx_val.mode, ctx_val.diff))
                        {
                            self.session_initial_record = Some((r, mc));
                        } else {
                            self.session_initial_record = Some((0.0, false));
                        }
                    }
                }
            } else {
                self.session_initial_record = None;
            }

            self.confidence = output.confidence;
            if self.session != output.state {
                changed = true;
                self.session = output.state.clone();
            }

            // ── IPC 이벤트 발행 (관찰자 — 파이프라인/DB 경로 무변경) ──
            // Publish current scene status; the IPC cache retains only stable song context.
            crate::system::ipc_server::update_latest_state(output.state.clone());
            self.publish_ipc_events(&output.state);

            if let Some(event) = output.event {
                // PB 판정: 결과창 진입 시점의 기존 기록(session_initial_record) 대비 향상 여부.
                // 스냅샷이 없으면(진입 직후 첫 이벤트 등) 보수적으로 false.
                let is_pb = self
                    .session_initial_record
                    .is_some_and(|(initial_rate, _)| event.rate > initial_rate);
                if self.record_manager.handle_verified_play(&event) {
                    debug_ui::push_log(
                        &self.debug_state.log_lines,
                        self.max_log_lines(),
                        format!(
                            "[Main] 기록 갱신: {}, {}, {}, {:.2}%, MaxCombo: {}",
                            event.song_id, event.mode, event.diff, event.rate, event.is_max_combo
                        ),
                    );
                    changed = true;
                }
                // play_verified는 DB 커밋 성공 여부와 무관하게 확정 이벤트로 통지
                let meta = self.song_meta_for(event.song_id, event.mode, event.diff);
                self.ipc_publisher
                    .publish(crate::system::ipc_server::IpcEvent::PlayVerified {
                        song_id: event.song_id,
                        mode: event.mode.as_str().to_string(),
                        diff: event.diff.as_str().to_string(),
                        rate: event.rate,
                        is_max_combo: event.is_max_combo,
                        is_pb,
                        meta,
                    });
            }
        }

        if changed {
            self.refresh_overlay_data();
            if let Ok(recs) = serde_json::to_value(&self.recommendations) {
                crate::system::ipc_server::update_latest_recommendations(recs);
            }
            self.log_overlay_state();
            ctx.request_repaint();
        }
    }

    /// 감지 상태 → IPC 이벤트 변환 발행 (compare-and-publish 중복 억제 포함).
    /// `scene`/`context` 조합이 직전 발행과 동일하면 아무것도 보내지 않는다.
    fn publish_ipc_events(&mut self, state: &GameSessionState) {
        use crate::system::ipc_server::IpcEvent;

        let scene_key = format!("{:?}", state.scene);
        if self.last_ipc_scene_key.as_deref() != Some(scene_key.as_str()) {
            self.ipc_publisher.publish(IpcEvent::SceneDetected {
                scene: scene_key.clone(),
            });
            self.last_ipc_scene_key = Some(scene_key);
        }

        if !state.is_stable {
            return;
        }
        let Some(ctx) = &state.context else {
            return;
        };
        let ctx_key = (
            ctx.song_id,
            ctx.mode,
            ctx.diff,
            ctx.rate.to_bits(),
            ctx.is_max_combo,
        );
        if self.last_ipc_context_key.as_ref() != Some(&ctx_key) {
            let meta = self.song_meta_for(ctx.song_id, ctx.mode, ctx.diff);
            self.ipc_publisher.publish(IpcEvent::SongDetected {
                song_id: ctx.song_id,
                mode: ctx.mode.as_str().to_string(),
                diff: ctx.diff.as_str().to_string(),
                rate: ctx.rate,
                is_max_combo: ctx.is_max_combo,
                meta,
            });
            self.last_ipc_context_key = Some(ctx_key);
        }
    }

    /// 곡 메타 정보 조회 (오버레이가 이미 사용하는 VArchiveDB 값 전달 — 새 결합 없음).
    /// 조회 실패 시 빈 meta (필드 생략) — 클라이언트는 song_id로 자체 해석 가능.
    fn song_meta_for(
        &self,
        song_id: i32,
        mode: overmax_core::Mode,
        diff: overmax_core::Difficulty,
    ) -> crate::system::ipc_server::SongMeta {
        let Some(song) = self.varchive_db.search_by_id(song_id) else {
            return Default::default();
        };
        let floor_name = song
            .get_pattern(mode, diff)
            .and_then(|p| p.floor_name.as_deref().map(String::from));
        crate::system::ipc_server::SongMeta {
            title: Some(song.name.to_string()),
            floor_name,
        }
    }

    pub(crate) fn current_song_label(&self) -> String {
        let Some(ctx) = &self.session.context else {
            return crate::t!("rec-select-song").into();
        };
        let Some(song) = self.varchive_db.search_by_id(ctx.song_id) else {
            return format!("Song #{}", ctx.song_id);
        };
        song.name.to_string()
    }

    pub(crate) fn refresh_overlay_data(&mut self) {
        self.pattern_tabs = self.pattern_tabs_for_state(&self.session);
        self.recommendations = self.recommend_for_state(&self.session);
    }

    fn recommend_for_state(&self, state: &GameSessionState) -> RecommendResult {
        let Some(ctx) = &state.context else {
            return RecommendResult::empty();
        };
        let rec_settings = self.settings.get_merged().recommend();
        let smart_recommend = rec_settings.smart_recommend;
        let target_rate = rec_settings.target_rate;
        let strategy = overmax_data::RecommendStrategy::from_smart_flag(smart_recommend);
        let rec_ctx = RecommendContext {
            song_id: ctx.song_id,
            button_mode: ctx.mode,
            difficulty: ctx.diff,
            floor_range: 0.0,
            max_results: 6,
            same_mode_only: true,
            v_id: self.varchive_user_id(),
            strategy,
            target_rate,
        };

        let provider_settings = self.settings.get_merged().recommend_provider();
        if provider_settings.enabled {
            if let Some(url) = &provider_settings.url {
                let clean_url = url.trim();
                if !clean_url.is_empty() {
                    let provider_name = provider_settings
                        .name
                        .as_deref()
                        .filter(|n| !n.is_empty())
                        .unwrap_or("external_provider");
                    let cache_dir = self
                        .root
                        .join("cache")
                        .join("recommend_provider")
                        .join(provider_name);

                    let manifest =
                        crate::system::recommend_provider_fetch::get_cached_manifest(clean_url);

                    let reader = overmax_data::ProviderCacheReader::new(
                        provider_name,
                        provider_name,
                        cache_dir.clone(),
                        manifest.vary,
                        std::time::Duration::from_secs(manifest.ttl_sec),
                    );

                    let clean_url_clone = clean_url.to_string();
                    let rec_ctx_clone = rec_ctx.clone();
                    let cache_dir_clone = cache_dir.clone();
                    let cache_key = reader.cache_key(&rec_ctx);
                    let cache_path = cache_dir_clone.join(format!("{}.json", cache_key));

                    let should_fetch = match std::fs::metadata(&cache_path) {
                        Ok(meta) => meta
                            .modified()
                            .ok()
                            .and_then(|m| m.elapsed().ok())
                            .map(|e| e.as_secs() > 10)
                            .unwrap_or(true),
                        Err(_) => true,
                    };

                    if should_fetch {
                        std::thread::spawn(move || {
                            let manifest =
                                crate::system::recommend_provider_fetch::fetch_manifest_blocking(
                                    &clean_url_clone,
                                );
                            let _ =
                                crate::system::recommend_provider_fetch::fetch_recommend_blocking(
                                    &clean_url_clone,
                                    &manifest,
                                    &rec_ctx_clone,
                                    &cache_path,
                                );
                        });
                    }

                    let composite = (*self.recommender).clone().with_provider(reader);
                    return composite.recommend_panel(&rec_ctx).as_legacy_result();
                }
            }
        }

        self.recommender
            .recommend_panel(&rec_ctx)
            .as_legacy_result()
    }

    fn pattern_tabs_for_state(&self, state: &GameSessionState) -> Vec<PatternTabInfo> {
        let Some(ctx) = &state.context else {
            return Vec::new();
        };
        let Some(song) = self.varchive_db.search_by_id(ctx.song_id) else {
            return Vec::new();
        };
        let m = ctx.mode;
        let patterns = &song.patterns[m as usize];
        Difficulty::ALL
            .iter()
            .filter_map(|&d| {
                let pattern = patterns[d as usize].as_ref()?;
                let meta = self.sheet_meta.get(&song.title, m, d);
                Some(PatternTabInfo {
                    diff: d,
                    level: pattern.level,
                    floor_name: pattern.floor_name.as_ref().map(|s| s.to_string()),
                    gold: meta.gold,
                    note: meta.note,
                    assist_key: meta.assist_key,
                    keypart: meta.keypart,
                })
            })
            .collect()
    }

    fn log_overlay_state(&self) {
        debug_ui::push_log(
            &self.debug_state.log_lines,
            self.max_log_lines(),
            format!(
                "[UI] overlay state <- {} / recs={}",
                self.session,
                self.recommendations.entries.len()
            ),
        );
    }
}
