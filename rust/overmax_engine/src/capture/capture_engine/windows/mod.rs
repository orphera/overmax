pub mod dxgi;
pub mod gdi;
pub mod hdr_pipeline;
pub mod normalizer;
pub mod shader_bytes;

use crate::capture::capture_engine::CaptureEngine;
use crate::capture::frame::CapturedFrame;
use crate::capture::window_tracker::{WindowRect, WindowTracker};
use dxgi::DxgiCaptureEngine;
use gdi::GdiCaptureEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferredCaptureEngine {
    #[default]
    Gdi,
    Dxgi,
    Auto,
}

impl std::str::FromStr for PreferredCaptureEngine {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "dxgi" => Self::Dxgi,
            "auto" => Self::Auto,
            _ => Self::Gdi,
        })
    }
}

pub struct AdaptiveCaptureEngine {
    tracker: WindowTracker,
    gdi_backend: Option<GdiCaptureEngine>,
    dxgi_backend: Option<DxgiCaptureEngine>,
    current_is_fullscreen: bool,
    last_dxgi_init_attempt: std::time::Instant,
    preferred_engine: PreferredCaptureEngine,
    enable_gpu_atlas: bool,
    hdr_sdr_white_level: Option<f32>,
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
            preferred_engine: PreferredCaptureEngine::Gdi,
            enable_gpu_atlas: false,
            hdr_sdr_white_level: None,
        })
    }

    pub fn set_preferred_engine(&mut self, preferred: PreferredCaptureEngine) {
        self.preferred_engine = preferred;
    }

    pub fn preferred_engine(&self) -> PreferredCaptureEngine {
        self.preferred_engine
    }

    pub fn set_enable_gpu_atlas(&mut self, enable: bool) {
        self.enable_gpu_atlas = enable;
        if let Some(ref mut dxgi) = self.dxgi_backend {
            dxgi.set_enable_gpu_atlas(enable);
        }
    }

    pub fn set_hdr_sdr_white_level(&mut self, level: Option<f32>) {
        self.hdr_sdr_white_level = level;
        if let Some(ref mut dxgi) = self.dxgi_backend {
            dxgi.set_hdr_sdr_white_level(level);
        }
    }

    #[allow(dead_code)]
    pub fn enable_gpu_atlas(&self) -> bool {
        self.enable_gpu_atlas
    }

    fn fallback_to_gdi(
        &mut self,
        rect: WindowRect,
        out_frame: &mut CapturedFrame,
        err_msg: &str,
    ) -> Result<(), String> {
        if let Some(ref mut gdi) = self.gdi_backend {
            gdi.capture_bgra_inplace(rect, out_frame)
        } else {
            Err(format!("{err_msg} and GDI fallback unavailable"))
        }
    }
}

fn is_multi_monitor() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CMONITORS};
    unsafe { GetSystemMetrics(SM_CMONITORS) > 1 }
}

impl CaptureEngine for AdaptiveCaptureEngine {
    fn set_preferred_engine(&mut self, preferred: PreferredCaptureEngine) {
        self.set_preferred_engine(preferred);
    }

    fn set_enable_gpu_atlas(&mut self, enable: bool) {
        self.set_enable_gpu_atlas(enable);
    }

    fn set_hdr_sdr_white_level(&mut self, level: Option<f32>) {
        self.set_hdr_sdr_white_level(level);
    }

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

        let try_dxgi = match self.preferred_engine {
            PreferredCaptureEngine::Gdi => false,
            PreferredCaptureEngine::Dxgi => true,
            PreferredCaptureEngine::Auto => {
                // 멀티모니터 환경이면 호환성과 안정성을 위해 GDI 우선 (false)
                // 단일모니터 환경이면 극강의 성능을 위해 DXGI 우선 (true)
                !is_multi_monitor()
            }
        };

        if try_dxgi {
            if self.dxgi_backend.is_none() {
                if self.last_dxgi_init_attempt.elapsed() >= std::time::Duration::from_secs(3) {
                    self.last_dxgi_init_attempt = std::time::Instant::now();
                    match DxgiCaptureEngine::new() {
                        Ok(mut dxgi) => {
                            dxgi.set_enable_gpu_atlas(self.enable_gpu_atlas);
                            dxgi.set_hdr_sdr_white_level(self.hdr_sdr_white_level);
                            self.dxgi_backend = Some(dxgi);
                        }
                        Err(e) => {
                            return self.fallback_to_gdi(
                                rect,
                                out_frame,
                                &format!("DXGI init failed ({e})"),
                            );
                        }
                    }
                } else {
                    return self.fallback_to_gdi(rect, out_frame, "DXGI retry cooldown active");
                }
            }

            if let Some(ref mut dxgi) = self.dxgi_backend {
                match dxgi.capture_bgra_inplace(rect, out_frame) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        self.dxgi_backend = None;
                        self.last_dxgi_init_attempt = std::time::Instant::now();
                        self.fallback_to_gdi(rect, out_frame, &format!("DXGI capture failed ({e})"))
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
