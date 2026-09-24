pub mod atlas_layout;
pub mod atlas_translator;
pub mod detection_pipeline;
pub mod detection_worker;
pub mod hysteresis;
pub mod play_state;
pub mod roi;
pub mod roi_config;
pub mod telemetry;
pub mod templates;

pub use atlas_layout::{AtlasSlot, ATLAS_HEIGHT, ATLAS_SLOTS, ATLAS_SLOT_COUNT, ATLAS_WIDTH};
pub use atlas_translator::AtlasTranslator;
pub use telemetry::{PipelineStatsCollector, PipelineTelemetrySnapshot, TimingAggregator};
