//! Deferred viewports + `eframe::App` (split from `native_app.rs` for file-size limits).

#[cfg(target_os = "windows")]
use eframe::egui::Vec2;
use eframe::egui::{self, RichText, ViewportBuilder, ViewportCommand};
use std::sync::atomic::Ordering;

use crate::system::native_helpers;
use crate::ui::debug_ui;
use crate::ui::dialog_theme::DialogTheme;
use crate::ui::native_app::NativeApp;
#[cfg(target_os = "windows")]
use crate::ui::overlay_ui;
use crate::ui::settings_ui;
use crate::ui::sync_ui;
use overmax_engine::capture::window_tracker;

fn game_window_title(settings: &overmax_data::Settings) -> &str {
    settings
        .window_tracker
        .as_ref()
        .map(|t| t.window_title.as_str())
        .unwrap_or("DJMAX RESPECT V")
}

static GLOBAL_LOG_TX: std::sync::Mutex<Option<std::sync::mpsc::Sender<String>>> =
    std::sync::Mutex::new(None);

pub fn set_global_log_tx(tx: std::sync::mpsc::Sender<String>) {
    if let Ok(mut lock) = GLOBAL_LOG_TX.lock() {
        *lock = Some(tx);
    }
}

pub fn send_debug_log(msg: impl Into<String>) {
    let s = msg.into();
    if let Ok(lock) = GLOBAL_LOG_TX.lock() {
        if let Some(tx) = lock.as_ref() {
            let _ = tx.send(s);
            return;
        }
    }
    eprintln!("{s}");
}

