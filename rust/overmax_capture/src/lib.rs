//! 게임 창 추적과 저부하 화면 캡처.
//!
//! 창 제목으로 대상 창을 찾아 위치·포커스·전체화면 여부를 추적하고, 그 영역을
//! BGRA 프레임([`overmax_frame::CapturedFrame`])으로 캡처한다. 메모리 접근이나 프로세스
//! 인젝션 없이 OS 캡처 API만 쓰며, 게임 플레이 중 성능 영향을 최소화하는 것을 우선한다.
//!
//! # 플랫폼별 구현
//! - Windows: `AdaptiveCaptureEngine` — DXGI Desktop Duplication / Windows Graphics Capture /
//!   GDI 중 사용 가능한 것을 고르고 실패 시 전환. HDR(scRGB) 화면은 SDR BGRA로 복원
//!   (`hdr_pipeline`). 창 추적은 `WindowTracker`, `find_hwnd_by_title`.
//! - Linux: `AdaptiveCaptureEngine` — XWayland 창을 XComposite + MIT-SHM으로 캡처.
//!   `portal-capture` feature로 XDG Desktop Portal(PipeWire) 캡처, `GameGeometrySource`로
//!   Wayland 컴포지터(Hyprland/niri/sway) 기준 창 위치를 얻는다.
//!
//! 공통 인터페이스는 [`CaptureEngine`] 트레이트다.
//!
//! # DJMAX 전용 항목
//! Windows의 `set_enable_gpu_atlas(true)`는 DJMAX ROI만 512×512 아틀라스로 잘라 받는
//! 모드다(`overmax_frame::ATLAS_SLOTS`). 기본값은 꺼짐이며, 끈 상태에서는 일반 전체 창
//! 캡처로 동작한다.
//!
//! ```no_run
//! # #[cfg(target_os = "windows")]
//! # fn main() -> Result<(), String> {
//! use overmax_capture::{AdaptiveCaptureEngine, CaptureEngine, WindowTracker};
//!
//! let tracker = WindowTracker::new("My Game Window");
//! let mut engine = AdaptiveCaptureEngine::new()?;
//! if let Some(rect) = tracker.game_rect() {
//!     let frame = engine.capture_bgra(rect)?;
//!     println!("{}x{} BGRA, {} bytes", frame.width, frame.height, frame.bgra.len());
//! }
//! # Ok(())
//! # }
//! # #[cfg(not(target_os = "windows"))]
//! # fn main() {}
//! ```
//!
//! 재사용 범위와 의존 방법: `docs/guides/module-contracts.md` §6.

mod capture_engine;
#[cfg(target_os = "linux")]
mod linux;
mod window_tracker;
#[cfg(target_os = "windows")]
mod windows;

pub use capture_engine::{CaptureEngine, CaptureErrorAction};
pub use window_tracker::{FocusState, PresentationObservation, SharedPresentationObservation};

#[cfg(all(target_os = "linux", feature = "portal-capture"))]
pub use linux::portal::{
    connection::{Confirmation, ConnectionStatus, PortalConnection, PortalControl, ResetScope},
    PortalCapture, PortalStatus,
};
#[cfg(target_os = "linux")]
pub use linux::{
    restore_foreground_by_title, select_geometry_source, spawn_game_geometry_source,
    AdaptiveCaptureEngine, FocusObservation, FocusSource, GameGeometry, GameGeometrySource,
    GeometryChangedCallback, NoGeometrySource, SharedGameGeometry, WindowTracker,
};
#[cfg(target_os = "windows")]
pub use windows::{
    capture::hdr_pipeline, encode_wide, find_hwnd_by_title, restore_foreground_by_title,
    AdaptiveCaptureEngine, PreferredCaptureEngine, WindowTracker,
};
