//! Single `eframe` app: overlay + deferred debug / settings / sync viewports.

use overmax_core::{Changed, GameSessionState};
use overmax_data::{
    load_base_settings_from_paths, load_merged_settings_from_paths, normalize_settings, AppPaths,
    PatternSheetMeta, RecommendResult, Recommender, RecordDB, RecordManager, SyncCandidate,
    VArchiveDB,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::system::cache_update;
use crate::system::single_instance::SingleInstanceGuard;
use crate::system::steam_session;
use crate::system::updater::{self, AppUpdateConfig};
use crate::ui::debug_ui;
use crate::ui::overlay_ui;
use crate::ui::platform;
use crate::ui::ui_command::UiCommand;
use crate::ui::upload_history_ui;
use eframe::egui;
use overmax_data::config::varchive_account::first_steam_from_settings;
use overmax_detect::DetectionOutput;
use overmax_engine::detection_worker;

mod sync;

pub fn run_native_app() -> eframe::Result<()> {
    if let Err(error) = platform::init_platform_on_startup() {
        platform::show_startup_error(&error);
        return Err(eframe::Error::AppCreation(Box::new(std::io::Error::other(
            error,
        ))));
    }

    let paths = Arc::new(AppPaths::resolve());
    if let Err(e) = paths.ensure_dirs_and_seed() {
        eprintln!("[AppPaths] 디렉터리 생성 및 시딩 실패: {e}");
    }

    let defaults: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../settings.json"
    )))
    .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
    let mut merged = load_merged_settings_from_paths(&paths.settings_paths(), defaults);
    normalize_settings(&mut merged);
    crate::ui::i18n::set_locale_from_settings(&merged);

    let Some(_single) = SingleInstanceGuard::try_acquire() else {
        std::process::exit(0);
    };

    let app_settings: overmax_data::Settings =
        serde_json::from_value(merged.clone()).unwrap_or_default();
    let upd_cfg = AppUpdateConfig::from_settings(&app_settings);
    let ok_notify = updater::notify_previous_update(paths.data_dir()).unwrap_or_else(|e| {
        eprintln!("[AppUpdater] notify: {e}");
        true
    });
    if !ok_notify {
        return Ok(());
    }
    match updater::check_and_apply_update_blocking(paths.data_dir(), &upd_cfg) {
        Ok(true) => {}
        Ok(false) => {
            drop(_single);
            if let Ok(exe) = std::env::current_exe() {
                if let Err(e) = std::process::Command::new(exe).spawn() {
                    eprintln!("[AppUpdater] 재시작 실패: {}", e);
                }
            } else {
                eprintln!("[AppUpdater] 실행 경로를 찾을 수 없습니다.");
            }
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[AppUpdater] {e}");
            std::process::exit(1);
        }
    }

    let options = platform::native_options(&app_settings);

    let app_paths = paths.clone();
    eframe::run_native(
        "Overmax",
        options,
        Box::new(move |cc| {
            let initial_hwnd = platform::init_overlay_window_immediate();
            let mut visuals = eframe::egui::Visuals::dark();
            visuals.panel_fill = eframe::egui::Color32::TRANSPARENT;
            visuals.window_fill = eframe::egui::Color32::TRANSPARENT;
            visuals.window_stroke = eframe::egui::Stroke::NONE;
            visuals.window_shadow = eframe::egui::Shadow::NONE;
            visuals.widgets.noninteractive.bg_stroke = eframe::egui::Stroke::NONE;
            cc.egui_ctx.set_visuals(visuals);
            let _ = overlay_ui::install_cjk_fonts(&cc.egui_ctx);
            NativeApp::new(cc.egui_ctx.clone(), initial_hwnd, app_paths.clone())
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|e| {
                    eprintln!("native app init: {e}");
                    platform::show_startup_error(&e);
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })
        }),
    )
}

#[derive(Clone)]
pub struct SharedSettings {
    pub defaults: Arc<Value>,
    pub base: Arc<Mutex<Value>>,
    pub merged: Arc<Mutex<Value>>,
    pub draft: Arc<Mutex<Value>>,
    pub writer: Arc<crate::system::settings_writer::SettingsDebounceWriter>,
    pub paths: Arc<AppPaths>,
}