impl NativeApp {
    fn auxiliary_viewport(title: &str, size: [f32; 2]) -> ViewportBuilder {
        ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size)
            .with_visible(true)
            .with_resizable(true)
            .with_decorations(true)
            .with_transparent(false)
            .with_taskbar(true)
            .with_always_on_top()
    }

    fn show_debug_viewport(&self, ctx: &egui::Context) {
        if !self.ui_state.debug_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.ui_state.debug_open.clone();
        let lines = self.debug_state.log_lines.clone();
        let paused = self.debug_state.paused.clone();
        let filters = self.debug_state.filters.clone();
        let title = self.debug_title();
        let app_state = self.build_debug_app_state_snapshot();

        ctx.show_viewport_deferred(
            native_helpers::vp_debug(),
            Self::auxiliary_viewport(&title, [760.0, 480.0]),
            move |ui, class| {
                // 디버그 창이 비활성(Inactive) 상태라도 게임 플레이 중 탐지 결과 및 로그가 실시간 모니터링되도록 갱신 요청
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(200));

                #[cfg(debug_assertions)]
                ui.ctx().all_styles_mut(|s| {
                    s.debug.show_expand_width = false;
                    s.debug.show_expand_height = false;
                    s.debug.show_resize = false;
                    s.debug.show_unaligned = false;
                    s.debug.debug_on_hover = false;
                });
                debug_ui::render_debug(ui, class, &title, &lines, &paused, &filters, &app_state);
                debug_ui::close_if_requested(ui.ctx(), &open);
            },
        );
    }

    fn build_debug_app_state_snapshot(&self) -> debug_ui::DebugAppStateSnapshot {
        let game_rect_val = *overmax_core::lock_or_recover(&self.game_rect);
        let game_found = game_rect_val.is_some();
        let ovs = read_overlay_settings(&self.settings.merged);
        let settings_merged = self.settings.get_merged();
        let cap_settings = settings_merged.screen_capture();

        let overlay_on = game_found
            && (ovs.always_visible || self.session.scene != overmax_core::SceneType::Unknown)
            && self.overlay_visible_override.unwrap_or(true);

        #[cfg(target_os = "windows")]
        let game_hwnd = self.platform.win_cache.cached_game_hwnd;
        #[cfg(not(target_os = "windows"))]
        let game_hwnd = None;

        #[cfg(target_os = "windows")]
        let is_active = self.determine_active_state(game_hwnd.map(|h| h as _));
        #[cfg(not(target_os = "windows"))]
        let is_active = true;

        #[cfg(target_os = "windows")]
        let cached_hwnd = self.platform.win_cache.cached_hwnd;
        #[cfg(not(target_os = "windows"))]
        let cached_hwnd = None;

        let song_info = self.current_song_label();
        let play_state_info = if let Some(ctx) = &self.session.context {
            format!(
                "{} {} | Rate: {:.2}% | Stable: {}",
                ctx.mode.as_str(),
                ctx.diff.as_str(),
                ctx.rate,
                self.session.is_stable
            )
        } else {
            format!("Stable: {}", self.session.is_stable)
        };
        let jacket_match_info = if let Some(ctx) = &self.session.context {
            format!("SongID: {}", ctx.song_id)
        } else {
            "No Match".to_string()
        };
        let capture_res_info = if let Some(r) = game_rect_val {
            let aspect = r.width as f32 / r.height.max(1) as f32;
            format!(
                "({},{}) {}x{} ({:.2})",
                r.left, r.top, r.width, r.height, aspect
            )
        } else {
            "No Window".to_string()
        };

        let top_jacket_similarity = self
            .last_detection_output
            .as_ref()
            .and_then(|o| o.top_jacket_similarity);
        let roi_scale = self
            .last_detection_output
            .as_ref()
            .map(|o| o.roi_scale)
            .unwrap_or(1.0);
        let roi_offset_y = self
            .last_detection_output
            .as_ref()
            .map(|o| o.roi_offset_y)
            .unwrap_or(0);
        let stable_hits = self
            .last_detection_output
            .as_ref()
            .map(|o| o.stable_hits)
            .unwrap_or(0);

        debug_ui::DebugAppStateSnapshot {
            scene_label: format!("{:?}", self.session.scene),
            confidence: self.confidence,
            game_found,
            is_active,
            overlay_on,
            always_visible: ovs.always_visible,
            opacity: ovs.opacity,
            capture_engine: cap_settings.engine,
            content_protected: cap_settings.content_protected,
            cached_hwnd,
            game_hwnd,
            song_info,
            play_state_info,
            jacket_match_info,
            capture_res_info,
            top_jacket_similarity,
            roi_scale,
            roi_offset_y,
            stable_hits,
            telemetry_snapshot: self.last_telemetry_snapshot,
        }
    }

    fn show_settings_viewport(&self, ctx: &egui::Context) {
        if !self.ui_state.settings_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.ui_state.settings_open.clone();
        let draft = self.settings.draft.clone();
        let paths = self.paths.clone();
        let defaults = self.settings.defaults.clone();
        let base = self.settings.base.clone();
        let merged = self.settings.merged.clone();

        let settings_ctx = settings_ui::SettingsUiContext {
            root: self.root.clone(),
            paths: paths.clone(),
            current_steam_id: self
                .sync_state
                .steam_id
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default(),
            sync_open: self.ui_state.sync_open.clone(),
            debug_open: self.ui_state.debug_open.clone(),
            scan_pending: self.ui_state.scan_pending.clone(),
            sync_steam_id: self.sync_state.steam_id.clone(),
            fetch_tx: self.sync_channels.fetch_req_tx.clone(),
            steam_users: self.sync_state.steam_users.clone(),
            ipc_bound_port: self.ipc_bound_port.clone(),
        };
        ctx.show_viewport_deferred(
            native_helpers::vp_settings(),
            Self::auxiliary_viewport(crate::t!("app-settings-window"), [580.0, 600.0]),
            move |ui, class| {
                ui.ctx().set_pixels_per_point(1.0);
                #[cfg(debug_assertions)]
                ui.ctx().all_styles_mut(|s| {
                    s.debug.show_expand_width = false;
                    s.debug.show_expand_height = false;
                    s.debug.show_resize = false;
                    s.debug.show_unaligned = false;
                    s.debug.debug_on_hover = false;
                });
                let mut local_draft = overmax_core::lock_clone_or_default(&draft);
                egui::Panel::bottom("sett_actions")
                    .frame(
                        egui::Frame::new()
                            .fill(DialogTheme::BG_WINDOW)
                            .inner_margin(egui::Margin::symmetric(
                                DialogTheme::PANEL_PADDING as i8,
                                14,
                            )),
                    )
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let close_btn = egui::Button::new(
                                        RichText::new(crate::t!("app-close"))
                                            .size(DialogTheme::FONT_BODY),
                                    )
                                    .min_size(egui::vec2(84.0, DialogTheme::CONTROL_HEIGHT))
                                    .fill(DialogTheme::SECONDARY)
                                    .corner_radius(egui::CornerRadius::same(DialogTheme::R_SM));
                                    if ui.add(close_btn).clicked() {
                                        open.store(false, Ordering::Relaxed);
                                        ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
                                    }

                                    ui.add_space(DialogTheme::GAP_SM);

                                    let save_btn = egui::Button::new(
                                        egui::RichText::new(crate::t!("app-save"))
                                            .color(egui::Color32::WHITE)
                                            .size(DialogTheme::FONT_BODY)
                                            .strong(),
                                    )
                                    .min_size(egui::vec2(100.0, DialogTheme::CONTROL_HEIGHT))
                                    .fill(DialogTheme::PRIMARY)
                                    .corner_radius(egui::CornerRadius::same(DialogTheme::R_SM));
                                    if ui.add(save_btn).clicked() {
                                        let base_g = overmax_core::lock_clone_or_default(&base);
                                        let mut merged_g =
                                            overmax_core::lock_clone_or_default(&merged);
                                        let prev_v_id = varchive_v_id(
                                            &merged_g,
                                            &settings_ctx.current_steam_id,
                                        );
                                        let _ = settings_ui::save_settings_to_paths(
                                            &paths.settings_paths(),
                                            defaults.as_ref(),
                                            &base_g,
                                            &mut local_draft,
                                            &mut merged_g,
                                        );

                                        crate::ui::i18n::set_locale_from_settings(&merged_g);
                                        let new_v_id = varchive_v_id(
                                            &merged_g,
                                            &settings_ctx.current_steam_id,
                                        );

                                        if !new_v_id.is_empty() && new_v_id != prev_v_id {
                                            let _ = settings_ctx.fetch_tx.send((
                                                settings_ctx.current_steam_id.clone(),
                                                new_v_id,
                                                0,
                                            ));
                                        }
                                        if let Ok(mut m) = merged.lock() {
                                            *m = merged_g;
                                        }
                                    }
                                },
                            );
                        });
                    });
                settings_ui::render_settings_deferred(
                    ui,
                    class,
                    crate::t!("settings-title"),
                    &mut local_draft,
                    &settings_ctx,
                );
                if let Ok(mut d) = draft.lock() {
                    *d = local_draft;
                }
                settings_ui::close_if_requested(ui.ctx(), &open);
            },
        );
    }

    fn show_sync_viewport(&self, ctx: &egui::Context) {
        if !self.ui_state.sync_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.ui_state.sync_open.clone();
        let scan_pending = self.ui_state.scan_pending.clone();
        let upload_tx = self.sync_channels.upload_req_tx.clone();
        let delete_tx = self.sync_channels.delete_req_tx.clone();
        let sync_state = self.sync_state.clone();
        let settings = self.settings.clone();
        let app_settings = settings.get_merged();

        let filter = app_settings.sync_filter();

        ctx.show_viewport_deferred(
            native_helpers::vp_sync(),
            Self::auxiliary_viewport(crate::t!("sync-varchive-sync"), [600.0, 720.0]),
            move |ui, class| {
                ui.ctx().set_pixels_per_point(1.0);
                #[cfg(debug_assertions)]
                ui.ctx().all_styles_mut(|s| {
                    s.debug.show_expand_width = false;
                    s.debug.show_expand_height = false;
                    s.debug.show_resize = false;
                    s.debug.show_unaligned = false;
                    s.debug.debug_on_hover = false;
                });
                let list = overmax_core::lock_clone_or_default(&sync_state.candidates);
                let users = overmax_core::lock_or_recover(&sync_state.steam_users);
                let mut steam_g = overmax_core::lock_or_recover(&sync_state.steam_id);
                let status_s = overmax_core::lock_clone_or_default(&sync_state.status);
                sync_ui::render_sync(
                    ui,
                    class,
                    sync_ui::SyncProps {
                        steam_id: &mut steam_g,
                        status: &status_s,
                        candidates: &list,
                        steam_users: &users,
                        initial_filter: &filter,
                        on_scan: || {
                            scan_pending.store(true, Ordering::Relaxed);
                        },
                        on_upload: |key| {
                            let _ = upload_tx.send(key);
                        },
                        on_delete: |key| {
                            let _ = delete_tx.send(key);
                        },
                        on_filter_change: |new_filter| {
                            settings.update_sync_filter(&new_filter);
                        },
                    },
                );
                sync_ui::close_if_requested(ui.ctx(), &open);
            },
        );
    }
}

