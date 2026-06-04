use crate::window_tracker::WindowRect;
use crate::screen_capture::{CapturedFrame, GdiCaptureEngine};

pub trait CaptureEngine: Send + Sync {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String>;
}

pub struct AdaptiveCaptureEngine {
    gdi_backend: Option<GdiCaptureEngine>,
    #[allow(dead_code)]
    current_is_fullscreen: bool,
}

impl AdaptiveCaptureEngine {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            gdi_backend: Some(GdiCaptureEngine::new()?),
            current_is_fullscreen: false,
        })
    }
}

impl CaptureEngine for AdaptiveCaptureEngine {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String> {
        // TODO: 런타임에 전체화면(Fullscreen) 상태를 점검하여
        // dxgi_backend로의 동적 스위칭을 처리할 예정입니다.
        // 현재는 Facade 리팩토링 단계이므로 GDI 백엔드로 위임합니다.
        if let Some(ref mut gdi) = self.gdi_backend {
            gdi.capture_bgra(rect)
        } else {
            Err("GdiCaptureEngine not initialized".to_string())
        }
    }
}
