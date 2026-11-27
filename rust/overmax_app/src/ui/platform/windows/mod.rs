//! Windows-specific UI platform implementation.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, ViewportBuilder};
use serde_json::Value;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

mod overlay_host;
mod tray_icon;

use crate::ui::ui_command::UiCommand;
use tray_icon::{force_cleanup_tray, TrayIcon};

pub fn load_icon() -> Option<eframe::egui::IconData> {
    let icon_bytes = include_bytes!("../../../../../../assets/overmax.ico");
    if let Ok(img) = image::load_from_memory(icon_bytes) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        return Some(eframe::egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        });
    }
    None
}

unsafe extern "system" fn console_ctrl_handler(_ctrl_type: u32) -> i32 {
    force_cleanup_tray();
    0 // FALSE
}

pub fn init_platform_on_startup() -> Result<(), String> {
    unsafe {
        // Windows 다중 모니터 이종 DPI(QHD 100% / 4K 150~200% 등) 환경에서 DPI Virtualization으로 인한
        // ClientToScreen 좌표 축소 및 캡처 찌그러짐(ROI Scale 0.158x) 현상을 원천 방지
        #[link(name = "user32")]
        extern "system" {
            fn SetProcessDpiAwarenessContext(value: isize) -> i32;
        }
        const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(console_ctrl_handler), 1);
    }
    Ok(())
}

pub fn show_startup_error(message: &str) {
    eprintln!("[Startup Error] {message}");
}

pub fn install_cjk_fonts(ctx: &egui::Context) -> bool {
    let mut fonts = FontDefinitions::default();

    let font_names = [
        ("malgun", "malgun.ttf"),
        ("msgothic", "msgothic.ttc"),
        ("msyh", "msyh.ttc"),
        ("meiryo", "meiryo.ttc"),
        ("gulim", "gulim.ttc"),
    ];

    let font_dirs = get_platform_font_dirs();
    let mut loaded_fonts = Vec::new();

    for (name, filename) in font_names {
        for dir in &font_dirs {
            let path = dir.join(filename);
            if let Ok(bytes) = std::fs::read(&path) {
                let mut font_data = FontData::from_owned(bytes);
                if filename.ends_with(".ttc") {
                    font_data.index = 0;
                }
                fonts
                    .font_data
                    .insert(name.to_string(), std::sync::Arc::new(font_data));
                loaded_fonts.push(name.to_string());
                break;
            }
        }
    }

    if loaded_fonts.is_empty() {
        return false;
    }

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let family_fonts = fonts.families.entry(family).or_default();
        for name in &loaded_fonts {
            family_fonts.push(name.clone());
        }
    }

    ctx.set_fonts(fonts);
    true
}

fn get_platform_font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(windir) = std::env::var("SystemRoot") {
        dirs.push(std::path::PathBuf::from(windir).join("Fonts"));
    } else if let Ok(windir) = std::env::var("WINDIR") {
        dirs.push(std::path::PathBuf::from(windir).join("Fonts"));
    } else {
        dirs.push(std::path::PathBuf::from(r"C:\Windows\Fonts"));
    }

    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        dirs.push(std::path::PathBuf::from(localappdata).join(r"Microsoft\Windows\Fonts"));
    }

    dirs
}

pub fn is_position_on_screen(x: f32, y: f32) -> bool {
    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vwidth = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vheight = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        if vwidth > 0 && vheight > 0 {
            let px = x as i32;
            let py = y as i32;
            px >= vx && px < (vx + vwidth) && py >= vy && py < (vy + vheight)
        } else {
            x >= 0.0 && y >= 0.0
        }
    }
}

pub fn init_overlay_window_immediate() -> Option<isize> {
    if let Some(hwnd) = find_window_by_title(OVERLAY_TITLE) {
        setup_overlay_window(hwnd);
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
            );
        }
        Some(hwnd as isize)
    } else {
        None
    }
}

pub fn native_options(settings: &overmax_data::Settings) -> eframe::NativeOptions {
    let overlay = settings.overlay();

    let mut vp = ViewportBuilder::default()
        .with_title("Overmax")
        .with_inner_size([1.0, 1.0])
        .with_min_inner_size([1.0, 1.0])
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_has_shadow(false)
        .with_taskbar(false)
        .with_always_on_top()
        .with_visible(false);

    let pos = &overlay.position;
    if let (Some(x), Some(y)) = (pos.x, pos.y) {
        let px = x as f32;
        let py = y as f32;
        if is_position_on_screen(px, py) {
            vp = vp.with_position([px, py]);
        }
    }

    if let Some(icon) = load_icon() {
        vp = vp.with_icon(icon);
    }

    eframe::NativeOptions {
        viewport: vp,
        ..Default::default()
    }
}

