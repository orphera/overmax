use crate::capture::capture_engine::CaptureEngine;
use crate::capture::frame::CapturedFrame;
use crate::capture::window_tracker::WindowRect;
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HBITMAP, HDC,
    RGBQUAD, SRCCOPY,
};

pub struct GdiCaptureEngine {
    screen_dc: Option<HDC>,
    memory_dc: Option<HDC>,
    hbitmap: Option<HBITMAP>,
    bits: *mut u8,
    width: i32,
    height: i32,
}

unsafe impl Send for GdiCaptureEngine {}
unsafe impl Sync for GdiCaptureEngine {}

impl GdiCaptureEngine {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            screen_dc: None,
            memory_dc: None,
            hbitmap: None,
            bits: null_mut(),
            width: 0,
            height: 0,
        })
    }

    fn init_resources(&mut self, width: i32, height: i32) -> Result<(), String> {
        unsafe {
            let screen_dc = GetDC(null_mut());
            if screen_dc.is_null() {
                return Err("GetDC failed".to_string());
            }
            self.screen_dc = Some(screen_dc);

            let memory_dc = CreateCompatibleDC(screen_dc);
            if memory_dc.is_null() {
                ReleaseDC(null_mut(), screen_dc);
                self.screen_dc = None;
                return Err("CreateCompatibleDC failed".to_string());
            }
            self.memory_dc = Some(memory_dc);

            let mut bits = null_mut();
            let info = bitmap_info(width, height);
            let hbitmap =
                CreateDIBSection(memory_dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if hbitmap.is_null() || bits.is_null() {
                DeleteDC(memory_dc);
                ReleaseDC(null_mut(), screen_dc);
                self.screen_dc = None;
                self.memory_dc = None;
                return Err("CreateDIBSection failed".to_string());
            }
            self.hbitmap = Some(hbitmap);
            self.bits = bits.cast();
            self.width = width;
            self.height = height;

            let previous = SelectObject(memory_dc, hbitmap);
            if previous.is_null() {
                self.release_resources();
                return Err("SelectObject failed".to_string());
            }
        }
        Ok(())
    }

    fn release_resources(&mut self) {
        unsafe {
            if let Some(hbitmap) = self.hbitmap.take() {
                DeleteObject(hbitmap);
            }
            if let Some(memory_dc) = self.memory_dc.take() {
                DeleteDC(memory_dc);
            }
            if let Some(screen_dc) = self.screen_dc.take() {
                ReleaseDC(null_mut(), screen_dc);
            }
            self.bits = null_mut();
            self.width = 0;
            self.height = 0;
        }
    }
}

impl CaptureEngine for GdiCaptureEngine {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String> {
        let mut frame = CapturedFrame {
            width: 0,
            height: 0,
            bgra: Vec::new(),
        };
        self.capture_bgra_inplace(rect, &mut frame)?;
        Ok(frame)
    }

    fn capture_bgra_inplace(
        &mut self,
        rect: WindowRect,
        out_frame: &mut CapturedFrame,
    ) -> Result<(), String> {
        if !rect.is_valid() {
            return Err("capture rect must have positive dimensions".to_string());
        }

        if self.width != rect.width || self.height != rect.height || self.hbitmap.is_none() {
            self.release_resources();
            self.init_resources(rect.width, rect.height)?;
        }

        let screen_dc = self.screen_dc.ok_or("Screen DC not initialized")?;
        let memory_dc = self.memory_dc.ok_or("Memory DC not initialized")?;

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

        if ok == 0 {
            return Err("BitBlt failed".to_string());
        }

        let len = (rect.width as usize) * (rect.height as usize) * 4;

        out_frame.width = rect.width;
        out_frame.height = rect.height;
        out_frame.bgra.resize(len, 0);

        unsafe {
            let src_slice = std::slice::from_raw_parts(self.bits, len);
            out_frame.bgra.copy_from_slice(src_slice);
        }

        Ok(())
    }
}

impl Drop for GdiCaptureEngine {
    fn drop(&mut self) {
        self.release_resources();
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
    use super::GdiCaptureEngine;
    use crate::capture::capture_engine::CaptureEngine;
    use crate::capture::window_tracker::WindowRect;

    #[test]
    fn rejects_invalid_capture_rect() {
        let mut capturer = GdiCaptureEngine::new().unwrap();
        let result = capturer.capture_bgra(WindowRect {
            left: 0,
            top: 0,
            width: 0,
            height: 10,
        });

        assert!(result.is_err());
    }
}
