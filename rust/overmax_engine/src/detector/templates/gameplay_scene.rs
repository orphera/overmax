//! PAUSE title crops from measured play-092 and play-115 frames at 1920x1080.
//! Each template is 148x28 pixels of row-major, 8-bit grayscale data.

pub(crate) const PAUSE_TITLE: &[&[u8]] = &[
    include_bytes!("gameplay_scene/pause_title-play-092.gray"),
    include_bytes!("gameplay_scene/pause_title-play-115.gray"),
];