#[derive(Default)]
pub struct WindowsWindowCache {
    pub cached_hwnd: Option<isize>,
    pub cached_game_hwnd: Option<isize>,
    pub logged_opacity_fail: bool,
    pub prev_snap_geometry: Option<(i32, i32, i32, i32)>,
}

pub struct PlatformState {
    pub is_dragging: bool,
    pub drag_anchor: Option<(i32, i32, i32, i32)>,
    pub _tray: Option<TrayIcon>,
    pub win_cache: WindowsWindowCache,
    pub last_painted_rect: Option<egui::Rect>,
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: windows_sys::Win32::Foundation::HWND,
        dw_attribute: u32,
        pv_attribute: *const std::ffi::c_void,
        cb_attribute: u32,
    ) -> i32;
    fn DwmExtendFrameIntoClientArea(
        hwnd: windows_sys::Win32::Foundation::HWND,
        p_mar_inset: *const Margins,
    ) -> i32;
}

#[link(name = "comctl32")]
extern "system" {
    fn SetWindowSubclass(
        hwnd: windows_sys::Win32::Foundation::HWND,
        pfn_subclass: Option<
            unsafe extern "system" fn(
                hwnd: windows_sys::Win32::Foundation::HWND,
                u_msg: u32,
                w_param: usize,
                l_param: isize,
                u_id_subclass: usize,
                dw_ref_data: usize,
            ) -> isize,
        >,
        u_id_subclass: usize,
        dw_ref_data: usize,
    ) -> i32;
    fn DefSubclassProc(
        hwnd: windows_sys::Win32::Foundation::HWND,
        u_msg: u32,
        w_param: usize,
        l_param: isize,
    ) -> isize;
}

#[repr(C)]
struct Margins {
    cx_left_width: i32,
    cx_right_width: i32,
    cy_top_height: i32,
    cy_bottom_height: i32,
}

unsafe extern "system" fn overlay_subclass_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
    _uidsubclass: usize,
    _refdata: usize,
) -> isize {
    const WM_NCCALCSIZE: u32 = 0x0083;
    const WM_NCPAINT: u32 = 0x0085;
    const WM_NCACTIVATE: u32 = 0x0086;

    if msg == WM_NCCALCSIZE && wparam != 0 {
        // 비클라이언트(캡션 버튼/상단 테두리) 영역을 0으로 만들어 DWM 캡션 렌더링을 완전히 제거
        return 0;
    }
    if msg == WM_NCPAINT {
        return 0;
    }
    if msg == WM_NCACTIVATE {
        return 1;
    }

    DefSubclassProc(hwnd, msg, wparam, lparam)
}

const OVERLAY_TITLE: &str = "Overmax";

