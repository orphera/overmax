use crate::capture::window_tracker::{WindowRect, WindowTracker};
use crate::capture::screen_capture::{CapturedFrame, GdiCaptureEngine};
use crate::capture::dxgi_capture::DxgiCaptureEngine;

pub trait CaptureEngine: Send + Sync {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String>;
    fn capture_bgra_inplace(
        &mut self,
        rect: WindowRect,
        out_frame: &mut CapturedFrame,
    ) -> Result<(), String>;
}

pub struct AdaptiveCaptureEngine {
    tracker: WindowTracker,
    gdi_backend: Option<GdiCaptureEngine>,
    dxgi_backend: Option<DxgiCaptureEngine>,
    current_is_fullscreen: bool,
    last_dxgi_init_attempt: std::time::Instant,
}

impl AdaptiveCaptureEngine {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            tracker: WindowTracker::new("DJMAX RESPECT V"),
            gdi_backend: Some(GdiCaptureEngine::new()?),
            dxgi_backend: None,
            current_is_fullscreen: false,
            last_dxgi_init_attempt: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(5))
                .unwrap_or_else(std::time::Instant::now),
        })
    }
}

impl CaptureEngine for AdaptiveCaptureEngine {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String> {
        let mut frame = CapturedFrame::default();
        self.capture_bgra_inplace(rect, &mut frame)?;
        Ok(frame)
    }

    fn capture_bgra_inplace(
        &mut self,
        rect: WindowRect,
        out_frame: &mut CapturedFrame,
    ) -> Result<(), String> {
        let is_fs = self.tracker.is_fullscreen();
        self.current_is_fullscreen = is_fs;

        if is_fs {
            if self.dxgi_backend.is_none() {
                if self.last_dxgi_init_attempt.elapsed() >= std::time::Duration::from_secs(3) {
                    self.last_dxgi_init_attempt = std::time::Instant::now();
                    match DxgiCaptureEngine::new() {
                        Ok(dxgi) => self.dxgi_backend = Some(dxgi),
                        Err(e) => {
                            if let Some(ref mut gdi) = self.gdi_backend {
                                return gdi.capture_bgra_inplace(rect, out_frame);
                            }
                            return Err(format!("DXGI init failed ({e}) and GDI fallback unavailable"));
                        }
                    }
                } else {
                    if let Some(ref mut gdi) = self.gdi_backend {
                        return gdi.capture_bgra_inplace(rect, out_frame);
                    }
                    return Err("DXGI retry cooldown active and GDI fallback unavailable".to_string());
                }
            }

            if let Some(ref mut dxgi) = self.dxgi_backend {
                match dxgi.capture_bgra_inplace(rect, out_frame) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        self.dxgi_backend = None;
                        if let Some(ref mut gdi) = self.gdi_backend {
                            gdi.capture_bgra_inplace(rect, out_frame)
                        } else {
                            Err(format!("DXGI capture failed ({e}) and GDI fallback unavailable"))
                        }
                    }
                }
            } else {
                Err("DXGI backend initialized but missing".to_string())
            }
        } else {
            if self.dxgi_backend.is_some() {
                self.dxgi_backend = None;
            }

            if let Some(ref mut gdi) = self.gdi_backend {
                gdi.capture_bgra_inplace(rect, out_frame)
            } else {
                Err("GdiCaptureEngine not initialized".to_string())
            }
        }
    }
}