impl SharedSettings {
    pub fn get_merged(&self) -> overmax_data::Settings {
        let val = match self.merged.lock() {
            Ok(g) => g.clone(),
            Err(_) => serde_json::Value::Object(serde_json::Map::new()),
        };
        serde_json::from_value(val).unwrap_or_default()
    }

    pub fn update_sync_filter(&self, filter: &overmax_data::SyncFilterSettings) {
        let base_g = match self.base.lock() {
            Ok(g) => g.clone(),
            Err(e) => e.into_inner().clone(),
        };
        let mut draft_g = match self.draft.lock() {
            Ok(g) => g.clone(),
            Err(e) => e.into_inner().clone(),
        };

        if let Ok(filter_val) = serde_json::to_value(filter) {
            if let Value::Object(ref mut map) = draft_g {
                map.insert("sync_filter".to_string(), filter_val);
            }
            overmax_data::normalize_settings(&mut draft_g);
            let diff = overmax_data::diff_settings(&base_g, &draft_g);

            if let Ok(mut g) = self.draft.lock() {
                *g = draft_g.clone();
            }
            if let Ok(mut m) = self.merged.lock() {
                *m = overmax_data::load_merged_settings_from_paths(
                    &self.paths.settings_paths(),
                    (*self.defaults).clone(),
                );
            }

            self.writer
                .queue_save(self.paths.settings_user_json(), diff);
        }
    }
}

pub struct SharedUiState {
    pub debug_open: Arc<AtomicBool>,
    pub settings_open: Arc<AtomicBool>,
    pub sync_open: Arc<AtomicBool>,
    pub upload_history_open: Arc<AtomicBool>,
    pub scan_pending: Arc<AtomicBool>,
}

pub struct SharedDebugState {
    pub log_lines: Arc<Mutex<VecDeque<Arc<str>>>>,
    pub paused: Arc<AtomicBool>,
    pub filters: Arc<Mutex<std::collections::HashMap<String, bool>>>,
}

#[derive(Clone)]
pub struct SharedSyncState {
    pub steam_id: Arc<Mutex<String>>,
    pub status: Arc<Mutex<String>>,
    pub candidates: Arc<Mutex<Vec<SyncCandidate>>>,
    pub steam_users: Arc<Mutex<std::collections::HashMap<String, steam_session::SteamUser>>>,
}

/// 업로드 결과: 키, 수동(quick) 업로드 여부, 상태, 메시지,
/// 실제로 업로드한 (Rate, MAX COMBO, 이전 V-Archive Rate, 이전 MAX COMBO, Top-50 순위).
pub(crate) type UploadResultMsg = (
    overmax_data::RecordKey,
    bool,
    String,
    String,
    (f64, bool, Option<f64>, Option<bool>, Option<usize>),
);

pub(crate) struct SyncWorkerChannels {
    pub(crate) sync_rx: Receiver<Result<Vec<SyncCandidate>, String>>,
    pub(crate) sync_tx: Sender<Result<Vec<SyncCandidate>, String>>,
    pub(crate) upload_req_rx: Receiver<overmax_data::RecordKey>,
    pub(crate) upload_req_tx: Sender<overmax_data::RecordKey>,
    pub(crate) upload_res_rx: Receiver<UploadResultMsg>,
    pub(crate) upload_res_tx: Sender<UploadResultMsg>,
    pub(crate) fetch_req_rx: Receiver<(String, String, i32)>,
    pub(crate) fetch_req_tx: Sender<(String, String, i32)>,
    pub(crate) fetch_res_rx: Receiver<(String, i32, Result<usize, String>)>,
    pub(crate) fetch_res_tx: Sender<(String, i32, Result<usize, String>)>,
    pub(crate) delete_req_rx: Receiver<overmax_data::RecordKey>,
    pub(crate) delete_req_tx: Sender<overmax_data::RecordKey>,
}

