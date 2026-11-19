//! V-Archive 동기화: 기록 스캔·업로드·삭제·조회 요청 처리와 자동 업로드.

use overmax_core::PlayContext;
use overmax_data::SyncCandidate;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::system::steam_session;
use crate::ui::debug_ui;
use crate::ui::upload_history_ui;
use eframe::egui;
use overmax_data::config::varchive_account::{
    account_path_for_steam, is_account_configured, varchive_user_id,
};
use overmax_runtime::varchive_jobs::{UploadCacheError, UploadOutcome};

use super::NativeApp;

impl NativeApp {
    pub(crate) fn poll_delete_requests(&mut self, ctx: &egui::Context) {
        while let Ok(key) = self.sync_channels.delete_req_rx.try_recv() {
            let cand = self
                .sync_state
                .candidates
                .lock()
                .ok()
                .and_then(|g| g.iter().find(|c| c.matches_key(&key)).cloned());
            if let Some(c) = cand {
                if self
                    .record_manager
                    .delete(c.song_id, c.button_mode, c.difficulty)
                {
                    debug_ui::push_log(
                        &self.debug_state.log_lines,
                        self.max_log_lines(),
                        format!(
                            "[Sync] 로컬 기록 삭제 완료: {} ({} {})",
                            c.song_name, c.button_mode, c.difficulty
                        ),
                    );
                    self.spawn_scan(ctx.clone());
                    self.refresh_overlay_data();
                } else {
                    debug_ui::push_log(
                        &self.debug_state.log_lines,
                        self.max_log_lines(),
                        format!(
                            "[Sync] 로컬 기록 삭제 실패: {} ({} {})",
                            c.song_name, c.button_mode, c.difficulty
                        ),
                    );
                }
            }
        }
    }

    pub(crate) fn poll_scan_requests(&mut self, ctx: &egui::Context) {
        if self.ui_state.scan_pending.swap(false, Ordering::Relaxed) {
            if let Ok(mut s) = self.sync_state.status.lock() {
                *s = crate::t!("status-scanning").to_string();
            }
            self.spawn_scan(ctx.clone());
        }
    }

    pub(crate) fn poll_upload_requests(&mut self, ctx: &egui::Context) {
        while let Ok(key) = self.sync_channels.upload_req_rx.try_recv() {
            let cand = self
                .sync_state
                .candidates
                .lock()
                .ok()
                .and_then(|g| g.iter().find(|c| c.matches_key(&key)).cloned());
            if let Some(c) = cand {
                self.spawn_upload(c, false, ctx.clone());
            }
        }
    }

    pub(crate) fn drain_sync_scan(&self) {
        while let Ok(res) = self.sync_channels.sync_rx.try_recv() {
            match res {
                Ok(list) => {
                    let n = list.len();
                    if let Ok(mut g) = self.sync_state.candidates.lock() {
                        *g = list;
                    }
                    if let Ok(mut s) = self.sync_state.status.lock() {
                        *s = crate::t!("candidate-count", n = n as i64);
                    }
                }
                Err(msg) => {
                    if let Ok(mut s) = self.sync_state.status.lock() {
                        *s = msg;
                    }
                }
            }
        }
    }

    /// 업로드 알림/로그에 쓰는 곡명 (곡 DB에 없으면 `#곡ID`).
    fn upload_song_name(&self, song_id: i32) -> String {
        self.varchive_db
            .search_by_id(song_id)
            .map(|song| song.name.to_string())
            .unwrap_or_else(|| format!("#{song_id}"))
    }

