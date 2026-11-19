#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        println!($($arg)*);
    };
}

#[cfg(test)]
mod atlas_layout_tests;
mod atlas_translator;
mod detection_pipeline;
mod hysteresis;
mod jacket_index;
mod play_state;
mod roi;
mod roi_config;
mod telemetry;
pub mod templates;

pub use atlas_translator::AtlasTranslator;
pub use detection_pipeline::{
    check_open_match_badge, detect_freestyle_result_colorbar_match, detect_static_scene,
    DetectionOutput, DetectionPipeline, JacketMatchStatus, SleepHint,
};
pub use jacket_index::{JacketHit, JacketIndex};
pub use play_state::{detect_button_mode, detect_difficulty, detect_max_combo, MIN_VALID_RATE};
pub use roi::RoiManager;
pub use roi_config::{GlobalRoiConfig, SceneRoiConfig};
pub use telemetry::{
    current_timestamp_str, format_duration_us, DetectionDeliveryTelemetry, PipelineStatsCollector,
    PipelineTelemetrySnapshot, RuntimeTelemetry, TimingAggregator,
};
