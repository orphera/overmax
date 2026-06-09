//! Deferred viewports + `eframe::App` (split from `native_app.rs` for file-size limits).

use eframe::egui::{self, Color32, RichText, Vec2, ViewportBuilder, ViewportCommand};
use std::sync::atomic::Ordering;

use crate::debug_ui;
use crate::native_app::NativeApp;
use crate::native_helpers;
use crate::overlay_theme::Theme;
use crate::overlay_ui;
use crate::settings_ui;
use crate::sync_ui;
use crate::window_tracker;

fn game_window_title(settings: &serde_json::Value) -> &str {
    settings
        .get("window_tracker")
        .and_then(|v| v.get("window_title"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("DJMAX RESPECT V")
}

fn is_mouse_over_overlay(ctx: &egui::Context, last_painted_rect: Option<egui::Rect>) -> bool {
    let Some(rect) = ctx.input(|i| i.viewport().outer_rect) else {
        return false;
    };
    let mut pos = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pos);
    }
    let ppi = ctx.pixels_per_point();
    let mouse_pos = egui::pos2(pos.x as f32 / ppi, pos.y as f32 / ppi);
    if let Some(paint_rect) = last_painted_rect {
        let global_paint_rect = paint_rect.translate(rect.min.to_vec2());
        global_paint_rect.contains(mouse_pos)
    } else {
        rect.contains(mouse_pos)
    }
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
            fetch_tx: self.fetch_req_tx.clone(),
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
                let mut local_draft = draft.lock().map(|g| g.clone()).unwrap_or_default();
                egui::TopBottomPanel::bottom("sett_actions")
                    .frame(egui::Frame::new().fill(Theme::PANEL_BG).inner_margin(egui::Margin::symmetric(24, 16)))
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let close_btn = egui::Button::new(RichText::new("닫기").size(Theme::FONT_BODY))
                                    .min_size(egui::vec2(80.0, Theme::CONTROL_HEIGHT))
                                    .fill(Theme::SECONDARY)
                                    .corner_radius(egui::CornerRadius::same(Theme::R_SM));
                                if ui.add(close_btn).clicked() {
                                    open.store(false, Ordering::Relaxed);
                                    ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
                                }
                                
                                ui.add_space(8.0);
                                
                                let save_btn = egui::Button::new(RichText::new("저장").size(Theme::FONT_BODY).strong())
                                    .min_size(egui::vec2(100.0, Theme::CONTROL_HEIGHT))
                                    .fill(Theme::PRIMARY)
                                    .corner_radius(egui::CornerRadius::same(Theme::R_SM));
                                if ui.add(save_btn).clicked() {
                                    let base_g = base.lock().map(|g| g.clone()).unwrap_or_default();
                                    let mut merged_g = merged.lock().map(|g| g.clone()).unwrap_or_default();
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
                            });
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
        let upload_tx = self.upload_req_tx.clone();
        let delete_tx = self.delete_req_tx.clone();
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
                let list = sync_state.candidates.lock().map(|g| g.clone()).unwrap_or_default();
                let users = sync_state.steam_users.lock().unwrap_or_else(|e| e.into_inner());
                let mut steam_g = sync_state.steam_id.lock().unwrap_or_else(|e| e.into_inner());
                let status_s = sync_state.status.lock().map(|g| g.clone()).unwrap_or_default();
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
                static STYLE_INIT: std::cell::Cell<bool> = std::cell::Cell::new(false);
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
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        let settings_on = self.ui_state.settings_open.load(Ordering::Relaxed);
        let sync_on = self.ui_state.sync_open.load(Ordering::Relaxed);

        if settings_on && !self.prev_settings_open {
            if let (Ok(m), Ok(mut d)) = (self.settings.merged.lock(), self.settings.draft.lock()) {
                *d = m.clone();
            }
            self.refresh_steam_session("설정 창 열림");
        }
        self.prev_settings_open = settings_on;

        if sync_on && !self.prev_sync_open {
            self.refresh_steam_session("동기화 창 열림");
        }
        self.prev_sync_open = sync_on;

        self.start_log_pump(ctx);
        ctx.request_repaint_after(std::time::Duration::from_secs(5));
        self.drain_detection_results();
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
        if self.prev_protected != Some(true) {
            ctx.send_viewport_cmd(ViewportCommand::ContentProtected(true));
            self.prev_protected = Some(true);
        }

        let scale = if let Ok(m) = self.settings.merged.lock() {

            m.get("overlay").and_then(|o| o.get("scale")).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32
        } else {
            1.0
        };

        let opacity = if let Ok(m) = self.settings.merged.lock() {
            m.get("overlay").and_then(|o| o.get("base_opacity")).and_then(|v| v.as_f64()).unwrap_or(0.8) as f32
        } else {
            0.8
        };

        // 라이트 모드 판정: 설정에서 명시적으로 활성화했을 때만 적용
        let is_lite = if let Ok(m) = self.settings.merged.lock() {
            m.get("overlay").and_then(|o| o.get("lite_mode")).and_then(|v| v.as_bool()).unwrap_or(false)
        } else {
            false
        };

        let lite_position = if let Ok(m) = self.settings.merged.lock() {
            m.get("overlay")
                .and_then(|o| o.get("lite_position"))
                .and_then(|v| v.as_str())
                .unwrap_or("top_right")
                .to_string()
        } else {
            "top_right".to_string()
        };

        let height = if is_lite {
            overlay_ui::LITE_BASE_HEIGHT
        } else {
            overlay_ui::BASE_HEIGHT
        };

        let game_found = self.game_rect.lock().map(|r| r.is_some()).unwrap_or(false);
        let overlay_on = game_found && self.confidence > 0.1;

        if overlay_on != self.prev_overlay_on 
            || (overlay_on && ((scale - self.prev_scale).abs() > 0.001 || is_lite != self.prev_is_lite))
        {
            debug_ui::push_log(
                &self.debug_state.log_lines,
                1000,
                format!(
                    "[Overlay] 레이아웃 업데이트: ON={}->{}, Scale={:.2}->{:.2}, Lite={}->{} (Game: {}, Conf: {:.2})",
                    self.prev_overlay_on,
                    overlay_on,
                    self.prev_scale,
                    scale,
                    self.prev_is_lite,
                    is_lite,
                    game_found,
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

        self.prev_overlay_on = overlay_on;
        self.prev_scale = scale;
        self.prev_is_lite = is_lite;

        let _visible_size = Vec2::new(overlay_ui::BASE_WIDTH * scale, height * scale);
        let _hidden_size = Vec2::new(1.0, 1.0);

        // 마우스가 오버레이 영역 위에 있을 때만 상호작용 가능하게 함 (보조창 조작을 위해)
        let is_over = is_mouse_over_overlay(ctx, self.last_painted_rect);
        let passthrough = !overlay_on || !is_over;
        if self.prev_passthrough != Some(passthrough) {
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(passthrough));
            self.prev_passthrough = Some(passthrough);
        }

        let mut force_topmost = false;
        if overlay_on && !self.prev_overlay_on {
            force_topmost = true;
        }

        self.show_debug_viewport(ctx);
        self.show_settings_viewport(ctx);
        self.show_sync_viewport(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgba_unmultiplied(0, 0, 0, 0)))
            .show(ctx, |ui| {
                if !overlay_on {
                    self.last_painted_rect = None;
                    return;
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
                        lite_mode: is_lite,
                    },
                );

                if actions.command == Some(crate::ui_command::UiCommand::UploadCurrentPattern) {
                    self.upload_current_pattern(ctx.clone());
                }

                if actions.start_drag {
                    if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                        self.drag_start_win_pos = Some(rect.min);
                        let mut pos = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
                        unsafe {
                            windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pos);
                        }
                        self.drag_start_cursor = Some((pos.x, pos.y));
                    }
                }

                if actions.drag_delta.is_some() {
                    if let (Some(start_win_pos), Some(start_cursor)) = (self.drag_start_win_pos, self.drag_start_cursor) {
                        let mut pos = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
                        unsafe {
                            windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pos);
                        }
                        let ppi = ctx.pixels_per_point();
                        let dx = (pos.x - start_cursor.0) as f32 / ppi;
                        let dy = (pos.y - start_cursor.1) as f32 / ppi;
                        let new_pos = start_win_pos + egui::vec2(dx, dy);

                        #[cfg(target_os = "windows")]
                        {
                            use windows_sys::Win32::UI::WindowsAndMessaging::*;
                            if let Some(hwnd_val) = self.cached_hwnd {
                                let hwnd = hwnd_val as HWND;
                                let px = (new_pos.x * ppi) as i32;
                                let py = (new_pos.y * ppi) as i32;
                                unsafe {
                                    SetWindowPos(hwnd, HWND_TOPMOST, px, py, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                                }
                            } else {
                                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(new_pos));
                            }
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(new_pos));
                        }
                    }
                }

                if actions.restore_game_focus {
                    self.drag_start_cursor = None;
                    self.drag_start_win_pos = None;

                    let max_log_lines = self.max_log_lines();
                    if let Ok(mut settings) = self.settings.merged.lock() {
                        window_tracker::restore_foreground_by_title(game_window_title(&settings));
                        
                        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                            if let Ok(mut draft) = self.settings.draft.lock() {
                                let mut overlay = settings.get("overlay").cloned().unwrap_or_else(|| serde_json::json!({}));
                                if let Some(overlay_obj) = overlay.as_object_mut() {
                                    overlay_obj.insert("position".to_string(), serde_json::json!({
                                        "x": rect.min.x as i32,
                                        "y": rect.min.y as i32
                                    }));
                                }
                                settings["overlay"] = overlay.clone();
                                draft["overlay"] = overlay;

                                let base_g = self.settings.base.lock().map(|g| g.clone()).unwrap_or_default();
                                let _ = settings_ui::save_settings_to_disk(
                                    self.root.as_ref(),
                                    self.settings.defaults.as_ref(),
                                    &base_g,
                                    &mut draft,
                                    &mut settings,
                                );
                                debug_ui::push_log(
                                    &self.debug_state.log_lines,
                                    max_log_lines,
                                    format!("[Overlay] 오버레이 위치 저장 (user.json): ({},{})", rect.min.x as i32, rect.min.y as i32),
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
                    force_topmost = true;
                }
                self.last_painted_rect = actions.response_rect;
            });

        // Windows 전용: 라이트 모드 구석 고정 위치 강제 적용
        #[cfg(target_os = "windows")]
        {
            if overlay_on && is_lite {
                if let Some(hwnd_val) = self.cached_hwnd {
                    if let Ok(g_rect_opt) = self.game_rect.lock() {
                        if let Some(g_rect) = *g_rect_opt {
                            use windows_sys::Win32::UI::WindowsAndMessaging::*;
                            let hwnd = hwnd_val as HWND;
                            let ppi = ctx.pixels_per_point();
                            let overlay_w_px = (((overlay_ui::BASE_WIDTH * scale).ceil() + 2.0) * ppi) as i32;
                            let overlay_h_px = (((height * scale).ceil() + 2.0) * ppi) as i32;
                            let margin_px = (16.0 * ppi) as i32;
                            
                            let (px, py) = match lite_position.as_str() {
                                "top_left" => {
                                    (g_rect.left + margin_px, g_rect.top + margin_px)
                                }
                                "top_right" => {
                                    (g_rect.left + g_rect.width - overlay_w_px - margin_px, g_rect.top + margin_px)
                                }
                                "bottom_left" => {
                                    (g_rect.left + margin_px, g_rect.top + g_rect.height - overlay_h_px - margin_px)
                                }
                                _ => { // bottom_right
                                    (g_rect.left + g_rect.width - overlay_w_px - margin_px, g_rect.top + g_rect.height - overlay_h_px - margin_px)
                                }
                            };
                            
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
                        }
                    }
                }
            }
        }

        // Windows 전용: 전체 창 투명도 및 최상위 권한 적용
        #[cfg(target_os = "windows")]
        {
            let now = ctx.input(|i| i.time);
            static mut TOPMOST_TIMER: f64 = 0.0;
            if overlay_on {
                unsafe {
                    if now - TOPMOST_TIMER > 1.0 {
                        force_topmost = true;
                        TOPMOST_TIMER = now;
                    }
                }
                let found = self.apply_window_opacity(opacity, force_topmost);
                if !found {
                    static mut LOGGED: bool = false;
                    unsafe {
                        if !LOGGED {
                            debug_ui::push_log(&self.debug_state.log_lines, self.max_log_lines(), format!("[Overlay] 투명도 조절용 창 핸들을 찾지 못함 (Opacity: {:.2})", opacity));
                            LOGGED = true;
                        }
                    }
                }
            } else {
                self.cached_hwnd = None;
                self.last_applied_opacity = None;
            }
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // [R, G, B, A] - 윈도우 버퍼를 완전 투명하게 설정.
        // OS 레벨의 전역 투명도(Layered Window Alpha)와 함께 작동함.
        [0.0, 0.0, 0.0, 0.0]
    }
}

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::HWND;

