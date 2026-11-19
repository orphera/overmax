//! 감지 코드는 `overmax_detect` crate로 옮겼다. 기존 `overmax_engine::detector::...` 경로를
//! 유지하기 위해 re-export하고, 워커(`detection_worker`)만 engine에 남긴다.

pub use overmax_detect::*;

pub mod detection_worker;

pub mod detection_pipeline {
    pub use overmax_detect::detection_pipeline::*;

    /// engine이 쓰는 구체 파이프라인: 자켓 인덱스로 `overmax_data` 이미지 인덱스를 쓴다.
    pub type DetectionPipeline = overmax_detect::detection_pipeline::DetectionPipeline<
        crate::image_index_jacket::ImageIndexJacket,
    >;
}