impl SyncWorkerChannels {
    /// 동기화 워커와 UI 간의 모든 mpsc 채널 쌍을 생성해 번들로 반환한다.
    fn new() -> Self {
        let (sync_tx, sync_rx) = mpsc::channel();
        let (upload_req_tx, upload_req_rx) = mpsc::channel();
        let (upload_res_tx, upload_res_rx) = mpsc::channel();
        let (delete_req_tx, delete_req_rx) = mpsc::channel::<overmax_data::RecordKey>();
        let (fetch_req_tx, fetch_req_rx) = mpsc::channel();
        let (fetch_res_tx, fetch_res_rx) = mpsc::channel();
        Self {
            sync_rx,
            sync_tx,
            upload_req_rx,
            upload_req_tx,
            upload_res_rx,
            upload_res_tx,
            fetch_req_rx,
            fetch_req_tx,
            fetch_res_rx,
            fetch_res_tx,
            delete_req_rx,
            delete_req_tx,
        }
    }
}

pub struct AppStateTracker {
    pub prev_debug_open: Changed<bool>,
    pub prev_settings_open: Changed<bool>,
    pub prev_sync_open: Changed<bool>,
    pub prev_scale: Changed<f32>,
    pub prev_overlay_on: Changed<bool>,
    pub prev_is_lite: Changed<bool>,
    pub prev_passthrough: Changed<Option<bool>>,
    pub prev_protected: Changed<Option<bool>>,
    pub prev_mouse_pos: Changed<Option<egui::Pos2>>,
}

impl Default for AppStateTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AppStateTracker {
    pub fn new() -> Self {
        Self {
            prev_debug_open: Changed::new(false),
            prev_settings_open: Changed::new(false),
            prev_sync_open: Changed::new(false),
            prev_scale: Changed::new(1.0),
            prev_overlay_on: Changed::new(false),
            prev_is_lite: Changed::new(false),
            prev_passthrough: Changed::new(None),
            prev_protected: Changed::new(None),
            prev_mouse_pos: Changed::new(None),
        }
    }
}

pub struct NativeApp {
    #[cfg(all(target_os = "linux", feature = "portal-capture"))]
    pub(crate) portal: overmax_capture::PortalControl,
    pub(crate) paths: Arc<AppPaths>,
    pub(crate) root: Arc<std::path::PathBuf>,
    pub(crate) settings: SharedSettings,
    pub(crate) ui_state: SharedUiState,
    pub(crate) debug_state: SharedDebugState,
    pub(crate) upload_history: upload_history_ui::SharedUploadHistory,
    pub(crate) sync_state: SharedSyncState,
    pub(crate) log_rx: Option<Receiver<String>>,
    pub(crate) game_rect: Arc<Mutex<Option<overmax_frame::WindowRect>>>,
    pub(crate) window_snapshot: Option<overmax_frame::WindowSnapshot>,
    pub(crate) capture_fatal: Option<String>,
    pub(crate) session: GameSessionState,
    pub(crate) confidence: f32,
    pub(crate) sync_channels: SyncWorkerChannels,
    pub(crate) detection_rx: Receiver<DetectionOutput>,
    pub(crate) ui_cmd_rx: Receiver<UiCommand>,
    pub(crate) varchive_db: Arc<VArchiveDB>,
    pub(crate) sheet_meta: Arc<PatternSheetMeta>,
    pub(crate) startup_cache_manager: cache_update::StartupCacheManager,
    pub(crate) image_index_reload_tx: mpsc::Sender<()>,
    pub(crate) recommendations: RecommendResult,
    pub(crate) pattern_tabs: Vec<crate::ui::overlay_recommend_ui::PatternTabInfo>,
    pub(crate) state_tracker: AppStateTracker,
    pub(crate) record_db: Arc<RecordDB>,
    pub(crate) record_manager: Arc<RecordManager>,
    pub(crate) recommender: Arc<Recommender>,
    pub(crate) game_found_rx: Receiver<()>,
    pub(crate) exit_requested: Arc<AtomicBool>,
    pub(crate) ctx_holder: Arc<Mutex<Option<egui::Context>>>,
    pub(crate) runtime: overmax_runtime::hub::Runtime,
    /// 오버레이 렌더 경로가 매 프레임 재계산하지 않도록 캐시한 업로드 필요 여부.
    /// 입력(`session.context` 의 rate/is_max_combo, `record_manager` 상태)이 바뀌는
    /// 모든 경로가 `refresh_overlay_data` 를 거치므로 그 안에서 갱신한다.
    pub(crate) overlay_upload_needed: bool,
    pub(crate) auto_upload: overmax_runtime::auto_upload::AutoUploadScheduler,
    pub(crate) platform: platform::PlatformState,
    pub(crate) toast: Option<crate::ui::components::ToastMessage>,
    pub(crate) last_detection_output: Option<DetectionOutput>,
    pub(crate) last_telemetry_snapshot: Option<overmax_detect::PipelineTelemetrySnapshot>,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    pub(crate) runtime_telemetry: Option<Arc<overmax_detect::RuntimeTelemetry>>,
    pub(crate) ipc_bound_port: crate::system::ipc_server::BoundPortSlot,
    pub(crate) ipc_handle: crate::system::ipc_server::IpcServerHandle,
    pub(crate) ipc_cmd_rx: Receiver<crate::system::ipc_server::IpcCommand>,
    pub(crate) overlay_visible_override: Option<bool>,
}

