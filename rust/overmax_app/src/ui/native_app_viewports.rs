//! Deferred viewports + `eframe::App` (split from `native_app.rs` for file-size limits).

use eframe::egui::{self, Color32, RichText, Vec2, ViewportBuilder, ViewportCommand};
use std::sync::atomic::Ordering;

use crate::system::native_helpers;
use crate::ui::debug_ui;
use crate::ui::native_app::NativeApp;
use crate::ui::overlay_theme::Theme;
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

fn get_local_mouse_pos(ctx: &egui::Context, hwnd_opt: Option<isize>) -> Option<egui::Pos2> {
    #[cfg(target_os = "windows")]
    {
        let hwnd_val = hwnd_opt?;
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let hwnd = hwnd_val as HWND;
        let mut pos = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        unsafe {
            if GetCursorPos(&mut pos) == 0 {
                return None;
            }
            if ScreenToClient(hwnd, &mut pos) == 0 {
                return None;
            }
        }

        let ppi = ctx.pixels_per_point();
        let local_pos = egui::pos2(pos.x as f32 / ppi, pos.y as f32 / ppi);

        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            let size = rect.size();
            let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            if bounds.contains(local_pos) {
                return Some(local_pos);
            }
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd_opt;
        ctx.input(|i| i.pointer.latest_pos())
    }
}

fn draw_custom_cursor(painter: &egui::Painter, p: egui::Pos2) {
    use egui::{Color32, Stroke};
    let len = 6.0;

    // 1. 대비 효과용 검은색 십자 (두껍게)
    let stroke_black = Stroke::new(2.5, Color32::BLACK);
    painter.line_segment(
        [p - egui::vec2(len, 0.0), p + egui::vec2(len, 0.0)],
        stroke_black,
    );
    painter.line_segment(
        [p - egui::vec2(0.0, len), p + egui::vec2(0.0, len)],
        stroke_black,
    );

    // 2. 가시성 확보용 흰색 십자 (얇게)
    let stroke_white = Stroke::new(1.0, Color32::WHITE);
    painter.line_segment(
        [p - egui::vec2(len, 0.0), p + egui::vec2(len, 0.0)],
        stroke_white,
    );
    painter.line_segment(
        [p - egui::vec2(0.0, len), p + egui::vec2(0.0, len)],
        stroke_white,
    );
}

