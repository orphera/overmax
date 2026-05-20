//! Single `eframe` app: overlay + deferred debug / settings / sync viewports.

use eframe::egui::ViewportBuilder;
use overmax_core::GameSessionState;
use overmax_data::{
    build_candidates, load_base_settings, load_merged_settings, normalize_settings,
    upsert_varchive_cache_record, DataCompatibility, PatternSheetMeta, RecommendResult, RecordDB,
    RecordManager, SyncCandidate, VArchiveDB,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::cache_update;
use crate::debug_ui;
use crate::detection_pipeline::DetectionOutput;
use crate::detection_worker;
use crate::native_helpers::{
    account_path_for_steam, button_num, first_steam_from_settings,
};
use crate::overlay_ui;
use crate::single_instance::SingleInstanceGuard;
use crate::steam_session;
#[cfg(target_os = "windows")]
use crate::tray_icon::TrayIcon;
use crate::ui_command::UiCommand;
use crate::updater::{self, AppUpdateConfig};
use crate::varchive_upload;

pub fn run_native_app() -> eframe::Result<()> {
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
    let upd_cfg = AppUpdateConfig::from_merged_settings(&merged);
    let ok_notify = updater::notify_previous_update(root.as_path()).unwrap_or_else(|e| {
        eprintln!("[AppUpdater] notify: {e}");
        true
    });
    if !ok_notify {
        return Ok(());
    }
    match updater::check_and_apply_update_blocking(root.as_path(), &upd_cfg) {
        Ok(true) => {}
        Ok(false) => std::process::exit(0),
        Err(e) => {
            eprintln!("[AppUpdater] {e}");
            std::process::exit(1);
        }
    }

    let options = native_options(&merged);

    eframe::run_native(
        "Overmax",
        options,
        Box::new(|cc| {
            overlay_ui::install_cjk_fonts(&cc.egui_ctx);
            NativeApp::new()
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|e| {
                    eprintln!("native app init: {e}");
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                        as Box<dyn std::error::Error + Send + Sync>
                })
        }),
    )
}