impl NativeApp {
    fn new(
        initial_ctx: egui::Context,
        initial_hwnd: Option<isize>,
        paths: Arc<AppPaths>,
    ) -> Result<Self, String> {
        let root = Arc::new(paths.data_dir().to_path_buf());
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        let runtime_telemetry = Some(Arc::new(overmax_detect::RuntimeTelemetry::new(
            paths.data_dir(),
            env!("CARGO_PKG_VERSION"),
        )));
        #[cfg(not(any(debug_assertions, feature = "telemetry")))]
        let runtime_telemetry: Option<Arc<overmax_detect::RuntimeTelemetry>> = None;
        let defaults: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../settings.json"
        )))
        .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
        let defaults = Arc::new(defaults);

        let base_settings = Arc::new(Mutex::new(load_base_settings_from_paths(
            &paths.settings_paths(),
            (*defaults).clone(),
        )));
        let mut merged =
            load_merged_settings_from_paths(&paths.settings_paths(), (*defaults).clone());

        normalize_settings(&mut merged);
        crate::ui::i18n::set_locale_from_settings(&merged);

        let (log_tx, log_rx) = mpsc::channel();
        crate::ui::native_app_viewports::set_global_log_tx(log_tx.clone());
        let (game_found_tx, game_found_rx) = mpsc::channel();
        let (detection_tx, detection_rx) = mpsc::channel();

        let app_settings: overmax_data::Settings =
            serde_json::from_value(merged.clone()).unwrap_or_default();

        let startup_cache_manager = cache_update::StartupCacheManager::init(
            paths.data_dir(),
            &app_settings,
            log_tx.clone(),
        );

        let merged_settings = Arc::new(Mutex::new(merged.clone()));
        let settings_draft = Arc::new(Mutex::new(merged.clone()));

        let recent_steam = steam_session::most_recent_steam_id();
        let mut record_db = RecordDB::new(paths.record_db(), recent_steam.as_deref());
        if !record_db.initialize() {
            let _ = log_tx.send(
                "[RecordDB] 기록 DB 초기화 실패: 플레이 기록 저장/조회가 비활성화됩니다"
                    .to_string(),
            );
        }
        let record_db = Arc::new(record_db);
        let upload_history = upload_history_ui::SharedUploadHistory::default();
        upload_history.replace(record_db.load_upload_history());

        // JSON 캐시 파일이 있다면 SQLite DB로 마이그레이션 실행
        let cache_root = paths.varchive_cache_dir();
        if let Err(e) = record_db.migrate_json_cache_to_db(&cache_root) {
            let _ = log_tx.send(format!("[VArchive] 캐시 마이그레이션 실패: {e}"));
        }

        let record_manager = Arc::new(RecordManager::new(record_db.clone()));
        record_manager.refresh();

        let mut varchive_db = VArchiveDB::new();
        let dlcs_path = paths.dlcs_json();
        let _ = varchive_db.load_dlcs_from_file(&dlcs_path);

        let songs_path = paths.songs_json();
        if let Err(e) = varchive_db.load_from_file(&songs_path) {
            let _ = log_tx.send(format!("[VArchive] songs load failed: {e}"));
        }
        let varchive_db = Arc::new(varchive_db);

        let recommender = Arc::new(Recommender::new(
            varchive_db.clone(),
            record_manager.clone(),
        ));

        let sheet_meta = Arc::new(PatternSheetMeta::load_cache(
            paths.pattern_meta_json(),
            &varchive_db,
        ));

        let steam0 = {
            let mut sid = first_steam_from_settings(&app_settings);
            if sid.is_empty() {
                sid = recent_steam.unwrap_or_default();
            }
            sid
        };

        let exit_requested = Arc::new(AtomicBool::new(false));
        let settings_open = Arc::new(AtomicBool::new(false));
        let sync_open = Arc::new(AtomicBool::new(false));
        let upload_history_open = Arc::new(AtomicBool::new(false));
        let debug_open = Arc::new(AtomicBool::new(false));

        let sync_channels = SyncWorkerChannels::new();
        let (ui_cmd_tx, ui_cmd_rx) = mpsc::channel();
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        let _ = &ui_cmd_tx;
        let ctx_holder: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(Some(initial_ctx)));

        let game_window_title = app_settings.window_tracker().window_title;
        let platform = platform::PlatformState::new(
            &ctx_holder,
            &merged_settings,
            &ui_cmd_tx,
            initial_hwnd,
            &game_window_title,
            runtime_telemetry.clone(),
        )?;

        let ctx_holder_clone = ctx_holder.clone();

        let repaint_callback = Box::new(move || {
            if let Ok(holder) = ctx_holder_clone.lock() {
                if let Some(ctx) = &*holder {
                    ctx.request_repaint();
                }
            }
        });

        let presentation_observation = platform.presentation_observation();

        let (image_index_reload_tx, image_index_reload_rx) = mpsc::channel();
        let _worker_control = detection_worker::spawn(
            paths.data_dir().to_path_buf(),
            app_settings.clone(),
            merged_settings.clone(),
            log_tx.clone(),
            game_found_tx,
            detection_tx,
            runtime_telemetry.clone(),
            presentation_observation,
            repaint_callback,
            image_index_reload_rx,
        );

        let mut filters = std::collections::HashMap::new();
        filters.insert("[ScreenCapture]".to_string(), true);
        filters.insert("[Overlay]".to_string(), true);
        filters.insert("[VArchive]".to_string(), true);
        filters.insert("[WindowTracker]".to_string(), true);
        filters.insert("[Main]".to_string(), true);

        let settings_writer =
            Arc::new(crate::system::settings_writer::SettingsDebounceWriter::new());

        // IPC 서버 매니저 (설정 OFF가 기본 — enabled 시에만 바인딩됨)
        let (ipc_cmd_tx, ipc_cmd_rx) = mpsc::channel();
        let ipc_data = crate::system::ipc_server::IpcDataSources {
            varchive_db: varchive_db.clone(),
            record_manager: record_manager.clone(),
        };
        let (ipc_publisher, ipc_handle, ipc_bound_port) =
            crate::system::ipc_server::spawn_ipc_manager(
                paths.data_dir().to_path_buf(),
                merged_settings.clone(),
                env!("CARGO_PKG_VERSION"),
                ipc_cmd_tx,
                ipc_data,
            );
        let runtime =
            overmax_runtime::hub::Runtime::new(record_manager.clone()).with_observer(Box::new(
                overmax_runtime::ipc_observer::IpcObserver::new(ipc_publisher),
            ));

        let settings = SharedSettings {
            defaults: defaults.clone(),
            base: base_settings.clone(),
            merged: merged_settings.clone(),
            draft: settings_draft.clone(),
            writer: settings_writer,
            paths: paths.clone(),
        };

        let ui_state = SharedUiState {
            debug_open: debug_open.clone(),
            settings_open: settings_open.clone(),
            sync_open: sync_open.clone(),
            upload_history_open: upload_history_open.clone(),
            scan_pending: Arc::new(AtomicBool::new(false)),
        };

        let debug_state = SharedDebugState {
            log_lines: Arc::new(Mutex::new(VecDeque::new())),
            paused: Arc::new(AtomicBool::new(false)),
            filters: Arc::new(Mutex::new(filters)),
        };

        let sync_state = SharedSyncState {
            steam_id: Arc::new(Mutex::new(steam0)),
            status: Arc::new(Mutex::new(String::new())),
            candidates: Arc::new(Mutex::new(Vec::new())),
            steam_users: Arc::new(Mutex::new(
                steam_session::all_login_users()
                    .into_iter()
                    .map(|u| (u.steam_id.clone(), u))
                    .collect(),
            )),
        };

        let mut app = Self {
            #[cfg(all(target_os = "linux", feature = "portal-capture"))]
            portal: _worker_control.portal,
            paths,
            root,
            settings,
            ui_state,
            debug_state,
            upload_history,
            sync_state,
            log_rx: Some(log_rx),
            game_rect: Arc::new(Mutex::new(None)),
            window_snapshot: None,
            capture_fatal: None,
            session: GameSessionState::detecting(),
            confidence: 0.0,
            sync_channels,
            detection_rx,
            ui_cmd_rx,
            varchive_db,
            sheet_meta,
            startup_cache_manager,
            image_index_reload_tx,
            recommendations: RecommendResult::empty(),
            pattern_tabs: Vec::new(),
            state_tracker: AppStateTracker::new(),
            record_db,
            record_manager,
            recommender,
            game_found_rx,
            exit_requested: exit_requested.clone(),
            ctx_holder: ctx_holder.clone(),
            runtime,
            overlay_upload_needed: false,
            auto_upload: Default::default(),
            platform,
            toast: None,
            last_detection_output: None,
            last_telemetry_snapshot: None,
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            runtime_telemetry,
            ipc_bound_port,
            ipc_handle,
            ipc_cmd_rx,
            overlay_visible_override: None,
        };

        app.handle_auto_refresh();
        Ok(app)
    }
    pub(crate) fn max_log_lines(&self) -> usize {
        let Ok(m) = self.settings.merged.lock() else {
            return 500;
        };
        m.get("debug_window")
            .and_then(|d| d.get("max_lines"))
            .and_then(|v| v.as_u64())
            .unwrap_or(500) as usize
    }

    pub(crate) fn debug_title(&self) -> String {
        let Ok(m) = self.settings.merged.lock() else {
            return "Overmax Debug Log".into();
        };
        m.get("debug_window")
            .and_then(|d| d.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("Overmax Debug Log")
            .to_string()
    }

    pub(crate) fn refresh_steam_session(&mut self, context: &str) {
        let sid = steam_session::most_recent_steam_id();
        let (changed, before, after) = self.record_manager.set_steam_id(sid.as_deref());

        if let Ok(mut steam_id_lock) = self.sync_state.steam_id.lock() {
            *steam_id_lock = sid.clone().unwrap_or_default();
        }

        if let Ok(mut map) = self.sync_state.steam_users.lock() {
            *map = steam_session::all_login_users()
                .into_iter()
                .map(|u| (u.steam_id.clone(), u))
                .collect();
        }

        if changed {
            self.upload_history
                .replace(self.record_db.load_upload_history());
            debug_ui::push_log(
                &self.debug_state.log_lines,
                self.max_log_lines(),
                format!("[Main] Steam 세션 갱신 ({context}): {before} -> {after}"),
            );
            self.refresh_overlay_data();
        } else if sid.is_some() {
            debug_ui::push_log(
                &self.debug_state.log_lines,
                self.max_log_lines(),
                format!("[Main] Steam 세션 유지 ({context}): {after}"),
            );
        }
    }

    pub(crate) fn drain_game_found_refresh_steam(&mut self) {
        while self.game_found_rx.try_recv().is_ok() {
            self.refresh_steam_session("게임 창 발견");
        }
    }

    #[inline]
    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    fn current_steam_id(&self) -> String {
        self.sync_state
            .steam_id
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub(crate) fn poll_startup_cache(&mut self) {
        let polled = self
            .startup_cache_manager
            .poll_updates(&mut self.varchive_db, &mut self.sheet_meta);
        if polled.data {
            self.on_varchive_db_updated();
        }
        if polled.image_index {
            let _ = self.image_index_reload_tx.send(());
        }
    }

    fn on_varchive_db_updated(&mut self) {
        self.recommender = Arc::new(self.recommender.with_varchive_db(self.varchive_db.clone()));
        self.record_manager.refresh();
        self.refresh_overlay_data();
    }
}

#[cfg(test)]
mod tests;
