//! Single `eframe` app: overlay + deferred debug / settings / sync viewports.

use eframe::egui::ViewportBuilder;
use overmax_core::{GameSessionState, Changed};
use overmax_data::{
    build_candidates, load_base_settings, load_merged_settings, normalize_settings,
    upsert_varchive_cache_record, DataCompatibility, PatternSheetMeta, RecommendResult, RecordDB,
    RecordManager, Recommender, SyncCandidate, VArchiveDB,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::system::cache_update;
use crate::ui::debug_ui;
use overmax_engine::detector::ocr_engine::OcrTelemetry;
use overmax_engine::detector::detection_pipeline::DetectionOutput;
use eframe::egui;
use overmax_engine::detector::detection_worker;
use crate::system::native_helpers::{
    account_path_for_steam, button_num, first_steam_from_settings,
};
use crate::ui::overlay_ui;
use crate::system::single_instance::SingleInstanceGuard;
use crate::system::steam_session;
#[cfg(target_os = "windows")]
use crate::ui::tray_icon::TrayIcon;
use crate::ui::ui_command::UiCommand;
use crate::system::updater::{self, AppUpdateConfig};
use crate::system::varchive_upload;

fn load_icon() -> Option<eframe::egui::IconData> {
    let icon_bytes = include_bytes!("../../../../assets/overmax.ico");
    if let Ok(img) = image::load_from_memory(icon_bytes) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        return Some(eframe::egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        });
    }
    None
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn console_ctrl_handler(_ctrl_type: u32) -> i32 {
    crate::ui::tray_icon::force_cleanup_tray();
    0 // FALSE
}

pub fn run_native_app() -> eframe::Result<()> {
    #[cfg(target_os = "windows")]
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(console_ctrl_handler), 1);
    }

    let Some(_single) = SingleInstanceGuard::try_acquire() else {
        std::process::exit(0);
    };

    let root = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("cwd: {e}");
        std::process::exit(1);
    });
    let defaults: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../settings.json"
    )))
    .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
    let mut merged = load_merged_settings(root.as_path(), defaults);
    normalize_settings(&mut merged);
    let app_settings: overmax_data::Settings = serde_json::from_value(merged.clone()).unwrap_or_default();
    let upd_cfg = AppUpdateConfig::from_settings(&app_settings);
    let ok_notify = updater::notify_previous_update(root.as_path()).unwrap_or_else(|e| {
        eprintln!("[AppUpdater] notify: {e}");
        true
    });
    if !ok_notify {
        return Ok(());
    }
    match updater::check_and_apply_update_blocking(root.as_path(), &upd_cfg) {
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

    let options = native_options(&app_settings);

    eframe::run_native(
        "Overmax",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(eframe::egui::Visuals::dark());
            overlay_ui::install_cjk_fonts(&cc.egui_ctx);
            NativeApp::new()
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|e| {
                    eprintln!("native app init: {e}");
                    Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
                })
        }),
    )
}