/// 메인 오버레이 창: 비활성 툴윈도우 + 포커스 비획득 + 프레임리스 투명 스타일.
fn setup_overlay_window(hwnd: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    apply_frameless_transparent_style(hwnd, (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as i32, true);
}

/// 제목으로 찾은 보조 창(업로드 기록 등)을 메인 오버레이와 같은 프레임리스 투명 스타일로 바꾼다.
/// 찾았으면 true를 반환한다. 창이 아직 없으면 false이므로 호출 쪽에서 다시 시도한다.
pub fn apply_frameless_transparent_style_by_title(title: &str) -> bool {
    match find_window_by_title(title) {
        Some(hwnd) => {
            apply_frameless_transparent_style(hwnd, 0, false);
            true
        }
        None => false,
    }
}

/// 캡션/테두리를 없애고 레이어드 투명 + DWM 프레임 확장을 적용한다. Win10/11 공통 경로.
/// `extra_ex_style`은 창 종류별 확장 스타일(오버레이만 NOACTIVATE/TOOLWINDOW)이다.
fn apply_frameless_transparent_style(
    hwnd: windows_sys::Win32::Foundation::HWND,
    extra_ex_style: i32,
    subclass_nonclient: bool,
) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        // 1. Win32 기본 스타일: 시스템 메뉴 / 캡션 버튼 (- ㅁ X) 완전 제거
        let win_style = GetWindowLongW(hwnd, GWL_STYLE);
        let target_win_style = (win_style
            & !(WS_CAPTION as i32
                | WS_THICKFRAME as i32
                | WS_MINIMIZEBOX as i32
                | WS_MAXIMIZEBOX as i32
                | WS_SYSMENU as i32
                | WS_BORDER as i32
                | WS_DLGFRAME as i32))
            | WS_POPUP as i32;
        if win_style != target_win_style {
            SetWindowLongW(hwnd, GWL_STYLE, target_win_style);
        }

        // 2. Win32 확장 스타일: 비활성 툴윈도우 + 레이어드 투명
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let target_style =
            (style & !(WS_EX_TOPMOST as i32)) | WS_EX_LAYERED as i32 | extra_ex_style;
        if style != target_style {
            SetWindowLongW(hwnd, GWL_EXSTYLE, target_style);
        }

        // 3. 서브클래싱 등록: WM_NCCALCSIZE를 가로채 비클라이언트 캡션 버튼(- ㅁ X)을 물리적으로 제거.
        //    이 서브클래스가 걸린 레이어드 창에서는 StartDrag(SC_MOVE)가 동작하지 않으므로,
        //    OS 네이티브 드래그를 쓰는 보조 창은 생략한다 (docs/decisions/ui_and_i18n.md 2026-11-27).
        if subclass_nonclient {
            SetWindowSubclass(hwnd, Some(overlay_subclass_proc), 1, 0);
        }

        // 4. DWM에 프레임 스타일 변경 1회 통보
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );

        // 5. DWM 테두리 비활성화 및 프레임버퍼 투명 마진 확장
        let border_color: u32 = 0xFFFFFFFE; // DWMWCB_NONE
        DwmSetWindowAttribute(
            hwnd,
            34, // DWMWA_BORDER_COLOR
            &border_color as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        let margins = Margins {
            cx_left_width: -1,
            cx_right_width: -1,
            cy_top_height: -1,
            cy_bottom_height: -1,
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
}

fn find_window_by_title(window_title: &str) -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    struct EnumData<'a> {
        target_pid: u32,
        window_title: &'a str,
        found_hwnd: Option<HWND>,
    }

    let target_pid = unsafe { GetCurrentProcessId() };
    let mut data = EnumData {
        target_pid,
        window_title,
        found_hwnd: None,
    };

    unsafe {
        extern "system" fn enum_callback(hwnd: HWND, lparam: isize) -> i32 {
            unsafe {
                let data = &mut *(lparam as *mut EnumData<'_>);
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                if pid == data.target_pid {
                    let mut text = [0u16; 512];
                    let len = GetWindowTextW(hwnd, text.as_mut_ptr(), 512);
                    let title = String::from_utf16_lossy(&text[..len as usize]);
                    if title == data.window_title {
                        data.found_hwnd = Some(hwnd);
                        return 0; // 즉시 중단
                    }
                }
                1 // 계속 검색
            }
        }

        EnumWindows(Some(enum_callback), &mut data as *mut EnumData as isize);
    }

    data.found_hwnd
}

impl PlatformState {
    pub fn new(
        ctx_holder: &Arc<Mutex<Option<egui::Context>>>,
        settings: &Arc<Mutex<Value>>,
        command_tx: &Sender<UiCommand>,
        initial_hwnd: Option<isize>,
        _game_window_title: &str,
        _runtime_telemetry: Option<Arc<overmax_detect::RuntimeTelemetry>>,
    ) -> Result<Self, String> {
        let tray = if settings
            .lock()
            .ok()
            .and_then(|v| v.get("tray_icon")?.as_bool())
            .unwrap_or(true)
        {
            Some(TrayIcon::spawn(command_tx.clone(), ctx_holder.clone()))
        } else {
            None
        };

        Ok(Self {
            is_dragging: false,
            drag_anchor: None,
            _tray: tray,
            win_cache: WindowsWindowCache {
                cached_hwnd: initial_hwnd,
                ..Default::default()
            },
            last_painted_rect: None,
        })
    }