struct OverlaySettingsSnapshot {
    #[cfg(target_os = "windows")]
    scale: f32,
    opacity: f32,
    #[cfg(target_os = "windows")]
    is_lite: bool,
    always_visible: bool,
    #[cfg(target_os = "windows")]
    snap_position: String,
}

fn read_overlay_settings(
    settings: &std::sync::Arc<std::sync::Mutex<serde_json::Value>>,
) -> OverlaySettingsSnapshot {
    let Ok(m) = settings.lock() else {
        return OverlaySettingsSnapshot {
            #[cfg(target_os = "windows")]
            scale: 1.0,
            opacity: 0.8,
            #[cfg(target_os = "windows")]
            is_lite: false,
            always_visible: false,
            #[cfg(target_os = "windows")]
            snap_position: "manual".into(),
        };
    };
    let overlay = m.get("overlay");
    OverlaySettingsSnapshot {
        #[cfg(target_os = "windows")]
        scale: overlay
            .and_then(|o| o.get("scale"))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32,
        opacity: overlay
            .and_then(|o| o.get("base_opacity"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8) as f32,
        #[cfg(target_os = "windows")]
        is_lite: overlay
            .and_then(|o| o.get("lite_mode"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        always_visible: overlay
            .and_then(|o| o.get("always_visible"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        #[cfg(target_os = "windows")]
        snap_position: overlay
            .and_then(|o| o.get("position"))
            .and_then(|p| p.get("snap"))
            .and_then(|v| v.as_str())
            .unwrap_or("manual")
            .to_string(),
    }
}

impl eframe::App for NativeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if let Ok(mut holder) = self.ctx_holder.lock() {
            if holder.is_none() {
                *holder = Some(ctx.clone());
            }
        }
        // 모든 레이아웃 디버그 시각화(노란 선 및 텍스트) 강제 비활성화
        #[cfg(debug_assertions)]
        {
            thread_local! {
                static STYLE_INIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
            }
            STYLE_INIT.with(|init| {
                if !init.get() {
                    ctx.all_styles_mut(|s| {
                        s.debug.show_expand_width = false;
                        s.debug.show_expand_height = false;
                        s.debug.show_resize = false;
                        s.debug.show_unaligned = false;
                        s.debug.debug_on_hover = false;
                    });
                    ctx.set_debug_on_hover(false);
                    init.set(true);
                }
            });
        }

        #[cfg(target_os = "linux")]
        if let Some(error) = self.platform.linux_overlay.take_runtime_failure() {
            let error = format!("Linux overlay stopped: {error}");
            crate::ui::platform::show_startup_error(&error);
            self.exit_requested.store(true, Ordering::Relaxed);
        }

        if self.exit_requested.load(Ordering::Relaxed) {
            self.ui_state.settings_open.store(false, Ordering::Relaxed);
            self.ui_state.sync_open.store(false, Ordering::Relaxed);
            self.ui_state.debug_open.store(false, Ordering::Relaxed);
            self.ipc_handle.shutdown();
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        self.poll_and_drain_events(&ctx);

        self.show_debug_viewport(&ctx);
        self.show_settings_viewport(&ctx);
        self.show_sync_viewport(&ctx);

        self.update_platform_overlay(ui);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }
}

impl NativeApp {
    fn update_platform_overlay(&mut self, ui: &mut egui::Ui) {
        #[cfg(target_os = "linux")]
        self.publish_linux_overlay(ui.ctx());

        #[cfg(target_os = "windows")]
        self.render_windows_overlay(ui);
    }

    #[cfg(target_os = "windows")]
    fn render_windows_overlay(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx();
        let ovs = read_overlay_settings(&self.settings.merged);
        let scale = ovs.scale;
        let opacity = ovs.opacity;
        let snap_position = ovs.snap_position;

        let height = if ovs.is_lite {
            overlay_ui::LITE_BASE_HEIGHT
        } else {
            overlay_ui::BASE_HEIGHT
        };

        // game_rect 락 단 1회 획득으로 통합하여 경합 방지
        let game_rect_val = *overmax_core::lock_or_recover(&self.game_rect);
        let game_found = game_rect_val.is_some();
        let overlay_on = game_found
            && (ovs.always_visible || self.session.scene != overmax_core::SceneType::Unknown)
            && self.overlay_visible_override.unwrap_or(true);

        let overlay_on_changed = self.state_tracker.prev_overlay_on.update(overlay_on);

        let mut force_topmost = self.update_overlay_geometry(
            ctx,
            scale,
            height,
            &snap_position,
            game_rect_val,
            overlay_on,
            overlay_on_changed,
        );

        self.render_overlay_panel(
            ui,
            scale,
            opacity,
            height,
            &snap_position,
            overlay_on,
            &mut force_topmost,
        );

        // Windows 전용: 오버레이 창 가시성 및 최상위 권한 적용 (게임 미실행 시 숨김 처리하여 까만 뷰포트 노출 차단)
        let game_hwnd = self.game_hwnd_cached();
        let is_active = self.determine_active_state(game_hwnd);
        let found = self
            .platform
            .apply_overlay_visibility(overlay_on, is_active, force_topmost);
        if !found && overlay_on && !self.platform.win_cache.logged_opacity_fail {
            debug_ui::push_log(
                &self.debug_state.log_lines,
                self.max_log_lines(),
                "[Overlay] 오버레이 스타일 적용용 창 핸들을 찾지 못함",
            );
            self.platform.win_cache.logged_opacity_fail = true;
        }
    }
}

impl NativeApp {
    #[cfg(target_os = "linux")]
    fn publish_linux_overlay(&mut self, ctx: &egui::Context) {
        let now = std::time::Instant::now();
        if self
            .toast
            .as_ref()
            .is_some_and(|toast| now >= toast.expires_at)
        {
            self.toast = None;
        }
        if let Some(toast) = &self.toast {
            ctx.request_repaint_after(toast.expires_at.saturating_duration_since(now));
        }

        let overlay = self.settings.get_merged().overlay();
        let position = overlay
            .position
            .x
            .zip(overlay.position.y)
            .map(|(x, y)| (x as i32, y as i32));
        self.platform
            .linux_overlay
            .publish(crate::ui::linux_layer_overlay::LinuxOverlaySnapshot {
                state: self.session.clone(),
                song_label: self.current_song_label(),
                pattern_tabs: self.pattern_tabs.clone(),
                recommendations: self.recommendations.clone(),
                settings_open: self.ui_state.settings_open.clone(),
                sync_open: self.ui_state.sync_open.clone(),
                scale: overlay.scale as f32,
                opacity: overlay.base_opacity as f32,
                varchive_upload_needed: self.current_pattern_needs_upload(),
                varchive_account_configured: self.is_varchive_account_configured(),
                lite_mode: overlay.lite_mode,
                always_visible: overlay.always_visible,
                snap: overlay.position.snap,
                position,
                record_manager: self.record_manager.clone(),
                session_initial_record: self.session_initial_record,
                toast: self.toast.clone(),
                window_snapshot: self.window_snapshot,
                capture_fatal: self.capture_fatal.clone(),
                #[cfg(any(debug_assertions, feature = "telemetry"))]
                delivery_telemetry: self
                    .last_detection_output
                    .as_ref()
                    .and_then(|output| output.delivery_telemetry),
            });
    }

    fn poll_and_drain_events(&mut self, ctx: &egui::Context) {
        let debug_on = self.ui_state.debug_open.load(Ordering::Relaxed);
        let settings_on = self.ui_state.settings_open.load(Ordering::Relaxed);
        let sync_on = self.ui_state.sync_open.load(Ordering::Relaxed);

        let debug_open_changed = self.state_tracker.prev_debug_open.update(debug_on);
        let settings_open_changed = self.state_tracker.prev_settings_open.update(settings_on);
        if settings_on && settings_open_changed {
            if let (Ok(m), Ok(mut d)) = (self.settings.merged.lock(), self.settings.draft.lock()) {
                *d = m.clone();
            }
            self.refresh_steam_session("설정 창 열림");
        }

        let sync_open_changed = self.state_tracker.prev_sync_open.update(sync_on);
        if sync_on && sync_open_changed {
            self.refresh_steam_session("동기화 창 열림");
        }

        if (debug_open_changed && !debug_on)
            || (settings_open_changed && !settings_on)
            || (sync_open_changed && !sync_on)
        {
            let settings = self.settings.get_merged();
            window_tracker::restore_foreground_by_title(game_window_title(&settings));
        }

        self.start_log_pump(ctx);
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        self.drain_detection_results(ctx);
        if self.drain_ui_commands() {
            ctx.request_repaint();
        }
        if self.drain_ipc_commands() {
            ctx.request_repaint();
        }
        self.poll_scan_requests(ctx);
        self.poll_upload_requests(ctx);
        self.poll_fetch_requests(ctx);
        self.poll_startup_cache();
        self.drain_sync_scan();
        self.drain_upload_results();
        self.drain_fetch_results();
        self.poll_delete_requests(ctx);
        self.drain_game_found_refresh_steam();
        let content_protected = self
            .settings
            .get_merged()
            .screen_capture()
            .content_protected;
        if self
            .state_tracker
            .prev_protected
            .update(Some(content_protected))
        {
            ctx.send_viewport_cmd(ViewportCommand::ContentProtected(content_protected));
        }
    }

    #[cfg(target_os = "windows")]
    #[allow(clippy::too_many_arguments)]
    fn update_overlay_geometry(
        &mut self,
        ctx: &egui::Context,
        scale: f32,
        height: f32,
        snap_position: &str,
        game_rect_val: Option<overmax_engine::capture::window_tracker::WindowRect>,
        overlay_on: bool,
        overlay_on_changed: bool,
    ) -> bool {
        let prev_overlay = *self.state_tracker.prev_overlay_on;
        let prev_scale_val = *self.state_tracker.prev_scale;
        let prev_lite = *self.state_tracker.prev_is_lite;

        let scale_changed =
            (scale - prev_scale_val).abs() > 0.001 && self.state_tracker.prev_scale.update(scale);
        let is_lite = height == overlay_ui::LITE_BASE_HEIGHT;
        let is_lite_changed = self.state_tracker.prev_is_lite.update(is_lite);

        if overlay_on_changed || (overlay_on && (scale_changed || is_lite_changed)) {
            debug_ui::push_log(
                &self.debug_state.log_lines,
                1000,
                format!(
                    "[Overlay] 레이아웃 업데이트: ON={}->{}, Scale={:.2}->{:.2}, Lite={}->{} (Game: {}, Conf: {:.2})",
                    prev_overlay,
                    overlay_on,
                    prev_scale_val,
                    scale,
                    prev_lite,
                    is_lite,
                    game_rect_val.is_some(),
                    self.confidence
                ),
            );

            if overlay_on {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(
                    (overlay_ui::BASE_WIDTH * scale).ceil(),
                    (height * scale).ceil(),
                )));
            }
        }

        // 마우스가 오버레이 영역 위에 있을 때만 상호작용 가능하게 함 (보조창 조작을 위해)
        let local_mouse =
            crate::ui::platform::get_local_mouse_pos(ctx, self.platform.win_cache.cached_hwnd);

        let is_over = local_mouse.is_some() || self.platform.is_dragging;
        let passthrough = !overlay_on || !is_over;
        if self
            .state_tracker
            .prev_passthrough
            .update(Some(passthrough))
        {
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(passthrough));
        }

        // 비활성 윈도우(WS_EX_NOACTIVATE) 상태에서 마우스가 위에 있고, 마우스가 실제로 움직였거나 드래그 중일 때만 렌더링 강제
        let mouse_moved = self.state_tracker.prev_mouse_pos.update(local_mouse);
        if overlay_on && is_over && (mouse_moved || self.platform.is_dragging) {
            ctx.request_repaint();
        }

        // 오버레이 메인 창이 우발적으로 포커스를 획득했을 때, 포커스를 자동으로 게임 창으로 되돌려 키 입력 씹힘 방지.
        // 단, snap이 manual(수동 위치 조정 모드)인 경우 사용자가 고의로 오버레이 창을 조작(드래그) 중이므로 예외로 둡니다.
        if overlay_on && snap_position != "manual" {
            if let (Some(overlay_hwnd), Some(game_hwnd)) = (
                self.platform.win_cache.cached_hwnd,
                self.platform.win_cache.cached_game_hwnd,
            ) {
                unsafe {
                    let fg = windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
                    if fg == overlay_hwnd as windows_sys::Win32::Foundation::HWND {
                        windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(
                            game_hwnd as windows_sys::Win32::Foundation::HWND,
                        );
                    }
                }
            }
        }

        let mut force_topmost = false;
        if overlay_on && overlay_on_changed {
            force_topmost = true;
        }

        // Windows 전용: 라이트 모드 구석 고정 위치 강제 적용
        if overlay_on && snap_position != "manual" {
            if let Some(hwnd_val) = self.platform.win_cache.cached_hwnd {
                if let Some(g_rect) = game_rect_val {
                    use windows_sys::Win32::UI::WindowsAndMessaging::*;

                    let hwnd = hwnd_val as HWND;

                    // 1. DPI Scale 구하기
                    let dpi_scale = ctx.pixels_per_point();

                    // 2. 현재 패널 높이(height)와 scale에 맞는 목표 물리적 크기(Physical Pixels) 구하기
                    let target_phys_w =
                        ((overlay_ui::BASE_WIDTH * scale).ceil() * dpi_scale) as i32;
                    let target_phys_h = ((height * scale).ceil() * dpi_scale) as i32;

                    let margin_px = (16.0 * dpi_scale) as i32;

                    // 3. 물리 픽셀 도메인에서만 구석 위치(px, py) 계산
                    let (px, py) = match snap_position {
                        "top_left" => (g_rect.left + margin_px, g_rect.top + margin_px),
                        "top_right" => (
                            g_rect.left + g_rect.width - target_phys_w - margin_px,
                            g_rect.top + margin_px,
                        ),
                        "bottom_left" => (
                            g_rect.left + margin_px,
                            g_rect.top + g_rect.height - target_phys_h - margin_px,
                        ),
                        _ => {
                            // bottom_right
                            (
                                g_rect.left + g_rect.width - target_phys_w - margin_px,
                                g_rect.top + g_rect.height - target_phys_h - margin_px,
                            )
                        }
                    };

                    // 4. 좌표 및 크기 변경 시 SetWindowPos로 윈도우 크기와 위치를 함께 갱신
                    let current_geom = (px, py, target_phys_w, target_phys_h);
                    let geom_changed =
                        self.platform.win_cache.prev_snap_geometry != Some(current_geom);

                    if geom_changed {
                        debug_ui::push_log(
                                &self.debug_state.log_lines,
                                1000,
                                format!(
                                    "[Win32 DPI] 오버레이 스냅 위치 보정: px={}, py={}, w={}, h={}, dpi_scale={:.2}",
                                    px, py, target_phys_w, target_phys_h, dpi_scale
                                ),
                            );

                        unsafe {
                            SetWindowPos(
                                hwnd,
                                HWND_TOPMOST,
                                px,
                                py,
                                target_phys_w,
                                target_phys_h,
                                SWP_NOACTIVATE,
                            );
                        }
                        self.platform.win_cache.prev_snap_geometry = Some(current_geom);
                    }
                }
            }
        }

        force_topmost
    }

    #[cfg(target_os = "windows")]
    #[allow(clippy::too_many_arguments)]
    fn render_overlay_panel(
        &mut self,
        ui: &mut egui::Ui,
        scale: f32,
        opacity: f32,
        height: f32,
        snap_position: &str,
        overlay_on: bool,
        force_topmost: &mut bool,
    ) {
        let local_mouse =
            crate::ui::platform::get_local_mouse_pos(ui.ctx(), self.platform.win_cache.cached_hwnd);
        if !overlay_on {
            self.platform.last_painted_rect = None;
            return;
        }

        // 하드웨어 커서가 나타나 가상 커서와 이중으로 보이는 현상을 예방하기 위해 하드웨어 커서를 숨김
        if local_mouse.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        }

        if let Some(toast) = &self.toast {
            let now = std::time::Instant::now();
            if now >= toast.expires_at {
                self.toast = None;
                ui.ctx().request_repaint();
            } else {
                ui.ctx()
                    .request_repaint_after(toast.expires_at.saturating_duration_since(now));
            }
        }

        let actions = overlay_ui::draw_overlay_panel(
            ui,
            &overlay_ui::OverlayProps {
                state: &self.session,
                song_label: &self.current_song_label(),
                pattern_tabs: &self.pattern_tabs,
                recommendations: &self.recommendations,
                settings_open: self.ui_state.settings_open.clone(),
                sync_open: self.ui_state.sync_open.clone(),
                scale,
                opacity,
                varchive_upload_needed: self.current_pattern_needs_upload(),
                varchive_account_configured: self.is_varchive_account_configured(),
                lite_mode: height == overlay_ui::LITE_BASE_HEIGHT,
                is_snap_manual: snap_position == "manual",
                record_manager: &self.record_manager,
                session_initial_record: self.session_initial_record,
                toast: self.toast.as_ref(),
            },
        );

        let is_drag = actions.start_drag || actions.drag_delta.is_some();
        let stop_drag = actions.restore_game_focus || ui.ctx().input(|i| !i.pointer.any_down());
        let dpi = ui.ctx().pixels_per_point();
        if let Some(cmd) = self.platform.handle_screen_drag(is_drag, stop_drag, dpi) {
            self.handle_ui_command(cmd);
        }

        if let Some(rect) = actions.response_rect {
            self.platform.last_painted_rect = Some(rect);
            let target_w = (overlay_ui::BASE_WIDTH * scale).ceil();
            let fit_h = rect.height().ceil();
            if target_w > 1.0 && fit_h > 1.0 {
                ui.ctx()
                    .send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(target_w, fit_h)));
            }
        }

        if actions.restore_game_focus {
            let settings = self.settings.get_merged();
            window_tracker::restore_foreground_by_title(game_window_title(&settings));

            if let Some(rect) = ui.ctx().input(|i| i.viewport().outer_rect) {
                self.handle_ui_command(crate::ui::ui_command::UiCommand::SetOverlayPosition {
                    x: rect.min.x as i32,
                    y: rect.min.y as i32,
                });
            }
        }
        if let Some(command) = actions.command {
            self.handle_ui_command(command);
            ui.ctx().request_repaint();
        }
        if actions.restore_game_focus || actions.start_drag {
            *force_topmost = true;
        }
        if let Some(mouse_pos) = local_mouse {
            // 비활성 윈도우 마우스 커서 숨김 제약을 우회하기 위해 가상 커서를 마우스 위치에 직접 렌더링
            crate::ui::platform::draw_custom_cursor(ui.painter(), mouse_pos);
        }
        self.platform.last_painted_rect = actions.response_rect;
    }
}

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::HWND;