#[cfg(target_os = "windows")]
impl NativeApp {
    fn check_cached_window_opacity(&self, hwnd: HWND, target_opacity: f32) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        if unsafe { IsWindow(hwnd) } == 0 {
            return false;
        }
        let style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) };
        let target_mask = WS_EX_LAYERED as i32 | WS_EX_NOACTIVATE as i32 | WS_EX_TOOLWINDOW as i32 | WS_EX_TOPMOST as i32;
        if (style & target_mask) != target_mask {
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
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;
        
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

    fn apply_window_opacity(&mut self, opacity: f32, force_topmost: bool) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        // 1. 캐싱된 핸들이 있고 투명도가 올바르게 유지되고 있다면 조기 반환
        if let Some(hwnd_val) = self.cached_hwnd {
            let hwnd = hwnd_val as HWND;

            // 게임 창을 Owner로 지정하여 항상 오버레이가 게임 위에 렌더링되도록 보장
            let game_title = if let Ok(m) = self.settings.merged.lock() {
                game_window_title(&m).to_string()
            } else {
                "DJMAX RESPECT V".to_string()
            };
            let title_wide = window_tracker::encode_wide(&game_title);
            if let Some(g_hwnd) = window_tracker::find_hwnd_by_title(&title_wide) {
                unsafe {
                    let current_owner = GetWindowLongPtrW(hwnd, GWL_HWNDPARENT) as HWND;
                    if current_owner != g_hwnd {
                        SetWindowLongPtrW(hwnd, GWL_HWNDPARENT, g_hwnd as isize);
                        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
                    }
                }
            }

            if force_topmost {
                unsafe {
                    SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
            if self.check_cached_window_opacity(hwnd, opacity) {
                return true;
            }
            
            // 캐시된 핸들은 유효하나 스타일이 풀린 경우: 바로 재적용 시도
            if unsafe { IsWindow(hwnd) } != 0 {
                let style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) };
                let target_style = style | WS_EX_LAYERED as i32 | WS_EX_NOACTIVATE as i32 | WS_EX_TOOLWINDOW as i32 | WS_EX_TOPMOST as i32;
                if style != target_style {
                    unsafe { SetWindowLongW(hwnd, GWL_EXSTYLE, target_style) };
                }
                unsafe {
                    SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
                }
                if unsafe { SetLayeredWindowAttributes(hwnd, 0, (opacity * 255.0) as u8, 0x00000002) } != 0 {
                    self.last_applied_opacity = Some(opacity);
                    return true;
                }
            }
        }

        // 2. 캐싱된 핸들이 없거나 유효하지 않은 경우 새롭게 검색 후 적용
        if let Some(hwnd) = self.find_overlay_window() {
            debug_ui::push_log(
                &self.debug_state.log_lines,
                self.max_log_lines(),
                format!("[Win32] 투명도 업데이트 시도: {:.2} (HWND: {:?})", opacity, hwnd),
            );

            // 신규 오버레이 윈도우 감지 시 게임 창을 Owner로 연결
            let game_title = if let Ok(m) = self.settings.merged.lock() {
                game_window_title(&m).to_string()
            } else {
                "DJMAX RESPECT V".to_string()
            };
            let title_wide = window_tracker::encode_wide(&game_title);
            if let Some(g_hwnd) = window_tracker::find_hwnd_by_title(&title_wide) {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWL_HWNDPARENT, g_hwnd as isize);
                }
            }

            unsafe {
                let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                let target_style = style | WS_EX_LAYERED as i32 | WS_EX_NOACTIVATE as i32 | WS_EX_TOOLWINDOW as i32 | WS_EX_TOPMOST as i32;
                if style != target_style {
                    SetWindowLongW(hwnd, GWL_EXSTYLE, target_style);
                }
                SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
                if SetLayeredWindowAttributes(hwnd, 0, (opacity * 255.0) as u8, 0x00000002) != 0 {
                    self.cached_hwnd = Some(hwnd as isize);
                    self.last_applied_opacity = Some(opacity);
                    return true;
                }
            }
        }
        false
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
        let settings = serde_json::json!({
            "window_tracker": {"window_title": "DJMAX TEST"}
        });

        assert_eq!(super::game_window_title(&settings), "DJMAX TEST");
        assert_eq!(
            super::game_window_title(&serde_json::json!({})),
            "DJMAX RESPECT V"
        );
    }
}