    pub fn apply_overlay_visibility(
        &mut self,
        visible: bool,
        is_active: bool,
        force_topmost: bool,
    ) -> bool {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        let hwnd = match self.win_cache.cached_hwnd {
            Some(h) => h as HWND,
            None => match find_window_by_title(OVERLAY_TITLE) {
                Some(h) => {
                    setup_overlay_window(h);
                    self.win_cache.cached_hwnd = Some(h as isize);
                    h
                }
                None => return false,
            },
        };

        if unsafe { IsWindow(hwnd) } == 0 {
            self.win_cache.cached_hwnd = None;
            return false;
        }

        let vis_flag = if visible {
            SWP_SHOWWINDOW
        } else {
            SWP_HIDEWINDOW
        };

        unsafe {
            SetWindowPos(
                hwnd,
                if is_active || force_topmost {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | vis_flag,
            );
        }

        true
    }

    pub fn handle_screen_drag(
        &mut self,
        is_dragging_intent: bool,
        stop_intent: bool,
        dpi: f32,
    ) -> Option<UiCommand> {
        use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        if is_dragging_intent {
            self.is_dragging = true;
            if let Some(hwnd_val) = self.win_cache.cached_hwnd {
                let hwnd = hwnd_val as HWND;
                let mut cur_pt = POINT { x: 0, y: 0 };
                unsafe {
                    GetCursorPos(&mut cur_pt);
                }
                if self.drag_anchor.is_none() {
                    let mut rect = RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    };
                    unsafe {
                        GetWindowRect(hwnd, &mut rect);
                    }
                    self.drag_anchor = Some((cur_pt.x, cur_pt.y, rect.left, rect.top));
                }
                if let Some((start_cx, start_cy, start_wx, start_wy)) = self.drag_anchor {
                    let target_x = start_wx + (cur_pt.x - start_cx);
                    let target_y = start_wy + (cur_pt.y - start_cy);
                    unsafe {
                        SetWindowPos(
                            hwnd,
                            std::ptr::null_mut(),
                            target_x,
                            target_y,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
            }
        }

        if stop_intent {
            self.is_dragging = false;
            if let Some((_, _, _, _)) = self.drag_anchor.take() {
                if let Some(hwnd_val) = self.win_cache.cached_hwnd {
                    let hwnd = hwnd_val as HWND;
                    let mut rect = RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    };
                    unsafe {
                        GetWindowRect(hwnd, &mut rect);
                    }
                    return Some(UiCommand::SetOverlayPosition {
                        x: (rect.left as f32 / dpi).round() as i32,
                        y: (rect.top as f32 / dpi).round() as i32,
                    });
                }
            }
        }

        None
    }

    pub fn presentation_observation(&self) -> overmax_capture::SharedPresentationObservation {
        std::sync::Arc::new(std::sync::Mutex::new(None))
    }
}

pub fn get_local_mouse_pos(ctx: &egui::Context, hwnd_opt: Option<isize>) -> Option<egui::Pos2> {
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

pub fn draw_custom_cursor(painter: &egui::Painter, p: egui::Pos2) {
    use egui::{Color32, Stroke};
    let len = 6.0;

    let stroke_black = Stroke::new(2.5_f32, Color32::BLACK);
    painter.line_segment(
        [egui::pos2(p.x - len, p.y), egui::pos2(p.x + len, p.y)],
        stroke_black,
    );
    painter.line_segment(
        [egui::pos2(p.x, p.y - len), egui::pos2(p.x, p.y + len)],
        stroke_black,
    );

    let stroke_white = Stroke::new(1.0_f32, Color32::WHITE);
    painter.line_segment(
        [egui::pos2(p.x - len, p.y), egui::pos2(p.x + len, p.y)],
        stroke_white,
    );
    painter.line_segment(
        [egui::pos2(p.x, p.y - len), egui::pos2(p.x, p.y + len)],
        stroke_white,
    );
}

#[link(name = "kernel32")]
extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

/// Detects the OS user interface display language.
/// Returns "ko", "ja", or "en" (fallback for other languages).
pub fn detect_os_language() -> &'static str {
    let lang_id = unsafe { GetUserDefaultUILanguage() };
    match lang_id & 0x03FF {
        0x12 => "ko", // LANG_KOREAN
        0x11 => "ja", // LANG_JAPANESE
        0x09 => "en", // LANG_ENGLISH
        _ => "en",    // Fallback to English for other locales
    }
}
