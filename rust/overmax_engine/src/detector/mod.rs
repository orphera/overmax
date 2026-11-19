pub use crate::frame::atlas_layout;
#[cfg(test)]
mod atlas_layout_tests;
pub mod atlas_translator;
pub mod detection_pipeline;
pub mod detection_worker;
pub mod hysteresis;
pub mod jacket_index;
pub mod play_state;
pub mod roi;
pub mod roi_config;
pub mod telemetry;
pub mod templates;

pub use atlas_layout::{AtlasSlot, ATLAS_HEIGHT, ATLAS_SLOTS, ATLAS_SLOT_COUNT, ATLAS_WIDTH};
pub use atlas_translator::AtlasTranslator;
pub use telemetry::{PipelineStatsCollector, PipelineTelemetrySnapshot, TimingAggregator};
