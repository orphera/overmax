pub mod community;
pub mod config;
pub mod gateway;
pub mod service;
pub mod store;

pub use community::cache_downloader::{CacheUpdateResult, StartupCacheManager};
pub use community::client::VArchiveDB;
pub use community::sheet_meta::{PatternSheetMeta, PatternSheetMetaItem};
pub use community::sync::{
    build_candidates, matches_filter, pattern_level_index, SyncCandidate, LEVEL_LABELS,
};
pub use community::varchive_api::{
    fetch_records_blocking, fetch_single_song_records_blocking, parse_account_file,
    upload_score_blocking, AccountInfo, UploadResult,
};
pub use config::compatibility::DataCompatibility;
pub use config::paths::{
    is_dir_writable, is_running_in_msix_package, system_local_data_dir, AppPaths, RuntimeMode,
};

pub use config::settings::{
    diff_settings, load_base_settings, load_base_settings_from_paths, load_merged_settings,
    load_merged_settings_from_paths, merge_settings_layers, normalize_settings, save_user_settings,
    save_user_settings_to_path, AppUpdateSettings, AutoUploadScope, DebugWindowSettings,
    JacketMatcherSettings, OverlayPosition, OverlaySettings, RecommendProviderSettings,
    RecommendSettings, ScreenCaptureSettings, Settings, SettingsPaths, SyncFilterSettings,
    VArchiveAutoUploadSettings, VArchiveSettings, VArchiveUserMap, WindowTrackerSettings,
};
pub use overmax_core::{RecordKey, RecordValue};
pub use service::jacket_matcher::{JacketMatcher, JacketMatcherConfig};
pub use service::recommend::{
    CompositeRecommender, LocalFloorRecommender, LocalRecommendFooter, ProviderCacheReader,
    RecommendBundle, RecommendContext, RecommendEntry, RecommendPanel, RecommendReason,
    RecommendReasonKind, RecommendResult, RecommendStrategy, RecommendationSource, Recommender,
    SourceStatus, VaryDim,
};
pub use service::recommend_provider_fetch::{
    fetch_manifest_blocking, fetch_recommend_blocking, get_cached_manifest,
    test_provider_connection_blocking, ProviderManifest, RECOMMEND_PROTOCOL_ID,
};
pub use service::record_manager::{RecordManager, RecordSource};
pub use store::image_index::{ImageEntry, ImageIndexDb, ImageMatch};
pub use store::record_db::RecordDB;