impl NativeApp {
    fn auxiliary_viewport(title: &str, size: [f32; 2]) -> ViewportBuilder {
        ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size)
            .with_visible(true)
            .with_resizable(true)
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
        let rate_ocr = self.debug_state.rate_ocr.clone();
        let rate_ocr_texture = self.debug_state.rate_ocr_texture.clone();
        let title = self.debug_title();
        ctx.show_viewport_deferred(
            native_helpers::vp_debug(),
            Self::auxiliary_viewport(&title, [720.0, 460.0]),
            move |ctx, class| {
                #[cfg(debug_assertions)]
                ctx.style_mut(|s| {
                    s.debug.show_expand_width = false;
                    s.debug.show_expand_height = false;
                    s.debug.show_resize = false;
                    s.debug.show_unaligned = false;
                    s.debug.debug_on_hover = false;
                });
                debug_ui::render_debug(
                    ctx,
                    class,
                    &title,
                    &lines,
                    &paused,
                    &filters,
                    &rate_ocr,
                    &rate_ocr_texture,
                );
                debug_ui::close_if_requested(ctx, &open);
            },
        );
    }

    fn show_settings_viewport(&self, ctx: &egui::Context) {
        if !self.ui_state.settings_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.ui_state.settings_open.clone();
        let draft = self.settings.draft.clone();
        let root = self.root.clone();
        let defaults = self.settings.defaults.clone();
        let base = self.settings.base.clone();
        let merged = self.settings.merged.clone();
        let settings_ctx = settings_ui::SettingsUiContext {
            current_steam_id: self
                .sync_state
                .steam_id
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default(),
            sync_open: self.ui_state.sync_open.clone(),
            scan_pending: self.ui_state.scan_pending.clone(),
            sync_steam_id: self.sync_state.steam_id.clone(),
            fetch_tx: self.sync_channels.fetch_req_tx.clone(),
            steam_users: self.sync_state.steam_users.clone(),
        };
        ctx.show_viewport_deferred(
            native_helpers::vp_settings(),
            Self::auxiliary_viewport("Overmax 설정", [520.0, 560.0]),
            move |ctx, class| {
                ctx.set_pixels_per_point(1.0);
                #[cfg(debug_assertions)]
                ctx.style_mut(|s| {
                    s.debug.show_expand_width = false;
                    s.debug.show_expand_height = false;
                    s.debug.show_resize = false;
                    s.debug.show_unaligned = false;
                    s.debug.debug_on_hover = false;
                });
                let mut local_draft = overmax_core::lock_clone_or_default(&draft);
                egui::TopBottomPanel::bottom("sett_actions")
                    .frame(
                        egui::Frame::new()
                            .fill(Theme::PANEL_BG)
                            .inner_margin(egui::Margin::symmetric(24, 16)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let close_btn = egui::Button::new(
                                        RichText::new("닫기").size(Theme::FONT_BODY),
                                    )
                                    .min_size(egui::vec2(80.0, Theme::CONTROL_HEIGHT))
                                    .fill(Theme::SECONDARY)
                                    .corner_radius(egui::CornerRadius::same(Theme::R_SM));
                                    if ui.add(close_btn).clicked() {
                                        open.store(false, Ordering::Relaxed);
                                        ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
                                    }

                                    ui.add_space(8.0);

                                    let save_btn = egui::Button::new(
                                        RichText::new("저장").size(Theme::FONT_BODY).strong(),
                                    )
                                    .min_size(egui::vec2(100.0, Theme::CONTROL_HEIGHT))
                                    .fill(Theme::PRIMARY)
                                    .corner_radius(egui::CornerRadius::same(Theme::R_SM));
                                    if ui.add(save_btn).clicked() {
                                        let base_g = overmax_core::lock_clone_or_default(&base);
                                        let mut merged_g =
                                            overmax_core::lock_clone_or_default(&merged);
                                        let _ = settings_ui::save_settings_to_disk(
                                            root.as_ref(),
                                            defaults.as_ref(),
                                            &base_g,
                                            &mut local_draft,
                                            &mut merged_g,
                                        );
                                        if let Ok(mut m) = merged.lock() {
                                            *m = merged_g;
                                        }
                                    }
                                },
                            );
                        });
                    });
                settings_ui::render_settings_deferred(
                    ctx,
                    class,
                    "설정",
                    &mut local_draft,
                    &settings_ctx,
                );
                if let Ok(mut d) = draft.lock() {
                    *d = local_draft;
                }
                settings_ui::close_if_requested(ctx, &open);
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
        ctx.show_viewport_deferred(
            native_helpers::vp_sync(),
            Self::auxiliary_viewport("V-Archive 동기화", [520.0, 560.0]),
            move |ctx, class| {
                ctx.set_pixels_per_point(1.0);
                #[cfg(debug_assertions)]
                ctx.style_mut(|s| {
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
                    ctx,
                    class,
                    sync_ui::SyncProps {
                        steam_id: &mut steam_g,
                        status: &status_s,
                        candidates: &list,
                        steam_users: &users,
                        on_scan: || {
                            scan_pending.store(true, Ordering::Relaxed);
                        },
                        on_upload: |i| {
                            let _ = upload_tx.send(i);
                        },
                        on_delete: |i| {
                            let _ = delete_tx.send(i);
                        },
                    },
                );
                sync_ui::close_if_requested(ctx, &open);
            },
        );
    }
}

struct OverlaySettingsSnapshot {
    scale: f32,
    opacity: f32,
    is_lite: bool,
    snap_position: String,
}