    pub(crate) fn drain_upload_results(&mut self) {
        let mut refreshed = false;
        while let Ok((key, is_quick_upload, status, msg, (rate, mc, prev_rate, prev_mc, rank))) =
            self.sync_channels.upload_res_rx.try_recv()
        {
            let success = status == "success";
            debug_ui::push_log(
                &self.debug_state.log_lines,
                self.max_log_lines(),
                format!(
                    "[VArchive] 업로드 {}: {} {} {} {:.2}% MaxCombo: {} - {}",
                    if success { "성공" } else { "실패" },
                    self.upload_song_name(key.0),
                    key.1,
                    key.2,
                    rate,
                    mc,
                    msg
                ),
            );
            let mut history_record = overmax_data::UploadHistoryRecord {
                uploaded_at: upload_history_ui::now_unix(),
                song_name: self.upload_song_name(key.0),
                pattern: format!("{} {}", key.1, key.2),
                rate,
                rate_delta: if success {
                    prev_rate.map(|prev| rate - prev)
                } else {
                    None
                },
                top50_rank: if success { rank } else { None },
                is_max_combo: mc,
                prev_max_combo: if success { prev_mc } else { None },
                success,
                message: msg.clone(),
            };
            // 상승폭·P/M 태그는 업로드 기록 창과 같은 규칙(UploadHistoryRecord::result_tags)으로 붙인다.
            let result_msg = match history_record.result_tags().filter(|_| success) {
                Some(tags) => format!("{msg} ({tags})"),
                None => msg.clone(),
            };
            history_record.message = result_msg.clone();
            let mut matched_candidate = false;
            if let Ok(mut list) = self.sync_state.candidates.lock() {
                if let Some(c) = list.iter_mut().find(|item| item.matches_key(&key)) {
                    c.upload_status = status.clone();
                    c.upload_message = result_msg.clone();
                    matched_candidate = true;
                }
            }
            self.record_db.insert_upload_history(&history_record);
            self.upload_history.push(history_record);
            if is_quick_upload || !matched_candidate {
                let result_text = if success {
                    result_msg
                } else {
                    crate::t!("status-varchive-failed-toast", error = &msg)
                };
                self.toast = Some(crate::ui::components::ToastMessage {
                    subject: Some(self.upload_song_name(key.0)),
                    text: format!("{} {} · {}", key.1, key.2, result_text),
                    is_success: success,
                    expires_at: std::time::Instant::now() + std::time::Duration::from_secs(3),
                });
            }
            if success {
                if let Some(val) = self.record_manager.get_local_record(key.0, key.1, key.2) {
                    self.record_manager.upsert_varchive_record(key, val);
                } else {
                    self.record_manager.refresh();
                }
                refreshed = true;
            }
        }
        if refreshed {
            self.refresh_overlay_data();
        }
    }

    fn spawn_scan(&self, ctx: egui::Context) {
        overmax_runtime::varchive_jobs::spawn_scan(
            self.paths.songs_json(),
            self.record_db.clone(),
            self.current_steam_id(),
            self.sync_channels.sync_tx.clone(),
            repaint_notify(ctx),
        );
    }

    fn spawn_upload(&self, candidate: SyncCandidate, is_quick_upload: bool, ctx: egui::Context) {
        let Some(key) = candidate.key() else {
            return;
        };
        let settings = self.settings.get_merged();
        let steam = self.current_steam_id();
        let account_path = account_path_for_steam(&settings, &steam);
        let v_id = settings
            .varchive()
            .user_map
            .get(&steam)
            .and_then(|u| u.v_id.clone())
            .unwrap_or_default();
        let tx = self.sync_channels.upload_res_tx.clone();
        let button_mode = candidate.button_mode;
        let uploaded = (
            candidate.overmax_rate,
            candidate.overmax_mc,
            candidate.varchive_rate,
            candidate.varchive_mc,
            None,
        );

        overmax_runtime::varchive_jobs::spawn_upload(
            candidate,
            account_path,
            v_id,
            steam,
            self.record_db.clone(),
            move |outcome| {
                let (status, msg, uploaded) = upload_result_message(outcome, button_mode, uploaded);
                let _ = tx.send((key, is_quick_upload, status.into(), msg, uploaded));
                ctx.request_repaint();
            },
        );
    }

