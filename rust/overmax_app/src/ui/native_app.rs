//! Single `eframe` app: overlay + deferred debug / settings / sync viewports.

use overmax_core::{Changed, GameSessionState, PlayContext};
use overmax_data::{
    build_candidates, load_base_settings_from_paths, load_merged_settings_from_paths,
    normalize_settings, AppPaths, PatternSheetMeta, RecommendResult, Recommender, RecordDB,
    RecordManager, SyncCandidate, VArchiveDB,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::system::cache_update;
use crate::system::native_helpers::{
    account_path_for_steam, button_num, first_steam_from_settings,
};
use crate::system::single_instance::SingleInstanceGuard;
use crate::system::steam_session;
use crate::system::updater::{self, AppUpdateConfig};
use crate::system::varchive_upload;
use crate::ui::debug_ui;
use crate::ui::overlay_ui;
use crate::ui::platform;
use crate::ui::ui_command::UiCommand;
use eframe::egui;
use overmax_engine::detector::detection_pipeline::DetectionOutput;
use overmax_engine::detector::detection_worker;

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

pub(crate) struct SyncWorkerChannels {
    pub(crate) sync_rx: Receiver<Result<Vec<SyncCandidate>, String>>,
    pub(crate) sync_tx: Sender<Result<Vec<SyncCandidate>, String>>,
    pub(crate) upload_req_rx: Receiver<overmax_data::RecordKey>,
    pub(crate) upload_req_tx: Sender<overmax_data::RecordKey>,
    pub(crate) upload_res_rx: Receiver<(overmax_data::RecordKey, bool, String, String)>,
    pub(crate) upload_res_tx: Sender<(overmax_data::RecordKey, bool, String, String)>,
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
    pub(crate) paths: Arc<AppPaths>,
    pub(crate) root: Arc<std::path::PathBuf>,
    pub(crate) settings: SharedSettings,
    pub(crate) ui_state: SharedUiState,
    pub(crate) debug_state: SharedDebugState,
    pub(crate) sync_state: SharedSyncState,
    pub(crate) log_rx: Option<Receiver<String>>,
    pub(crate) game_rect: Arc<Mutex<Option<overmax_engine::capture::window_tracker::WindowRect>>>,
    pub(crate) window_snapshot: Option<overmax_engine::capture::window_tracker::WindowSnapshot>,
    pub(crate) capture_fatal: Option<String>,
    pub(crate) session: GameSessionState,
    pub(crate) confidence: f32,
    pub(crate) sync_channels: SyncWorkerChannels,
    pub(crate) detection_rx: Receiver<DetectionOutput>,
    pub(crate) ui_cmd_rx: Receiver<UiCommand>,
    pub(crate) varchive_db: Arc<VArchiveDB>,
    pub(crate) sheet_meta: Arc<PatternSheetMeta>,
    pub(crate) startup_cache_manager: cache_update::StartupCacheManager,
    pub(crate) recommendations: RecommendResult,
    pub(crate) pattern_tabs: Vec<crate::ui::overlay_recommend_ui::PatternTabInfo>,
    pub(crate) state_tracker: AppStateTracker,
    pub(crate) record_db: Arc<RecordDB>,
    pub(crate) record_manager: Arc<RecordManager>,
    pub(crate) recommender: Arc<Recommender>,
    pub(crate) game_found_rx: Receiver<()>,
    pub(crate) exit_requested: Arc<AtomicBool>,
    pub(crate) ctx_holder: Arc<Mutex<Option<egui::Context>>>,
    pub(crate) session_initial_record: Option<overmax_data::RecordValue>,
    /// 오버레이 렌더 경로가 매 프레임 재계산하지 않도록 캐시한 업로드 필요 여부.
    /// 입력(`session.context` 의 rate/is_max_combo, `record_manager` 상태)이 바뀌는
    /// 모든 경로가 `refresh_overlay_data` 를 거치므로 그 안에서 갱신한다.
    pub(crate) overlay_upload_needed: bool,
    pub(crate) auto_upload: crate::ui::auto_upload::AutoUploadScheduler,
    pub(crate) platform: platform::PlatformState,
    pub(crate) toast: Option<crate::ui::components::ToastMessage>,
    pub(crate) last_detection_output: Option<DetectionOutput>,
    pub(crate) last_telemetry_snapshot:
        Option<overmax_engine::detector::telemetry::PipelineTelemetrySnapshot>,
    #[cfg(any(debug_assertions, feature = "telemetry"))]
    pub(crate) runtime_telemetry:
        Option<Arc<overmax_engine::detector::telemetry::RuntimeTelemetry>>,
    pub(crate) ipc_publisher: crate::system::ipc_server::IpcPublisher,
    pub(crate) ipc_bound_port: crate::system::ipc_server::BoundPortSlot,
    pub(crate) ipc_handle: crate::system::ipc_server::IpcServerHandle,
    pub(crate) ipc_cmd_rx: Receiver<crate::system::ipc_server::IpcCommand>,
    pub(crate) overlay_visible_override: Option<bool>,
    pub(crate) last_ipc_scene_key: Option<String>,
    pub(crate) last_ipc_context_key:
        Option<(i32, overmax_core::Mode, overmax_core::Difficulty, u32, bool)>,
}

impl NativeApp {
    fn new(
        initial_ctx: egui::Context,
        initial_hwnd: Option<isize>,
        paths: Arc<AppPaths>,
    ) -> Result<Self, String> {
        let root = Arc::new(paths.data_dir().to_path_buf());
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        let runtime_telemetry = Some(Arc::new(
            overmax_engine::detector::telemetry::RuntimeTelemetry::new(
                paths.data_dir(),
                env!("CARGO_PKG_VERSION"),
            ),
        ));
        #[cfg(not(any(debug_assertions, feature = "telemetry")))]
        let runtime_telemetry: Option<
            Arc<overmax_engine::detector::telemetry::RuntimeTelemetry>,
        > = None;
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

        detection_worker::spawn(
            paths.data_dir().to_path_buf(),
            app_settings.clone(),
            merged_settings.clone(),
            log_tx.clone(),
            game_found_tx,
            detection_tx,
            runtime_telemetry.clone(),
            presentation_observation,
            repaint_callback,
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
            paths,
            root,
            settings,
            ui_state,
            debug_state,
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
            recommendations: RecommendResult::empty(),
            pattern_tabs: Vec::new(),
            state_tracker: AppStateTracker::new(),
            record_db,
            record_manager,
            recommender,
            game_found_rx,
            exit_requested: exit_requested.clone(),
            ctx_holder: ctx_holder.clone(),
            session_initial_record: None,
            overlay_upload_needed: false,
            auto_upload: Default::default(),
            platform,
            toast: None,
            last_detection_output: None,
            last_telemetry_snapshot: None,
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            runtime_telemetry,
            ipc_publisher,
            ipc_bound_port,
            ipc_handle,
            ipc_cmd_rx,
            overlay_visible_override: None,
            last_ipc_scene_key: None,
            last_ipc_context_key: None,
        };

        app.handle_auto_refresh();
        Ok(app)
    }
    pub(crate) fn poll_delete_requests(&mut self, ctx: &egui::Context) {
        while let Ok(key) = self.sync_channels.delete_req_rx.try_recv() {
            let cand = self
                .sync_state
                .candidates
                .lock()
                .ok()
                .and_then(|g| g.iter().find(|c| c.matches_key(&key)).cloned());
            if let Some(c) = cand {
                if self
                    .record_manager
                    .delete(c.song_id, c.button_mode, c.difficulty)
                {
                    debug_ui::push_log(
                        &self.debug_state.log_lines,
                        self.max_log_lines(),
                        format!(
                            "[Sync] 로컬 기록 삭제 완료: {} ({} {})",
                            c.song_name, c.button_mode, c.difficulty
                        ),
                    );
                    self.spawn_scan(ctx.clone());
                    self.refresh_overlay_data();
                } else {
                    debug_ui::push_log(
                        &self.debug_state.log_lines,
                        self.max_log_lines(),
                        format!(
                            "[Sync] 로컬 기록 삭제 실패: {} ({} {})",
                            c.song_name, c.button_mode, c.difficulty
                        ),
                    );
                }
            }
        }
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

    pub(crate) fn poll_scan_requests(&mut self, ctx: &egui::Context) {
        if self.ui_state.scan_pending.swap(false, Ordering::Relaxed) {
            if let Ok(mut s) = self.sync_state.status.lock() {
                *s = crate::t!("status-scanning").to_string();
            }
            self.spawn_scan(ctx.clone());
        }
    }

    pub(crate) fn poll_upload_requests(&mut self, ctx: &egui::Context) {
        while let Ok(key) = self.sync_channels.upload_req_rx.try_recv() {
            let cand = self
                .sync_state
                .candidates
                .lock()
                .ok()
                .and_then(|g| g.iter().find(|c| c.matches_key(&key)).cloned());
            if let Some(c) = cand {
                self.spawn_upload(c, false, ctx.clone());
            }
        }
    }

    pub(crate) fn drain_sync_scan(&self) {
        while let Ok(res) = self.sync_channels.sync_rx.try_recv() {
            match res {
                Ok(list) => {
                    let n = list.len();
                    if let Ok(mut g) = self.sync_state.candidates.lock() {
                        *g = list;
                    }
                    if let Ok(mut s) = self.sync_state.status.lock() {
                        *s = crate::t!("candidate-count", n = n as i64);
                    }
                }
                Err(msg) => {
                    if let Ok(mut s) = self.sync_state.status.lock() {
                        *s = msg;
                    }
                }
            }
        }
    }

    pub(crate) fn drain_upload_results(&mut self) {
        let mut refreshed = false;
        while let Ok((key, is_quick_upload, status, msg)) =
            self.sync_channels.upload_res_rx.try_recv()
        {
            let success = status == "success";
            let mut matched_candidate = false;
            if let Ok(mut list) = self.sync_state.candidates.lock() {
                if let Some(c) = list.iter_mut().find(|item| item.matches_key(&key)) {
                    c.upload_status = status.clone();
                    c.upload_message = msg.clone();
                    matched_candidate = true;
                }
            }
            if is_quick_upload || !matched_candidate {
                let toast_text = if success {
                    format!("V-Archive: {}", msg)
                } else {
                    crate::t!("status-varchive-failed-toast", error = &msg)
                };
                self.toast = Some(crate::ui::components::ToastMessage {
                    text: toast_text,
                    is_success: success,
                    expires_at: std::time::Instant::now() + std::time::Duration::from_secs(3),
                });
            }
            if success {
                if let Some(val) = self.record_manager.get_local_record(key.0, key.1, key.2) {
                    self.record_manager.upsert_varchive_record(key, val);
                } else {
                    self.record_manager.refresh();
                }
                refreshed = true;
            }
        }
        if refreshed {
            self.refresh_overlay_data();
        }
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

    fn spawn_scan(&self, ctx: egui::Context) {
        let steam = self
            .sync_state
            .steam_id
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let tx = self.sync_channels.sync_tx.clone();
        let songs_path = self.paths.songs_json();
        let rdb = self.record_db.clone();
        std::thread::spawn(move || {
            let mut db = VArchiveDB::new();
            if let Err(e) = db.load_from_file(&songs_path) {
                let _ = tx.send(Err(format!("songs.json: {e}")));
                ctx.request_repaint();
                return;
            }
            let list = build_candidates(&db, rdb.as_ref(), &steam);
            let _ = tx.send(Ok(list));
            ctx.request_repaint();
        });
    }

    fn spawn_upload(&self, candidate: SyncCandidate, is_quick_upload: bool, ctx: egui::Context) {
        let Some(key) = candidate.key() else {
            return;
        };
        let settings = self.settings.get_merged();
        let steam = self
            .sync_state
            .steam_id
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let account_path = account_path_for_steam(&settings, &steam);
        let tx = self.sync_channels.upload_res_tx.clone();
        let record_db = self.record_db.clone();

        std::thread::spawn(move || {
            let path = Path::new(&account_path);
            if account_path.is_empty() || !path.exists() {
                let _ = tx.send((
                    key,
                    is_quick_upload,
                    "error".into(),
                    crate::t!("status-account-path-missing").to_string(),
                ));
                ctx.request_repaint();
                return;
            }
            let Some(account) = varchive_upload::parse_account_file(path) else {
                let _ = tx.send((
                    key,
                    is_quick_upload,
                    "error".into(),
                    crate::t!("status-account-parse-failed").to_string(),
                ));
                ctx.request_repaint();
                return;
            };
            let res = varchive_upload::upload_score_blocking(
                &account,
                &candidate.song_name,
                candidate.button_mode,
                candidate.difficulty,
                candidate.overmax_rate,
                candidate.overmax_mc,
                &candidate.composer,
            );
            if res.success {
                let base_message = if res.updated {
                    crate::t!("status-updated")
                } else {
                    crate::t!("status-registered")
                };

                match sync_varchive_cache_after_upload(
                    &record_db,
                    &settings,
                    &steam,
                    &candidate,
                    base_message,
                ) {
                    Ok(msg) => {
                        let _ = tx.send((key, is_quick_upload, "success".into(), msg));
                    }
                    Err(err_msg) => {
                        let _ = tx.send((
                            key,
                            is_quick_upload,
                            "success".into(),
                            crate::t!("sys-upload-cache-error", error = err_msg),
                        ));
                    }
                }
            } else {
                let _ = tx.send((key, is_quick_upload, "error".into(), res.message));
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn is_varchive_account_configured(&self) -> bool {
        let settings = self.settings.get_merged();
        let steam = self
            .sync_state
            .steam_id
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let account_path = account_path_for_steam(&settings, &steam);
        if account_path.is_empty() {
            return false;
        }
        std::path::Path::new(&account_path).exists()
    }

    pub(crate) fn varchive_user_id(&self) -> Option<String> {
        let settings = self.settings.get_merged();
        let steam = self
            .sync_state
            .steam_id
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        if steam.is_empty() {
            return None;
        }
        let varchive = settings.varchive();
        varchive
            .user_map
            .get(&steam)
            .and_then(|u| u.v_id.clone())
            .filter(|id| !id.is_empty())
    }

    pub(crate) fn current_pattern_needs_upload(&self) -> bool {
        self.session
            .context
            .as_ref()
            .is_some_and(|ctx| self.pattern_needs_upload(ctx))
    }

    /// `ctx` 기록이 V-Archive 기록보다 나은지(Rate +0.01 이상 또는 MAX COMBO 신규 달성) 판단한다.
    pub(crate) fn pattern_needs_upload(&self, ctx: &PlayContext) -> bool {
        let song_id = ctx.song_id;
        let mode = ctx.mode;
        let diff = ctx.diff;

        let local = self.record_manager.get_local_record(song_id, mode, diff);
        let varchive = self
            .record_manager
            .get_varchive_cache_record(song_id, mode, diff);

        let (mut local_rate, mut local_mc) = local.unwrap_or((0.0, false));
        if ctx.rate > local_rate {
            local_rate = ctx.rate;
        }
        if ctx.is_max_combo {
            local_mc = true;
        }

        match (Some((local_rate, local_mc)), varchive) {
            (Some((l_rate, l_mc)), Some((v_rate, v_mc))) => {
                (l_rate - v_rate) >= 0.01 || (l_mc && !v_mc)
            }
            (Some((l_rate, _)), None) => l_rate > 0.0,
            _ => false,
        }
    }

    /// 자동 업로드 스케줄러를 한 프레임 진행하고, 대기가 끝난 값을 업로드한다.
    pub(crate) fn tick_auto_upload(&mut self, ctx: &egui::Context) {
        let stable_context = self
            .session
            .context
            .as_ref()
            .filter(|_| self.session.is_stable);
        // 대기 중이 아니고 업로드 후보도 없으면 설정 조회 없이 빠져나간다.
        let has_candidate = stable_context.is_some() && self.overlay_upload_needed;
        if !(self.auto_upload.is_pending() || has_candidate) {
            return;
        }

        let settings = self.settings.get_merged().varchive().auto_upload;
        let obs = crate::ui::auto_upload::AutoUploadObservation {
            scene: self.session.scene,
            stable_context,
            eligible: self.overlay_upload_needed && self.is_varchive_account_configured(),
        };
        let fired = self
            .auto_upload
            .tick(std::time::Instant::now(), &obs, &settings);

        // 결과창 이탈 flush 는 현재 세션이 아닌 스냅샷 기준이므로 업로드 조건을 다시 확인한다.
        if let Some(snapshot) =
            fired.filter(|s| self.pattern_needs_upload(s) && self.is_varchive_account_configured())
        {
            debug_ui::push_log(
                &self.debug_state.log_lines,
                self.max_log_lines(),
                format!(
                    "[VArchive] 자동 업로드: {}, {}, {}, {:.2}%, MaxCombo: {}",
                    snapshot.song_id,
                    snapshot.mode,
                    snapshot.diff,
                    snapshot.rate,
                    snapshot.is_max_combo
                ),
            );
            self.upload_pattern(&snapshot, ctx.clone());
        }

        if self.auto_upload.is_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
    }

    pub(crate) fn upload_current_pattern(&self, ctx: egui::Context) {
        if let Some(session_ctx) = &self.session.context {
            self.upload_pattern(session_ctx, ctx);
        }
    }

    /// `session_ctx` 기록을 로컬 최고 기록과 합쳐 V-Archive에 단일 패턴 업로드한다.
    pub(crate) fn upload_pattern(&self, session_ctx: &PlayContext, ctx: egui::Context) {
        let song_id = session_ctx.song_id;
        let mode = session_ctx.mode;
        let diff = session_ctx.diff;

        let Some(song) = self.varchive_db.search_by_id(song_id) else {
            return;
        };
        let local = self.record_manager.get_local_record(song_id, mode, diff);
        let varchive = self
            .record_manager
            .get_varchive_cache_record(song_id, mode, diff);

        let (mut overmax_rate, mut overmax_mc) = local.unwrap_or((0.0, false));
        if session_ctx.rate > overmax_rate {
            overmax_rate = session_ctx.rate;
        }
        if session_ctx.is_max_combo {
            overmax_mc = true;
        }

        if overmax_rate > 0.0 || overmax_mc {
            self.record_manager
                .upsert(song_id, mode, diff, overmax_rate, overmax_mc, true);
        }

        let (v_rate, v_mc) = match varchive {
            Some((r, mc)) => (Some(r), Some(mc)),
            None => (None, None),
        };

        let pattern_level = song
            .get_pattern(session_ctx.mode, session_ctx.diff)
            .and_then(|p| p.level);

        let candidate = overmax_data::SyncCandidate {
            song_id,
            song_name: song.name.to_string(),
            composer: song.composer.to_string(),
            dlc: song.dlc_code.to_string(),
            button_mode: session_ctx.mode,
            difficulty: session_ctx.diff,
            pattern_level,
            overmax_rate: overmax_rate as f64,
            overmax_mc,
            varchive_rate: v_rate.map(|r| r as f64),
            varchive_mc: v_mc,
            upload_status: String::new(),
            upload_message: String::new(),
        };

        self.spawn_upload(candidate, true, ctx);
    }

    pub(crate) fn handle_auto_refresh(&mut self) {
        let settings = self.settings.get_merged();
        let varchive = settings.varchive();

        let sid = steam_session::most_recent_steam_id().unwrap_or_default();
        if sid.is_empty() {
            return;
        }

        let v_id = varchive
            .user_map
            .get(&sid)
            .and_then(|u| u.v_id.as_deref())
            .unwrap_or("");

        if !v_id.is_empty() {
            debug_ui::push_log(
                &self.debug_state.log_lines,
                self.max_log_lines(),
                format!(
                    "[VArchive] 자동 갱신 시작 (SteamID: {}, V-ID: {})",
                    sid, v_id
                ),
            );
            let _ = self
                .sync_channels
                .fetch_req_tx
                .send((sid, v_id.to_string(), 0));
        }
    }

    pub(crate) fn poll_fetch_requests(&mut self, ctx: &egui::Context) {
        while let Ok((steam_id, v_id, button)) = self.sync_channels.fetch_req_rx.try_recv() {
            self.spawn_fetch(steam_id, v_id, button, ctx.clone());
        }
    }

    pub(crate) fn poll_startup_cache(&mut self) {
        if self
            .startup_cache_manager
            .poll_updates(&mut self.varchive_db, &mut self.sheet_meta)
        {
            self.on_varchive_db_updated();
        }
    }

    fn on_varchive_db_updated(&mut self) {
        self.recommender = Arc::new(self.recommender.with_varchive_db(self.varchive_db.clone()));
        self.record_manager.refresh();
        self.refresh_overlay_data();
    }

    pub(crate) fn drain_fetch_results(&mut self) {
        let mut refreshed = false;
        while let Ok((v_id, btn, res)) = self.sync_channels.fetch_res_rx.try_recv() {
            match res {
                Ok(_) => {
                    refreshed = true;
                }
                Err(e) => {
                    debug_ui::push_log(
                        &self.debug_state.log_lines,
                        self.max_log_lines(),
                        format!("[VArchiveClient] {} ({}B) API 요청 실패: {}", v_id, btn, e),
                    );
                }
            }
        }
        if refreshed {
            self.record_manager.refresh();
            self.refresh_overlay_data();
        }
    }

    fn spawn_fetch(&self, steam_id: String, v_id: String, button: i32, ctx: egui::Context) {
        let tx = self.sync_channels.fetch_res_tx.clone();
        let log_lines = self.debug_state.log_lines.clone();
        let max_lines = self.max_log_lines();
        let record_db = self.record_db.clone();

        std::thread::spawn(move || {
            let buttons = if button == 0 {
                vec![4, 5, 6, 8]
            } else {
                vec![button]
            };
            for b in buttons {
                debug_ui::push_log(
                    &log_lines,
                    max_lines,
                    format!("[VArchiveClient] 기록 요청 중: {} ({}B)", v_id, b),
                );

                let since = record_db.get_latest_updated_at_from_db(&steam_id, b);
                if let Some(ref s) = since {
                    debug_ui::push_log(
                        &log_lines,
                        max_lines,
                        format!("[VArchiveClient] 증분 조회 적용 (since={})", s),
                    );
                }

                match varchive_upload::fetch_records_blocking(&v_id, b, since.as_deref()) {
                    Ok(data) => {
                        let clear_first = since.is_none();
                        let save_res = record_db.merge_varchive_fetched_records(
                            &steam_id,
                            b,
                            &data,
                            clear_first,
                        );

                        if let Err(e) = save_res {
                            debug_ui::push_log(
                                &log_lines,
                                max_lines,
                                format!("[VArchiveClient] 캐시 저장 실패: {}", e),
                            );
                            let _ = tx.send((v_id.clone(), b, Err(e)));
                        } else {
                            debug_ui::push_log(
                                &log_lines,
                                max_lines,
                                format!("[VArchiveClient] 캐시 저장 완료 ({}B)", b),
                            );
                            let _ = tx.send((v_id.clone(), b, Ok(1)));
                        }
                    }
                    Err(e) => {
                        let _ = tx.send((v_id.clone(), b, Err(e)));
                    }
                }
            }
            ctx.request_repaint();
        });
    }
}

/// 업로드 성공 후 V-Archive 캐시 병합 및 Top-50 랭크 메시지를 생성한다.
/// 캐시 갱신 실패 시 Err(사유)를 반환하되 업로드 자체는 성공임에 유의한다.
fn sync_varchive_cache_after_upload(
    record_db: &RecordDB,
    settings: &overmax_data::Settings,
    steam: &str,
    candidate: &SyncCandidate,
    base_message: &str,
) -> Result<String, String> {
    let btn = button_num(candidate.button_mode.as_str());

    let varchive_settings = settings.varchive();
    let v_id = varchive_settings
        .user_map
        .get(steam)
        .and_then(|u| u.v_id.as_deref())
        .unwrap_or("")
        .to_string();

    let cache_updated = if !v_id.is_empty() {
        match varchive_upload::fetch_single_song_records_blocking(&v_id, btn, candidate.song_id) {
            Ok(data) => record_db
                .merge_varchive_fetched_records(steam, btn, &data, false)
                .map_err(|e| crate::t!("sys-api-ok-cache-failed", error = e)),
            Err(e) => {
                let payload = upload_fallback_payload(candidate);
                record_db
                    .merge_varchive_fetched_records(steam, btn, &payload, false)
                    .map_err(|ue| {
                        crate::t!(
                            "sys-api-and-fallback-failed",
                            api_error = e,
                            fallback_error = ue
                        )
                    })
            }
        }
    } else {
        let payload = upload_fallback_payload(candidate);
        record_db
            .merge_varchive_fetched_records(steam, btn, &payload, false)
            .map_err(|e| crate::t!("sys-fallback-cache-failed", error = e))
    };

    let mut msg = base_message.to_string();
    match cache_updated {
        Ok(_) => {
            if let Ok(Some(rank)) = record_db.get_varchive_top50_rank(
                steam,
                candidate.button_mode,
                &candidate.song_id.to_string(),
                candidate.difficulty,
            ) {
                let place_msg = crate::t!(
                    "sys-place-achieved",
                    mode = candidate.button_mode,
                    rank = rank
                );
                msg = crate::t!(
                    "sys-upload-msg-with-rank",
                    message = &msg,
                    rank_msg = &place_msg
                );
            }
            Ok(msg)
        }
        Err(err_msg) => Err(err_msg),
    }
}

/// 오프라인/실패 폴백용 단일 곡 기록 페이로드.
fn upload_fallback_payload(candidate: &SyncCandidate) -> serde_json::Value {
    serde_json::json!({
        "records": [
            {
                "title": candidate.song_id,
                "pattern": candidate.difficulty,
                "score": candidate.overmax_rate,
                "maxCombo": candidate.overmax_mc,
                "updatedAt": ""
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::platform::native_options;

    #[test]
    fn main_overlay_stays_out_of_taskbar() {
        let options = native_options(&overmax_data::Settings::default());

        assert_eq!(options.viewport.taskbar, Some(false));
    }
}
