//! Single `eframe` app: overlay + deferred debug / settings / sync viewports.

use eframe::egui::{self, Color32, ViewportBuilder};
use overmax_core::GameSessionState;
use overmax_data::{
    build_candidates, load_base_settings, load_merged_settings, normalize_settings, upsert_varchive_cache_record,
    DataCompatibility, RecordDB, SyncCandidate, VArchiveDB,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::debug_ui;
use crate::global_hotkey::GlobalHotkey;
use crate::native_helpers::{account_path_for_steam, button_num, first_steam_from_settings, toggle_hotkey_from_settings};
use crate::overlay_ui;
use crate::probe_worker;
use crate::single_instance::SingleInstanceGuard;
use crate::steam_session;
#[cfg(target_os = "windows")]
use crate::tray_icon::TrayIcon;
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

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Overmax")
            .with_inner_size([overlay_ui::WIDTH, overlay_ui::HEIGHT])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        "Overmax",
        options,
        Box::new(|cc| {
            overlay_ui::install_korean_font(&cc.egui_ctx);
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
    pub(crate) log_rx: Receiver<String>,
    pub(crate) session: GameSessionState,
    pub(crate) confidence: f32,
    pub(crate) sync_steam_id: Arc<Mutex<String>>,
    pub(crate) sync_status: Arc<Mutex<String>>,
    pub(crate) sync_candidates: Arc<Mutex<Vec<SyncCandidate>>>,
    pub(crate) sync_rx: Receiver<Result<Vec<SyncCandidate>, String>>,
    pub(crate) sync_tx: Sender<Result<Vec<SyncCandidate>, String>>,
    pub(crate) upload_req_rx: Receiver<usize>,
    pub(crate) upload_req_tx: Sender<usize>,
    pub(crate) upload_res_rx: Receiver<(usize, String, String)>,
    pub(crate) upload_res_tx: Sender<(usize, String, String)>,
    pub(crate) prev_settings_open: bool,
    pub(crate) record_db: Arc<RecordDB>,
    pub(crate) game_found_rx: Receiver<()>,
    pub(crate) overlay_visible: Arc<AtomicBool>,
    pub(crate) exit_requested: Arc<AtomicBool>,
    pub(crate) _hotkey: Option<GlobalHotkey>,
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

        let base_settings = Arc::new(Mutex::new(load_base_settings(root.as_ref(), (*defaults).clone())));
        let mut merged = load_merged_settings(root.as_ref(), (*defaults).clone());
        normalize_settings(&mut merged);
        let merged_settings = Arc::new(Mutex::new(merged.clone()));
        let settings_draft = Arc::new(Mutex::new(merged));

        let (log_tx, log_rx) = mpsc::channel();
        let (game_found_tx, game_found_rx) = mpsc::channel();
        probe_worker::spawn((*root).clone(), log_tx.clone(), game_found_tx);

        let compat = DataCompatibility::current();
        let recent_steam = steam_session::most_recent_steam_id();
        let mut record_db = RecordDB::new(root.join(compat.record_db), recent_steam.as_deref());
        record_db.initialize();
        let record_db = Arc::new(record_db);

        let mut varchive_db = VArchiveDB::new();
        let songs_path = root.join(compat.songs_json);
        if let Err(e) = varchive_db.load_from_file(&songs_path) {
            let _ = log_tx.send(format!("[VArchive] songs load failed: {e}"));
        }

        let steam0 = {
            let mg = merged_settings.lock().map_err(|_| "settings lock poisoned")?;
            let mut sid = first_steam_from_settings(mg.clone());
            if sid.is_empty() {
                sid = recent_steam.unwrap_or_default();
            }
            sid
        };

        let overlay_visible = Arc::new(AtomicBool::new(true));
        let exit_requested = Arc::new(AtomicBool::new(false));
        let settings_open = Arc::new(AtomicBool::new(false));
        let sync_open = Arc::new(AtomicBool::new(false));
        let debug_open = Arc::new(AtomicBool::new(false));
        let hk_key = {
            let mg = merged_settings.lock().map_err(|_| "settings lock poisoned")?;
            toggle_hotkey_from_settings(&mg)
        };
        let _hotkey = GlobalHotkey::spawn_toggle(&hk_key, overlay_visible.clone());

        let (sync_tx, sync_rx) = mpsc::channel();
        let (upload_req_tx, upload_req_rx) = mpsc::channel();
        let (upload_res_tx, upload_res_rx) = mpsc::channel();

        Ok(Self {
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
            log_rx,
            session: GameSessionState::detecting(),
            confidence: 0.0,
            sync_steam_id: Arc::new(Mutex::new(steam0)),
            sync_status: Arc::new(Mutex::new(String::new())),
            sync_candidates: Arc::new(Mutex::new(Vec::new())),
            sync_rx,
            sync_tx,
            upload_req_rx,
            upload_req_tx,
            upload_res_rx,
            upload_res_tx,
            prev_settings_open: false,
            record_db,
            game_found_rx,
            overlay_visible: overlay_visible.clone(),
            exit_requested: exit_requested.clone(),
            _hotkey,
            #[cfg(target_os = "windows")]
            _tray: Some(TrayIcon::spawn(
                overlay_visible,
                settings_open,
                sync_open,
                debug_open,
                exit_requested,
            )),
        })
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

    pub(crate) fn apply_overlay_visual(&self, ctx: &egui::Context) {
        let Ok(merged) = self.merged_settings.lock() else {
            return;
        };
        let opacity = merged
            .get("overlay")
            .and_then(|o| o.get("base_opacity"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8) as f32;
        let scale = merged
            .get("overlay")
            .and_then(|o| o.get("scale"))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32;
        ctx.set_pixels_per_point(scale);
        ctx.style_mut(|s| {
            s.visuals.widgets.noninteractive.bg_fill =
                Color32::from_rgba_unmultiplied(18, 24, 38, (255.0 * opacity.clamp(0.1, 1.0)) as u8);
        });
    }

    pub(crate) fn drain_logs(&self) {
        let max = self.max_log_lines();
        debug_ui::drain_channel(&self.log_lines, &self.log_rx, max);
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

    pub(crate) fn drain_upload_results(&self) {
        while let Ok((idx, status, msg)) = self.upload_res_rx.try_recv() {
            if let Ok(mut list) = self.sync_candidates.lock() {
                if let Some(c) = list.get_mut(idx) {
                    c.upload_status = status;
                    c.upload_message = msg;
                }
            }
        }
    }

    pub(crate) fn drain_game_found_refresh_steam(&self) {
        while self.game_found_rx.try_recv().is_ok() {
            let sid = steam_session::most_recent_steam_id();
            let (changed, before, after) = self.record_db.set_steam_id(sid.as_deref());
            if changed {
                debug_ui::push_log(
                    &self.log_lines,
                    self.max_log_lines(),
                    format!("[Main] Steam 세션 갱신 (게임 창 발견): {before} -> {after}"),
                );
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
        let steam = self.sync_steam_id.lock().map(|g| g.clone()).unwrap_or_default();
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
        let steam = self.sync_steam_id.lock().map(|g| g.clone()).unwrap_or_default();
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
                    let _ = tx.send((index, "success".into(), format!("업로드 OK, 캐시 갱신 실패: {e}")));
                } else {
                    let _ = tx.send((index, "success".into(), "등록 완료".into()));
                }
            } else {
                let _ = tx.send((index, "error".into(), res.message));
            }
        });
    }
}
