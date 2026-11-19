//! Windows overlay host: egui viewport geometry, topmost/active-state handling
//! and the in-process overlay panel for `NativeApp`.

use eframe::egui::{self, Vec2, ViewportCommand};

use crate::ui::debug_ui;
use crate::ui::native_app::NativeApp;
use crate::ui::native_app_viewports::{game_window_title, read_overlay_settings};
use crate::ui::overlay_ui;
use windows_sys::Win32::Foundation::HWND;

impl NativeApp {
    pub(crate) fn render_windows_overlay(&mut self, ui: &mut egui::Ui) {
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
            && (ovs.always_visible
                || (self.session.scene != overmax_core::SceneType::Unknown
                    && !self.session.scene.is_ingame()))
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

    #[allow(clippy::too_many_arguments)]
    fn update_overlay_geometry(
        &mut self,
        ctx: &egui::Context,
        scale: f32,
        height: f32,
        snap_position: &str,
        game_rect_val: Option<overmax_frame::WindowRect>,
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
                varchive_upload_needed: self.overlay_upload_needed,
                varchive_upload_progress: self.auto_upload.progress(std::time::Instant::now()),
                varchive_account_configured: self.is_varchive_account_configured(),
                lite_mode: height == overlay_ui::LITE_BASE_HEIGHT,
                is_snap_manual: snap_position == "manual",
                record_manager: &self.record_manager,
                session_initial_record: self.runtime.session_initial_record(),
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
            // 크기가 실제로 다를 때만 보낸다. 매 프레임 InnerSize를 보내면 SetWindowPos가
            // 반복되고, NVIDIA OpenGL 드라이버가 창 크기 처리마다 스핀 대기해 선곡 화면에서
            // UI 스레드 CPU가 코어 1개 가까이 오른다 (#44).
            let current = ui
                .ctx()
                .input(|i| i.viewport().inner_rect.map(|r| r.size()));
            let needs_resize = current
                .is_none_or(|size| (size.x - target_w).abs() > 0.5 || (size.y - fit_h).abs() > 0.5);
            if target_w > 1.0 && fit_h > 1.0 && needs_resize {
                ui.ctx()
                    .send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(target_w, fit_h)));
            }
        }

        if actions.restore_game_focus {
            let settings = self.settings.get_merged();
            overmax_capture::restore_foreground_by_title(game_window_title(&settings));

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

    pub(crate) fn determine_active_state(&self, game_hwnd: Option<HWND>) -> bool {
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
            let title_wide = overmax_capture::encode_wide(&game_title);
            g_hwnd = overmax_capture::find_hwnd_by_title(&title_wide);
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
