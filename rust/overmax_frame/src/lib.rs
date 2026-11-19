//! 캡처 프레임과 ROI(관심 영역) 기하 타입.
//!
//! 캡처 쪽(`overmax-capture`)과 판독 쪽이 공유하는 BGRA 프레임 버퍼, 사각형, 창 기하,
//! 그리고 영역 단위 연산(크롭 뷰, 평균색, 썸네일 변화 감지, 픽셀 체크섬)을 제공한다.
//!
//! # 범용 항목
//! - [`CapturedFrame`]: BGRA8 프레임 (`width`, `height`, `bgra`)
//! - [`RoiRect`] / [`RawRoiRect`]: 끝점형 / 원점+크기형 사각형
//! - [`ImageView`], [`crop_roi`]: 복사 없는 영역 뷰
//! - [`make_thumbnail`], [`thumbnail_changed`], [`mean_abs_diff`], [`compute_pixel_checksum`]:
//!   매 프레임 무거운 판독을 건너뛰기 위한 변화 감지
//! - [`WindowRect`], [`WindowSnapshot`]: 화면 좌표의 창 위치·상태
//!
//! # DJMAX 전용 항목
//! [`ATLAS_SLOTS`], [`AtlasSlot`], [`build_virtual_atlas`], `ATLAS_*` 상수는 DJMAX RESPECT V의
//! 1080p ROI 46개를 512×512 텍스처에 배치한 고정 표다. 다른 게임에는 의미가 없으며,
//! 이 항목 때문에 앱 전용 크레이트 `overmax-core`에 의존한다. (분리는 후속 작업)
//!
//! ```
//! use overmax_frame::{crop_roi, CapturedFrame, RawRoiRect, RoiRect};
//!
//! let frame = CapturedFrame { width: 4, height: 4, bgra: vec![200; 4 * 4 * 4] };
//! let roi = RoiRect::from_raw(RawRoiRect { x: 1, y: 1, width: 2, height: 2 });
//! let view = crop_roi(&frame, roi).unwrap();
//! assert_eq!(view.region_mean_bgr().b, 200);
//! ```
//!
//! 재사용 범위와 의존 방법: `docs/guides/module-contracts.md` §6.

mod atlas_layout;
mod captured_frame;
mod roi_rect;
mod utils;
mod window;

pub use atlas_layout::{
    build_virtual_atlas, AtlasSlot, ATLAS_HEIGHT, ATLAS_SLOTS, ATLAS_SLOT_COUNT, ATLAS_WIDTH,
};
pub use captured_frame::CapturedFrame;
pub use roi_rect::{RawRoiRect, RoiRect};
pub use utils::{
    compute_pixel_checksum, crop_roi, make_thumbnail, mean_abs_diff, region_mean_bgr,
    thumbnail_changed, ImageRegion, ImageView,
};
pub use window::{WindowRect, WindowSnapshot};
