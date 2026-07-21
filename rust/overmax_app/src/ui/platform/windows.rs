//! Windows-specific UI platform implementation.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, ViewportBuilder};
use serde_json::Value;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use crate::ui::tray_icon::{force_cleanup_tray, TrayIcon};
use crate::ui::ui_command::UiCommand;

pub fn load_icon() -> Option<eframe::egui::IconData> {
    let icon_bytes = include_bytes!("../../../../../assets/overmax.ico");
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

pub fn native_options(settings: &overmax_data::Settings) -> eframe::NativeOptions {
    let overlay = settings.overlay();
    let is_lite = overlay.lite_mode;

    let init_w = crate::ui::overlay_ui::BASE_WIDTH;
    let init_h = if is_lite {
        crate::ui::overlay_ui::LITE_BASE_HEIGHT
    } else {
        crate::ui::overlay_ui::BASE_HEIGHT
    };

    let mut vp = ViewportBuilder::default()
        .with_title("Overmax")
        .with_inner_size([init_w, init_h])
        .with_min_inner_size([120.0, 40.0])
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_always_on_top()
        .with_visible(!is_lite);

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
    pub last_applied_opacity: Option<f32>,
    pub logged_opacity_fail: bool,
    pub prev_snap_geometry: Option<(i32, i32, i32, i32)>,
}

pub struct PlatformState {
    pub is_dragging: bool,
    pub _tray: Option<TrayIcon>,
    pub win_cache: WindowsWindowCache,
    pub last_painted_rect: Option<egui::Rect>,
}

impl PlatformState {
    pub fn new(
        ctx_holder: &Arc<Mutex<Option<egui::Context>>>,
        settings: &Arc<Mutex<Value>>,
        command_tx: &Sender<UiCommand>,
    ) -> Result<Self, String> {
        let tray = if settings
            .lock()
            .ok()
            .and_then(|v| v.get("tray_icon")?.as_bool())
            .unwrap_or(true)
        {
            Some(TrayIcon::spawn(
                command_tx.clone(),
                settings.clone(),
                ctx_holder.clone(),
            ))
        } else {
            None
        };

        Ok(Self {
            is_dragging: false,
            _tray: tray,
            win_cache: WindowsWindowCache::default(),
            last_painted_rect: None,
        })
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

    let stroke_black = Stroke::new(2.5, Color32::BLACK);
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