#[cfg(target_os = "windows")]
impl NativeApp {
    fn determine_active_state(&self, game_hwnd: Option<HWND>) -> bool {
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        let Some(g_hwnd) = game_hwnd else {
            return false;
        };

        let fg = unsafe { GetForegroundWindow() };
        if fg.is_null() {
            return false;
        }

        let is_act = if fg == g_hwnd {
            true
        } else {
            unsafe {
                let mut fg_pid = 0u32;
                GetWindowThreadProcessId(fg, &mut fg_pid);
                let my_pid = GetCurrentProcessId();
                fg_pid == my_pid
            }
        };

        static mut PREV_ACTIVE: Option<bool> = None;
        unsafe {
            if PREV_ACTIVE != Some(is_act) {
                debug_ui::push_log(
                    &self.debug_state.log_lines,
                    self.max_log_lines(),
                    format!(
                        "[Win32] 포커스 상태 변경: Active={} (FG HWND: {:?}, Game HWND: {:?})",
                        is_act, fg, g_hwnd
                    ),
                );
                PREV_ACTIVE = Some(is_act);
            }
        }

        is_act
    }

    fn game_hwnd_cached(&mut self) -> Option<HWND> {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let mut g_hwnd = self.platform.win_cache.cached_game_hwnd.map(|h| h as HWND);
        let is_valid = g_hwnd.map(|h| unsafe { IsWindow(h) } != 0).unwrap_or(false);
        if !is_valid {
            let settings = self.settings.get_merged();
            let game_title = game_window_title(&settings).to_string();
            let title_wide = window_tracker::encode_wide(&game_title);
            g_hwnd = window_tracker::find_hwnd_by_title(&title_wide);
            self.platform.win_cache.cached_game_hwnd = g_hwnd.map(|h| h as isize);
            if let Some(h) = g_hwnd {
                debug_ui::push_log(
                    &self.debug_state.log_lines,
                    self.max_log_lines(),
                    format!(
                        "[Win32] 게임 창 HWND 새로 감지됨: {:?} ('{}')",
                        h, game_title
                    ),
                );
            }
        }
        g_hwnd
    }
}