fn read_overlay_settings(
    settings: &std::sync::Arc<std::sync::Mutex<serde_json::Value>>,
) -> OverlaySettingsSnapshot {
    let Ok(m) = settings.lock() else {
        return OverlaySettingsSnapshot {
            scale: 1.0,
            opacity: 0.8,
            is_lite: false,
            snap_position: "manual".into(),
        };
    };
    let overlay = m.get("overlay");
    OverlaySettingsSnapshot {
        scale: overlay
            .and_then(|o| o.get("scale"))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32,
        opacity: overlay
            .and_then(|o| o.get("base_opacity"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8) as f32,
        is_lite: overlay
            .and_then(|o| o.get("lite_mode"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        snap_position: overlay
            .and_then(|o| o.get("position"))
            .and_then(|p| p.get("snap"))
            .and_then(|v| v.as_str())
            .unwrap_or("manual")
            .to_string(),
    }
}

impl eframe::App for NativeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                    ctx.style_mut(|s| {
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

        if self.exit_requested.load(Ordering::Relaxed) {
            self.ui_state.settings_open.store(false, Ordering::Relaxed);
            self.ui_state.sync_open.store(false, Ordering::Relaxed);
            self.ui_state.debug_open.store(false, Ordering::Relaxed);
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        self.poll_and_drain_events(ctx);

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
        let overlay_on = game_found && self.confidence > 0.1;

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

        self.show_debug_viewport(ctx);
        self.show_settings_viewport(ctx);
        self.show_sync_viewport(ctx);

        self.render_overlay_panel(
            ctx,
            scale,
            height,
            &snap_position,
            overlay_on,
            &mut force_topmost,
        );

        // Windows 전용: 전체 창 투명도 및 최상위 권한 적용
        #[cfg(target_os = "windows")]
        {
            if overlay_on {
                let found = self.apply_window_opacity(opacity, force_topmost);
                if !found && !self.win_cache.logged_opacity_fail {
                    debug_ui::push_log(
                        &self.debug_state.log_lines,
                        self.max_log_lines(),
                        format!(
                            "[Overlay] 투명도 조절용 창 핸들을 찾지 못함 (Opacity: {:.2})",
                            opacity
                        ),
                    );
                    self.win_cache.logged_opacity_fail = true;
                }
            } else {
                // 숨겨질 때: 투명도를 즉시 0.0(완전 투명)으로 덮어씌워 윈도우 잔상 소멸을 보장
                if let Some(hwnd) = self.find_overlay_window() {
                    unsafe {
                        windows_sys::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes(
                            hwnd as _, 0, 0, 0x00000002,
                        );
                    }
                }
                self.win_cache.cached_hwnd = None;
                self.win_cache.last_applied_opacity = None;
            }
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // [R, G, B, A] - 윈도우 버퍼를 완전 투명하게 설정.
        // OS 레벨의 전역 투명도(Layered Window Alpha)와 함께 작동함.
        [0.0, 0.0, 0.0, 0.0]
    }
}

impl NativeApp {
    fn poll_and_drain_events(&mut self, ctx: &egui::Context) {
        let settings_on = self.ui_state.settings_open.load(Ordering::Relaxed);
        let sync_on = self.ui_state.sync_open.load(Ordering::Relaxed);

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

        self.start_log_pump(ctx);
        ctx.request_repaint_after(std::time::Duration::from_secs(5));
        self.drain_detection_results(ctx);
        if self.drain_ui_commands() {
            ctx.request_repaint();
        }
        self.poll_scan_requests(ctx);
        self.poll_upload_requests(ctx);
        self.poll_fetch_requests(ctx);
        self.drain_sync_scan();
        self.drain_upload_results();
        self.drain_fetch_results();
        self.poll_delete_requests(ctx);
        self.drain_game_found_refresh_steam();
        if self.state_tracker.prev_protected.update(Some(true)) {
            ctx.send_viewport_cmd(ViewportCommand::ContentProtected(true));
        }
    }

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
                    (overlay_ui::BASE_WIDTH * scale).ceil() + 2.0,
                    (height * scale).ceil() + 2.0,
                )));
            } else {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(1.0, 1.0)));
            }
        }

        // 마우스가 오버레이 영역 위에 있을 때만 상호작용 가능하게 함 (보조창 조작을 위해)
        #[cfg(target_os = "windows")]
        let local_mouse = get_local_mouse_pos(ctx, self.win_cache.cached_hwnd);
        #[cfg(not(target_os = "windows"))]
        let local_mouse = get_local_mouse_pos(ctx, None);

        let is_over = local_mouse.is_some() || self.is_dragging;
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
        if overlay_on && is_over && (mouse_moved || self.is_dragging) {
            ctx.request_repaint();
        }

        // 오버레이 메인 창이 우발적으로 포커스를 획득했을 때, 포커스를 자동으로 게임 창으로 되돌려 키 입력 씹힘 방지.
        // 단, snap이 manual(수동 위치 조정 모드)인 경우 사용자가 고의로 오버레이 창을 조작(드래그) 중이므로 예외로 둡니다.
        #[cfg(target_os = "windows")]
        {
            if overlay_on && snap_position != "manual" {
                if let (Some(overlay_hwnd), Some(game_hwnd)) =
                    (self.win_cache.cached_hwnd, self.win_cache.cached_game_hwnd)
                {
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
        }

        let mut force_topmost = false;
        if overlay_on && overlay_on_changed {
            force_topmost = true;
        }

        // Windows 전용: 라이트 모드 구석 고정 위치 강제 적용
        #[cfg(target_os = "windows")]
        {
            if overlay_on && snap_position != "manual" {
                if let Some(hwnd_val) = self.win_cache.cached_hwnd {
                    if let Some(g_rect) = game_rect_val {
                        use windows_sys::Win32::UI::WindowsAndMessaging::*;
                        let hwnd = hwnd_val as HWND;
                        let ppi = ctx.pixels_per_point();
                        let overlay_w_px =
                            (((overlay_ui::BASE_WIDTH * scale).ceil() + 2.0) * ppi) as i32;
                        let overlay_h_px = (((height * scale).ceil() + 2.0) * ppi) as i32;
                        let margin_px = (16.0 * ppi) as i32;

                        let (px, py) = match snap_position {
                            "top_left" => (g_rect.left + margin_px, g_rect.top + margin_px),
                            "top_right" => (
                                g_rect.left + g_rect.width - overlay_w_px - margin_px,
                                g_rect.top + margin_px,
                            ),
                            "bottom_left" => (
                                g_rect.left + margin_px,
                                g_rect.top + g_rect.height - overlay_h_px - margin_px,
                            ),
                            _ => {
                                // bottom_right
                                (
                                    g_rect.left + g_rect.width - overlay_w_px - margin_px,
                                    g_rect.top + g_rect.height - overlay_h_px - margin_px,
                                )
                            }
                        };

                        // 이전 설정 좌표 및 크기와 다른 경우에만 SetWindowPos 호출
                        let current_geom = (px, py, overlay_w_px, overlay_h_px);
                        let geom_changed = self.win_cache.prev_snap_geometry != Some(current_geom);

                        if geom_changed {
                            unsafe {
                                SetWindowPos(
                                    hwnd,
                                    HWND_TOPMOST,
                                    px,
                                    py,
                                    overlay_w_px,
                                    overlay_h_px,
                                    SWP_NOACTIVATE,
                                );
                            }
                            self.win_cache.prev_snap_geometry = Some(current_geom);
                        }
                    }
                }
            }
        }

        force_topmost
    }

    fn render_overlay_panel(
        &mut self,
        ctx: &egui::Context,
        scale: f32,
        height: f32,
        snap_position: &str,
        overlay_on: bool,
        force_topmost: &mut bool,
    ) {
        #[cfg(target_os = "windows")]
        let local_mouse = get_local_mouse_pos(ctx, self.win_cache.cached_hwnd);
        #[cfg(not(target_os = "windows"))]
        let local_mouse = get_local_mouse_pos(ctx, None);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgba_unmultiplied(0, 0, 0, 0)))
            .show(ctx, |ui| {
                if !overlay_on {
                    self.last_painted_rect = None;
                    return;
                }

                // 하드웨어 커서가 나타나 가상 커서와 이중으로 보이는 현상을 예방하기 위해 하드웨어 커서를 숨김
                if local_mouse.is_some() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                }

                if let Some(toast) = &self.toast {
                    if std::time::Instant::now() >= toast.expires_at {
                        self.toast = None;
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
                        varchive_upload_needed: self.current_pattern_needs_upload(),
                        varchive_account_configured: self.is_varchive_account_configured(),
                        lite_mode: height == overlay_ui::LITE_BASE_HEIGHT,
                        is_snap_manual: snap_position == "manual",
                        record_manager: &self.record_manager,
                        session_initial_record: self.session_initial_record,
                        toast: self.toast.as_ref(),
                    },
                );

                if actions.command == Some(crate::ui::ui_command::UiCommand::UploadCurrentPattern) {
                    self.upload_current_pattern(ctx.clone());
                }

                if actions.start_drag {
                    self.is_dragging = true;
                }
                if actions.restore_game_focus || ctx.input(|i| !i.pointer.any_down()) {
                    self.is_dragging = false;
                }

                #[cfg(target_os = "windows")]
                self.handle_window_drag(ctx, actions.start_drag);
                #[cfg(not(target_os = "windows"))]
                if actions.start_drag {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }

                if actions.restore_game_focus {
                    let max_log_lines = self.max_log_lines();
                    let settings = self.settings.get_merged();
                    window_tracker::restore_foreground_by_title(game_window_title(&settings));

                    if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                        if let Ok(mut draft) = self.settings.draft.lock() {
                            if let Ok(mut merged_lock) = self.settings.merged.lock() {
                                let mut overlay = merged_lock
                                    .get("overlay")
                                    .cloned()
                                    .unwrap_or_else(|| serde_json::json!({}));
                                if let Some(overlay_obj) = overlay.as_object_mut() {
                                    let mut position_map = overlay_obj
                                        .get("position")
                                        .and_then(|v| v.as_object())
                                        .cloned()
                                        .unwrap_or_default();
                                    position_map.insert(
                                        "x".to_string(),
                                        serde_json::json!(rect.min.x as i32),
                                    );
                                    position_map.insert(
                                        "y".to_string(),
                                        serde_json::json!(rect.min.y as i32),
                                    );
                                    overlay_obj.insert(
                                        "position".to_string(),
                                        serde_json::Value::Object(position_map),
                                    );
                                }
                                merged_lock["overlay"] = overlay.clone();
                                draft["overlay"] = overlay;

                                let base_g =
                                    overmax_core::lock_clone_or_default(&self.settings.base);
                                let _ = settings_ui::save_settings_to_disk(
                                    self.root.as_ref(),
                                    self.settings.defaults.as_ref(),
                                    &base_g,
                                    &mut draft,
                                    &mut merged_lock,
                                );
                                debug_ui::push_log(
                                    &self.debug_state.log_lines,
                                    max_log_lines,
                                    format!(
                                        "[Overlay] 오버레이 위치 저장 (user.json): ({},{})",
                                        rect.min.x as i32, rect.min.y as i32
                                    ),
                                );
                            }
                        }
                    }
                }
                if let Some(command) = actions.command {
                    self.handle_ui_command(command);
                    ctx.request_repaint();
                }
                if actions.restore_game_focus || actions.start_drag {
                    *force_topmost = true;
                }
                if let Some(mouse_pos) = local_mouse {
                    // 비활성 윈도우 마우스 커서 숨김 제약을 우회하기 위해 가상 커서를 마우스 위치에 직접 렌더링
                    draw_custom_cursor(ui.painter(), mouse_pos);
                }
                self.last_painted_rect = actions.response_rect;
            });
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

        if fg == g_hwnd {
            return true;
        }

        unsafe {
            let mut fg_pid = 0u32;
            GetWindowThreadProcessId(fg, &mut fg_pid);
            let my_pid = GetCurrentProcessId();
            fg_pid == my_pid
        }
    }

    fn check_cached_window_opacity(
        &self,
        hwnd: HWND,
        target_opacity: f32,
        is_active: bool,
    ) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        if unsafe { IsWindow(hwnd) } == 0 {
            return false;
        }
        let style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) };
        let mut target_mask =
            WS_EX_LAYERED as i32 | WS_EX_NOACTIVATE as i32 | WS_EX_TOOLWINDOW as i32;
        if is_active {
            target_mask |= WS_EX_TOPMOST as i32;
        }

        let topmost_ok = if is_active {
            (style & WS_EX_TOPMOST as i32) != 0
        } else {
            (style & WS_EX_TOPMOST as i32) == 0
        };

        if (style & target_mask) != target_mask || !topmost_ok {
            return false;
        }
        let mut alpha = 0u8;
        let mut flags = 0u32;
        let success = unsafe {
            GetLayeredWindowAttributes(hwnd, std::ptr::null_mut(), &mut alpha, &mut flags)
        };
        if success == 0 || (flags & 0x00000002) == 0 {
            return false;
        }
        let current_opacity = alpha as f32 / 255.0;
        (target_opacity - current_opacity).abs() < 0.005
    }

    fn find_overlay_window(&self) -> Option<HWND> {
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        struct EnumData {
            target_pid: u32,
            found_hwnd: Option<HWND>,
        }

        let target_pid = unsafe { GetCurrentProcessId() };
        let mut data = EnumData {
            target_pid,
            found_hwnd: None,
        };

        unsafe {
            extern "system" fn enum_callback(hwnd: HWND, lparam: isize) -> i32 {
                unsafe {
                    let data = &mut *(lparam as *mut EnumData);
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(hwnd, &mut pid);
                    if pid == data.target_pid {
                        let mut text = [0u16; 512];
                        let len = GetWindowTextW(hwnd, text.as_mut_ptr(), 512);
                        let title = String::from_utf16_lossy(&text[..len as usize]);
                        let visible = IsWindowVisible(hwnd) != 0;

                        if title == "Overmax" {
                            data.found_hwnd = Some(hwnd);
                            return 0; // 즉시 중단
                        }

                        if data.found_hwnd.is_none() && (title.contains("Overmax") || visible) {
                            data.found_hwnd = Some(hwnd);
                        }
                    }
                    1
                }
            }

            EnumWindows(Some(enum_callback), &mut data as *mut _ as isize);
        }
        data.found_hwnd
    }

    fn game_hwnd_cached(&mut self) -> Option<HWND> {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let mut g_hwnd = self.win_cache.cached_game_hwnd.map(|h| h as HWND);
        let is_valid = g_hwnd.map(|h| unsafe { IsWindow(h) } != 0).unwrap_or(false);
        if !is_valid {
            let settings = self.settings.get_merged();
            let game_title = game_window_title(&settings).to_string();
            let title_wide = window_tracker::encode_wide(&game_title);
            g_hwnd = window_tracker::find_hwnd_by_title(&title_wide);
            self.win_cache.cached_game_hwnd = g_hwnd.map(|h| h as isize);
        }
        g_hwnd
    }

    fn apply_style_and_opacity(hwnd: HWND, is_active: bool, opacity: f32) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        unsafe {
            if IsWindow(hwnd) == 0 {
                return false;
            }
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            let topmost_flag = if is_active { WS_EX_TOPMOST as i32 } else { 0 };
            let target_style = (style & !(WS_EX_TOPMOST as i32))
                | topmost_flag
                | WS_EX_LAYERED as i32
                | WS_EX_NOACTIVATE as i32
                | WS_EX_TOOLWINDOW as i32;
            if style != target_style {
                SetWindowLongW(hwnd, GWL_EXSTYLE, target_style);
            }
            SetWindowPos(
                hwnd,
                if is_active {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            SetLayeredWindowAttributes(hwnd, 0, (opacity * 255.0) as u8, 0x00000002) != 0
        }
    }

    fn apply_window_opacity(&mut self, opacity: f32, force_topmost: bool) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        // 1. 캐싱된 핸들이 있고 투명도가 올바르게 유지되고 있다면 조기 반환
        if let Some(hwnd_val) = self.win_cache.cached_hwnd {
            let hwnd = hwnd_val as HWND;
            let game_hwnd = self.game_hwnd_cached();

            // 게임 창을 Owner로 지정하여 항상 오버레이가 게임 위에 렌더링되도록 보장
            if let Some(g_hwnd) = game_hwnd {
                unsafe {
                    let current_owner = GetWindowLongPtrW(hwnd, GWL_HWNDPARENT) as HWND;
                    if current_owner != g_hwnd {
                        SetWindowLongPtrW(hwnd, GWL_HWNDPARENT, g_hwnd as isize);
                        SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                        );
                    }
                }
            }

            // 현재 활성 상태 검사
            let is_active = self.determine_active_state(game_hwnd);

            if force_topmost {
                unsafe {
                    SetWindowPos(
                        hwnd,
                        if is_active {
                            HWND_TOPMOST
                        } else {
                            HWND_NOTOPMOST
                        },
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }

            if self.check_cached_window_opacity(hwnd, opacity, is_active) {
                return true;
            }

            // 캐시된 핸들은 유효하나 스타일이나 투명도가 풀린 경우: 바로 재적용 시도
            if Self::apply_style_and_opacity(hwnd, is_active, opacity) {
                self.win_cache.last_applied_opacity = Some(opacity);
                return true;
            }
        }

        // 2. 캐싱된 핸들이 없거나 유효하지 않은 경우 새롭게 검색 후 적용
        if let Some(hwnd) = self.find_overlay_window() {
            debug_ui::push_log(
                &self.debug_state.log_lines,
                self.max_log_lines(),
                format!(
                    "[Win32] 투명도 업데이트 시도: {:.2} (HWND: {:?})",
                    opacity, hwnd
                ),
            );

            let game_hwnd = self.game_hwnd_cached();

            if let Some(g_hwnd) = game_hwnd {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWL_HWNDPARENT, g_hwnd as isize);
                }
            }

            let is_active = self.determine_active_state(game_hwnd);

            if Self::apply_style_and_opacity(hwnd, is_active, opacity) {
                self.win_cache.cached_hwnd = Some(hwnd as isize);
                self.win_cache.last_applied_opacity = Some(opacity);
                return true;
            }
        }
        false
    }

    fn handle_window_drag(&mut self, ctx: &egui::Context, start_drag: bool) {
        if start_drag {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
    }
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