fn native_options(merged: &Value) -> eframe::NativeOptions {
    let mut builder = ViewportBuilder::default()
        .with_title("Overmax")
        .with_inner_size([overlay_ui::BASE_WIDTH, overlay_ui::BASE_HEIGHT])
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_always_on_top();

    if let Some(pos) = merged.get("overlay").and_then(|o| o.get("position")) {
        if let (Some(x), Some(y)) = (
            pos.get("x").and_then(|v| v.as_f64()),
            pos.get("y").and_then(|v| v.as_f64()),
        ) {
            builder = builder.with_position(eframe::egui::pos2(x as f32, y as f32));
        }
    }

    eframe::NativeOptions {
        viewport: builder,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::native_options;

    #[test]
    fn main_overlay_stays_out_of_taskbar() {
        let options = native_options(&serde_json::json!({}));

        assert_eq!(options.viewport.taskbar, Some(false));
    }
}

pub struct NativeApp {
    pub(crate) root: Arc<std::path::PathBuf>,
    pub(crate) defaults: Arc<Value>,
    pub(crate) base_settings: Arc<Mutex<Value>>,
    pub(crate) merged_settings: Arc<Mutex<Value>>,
    pub(crate) settings_draft: Arc<Mutex<Value>>,
    pub(crate) debug_open: Arc<AtomicBool>,
    pub(crate) settings_open: Arc<AtomicBool>,
    pub(crate) sync_open: Arc<AtomicBool>,
    pub(crate) scan_pending: Arc<AtomicBool>,
    pub(crate) log_lines: Arc<Mutex<VecDeque<String>>>,
    pub(crate) log_rx: Option<Receiver<String>>,
    pub(crate) debug_paused: Arc<AtomicBool>,
    pub(crate) game_rect: Arc<Mutex<Option<crate::window_tracker::WindowRect>>>,
    pub(crate) debug_filters: Arc<Mutex<std::collections::HashMap<String, bool>>>,
    pub(crate) session: GameSessionState,
    pub(crate) confidence: f32,
    pub(crate) recorded_states: std::collections::HashSet<(u32, String, String)>,
    pub(crate) sync_steam_id: Arc<Mutex<String>>,
    pub(crate) sync_status: Arc<Mutex<String>>,
    pub(crate) sync_candidates: Arc<Mutex<Vec<SyncCandidate>>>,
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
    pub(crate) detection_rx: Receiver<DetectionOutput>,
    pub(crate) ui_cmd_rx: Receiver<UiCommand>,
    pub(crate) varchive_db: Arc<VArchiveDB>,
    pub(crate) sheet_meta: Arc<PatternSheetMeta>,
    pub(crate) recommendations: RecommendResult,
    pub(crate) pattern_tabs: Vec<crate::overlay_recommend_ui::PatternTabInfo>,
    pub(crate) prev_settings_open: bool,
    pub(crate) prev_scale: f32,
    pub(crate) prev_overlay_on: bool,
    pub(crate) record_db: Arc<RecordDB>,
    pub(crate) record_manager: Arc<RecordManager>,
    pub(crate) game_found_rx: Receiver<()>,
    pub(crate) exit_requested: Arc<AtomicBool>,
    #[cfg(target_os = "windows")]
    pub(crate) _tray: Option<TrayIcon>,
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

        cache_update::refresh_startup_caches(root.as_ref(), &merged, &mut |msg| {
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
        let songs_path = root.join(compat.songs_json);
        if let Err(e) = varchive_db.load_from_file(&songs_path) {
            let _ = log_tx.send(format!("[VArchive] songs load failed: {e}"));
        }
        let varchive_db = Arc::new(varchive_db);
        let sheet_meta = Arc::new(PatternSheetMeta::load_cache(
            root.join("cache").join("pattern_meta.json"),
        ));

        let steam0 = {
            let mg = merged_settings
                .lock()
                .map_err(|_| "settings lock poisoned")?;
            let mut sid = first_steam_from_settings(mg.clone());
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
        if merged.get("varchive").and_then(|v| v.get("auto_refresh")).and_then(|v| v.as_bool()).unwrap_or(false) {
            let v_id = merged.get("varchive")
                .and_then(|v| v.get("user_map"))
                .and_then(|m| m.get(&steam0))
                .and_then(|u| {
                    if u.is_object() {
                        u.get("v_id").and_then(|v| v.as_str())
                    } else {
                        u.as_str()
                    }
                })
                .unwrap_or("");
            if !v_id.is_empty() {
                let _ = fetch_req_tx.send((steam0.clone(), v_id.to_string(), 0));
            }
        }

        detection_worker::spawn(
            (*root).clone(),
            merged_settings
                .lock()
                .map_err(|_| "settings lock poisoned")?
                .clone(),
            log_tx.clone(),
            game_found_tx,
            detection_tx,
        );

        let mut filters = std::collections::HashMap::new();
        filters.insert("[ScreenCapture]".to_string(), true);
        filters.insert("[Overlay]".to_string(), true);
        filters.insert("[VArchive]".to_string(), true);
        filters.insert("[WindowTracker]".to_string(), true);
        filters.insert("[Main]".to_string(), true);

        let mut app = Self {
            root,
            defaults,
            base_settings,
            merged_settings,
            settings_draft,
            debug_open: debug_open.clone(),
            settings_open: settings_open.clone(),
            sync_open: sync_open.clone(),
            scan_pending: Arc::new(AtomicBool::new(false)),
            log_lines: Arc::new(Mutex::new(VecDeque::new())),
            log_rx: Some(log_rx),
            debug_paused: Arc::new(AtomicBool::new(false)),
            game_rect: Arc::new(Mutex::new(None)),
            debug_filters: Arc::new(Mutex::new(filters)),
            session: GameSessionState::detecting(),
            confidence: 0.0,
            recorded_states: std::collections::HashSet::new(),
            sync_steam_id: Arc::new(Mutex::new(steam0)),
            sync_status: Arc::new(Mutex::new(String::new())),
            sync_candidates: Arc::new(Mutex::new(Vec::new())),
            sync_rx,
            sync_tx,
            upload_req_rx,
            upload_req_tx,
            upload_res_rx,
            upload_res_tx,
            delete_req_rx,
            delete_req_tx,
            fetch_req_rx,
            fetch_req_tx,
            fetch_res_rx,
            fetch_res_tx,
            detection_rx,
            ui_cmd_rx,
            varchive_db,
            sheet_meta,
            recommendations: RecommendResult::empty(),
            pattern_tabs: Vec::new(),
            prev_settings_open: false,
            prev_scale: 1.0,
            prev_overlay_on: false,
            record_db,
            record_manager,
            game_found_rx,
            exit_requested: exit_requested.clone(),
            #[cfg(target_os = "windows")]
            _tray: Some(TrayIcon::spawn(ui_cmd_tx)),
        };

        app.handle_auto_refresh();
        Ok(app)
    }

    pub(crate) fn poll_delete_requests(&mut self) {
        while let Ok(idx) = self.delete_req_rx.try_recv() {
            let cand = self
                .sync_candidates
                .lock()
                .ok()
                .and_then(|g| g.get(idx).cloned());
            if let Some(c) = cand {
                if self.record_manager.delete(c.song_id, &c.button_mode, &c.difficulty) {
                    debug_ui::push_log(
                        &self.log_lines,
                        self.max_log_lines(),
                        format!("[Sync] 로컬 기록 삭제 완료: {} ({} {})", c.song_name, c.button_mode, c.difficulty),
                    );
                    self.spawn_scan();
                    self.refresh_overlay_data();
                } else {
                    debug_ui::push_log(
                        &self.log_lines,
                        self.max_log_lines(),
                        format!("[Sync] 로컬 기록 삭제 실패: {} ({} {})", c.song_name, c.button_mode, c.difficulty),
                    );
                }
            }
        }
    }

    pub(crate) fn max_log_lines(&self) -> usize {
        let Ok(m) = self.merged_settings.lock() else {
            return 500;
        };
        m.get("debug_window")
            .and_then(|d| d.get("max_lines"))
            .and_then(|v| v.as_u64())
            .unwrap_or(500) as usize
    }

    pub(crate) fn debug_title(&self) -> String {
        let Ok(m) = self.merged_settings.lock() else {
            return "Overmax Debug Log".into();
        };
        m.get("debug_window")
            .and_then(|d| d.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("Overmax Debug Log")
            .to_string()
    }

    pub(crate) fn poll_scan_requests(&mut self) {
        if self.scan_pending.swap(false, Ordering::Relaxed) {
            if let Ok(mut s) = self.sync_status.lock() {
                *s = "스캔 중…".into();
            }
            self.spawn_scan();
        }
    }

    pub(crate) fn poll_upload_requests(&mut self) {
        while let Ok(idx) = self.upload_req_rx.try_recv() {
            let cand = self
                .sync_candidates
                .lock()
                .ok()
                .and_then(|g| g.get(idx).cloned());
            if let Some(c) = cand {
                self.spawn_upload(idx, c);
            }
        }
    }

    pub(crate) fn drain_sync_scan(&self) {
        while let Ok(res) = self.sync_rx.try_recv() {
            match res {
                Ok(list) => {
                    let n = list.len();
                    if let Ok(mut g) = self.sync_candidates.lock() {
                        *g = list;
                    }
                    if let Ok(mut s) = self.sync_status.lock() {
                        *s = format!("후보 {n}건");
                    }
                }
                Err(msg) => {
                    if let Ok(mut s) = self.sync_status.lock() {
                        *s = msg;
                    }
                }
            }
        }
    }

    pub(crate) fn drain_upload_results(&mut self) {
        let mut refreshed = false;
        while let Ok((idx, status, msg)) = self.upload_res_rx.try_recv() {
            let success = status == "success";
            if let Ok(mut list) = self.sync_candidates.lock() {
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

    pub(crate) fn drain_game_found_refresh_steam(&mut self) {
        while self.game_found_rx.try_recv().is_ok() {
            let sid = steam_session::most_recent_steam_id();
            let (changed, before, after) = self.record_manager.set_steam_id(sid.as_deref());
            if changed {
                debug_ui::push_log(
                    &self.log_lines,
                    self.max_log_lines(),
                    format!("[Main] Steam 세션 갱신 (게임 창 발견): {before} -> {after}"),
                );
                self.refresh_overlay_data();
            } else if sid.is_some() {
                debug_ui::push_log(
                    &self.log_lines,
                    self.max_log_lines(),
                    format!("[Main] Steam 세션 유지 (게임 창 발견): {after}"),
                );
            }
        }
    }

    fn spawn_scan(&self) {
        let steam = self
            .sync_steam_id
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let tx = self.sync_tx.clone();
        let root = self.root.clone();
        let rdb = self.record_db.clone();
        std::thread::spawn(move || {
            let compat = DataCompatibility::current();
            let songs_path = root.join(compat.songs_json);
            let mut db = VArchiveDB::new();
            if let Err(e) = db.load_from_file(&songs_path) {
                let _ = tx.send(Err(format!("songs.json: {e}")));
                return;
            }
            let cache_root = root.join("cache").join("varchive");
            let list = build_candidates(&db, rdb.as_ref(), &steam, &cache_root);
            let _ = tx.send(Ok(list));
        });
    }

    fn spawn_upload(&self, index: usize, candidate: SyncCandidate) {
        let merged = match self.merged_settings.lock() {
            Ok(g) => g.clone(),
            Err(_) => return,
        };
        let steam = self
            .sync_steam_id
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let account_path = account_path_for_steam(&merged, &steam);
        let tx = self.upload_res_tx.clone();
        let root = self.root.clone();

        std::thread::spawn(move || {
            let path = Path::new(&account_path);
            if account_path.is_empty() || !path.exists() {
                let _ = tx.send((index, "error".into(), "account.txt 경로 없음".into()));
                return;
            }
            let Some(account) = varchive_upload::parse_account_file(path) else {
                let _ = tx.send((index, "error".into(), "account.txt 파싱 실패".into()));
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
        });
    }

    pub(crate) fn handle_auto_refresh(&mut self) {
        let merged = match self.merged_settings.lock() {
            Ok(g) => g.clone(),
            Err(_) => return,
        };
        let auto_refresh = merged.get("varchive").and_then(|v| v.get("auto_refresh")).and_then(|v| v.as_bool()).unwrap_or(false);
        if !auto_refresh {
            return;
        }

        let sid = steam_session::most_recent_steam_id().unwrap_or_default();
        if sid.is_empty() {
            return;
        }

        let user_map = merged.get("varchive").and_then(|v| v.get("user_map")).and_then(|v| v.as_object());
        let entry = user_map.and_then(|m| m.get(&sid));
        let v_id = match entry {
            Some(Value::Object(o)) => o.get("v_id").and_then(|v| v.as_str()).unwrap_or(""),
            Some(Value::String(s)) => s.as_str(),
            _ => "",
        };

        if !v_id.is_empty() {
            debug_ui::push_log(
                &self.log_lines,
                self.max_log_lines(),
                format!("[VArchive] 자동 갱신 시작 (SteamID: {}, V-ID: {})", sid, v_id),
            );
            let _ = self.fetch_req_tx.send((sid, v_id.to_string(), 0));
        }
    }

    pub(crate) fn poll_fetch_requests(&mut self) {
        while let Ok((steam_id, v_id, button)) = self.fetch_req_rx.try_recv() {
            self.spawn_fetch(steam_id, v_id, button);
        }
    }

    pub(crate) fn drain_fetch_results(&mut self) {
        let mut refreshed = false;
        while let Ok((v_id, btn, res)) = self.fetch_res_rx.try_recv() {
            match res {
                Ok(_) => {
                    refreshed = true;
                }
                Err(e) => {
                    debug_ui::push_log(
                        &self.log_lines,
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

    fn spawn_fetch(&self, steam_id: String, v_id: String, button: i32) {
        let tx = self.fetch_res_tx.clone();
        let cache_root = self.root.join("cache").join("varchive");
        let log_lines = self.log_lines.clone();
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
        });
    }
}