    pub(crate) fn is_varchive_account_configured(&self) -> bool {
        is_account_configured(&self.settings.get_merged(), &self.current_steam_id())
    }

    pub(crate) fn varchive_user_id(&self) -> Option<String> {
        varchive_user_id(&self.settings.get_merged(), &self.current_steam_id())
    }

    pub(crate) fn current_pattern_needs_upload(&self) -> bool {
        self.session
            .context
            .as_ref()
            .is_some_and(|ctx| self.pattern_needs_upload(ctx))
    }

    pub(crate) fn pattern_needs_upload(&self, ctx: &PlayContext) -> bool {
        overmax_runtime::upload_plan::pattern_needs_upload(&self.record_manager, ctx)
    }

    /// 자동 업로드 스케줄러를 한 프레임 진행하고, 대기가 끝난 값을 업로드한다.
    pub(crate) fn tick_auto_upload(&mut self, ctx: &egui::Context) {
        let stable_context = self
            .session
            .context
            .as_ref()
            .filter(|_| self.session.is_stable);
        // 대기 중이 아니고 업로드 후보도 없으면 설정 조회 없이 빠져나간다.
        let has_candidate = stable_context.is_some() && self.overlay_upload_needed;
        if !(self.auto_upload.is_pending() || has_candidate) {
            return;
        }

        let settings = self.settings.get_merged().varchive().auto_upload;
        let obs = overmax_runtime::auto_upload::AutoUploadObservation {
            scene: self.session.scene,
            stable_context,
            eligible: self.overlay_upload_needed && self.is_varchive_account_configured(),
        };
        let fired = self
            .auto_upload
            .tick(std::time::Instant::now(), &obs, &settings);

        // 결과창 이탈 flush 는 현재 세션이 아닌 스냅샷 기준이므로 업로드 조건을 다시 확인한다.
        if let Some(snapshot) =
            fired.filter(|s| self.pattern_needs_upload(s) && self.is_varchive_account_configured())
        {
            debug_ui::push_log(
                &self.debug_state.log_lines,
                self.max_log_lines(),
                format!(
                    "[VArchive] 자동 업로드 시작: {}, {}, {}, {:.2}%, MaxCombo: {}",
                    self.upload_song_name(snapshot.song_id),
                    snapshot.mode,
                    snapshot.diff,
                    snapshot.rate,
                    snapshot.is_max_combo
                ),
            );
            self.upload_pattern(&snapshot, ctx.clone());
        }

        if self.auto_upload.is_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
    }

    pub(crate) fn upload_current_pattern(&self, ctx: egui::Context) {
        if let Some(session_ctx) = &self.session.context {
            self.upload_pattern(session_ctx, ctx);
        }
    }

    /// `session_ctx` 기록을 로컬 최고 기록과 합쳐 V-Archive에 단일 패턴 업로드한다.
    pub(crate) fn upload_pattern(&self, session_ctx: &PlayContext, ctx: egui::Context) {
        let Some(candidate) = overmax_runtime::upload_plan::prepare_pattern_upload(
            &self.record_manager,
            &self.varchive_db,
            session_ctx,
        ) else {
            return;
        };
        self.spawn_upload(candidate, true, ctx);
    }

    pub(crate) fn handle_auto_refresh(&mut self) {
        let settings = self.settings.get_merged();
        let varchive = settings.varchive();

        let sid = steam_session::most_recent_steam_id().unwrap_or_default();
        if sid.is_empty() {
            return;
        }

        let v_id = varchive
            .user_map
            .get(&sid)
            .and_then(|u| u.v_id.as_deref())
            .unwrap_or("");

        if !v_id.is_empty() {
            debug_ui::push_log(
                &self.debug_state.log_lines,
                self.max_log_lines(),
                format!(
                    "[VArchive] 자동 갱신 시작 (SteamID: {}, V-ID: {})",
                    sid, v_id
                ),
            );
            let _ = self
                .sync_channels
                .fetch_req_tx
                .send((sid, v_id.to_string(), 0));
        }
    }

