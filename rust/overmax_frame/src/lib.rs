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
