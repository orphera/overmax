#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        println!($($arg)*);
    };
}

pub use overmax_capture as capture;
pub mod detector;
pub use overmax_frame as frame;
pub mod image_index_jacket;

pub use detector::detection_worker;
pub use image_index_jacket::ImageIndexJacket;

/// engine이 쓰는 구체 파이프라인: 자켓 인덱스로 `overmax_data` 이미지 인덱스를 쓴다.
pub type DetectionPipeline = overmax_detect::DetectionPipeline<ImageIndexJacket>;
