//! Deferred viewports + `eframe::App` (split from `native_app.rs` for file-size limits).

use eframe::egui::{self, RichText, ViewportBuilder, ViewportCommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::system::native_helpers;
use crate::ui::debug_ui;
use crate::ui::dialog_theme::DialogTheme;
use crate::ui::native_app::NativeApp;
#[cfg(target_os = "windows")]
use crate::ui::platform;
use crate::ui::settings_ui;
use crate::ui::sync_ui;
use crate::ui::upload_history_ui;

pub(crate) fn game_window_title(settings: &overmax_data::Settings) -> &str {
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
            && (ovs.always_visible
                || (self.session.scene != overmax_core::SceneType::Unknown
                    && !self.session.scene.is_ingame()))
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
            #[cfg(all(target_os = "linux", feature = "portal-capture"))]
            portal: self.portal.clone(),
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

    /// 업로드 기록 창: 제목표시줄 없이 메인 오버레이와 같은 카드 룩으로 그린다 (헤더 드래그 · ✕ 닫기).
    fn show_upload_history_viewport(&self, ctx: &egui::Context) {
        if !self.ui_state.upload_history_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.ui_state.upload_history_open.clone();
        let history = self.upload_history.clone();
        let scale = self.settings.get_merged().overlay().scale as f32;
        let title = crate::t!("upload-history-title").to_string();
        // 창이 생성된 뒤 첫 프레임에 한 번만 Win32 프레임리스/투명 스타일을 입힌다.
        let style_applied = Arc::new(AtomicBool::new(false));
        // 헤더 드래그는 메인 오버레이와 같이 마우스 스크린 좌표로 창을 옮긴다 (드래그 시작 앵커 보관).
        #[cfg(target_os = "windows")]
        let drag_anchor = Arc::new(Mutex::new(None));

        ctx.show_viewport_deferred(
            native_helpers::vp_upload_history(),
            ViewportBuilder::default()
                .with_title(&title)
                .with_inner_size(upload_history_ui::WINDOW_SIZE.map(|v| v * scale))
                .with_decorations(false)
                .with_transparent(true)
                .with_resizable(false)
                .with_taskbar(true),
            move |ui, _class| {
                #[cfg(target_os = "windows")]
                if !style_applied.load(Ordering::Relaxed)
                    && platform::apply_frameless_transparent_style_by_title(&title)
                {
                    style_applied.store(true, Ordering::Relaxed);
                }
                let entries = history.snapshot();
                let dragging = upload_history_ui::render_upload_history(ui, &entries, &open, scale);
                if dragging {
                    // 창이 커서를 따라 움직이는 동안 매 프레임 다시 그려서 위치 갱신이 끊기지 않게 한다.
                    ui.ctx().request_repaint();
                }
                #[cfg(target_os = "windows")]
                if let Ok(mut anchor) = drag_anchor.lock() {
                    platform::drag_window_by_title(&title, &mut anchor, dragging);
                }
                #[cfg(not(target_os = "windows"))]
                let _ = dragging;
                upload_history_ui::close_if_requested(ui.ctx(), &open);
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

pub(crate) struct OverlaySettingsSnapshot {
    #[cfg(target_os = "windows")]
    pub(crate) scale: f32,
    pub(crate) opacity: f32,
    #[cfg(target_os = "windows")]
    pub(crate) is_lite: bool,
    pub(crate) always_visible: bool,
    #[cfg(target_os = "windows")]
    pub(crate) snap_position: String,
}

pub(crate) fn read_overlay_settings(
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
        self.show_upload_history_viewport(&ctx);

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
}

impl NativeApp {
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
            overmax_capture::restore_foreground_by_title(game_window_title(&settings));
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
        self.tick_auto_upload(ctx);
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