#[cfg(target_os = "windows")]
fn is_position_on_screen(x: f32, y: f32) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };
    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vwidth = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vheight = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        if vwidth > 0 && vheight > 0 {
            let px = x as i32;
            let py = y as i32;
            px >= vx && px < (vx + vwidth) && py >= vy && py < (vy + vheight)
        } else {
            x >= 0.0 && y >= 0.0
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn is_position_on_screen(_x: f32, _y: f32) -> bool {
    true
}

fn native_options(settings: &overmax_data::Settings) -> eframe::NativeOptions {
    let overlay = settings.overlay();
    let is_lite = overlay.lite_mode;

    let mut builder = ViewportBuilder::default()
        .with_title("Overmax")
        .with_inner_size([overlay_ui::BASE_WIDTH, overlay_ui::BASE_HEIGHT])
        .with_resizable(true)
        .with_decorations(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_always_on_top()
        .with_visible(!is_lite);

    if let Some(icon) = load_icon() {
        builder = builder.with_icon(icon);
    }

    if !is_lite {
        let pos = &overlay.position;
        if let (Some(x), Some(y)) = (pos.x, pos.y) {
            let px = x as f32;
            let py = y as f32;
            if is_position_on_screen(px, py) {
                builder = builder.with_position(eframe::egui::pos2(px, py));
            }
        }
    }

    eframe::NativeOptions {
        viewport: builder,
        ..Default::default()
    }
}

pub struct SharedSettings {
    pub defaults: Arc<Value>,
    pub base: Arc<Mutex<Value>>,
    pub merged: Arc<Mutex<Value>>,
    pub draft: Arc<Mutex<Value>>,
}

impl SharedSettings {
    pub fn get_merged(&self) -> overmax_data::Settings {
        let val = match self.merged.lock() {
            Ok(g) => g.clone(),
            Err(_) => serde_json::Value::Object(serde_json::Map::new()),
        };
        serde_json::from_value(val).unwrap_or_default()
    }
}

pub struct SharedUiState {
    pub debug_open: Arc<AtomicBool>,
    pub settings_open: Arc<AtomicBool>,
    pub sync_open: Arc<AtomicBool>,
    pub scan_pending: Arc<AtomicBool>,
}

pub struct SharedDebugState {
    pub log_lines: Arc<Mutex<VecDeque<String>>>,
    pub paused: Arc<AtomicBool>,
    pub filters: Arc<Mutex<std::collections::HashMap<String, bool>>>,
    pub rate_ocr: Arc<Mutex<Option<OcrTelemetry>>>,
    pub rate_ocr_texture: Arc<Mutex<Option<egui::TextureHandle>>>,
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
    pub(crate) upload_req_rx: Receiver<usize>,
    pub(crate) upload_req_tx: Sender<usize>,
    pub(crate) upload_res_rx: Receiver<(usize, String, String)>,
    pub(crate) upload_res_tx: Sender<(usize, String, String)>,
    pub(crate) fetch_req_rx: Receiver<(String, String, i32)>,
    pub(crate) fetch_req_tx: Sender<(String, String, i32)>,
    pub(crate) fetch_res_rx: Receiver<(String, i32, Result<usize, String>)>,
    pub(crate) fetch_res_tx: Sender<(String, i32, Result<usize, String>)>,
    pub(crate) delete_req_rx: Receiver<usize>,
    pub(crate) delete_req_tx: Sender<usize>,
}

pub struct AppStateTracker {
    pub prev_settings_open: Changed<bool>,
    pub prev_sync_open: Changed<bool>,
    pub prev_scale: Changed<f32>,
    pub prev_overlay_on: Changed<bool>,
    pub prev_is_lite: Changed<bool>,
    pub prev_passthrough: Changed<Option<bool>>,
    pub prev_protected: Changed<Option<bool>>,
}

impl AppStateTracker {
    pub fn new() -> Self {
        Self {
            prev_settings_open: Changed::new(false),
            prev_sync_open: Changed::new(false),
            prev_scale: Changed::new(1.0),
            prev_overlay_on: Changed::new(false),
            prev_is_lite: Changed::new(false),
            prev_passthrough: Changed::new(None),
            prev_protected: Changed::new(None),
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Default)]
pub struct WindowsWindowCache {
    pub cached_hwnd: Option<isize>,
    pub cached_game_hwnd: Option<isize>,
    pub last_applied_opacity: Option<f32>,
    pub logged_opacity_fail: bool,
    pub prev_snap_geometry: Option<(i32, i32, i32, i32)>,
}

pub struct NativeApp {
    pub(crate) root: Arc<std::path::PathBuf>,
    pub(crate) settings: SharedSettings,
    pub(crate) ui_state: SharedUiState,
    pub(crate) debug_state: SharedDebugState,
    pub(crate) sync_state: SharedSyncState,
    pub(crate) log_rx: Option<Receiver<String>>,
    pub(crate) game_rect: Arc<Mutex<Option<overmax_engine::capture::window_tracker::WindowRect>>>,
    pub(crate) session: GameSessionState,
    pub(crate) confidence: f32,
    pub(crate) recorded_states: std::collections::HashSet<(u32, String, String)>,
    pub(crate) sync_channels: SyncWorkerChannels,
    pub(crate) detection_rx: Receiver<DetectionOutput>,
    pub(crate) ui_cmd_rx: Receiver<UiCommand>,
    pub(crate) varchive_db: Arc<VArchiveDB>,
    pub(crate) sheet_meta: Arc<PatternSheetMeta>,
    pub(crate) recommendations: RecommendResult,
    pub(crate) pattern_tabs: Vec<crate::ui::overlay_recommend_ui::PatternTabInfo>,
    pub(crate) state_tracker: AppStateTracker,
    pub(crate) is_dragging: bool,
    pub(crate) record_db: Arc<RecordDB>,
    pub(crate) record_manager: Arc<RecordManager>,
    pub(crate) recommender: Arc<Recommender>,
    pub(crate) game_found_rx: Receiver<()>,
    pub(crate) exit_requested: Arc<AtomicBool>,
    pub(crate) ctx_holder: Arc<Mutex<Option<egui::Context>>>,
    #[cfg(target_os = "windows")]
    pub(crate) _tray: Option<TrayIcon>,
    #[cfg(target_os = "windows")]
    pub(crate) win_cache: WindowsWindowCache,
    pub(crate) last_painted_rect: Option<egui::Rect>,
}

impl NativeApp {
    fn new() -> Result<Self, String> {
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = Arc::new(root);
        let defaults: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../settings.json"
        )))
        .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
        let defaults = Arc::new(defaults);

        let base_settings = Arc::new(Mutex::new(load_base_settings(
            root.as_ref(),
            (*defaults).clone(),
        )));
        let mut merged = load_merged_settings(root.as_ref(), (*defaults).clone());
        normalize_settings(&mut merged);

        let (log_tx, log_rx) = mpsc::channel();
        let (game_found_tx, game_found_rx) = mpsc::channel();
        let (detection_tx, detection_rx) = mpsc::channel();

        let app_settings: overmax_data::Settings = serde_json::from_value(merged.clone()).unwrap_or_default();
        cache_update::refresh_startup_caches(root.as_ref(), &app_settings, &mut |msg| {
            let _ = log_tx.send(msg);
        });

        let merged_settings = Arc::new(Mutex::new(merged.clone()));
        let settings_draft = Arc::new(Mutex::new(merged.clone()));

        let compat = DataCompatibility::current();
        let recent_steam = steam_session::most_recent_steam_id();
        let mut record_db = RecordDB::new(root.join(compat.record_db), recent_steam.as_deref());
        record_db.initialize();
        let record_db = Arc::new(record_db);
        let record_manager = Arc::new(RecordManager::new(
            record_db.clone(),
            root.join("cache").join("varchive"),
        ));
        record_manager.refresh();

        let mut varchive_db = VArchiveDB::new();
        let dlcs_path = root.join(compat.dlcs_json);
        let _ = varchive_db.load_dlcs_from_file(&dlcs_path);

        let songs_path = root.join(compat.songs_json);
        if let Err(e) = varchive_db.load_from_file(&songs_path) {
            let _ = log_tx.send(format!("[VArchive] songs load failed: {e}"));
        }
        let varchive_db = Arc::new(varchive_db);

        let recommender = Arc::new(Recommender::new(
            varchive_db.clone(),
            record_manager.clone(),
        ));

        let sheet_meta = Arc::new(PatternSheetMeta::load_cache(
            root.join("cache").join("pattern_meta.json"),
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

        let (sync_tx, sync_rx) = mpsc::channel();
        let (upload_req_tx, upload_req_rx) = mpsc::channel();
        let (upload_res_tx, upload_res_rx) = mpsc::channel();
        let (delete_req_tx, delete_req_rx) = mpsc::channel::<usize>();
        let (ui_cmd_tx, ui_cmd_rx) = mpsc::channel();
        let (fetch_req_tx, fetch_req_rx) = mpsc::channel();
        let (fetch_res_tx, fetch_res_rx) = mpsc::channel();

        // 시작 시 자동 갱신
        let varchive_settings = app_settings.varchive();
        if varchive_settings.auto_refresh {
            if let Some(user_info) = varchive_settings.user_map.get(&steam0) {
                if let Some(v_id) = &user_info.v_id {
                    if !v_id.is_empty() {
                        let _ = fetch_req_tx.send((steam0.clone(), v_id.to_string(), 0));
                    }
                }
            }
        }

        let ctx_holder: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(None));
        let ctx_holder_clone = ctx_holder.clone();

        let repaint_callback = Box::new(move || {
            if let Ok(holder) = ctx_holder_clone.lock() {
                if let Some(ctx) = &*holder {
                    ctx.request_repaint();
                }
            }
        });

        detection_worker::spawn(
            (*root).clone(),
            merged_settings
                .lock()
                .map_err(|_| "settings lock poisoned")?
                .clone(),
            log_tx.clone(),
            game_found_tx,
            detection_tx,
            repaint_callback,
        );

        let mut filters = std::collections::HashMap::new();
        filters.insert("[ScreenCapture]".to_string(), true);
        filters.insert("[Overlay]".to_string(), true);
        filters.insert("[VArchive]".to_string(), true);
        filters.insert("[WindowTracker]".to_string(), true);
        filters.insert("[Main]".to_string(), true);

        let settings = SharedSettings {
            defaults: defaults.clone(),
            base: base_settings.clone(),
            merged: merged_settings.clone(),
            draft: settings_draft.clone(),
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
            rate_ocr: Arc::new(Mutex::new(None)),
            rate_ocr_texture: Arc::new(Mutex::new(None)),
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
            root,
            settings,
            ui_state,
            debug_state,
            sync_state,
            log_rx: Some(log_rx),
            game_rect: Arc::new(Mutex::new(None)),
            session: GameSessionState::detecting(),
            confidence: 0.0,
            recorded_states: std::collections::HashSet::new(),
            sync_channels: SyncWorkerChannels {
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
            },
            detection_rx,
            ui_cmd_rx,
            varchive_db,
            sheet_meta,
            recommendations: RecommendResult::empty(),
            pattern_tabs: Vec::new(),
            state_tracker: AppStateTracker::new(),
            is_dragging: false,
            record_db,
            record_manager,
            recommender,
            game_found_rx,
            exit_requested: exit_requested.clone(),
            ctx_holder: ctx_holder.clone(),
            #[cfg(target_os = "windows")]
            _tray: Some(TrayIcon::spawn(ui_cmd_tx, merged_settings.clone(), ctx_holder)),
            #[cfg(target_os = "windows")]
            win_cache: WindowsWindowCache::default(),
            last_painted_rect: None,
        };

        app.handle_auto_refresh();
        Ok(app)
    }

    pub(crate) fn poll_delete_requests(&mut self, ctx: &egui::Context) {
        while let Ok(idx) = self.sync_channels.delete_req_rx.try_recv() {
            let cand = self.sync_state.candidates.lock()
                .ok()
                .and_then(|g| g.get(idx).cloned());
            if let Some(c) = cand {
                if self.record_manager.delete(c.song_id, &c.button_mode, &c.difficulty) {
                    debug_ui::push_log(
                        &self.debug_state.log_lines,
                        self.max_log_lines(),
                        format!("[Sync] 로컬 기록 삭제 완료: {} ({} {})", c.song_name, c.button_mode, c.difficulty),
                    );
                    self.spawn_scan(ctx.clone());
                    self.refresh_overlay_data();
                } else {
                    debug_ui::push_log(
                        &self.debug_state.log_lines,
                        self.max_log_lines(),
                        format!("[Sync] 로컬 기록 삭제 실패: {} ({} {})", c.song_name, c.button_mode, c.difficulty),
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
                *s = "스캔 중…".into();
            }
            self.spawn_scan(ctx.clone());
        }
    }

    pub(crate) fn poll_upload_requests(&mut self, ctx: &egui::Context) {
        while let Ok(idx) = self.sync_channels.upload_req_rx.try_recv() {
            let cand = self.sync_state.candidates.lock()
                .ok()
                .and_then(|g| g.get(idx).cloned());
            if let Some(c) = cand {
                self.spawn_upload(idx, c, ctx.clone());
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
                        *s = format!("후보 {n}건");
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
        while let Ok((idx, status, msg)) = self.sync_channels.upload_res_rx.try_recv() {
            let success = status == "success";
            if let Ok(mut list) = self.sync_state.candidates.lock() {
                if let Some(c) = list.get_mut(idx) {
                    c.upload_status = status;
                    c.upload_message = msg;
                }
            }
            if success {
                self.record_manager.refresh();
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

    fn spawn_scan(&self, ctx: egui::Context) {
        let steam = self.sync_state.steam_id.lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let tx = self.sync_channels.sync_tx.clone();
        let root = self.root.clone();
        let rdb = self.record_db.clone();
        std::thread::spawn(move || {
            let compat = DataCompatibility::current();
            let songs_path = root.join(compat.songs_json);
            let mut db = VArchiveDB::new();
            if let Err(e) = db.load_from_file(&songs_path) {
                let _ = tx.send(Err(format!("songs.json: {e}")));
                ctx.request_repaint();
                return;
            }
            let cache_root = root.join("cache").join("varchive");
            let list = build_candidates(&db, rdb.as_ref(), &steam, &cache_root);
            let _ = tx.send(Ok(list));
            ctx.request_repaint();
        });
    }

    fn spawn_upload(&self, index: usize, candidate: SyncCandidate, ctx: egui::Context) {
        let settings = self.settings.get_merged();
        let steam = self.sync_state.steam_id.lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let account_path = account_path_for_steam(&settings, &steam);
        let tx = self.sync_channels.upload_res_tx.clone();
        let root = self.root.clone();

        std::thread::spawn(move || {
            let path = Path::new(&account_path);
            if account_path.is_empty() || !path.exists() {
                let _ = tx.send((index, "error".into(), "account.txt 경로 없음".into()));
                ctx.request_repaint();
                return;
            }
            let Some(account) = varchive_upload::parse_account_file(path) else {
                let _ = tx.send((index, "error".into(), "account.txt 파싱 실패".into()));
                ctx.request_repaint();
                return;
            };
            let res = varchive_upload::upload_score_blocking(
                &account,
                &candidate.song_name,
                &candidate.button_mode,
                &candidate.difficulty,
                candidate.overmax_rate,
                candidate.overmax_mc,
                &candidate.composer,
            );
            if res.success {
                let success_message = if res.updated {
                    "갱신 완료"
                } else {
                    "등록 완료"
                };
                let btn = button_num(&candidate.button_mode);
                let cache_root = root.join("cache").join("varchive");
                if let Err(e) = upsert_varchive_cache_record(
                    &cache_root,
                    &steam,
                    btn,
                    candidate.song_id,
                    &candidate.difficulty,
                    candidate.overmax_rate,
                    candidate.overmax_mc,
                ) {
                    let _ = tx.send((
                        index,
                        "success".into(),
                        format!("업로드 OK, 캐시 갱신 실패: {e}"),
                    ));
                } else {
                    let _ = tx.send((index, "success".into(), success_message.into()));
                }
            } else {
                let _ = tx.send((index, "error".into(), res.message));
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn is_varchive_account_configured(&self) -> bool {
        let settings = self.settings.get_merged();
        let steam = self.sync_state.steam_id.lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let account_path = account_path_for_steam(&settings, &steam);
        if account_path.is_empty() {
            return false;
        }
        std::path::Path::new(&account_path).exists()
    }

    pub(crate) fn current_pattern_needs_upload(&self) -> bool {
        let Some(ctx) = &self.session.context else {
            return false;
        };
        let song_id = ctx.song_id as i32;
        let mode = &ctx.mode;
        let diff = &ctx.diff;

        let local = self.record_manager.get_local_record(song_id, mode, diff);
        let varchive = self.record_manager.get_varchive_cache_record(song_id, mode, diff);

        match (local, varchive) {
            (Some((l_rate, l_mc)), Some((v_rate, v_mc))) => {
                (l_rate - v_rate) >= 0.01 || (l_mc && !v_mc)
            }
            (Some((l_rate, _)), None) => {
                l_rate > 0.0
            }
            _ => false,
        }
    }

    pub(crate) fn upload_current_pattern(&self, ctx: egui::Context) {
        let Some(session_ctx) = &self.session.context else {
            return;
        };
        let song_id = session_ctx.song_id as i32;
        let mode = &session_ctx.mode;
        let diff = &session_ctx.diff;

        let Some(song) = self.varchive_db.search_by_id(song_id) else {
            return;
        };
        let local = self.record_manager.get_local_record(song_id, mode, diff);
        let varchive = self.record_manager.get_varchive_cache_record(song_id, mode, diff);

        let (overmax_rate, overmax_mc) = local.unwrap_or((0.0, false));
        let (v_rate, v_mc) = match varchive {
            Some((r, mc)) => (Some(r), Some(mc)),
            None => (None, None),
        };

        let candidate = overmax_data::SyncCandidate {
            song_id,
            song_name: song.name.clone(),
            composer: song.composer.to_string(),
            dlc: song.dlc_code.to_string(),
            button_mode: mode.clone(),
            difficulty: diff.clone(),
            overmax_rate,
            overmax_mc,
            varchive_rate: v_rate,
            varchive_mc: v_mc,
            upload_status: String::new(),
            upload_message: String::new(),
        };

        self.spawn_upload(999999, candidate, ctx);
    }

    pub(crate) fn handle_auto_refresh(&mut self) {
        let settings = self.settings.get_merged();
        let varchive = settings.varchive();
        if !varchive.auto_refresh {
            return;
        }

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
                format!("[VArchive] 자동 갱신 시작 (SteamID: {}, V-ID: {})", sid, v_id),
            );
            let _ = self.sync_channels.fetch_req_tx.send((sid, v_id.to_string(), 0));
        }
    }

    pub(crate) fn poll_fetch_requests(&mut self, ctx: &egui::Context) {
        while let Ok((steam_id, v_id, button)) = self.sync_channels.fetch_req_rx.try_recv() {
            self.spawn_fetch(steam_id, v_id, button, ctx.clone());
        }
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
        let cache_root = self.root.join("cache").join("varchive");
        let log_lines = self.debug_state.log_lines.clone();
        let max_lines = self.max_log_lines();
        
        std::thread::spawn(move || {
            let buttons = if button == 0 { vec![4, 5, 6, 8] } else { vec![button] };
            for b in buttons {
                debug_ui::push_log(
                    &log_lines,
                    max_lines,
                    format!("[VArchiveClient] 기록 요청 중: {} ({}B)", v_id, b),
                );
                
                match varchive_upload::fetch_records_blocking(&v_id, b) {
                    Ok(data) => {
                        if let Err(e) = overmax_data::save_fetched_records_to_cache(&cache_root, &steam_id, &v_id, b, &data) {
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
                    },
                    Err(e) => {
                        let _ = tx.send((v_id.clone(), b, Err(e)));
                    }
                }
            }
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::native_options;

    #[test]
    fn main_overlay_stays_out_of_taskbar() {
        let options = native_options(&overmax_data::Settings::default());

        assert_eq!(options.viewport.taskbar, Some(false));
    }
}