    pub(crate) fn poll_fetch_requests(&mut self, ctx: &egui::Context) {
        while let Ok((steam_id, v_id, button)) = self.sync_channels.fetch_req_rx.try_recv() {
            self.spawn_fetch(steam_id, v_id, button, ctx.clone());
        }
    }

    pub(crate) fn drain_fetch_results(&mut self) {
        let mut refreshed = false;
        while let Ok((v_id, btn, res)) = self.sync_channels.fetch_res_rx.try_recv() {
            match res {
                Ok(_) => {
                    refreshed = true;
                }
                Err(e) => {
                    debug_ui::push_log(
                        &self.debug_state.log_lines,
                        self.max_log_lines(),
                        format!("[VArchiveClient] {} ({}B) API 요청 실패: {}", v_id, btn, e),
                    );
                }
            }
        }
        if refreshed {
            self.record_manager.refresh();
            self.refresh_overlay_data();
        }
    }

    fn spawn_fetch(&self, steam_id: String, v_id: String, button: i32, ctx: egui::Context) {
        let log_lines = self.debug_state.log_lines.clone();
        let max_lines = self.max_log_lines();
        overmax_runtime::varchive_jobs::spawn_fetch(
            steam_id,
            v_id,
            button,
            self.record_db.clone(),
            self.sync_channels.fetch_res_tx.clone(),
            Arc::new(move |line: String| debug_ui::push_log(&log_lines, max_lines, line)),
            repaint_notify(ctx),
        );
    }
}

fn repaint_notify(ctx: egui::Context) -> overmax_runtime::varchive_jobs::Notify {
    Arc::new(move || ctx.request_repaint())
}

/// 업로드 결과를 업로드 결과 채널의 (상태, 메시지, 업로드 값)으로 바꾼다.
pub(super) fn upload_result_message(
    outcome: UploadOutcome,
    button_mode: overmax_core::Mode,
    uploaded: (f64, bool, Option<f64>, Option<bool>, Option<usize>),
) -> (
    &'static str,
    String,
    (f64, bool, Option<f64>, Option<bool>, Option<usize>),
) {
    match outcome {
        UploadOutcome::AccountPathMissing => (
            "error",
            crate::t!("status-account-path-missing").to_string(),
            uploaded,
        ),
        UploadOutcome::AccountParseFailed => (
            "error",
            crate::t!("status-account-parse-failed").to_string(),
            uploaded,
        ),
        UploadOutcome::Rejected(message) => ("error", message, uploaded),
        UploadOutcome::Uploaded { updated, cache } => {
            let base_message = if updated {
                crate::t!("status-updated")
            } else {
                crate::t!("status-registered")
            };
            match cache {
                Ok(rank) => {
                    let mut msg = base_message.to_string();
                    if let Some(rank) = rank {
                        let place_msg =
                            crate::t!("sys-place-achieved", mode = button_mode, rank = rank);
                        msg = crate::t!(
                            "sys-upload-msg-with-rank",
                            message = &msg,
                            rank_msg = &place_msg
                        );
                    }
                    let (rate, mc, prev_rate, prev_mc, _) = uploaded;
                    ("success", msg, (rate, mc, prev_rate, prev_mc, rank))
                }
                Err(error) => {
                    let err_msg = match error {
                        UploadCacheError::ApiOkMergeFailed(e) => {
                            crate::t!("sys-api-ok-cache-failed", error = e)
                        }
                        UploadCacheError::ApiAndFallbackFailed { api, fallback } => crate::t!(
                            "sys-api-and-fallback-failed",
                            api_error = api,
                            fallback_error = fallback
                        ),
                        UploadCacheError::FallbackFailed(e) => {
                            crate::t!("sys-fallback-cache-failed", error = e)
                        }
                    };
                    (
                        "success",
                        crate::t!("sys-upload-cache-error", error = err_msg),
                        uploaded,
                    )
                }
            }
        }
    }
}
