//! Deferred viewports + `eframe::App` (split from `native_app.rs` for file-size limits).

use eframe::egui::{self, Color32, RichText, Vec2, ViewportBuilder, ViewportCommand};
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

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

fn is_mouse_over_overlay(ctx: &egui::Context, _scale: f32) -> bool {
    let Some(rect) = ctx.input(|i| i.viewport().outer_rect) else {
        return false;
    };
    let mut pos = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pos);
    }
    // We no longer use pixels_per_point for scaling, so the mouse position 
    // from Windows is in physical pixels, and rect from egui is also effectively in 
    // physical pixels (since PPI=1.0).
    let mouse_pos = egui::pos2(pos.x as f32, pos.y as f32);
    rect.contains(mouse_pos)
}

impl NativeApp {
    fn auxiliary_viewport(title: &str, size: [f32; 2]) -> ViewportBuilder {
        ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size)
            .with_visible(true)
            .with_resizable(true)
            .with_taskbar(false)
    }

    fn show_debug_viewport(&self, ctx: &egui::Context) {
        if !self.debug_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.debug_open.clone();
        let lines = self.log_lines.clone();
        let paused = self.debug_paused.clone();
        let filters = self.debug_filters.clone();
        let title = self.debug_title();
        ctx.show_viewport_deferred(
            native_helpers::vp_debug(),
            Self::auxiliary_viewport(&title, [720.0, 460.0]),
            move |ctx, class| {
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
                );
                debug_ui::close_if_requested(ctx, &open);
            },
        );
    }

    fn show_settings_viewport(&self, ctx: &egui::Context) {
        if !self.settings_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.settings_open.clone();
        let draft = self.settings_draft.clone();
        let root = self.root.clone();
        let defaults = self.defaults.clone();
        let base = self.base_settings.clone();
        let merged = self.merged_settings.clone();
        let settings_ctx = settings_ui::SettingsUiContext {
            current_steam_id: self
                .sync_steam_id
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default(),
            sync_open: self.sync_open.clone(),
            scan_pending: self.scan_pending.clone(),
            sync_steam_id: self.sync_steam_id.clone(),
            fetch_tx: self.fetch_req_tx.clone(),
        };
        ctx.show_viewport_deferred(
            native_helpers::vp_settings(),
            Self::auxiliary_viewport("Overmax 설정", [520.0, 560.0]),
            move |ctx, class| {
                ctx.set_pixels_per_point(1.0);
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
        if !self.sync_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.sync_open.clone();
        let scan_pending = self.scan_pending.clone();
        let steam = self.sync_steam_id.clone();
        let status = self.sync_status.clone();
        let candidates = self.sync_candidates.clone();
        let upload_tx = self.upload_req_tx.clone();
        let delete_tx = self.delete_req_tx.clone();
        ctx.show_viewport_deferred(
            native_helpers::vp_sync(),
            Self::auxiliary_viewport("V-Archive 동기화", [520.0, 560.0]),
            move |ctx, class| {
                ctx.set_pixels_per_point(1.0);
                ctx.style_mut(|s| {
                    s.debug.show_expand_width = false;
                    s.debug.show_expand_height = false;
                    s.debug.show_resize = false;
                    s.debug.show_unaligned = false;
                    s.debug.debug_on_hover = false;
                });
                let list = candidates.lock().map(|g| g.clone()).unwrap_or_default();
                let mut steam_g = steam.lock().unwrap_or_else(|e| e.into_inner());
                let status_s = status.lock().map(|g| g.clone()).unwrap_or_default();
                sync_ui::render_sync(
                    ctx,
                    class,
                    &mut *steam_g,
                    &status_s,
                    &list,
                    || {
                        scan_pending.store(true, Ordering::Relaxed);
                    },
                    |i| {
                        let _ = upload_tx.send(i);
                    },
                    |i| {
                        let _ = delete_tx.send(i);
                    },
                );
                sync_ui::close_if_requested(ctx, &open);
            },
        );
    }
}

impl eframe::App for NativeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 모든 레이아웃 디버그 시각화(노란 선 및 텍스트) 강제 비활성화
        ctx.style_mut(|s| {
            s.debug.show_expand_width = false;
            s.debug.show_expand_height = false;
            s.debug.show_resize = false;
            s.debug.show_unaligned = false;
            s.debug.debug_on_hover = false;
        });
        ctx.set_debug_on_hover(false);

        if self.exit_requested.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        let settings_on = self.settings_open.load(Ordering::Relaxed);
        if settings_on && !self.prev_settings_open {
            if let (Ok(m), Ok(mut d)) = (self.merged_settings.lock(), self.settings_draft.lock()) {
                *d = m.clone();
            }
        }
        self.prev_settings_open = settings_on;

        self.start_log_pump(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        self.drain_detection_results();
        if self.drain_ui_commands() {
            ctx.request_repaint();
        }
        self.poll_scan_requests();
        self.poll_upload_requests();
        self.poll_fetch_requests();
        self.drain_sync_scan();
        self.drain_upload_results();
        self.drain_fetch_results();
        self.poll_delete_requests();
        self.drain_game_found_refresh_steam();
        ctx.send_viewport_cmd(ViewportCommand::ContentProtected(true));

        let scale = if let Ok(m) = self.merged_settings.lock() {

            m.get("overlay").and_then(|o| o.get("scale")).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32
        } else {
            1.0
        };

        let opacity = if let Ok(m) = self.merged_settings.lock() {
            m.get("overlay").and_then(|o| o.get("base_opacity")).and_then(|v| v.as_f64()).unwrap_or(0.8) as f32
        } else {
            0.8
        };

        let game_found = self.game_rect.lock().map(|r| r.is_some()).unwrap_or(false);
        let overlay_on = game_found && self.confidence > 0.1;

        if overlay_on != self.prev_overlay_on || (overlay_on && (scale - self.prev_scale).abs() > 0.001) {
            debug_ui::push_log(
                &self.log_lines,
                1000,
                format!(
                    "[Overlay] 레이아웃 업데이트: ON={}->{}, Scale={:.2}->{:.2} (Game: {}, Conf: {:.2})",
                    self.prev_overlay_on,
                    overlay_on,
                    self.prev_scale,
                    scale,
                    game_found,
                    self.confidence
                ),
            );

            if overlay_on {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(
                    (overlay_ui::BASE_WIDTH * scale).ceil() + 2.0,
                    (overlay_ui::BASE_HEIGHT * scale).ceil() + 2.0,
                )));
            } else {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(1.0, 1.0)));
            }
        }

        let mut_self = unsafe { &mut *(self as *const Self as *mut Self) };
        mut_self.prev_overlay_on = overlay_on;
        mut_self.prev_scale = scale;

        let _visible_size = Vec2::new(overlay_ui::BASE_WIDTH * scale, overlay_ui::BASE_HEIGHT * scale);
        let _hidden_size = Vec2::new(1.0, 1.0);

        // 마우스가 오버레이 영역 위에 있을 때만 상호작용 가능하게 함 (보조창 조작을 위해)
        let is_over = is_mouse_over_overlay(ctx, scale);
        ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(!overlay_on || !is_over));

        // Windows 전용: 전체 창 투명도 적용
        #[cfg(target_os = "windows")]
        {
            let found = apply_window_opacity(opacity, &self.log_lines);
            if !found {
                // 핸들을 못 찾았으면 로그에 한 번만 찍음
                static mut LOGGED: bool = false;
                unsafe {
                    if !LOGGED {
                        debug_ui::push_log(&self.log_lines, 1000, format!("[Overlay] 투명도 조절용 창 핸들을 찾지 못함 (Opacity: {:.2})", opacity));
                        LOGGED = true;
                    }
                }
            }
        }

        self.show_debug_viewport(ctx);
        self.show_settings_viewport(ctx);
        self.show_sync_viewport(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgba_unmultiplied(0, 0, 0, 0)))
            .show(ctx, |ui| {
                if !overlay_on {
                    return;
                }
                let actions = overlay_ui::draw_overlay_panel(
                    ui,
                    &self.session,
                    self.confidence,
                    &self.current_song_label(),
                    &self.pattern_tabs,
                    &self.recommendations,
                    self.settings_open.clone(),
                    self.debug_open.clone(),
                    self.sync_open.clone(),
                    scale,
                );

                if actions.start_drag {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if actions.restore_game_focus {
                    let max_log_lines = self.max_log_lines();
                    if let Ok(mut settings) = self.merged_settings.lock() {
                        window_tracker::restore_foreground_by_title(game_window_title(&settings));
                        
                        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                            if let Ok(mut draft) = self.settings_draft.lock() {
                                let mut overlay = settings.get("overlay").cloned().unwrap_or_else(|| serde_json::json!({}));
                                if let Some(overlay_obj) = overlay.as_object_mut() {
                                    overlay_obj.insert("position".to_string(), serde_json::json!({
                                        "x": rect.min.x.max(0.0) as i32,
                                        "y": rect.min.y.max(0.0) as i32
                                    }));
                                }
                                settings["overlay"] = overlay.clone();
                                draft["overlay"] = overlay;

                                let base_g = self.base_settings.lock().map(|g| g.clone()).unwrap_or_default();
                                let _ = settings_ui::save_settings_to_disk(
                                    self.root.as_ref(),
                                    self.defaults.as_ref(),
                                    &base_g,
                                    &mut draft,
                                    &mut settings,
                                );
                                debug_ui::push_log(
                                    &self.log_lines,
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
            });
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // [R, G, B, A] - 윈도우 버퍼를 완전 투명하게 설정.
        // OS 레벨의 전역 투명도(Layered Window Alpha)와 함께 작동함.
        [0.0, 0.0, 0.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::NativeApp;
    use eframe::egui;

    #[test]
    fn auxiliary_viewports_stay_out_of_taskbar() {
        let builder = NativeApp::auxiliary_viewport("debug", [720.0, 420.0]);

        assert_eq!(builder.taskbar, Some(false));
        assert_eq!(builder.visible, Some(true));
        assert_eq!(builder.resizable, Some(true));
        assert_ne!(
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

#[cfg(target_os = "windows")]
fn apply_window_opacity(opacity: f32, log_lines: &Arc<Mutex<VecDeque<String>>>) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::Foundation::HWND;

    struct EnumData {
        target_pid: u32,
        found_hwnd: Option<HWND>,
        log: Arc<Mutex<VecDeque<String>>>,
    }

    let target_pid = unsafe { GetCurrentProcessId() };
    let mut data = EnumData {
        target_pid,
        found_hwnd: None,
        log: log_lines.clone(),
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
                    
                    // 제목이 "Overmax"라면 가시성과 상관없이 최우선 타겟
                    if title == "Overmax" {
                        data.found_hwnd = Some(hwnd);
                        return 0; // 즉시 중단
                    }
                    
                    // 제목에 Overmax가 포함되거나, 가시적인 창을 차선책으로 저장
                    if data.found_hwnd.is_none() && (title.contains("Overmax") || visible) {
                        data.found_hwnd = Some(hwnd);
                    }
                }
                1
            }
        }

        EnumWindows(Some(enum_callback), &mut data as *mut _ as isize);

        if let Some(hwnd) = data.found_hwnd {
            // 현재 투명도 값 추적 로그 (값 변화가 있을 때만)
            static mut LAST_OPACITY: f32 = -1.0;
            if (opacity - LAST_OPACITY).abs() > 0.01 {
                debug_ui::push_log(&data.log, 1000, format!("[Win32] 투명도 업데이트 시도: {:.2} (HWND: {:?})", opacity, hwnd));
                LAST_OPACITY = opacity;
            }

            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            if (style & WS_EX_LAYERED as i32) == 0 {
                SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED as i32);
            }
            
            if SetLayeredWindowAttributes(hwnd, 0, (opacity * 255.0) as u8, 0x00000002) != 0 {
                return true;
            }
        }
    }
    false
}
