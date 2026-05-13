use crate::window_tracker::WindowRect;
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, GetDC, HBITMAP, RGBQUAD, ReleaseDC,
    SRCCOPY,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedFrame {
    pub width: i32,
    pub height: i32,
    pub bgra: Vec<u8>,
}

pub struct ScreenCapturer;

impl ScreenCapturer {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn capture_bgra(&self, rect: WindowRect) -> Result<CapturedFrame, String> {
        if !rect.is_valid() {
            return Err("capture rect must have positive dimensions".to_string());
        }
        capture_screen_rect(rect)
    }
}

fn capture_screen_rect(rect: WindowRect) -> Result<CapturedFrame, String> {
    let screen_dc = ScreenDc::new()?;
    let memory_dc = MemoryDc::new(screen_dc.handle)?;
    let bitmap = DibSection::new(memory_dc.handle, rect.width, rect.height)?;
    bitmap.select_into(memory_dc.handle)?;
    blit_to_bitmap(screen_dc.handle, memory_dc.handle, rect)?;
    Ok(bitmap.to_frame(rect.width, rect.height))
}

fn blit_to_bitmap(
    screen_dc: windows_sys::Win32::Graphics::Gdi::HDC,
    memory_dc: windows_sys::Win32::Graphics::Gdi::HDC,
    rect: WindowRect,
) -> Result<(), String> {
    let ok = unsafe {
        BitBlt(
            memory_dc,
            0,
            0,
            rect.width,
            rect.height,
            screen_dc,
            rect.left,
            rect.top,
            SRCCOPY | CAPTUREBLT,
        )
    };
    (ok != 0)
        .then_some(())
        .ok_or_else(|| "BitBlt failed".to_string())
}

struct ScreenDc {
    handle: windows_sys::Win32::Graphics::Gdi::HDC,
}

impl ScreenDc {
    fn new() -> Result<Self, String> {
        let handle = unsafe { GetDC(null_mut()) };
        (!handle.is_null())
            .then_some(Self { handle })
            .ok_or_else(|| "GetDC failed".to_string())
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(null_mut(), self.handle);
        }
    }
}

struct MemoryDc {
    handle: windows_sys::Win32::Graphics::Gdi::HDC,
}

impl MemoryDc {
    fn new(screen_dc: windows_sys::Win32::Graphics::Gdi::HDC) -> Result<Self, String> {
        let handle = unsafe { CreateCompatibleDC(screen_dc) };
        (!handle.is_null())
            .then_some(Self { handle })
            .ok_or_else(|| "CreateCompatibleDC failed".to_string())
    }
}

impl Drop for MemoryDc {
    fn drop(&mut self) {
        unsafe {
            DeleteDC(self.handle);
        }
    }
}

struct DibSection {
    handle: HBITMAP,
    data: *mut u8,
    len: usize,
}

impl DibSection {
    fn new(
        dc: windows_sys::Win32::Graphics::Gdi::HDC,
        width: i32,
        height: i32,
    ) -> Result<Self, String> {
        let mut bits = null_mut();
        let mut info = bitmap_info(width, height);
        let handle =
            unsafe { CreateDIBSection(dc, &mut info, DIB_RGB_COLORS, &mut bits, null_mut(), 0) };
        if handle.is_null() || bits.is_null() {
            return Err("CreateDIBSection failed".to_string());
        }
        Ok(Self {
            handle,
            data: bits.cast(),
            len: (width as usize) * (height as usize) * 4,
        })
    }

    fn select_into(&self, dc: windows_sys::Win32::Graphics::Gdi::HDC) -> Result<(), String> {
        let previous = unsafe { SelectObject(dc, self.handle) };
        (!previous.is_null())
            .then_some(())
            .ok_or_else(|| "SelectObject failed".to_string())
    }

    fn to_frame(&self, width: i32, height: i32) -> CapturedFrame {
        let bgra = unsafe { std::slice::from_raw_parts(self.data, self.len).to_vec() };
        CapturedFrame {
            width,
            height,
            bgra,
        }
    }
}

impl Drop for DibSection {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.handle);
        }
    }
}

fn bitmap_info(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..BITMAPINFOHEADER::default()
        },
        bmiColors: [RGBQUAD::default(); 1],
    }
}

#[cfg(test)]
mod tests {
    use super::ScreenCapturer;
    use crate::window_tracker::WindowRect;

    #[test]
    fn rejects_invalid_capture_rect() {
        let capturer = ScreenCapturer::new().unwrap();
        let result = capturer.capture_bgra(WindowRect {
            left: 0,
            top: 0,
            width: 0,
            height: 10,
        });

        assert!(result.is_err());
    }
}