fn varchive_v_id(settings: &serde_json::Value, steam_id: &str) -> String {
    settings
        .get("varchive")
        .and_then(|v| v.get("user_map"))
        .and_then(|m| m.get(steam_id))
        .and_then(|e| e.get("v_id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::NativeApp;
    use eframe::egui;

    #[test]
    fn auxiliary_viewports_are_topmost_and_in_taskbar() {
        let builder = NativeApp::auxiliary_viewport("debug", [720.0, 420.0]);

        assert_eq!(builder.taskbar, Some(true));
        assert_eq!(builder.visible, Some(true));
        assert_eq!(builder.resizable, Some(true));
        assert_eq!(
            builder.window_level,
            Some(egui::viewport::WindowLevel::AlwaysOnTop)
        );
        assert_ne!(builder.active, Some(true));
    }

    #[test]
    fn game_window_title_uses_settings_or_python_default() {
        let val = serde_json::json!({
            "window_tracker": {"window_title": "DJMAX TEST"}
        });
        let settings: overmax_data::Settings = serde_json::from_value(val).unwrap_or_default();

        assert_eq!(super::game_window_title(&settings), "DJMAX TEST");
        assert_eq!(
            super::game_window_title(&overmax_data::Settings::default()),
            "DJMAX RESPECT V"
        );
    }
}
