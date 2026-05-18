//! `--update-worker` child process: wait parent, copy payload, restart app.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use std::os::windows::process::CommandExt;

use super::paths::result_path;
use super::result_io::{write_applied_tag, write_result};

const CREATE_NO_WINDOW: u32 = 0x08000000;
const DETACHED_PROCESS: u32 = 0x00000008;
const WORKER_SKIP_NAME: &str = "overmax_updater_worker.exe";

#[derive(Debug)]
pub struct WorkerArgs {
    pub parent_pid: u32,
    pub app_dir: PathBuf,
    pub payload_dir: PathBuf,
    pub from_version: String,
    pub to_version: String,
}

pub fn parse_worker_args(args: &[String]) -> Option<WorkerArgs> {
    if !args.iter().any(|a| a == "--update-worker") {
        return None;
    }
    let mut parent_pid = None;
    let mut app_dir = None;
    let mut payload_dir = None;
    let mut from_version = None;
    let mut to_version = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--parent-pid" => {
                parent_pid = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
                continue;
            }
            "--app-dir" => {
                app_dir = args.get(i + 1).map(PathBuf::from);
                i += 2;
                continue;
            }
            "--payload-dir" => {
                payload_dir = args.get(i + 1).map(PathBuf::from);
                i += 2;
                continue;
            }
            "--from-version" => {
                from_version = args.get(i + 1).cloned();
                i += 2;
                continue;
            }
            "--to-version" => {
                to_version = args.get(i + 1).cloned();
                i += 2;
                continue;
            }
            _ => i += 1,
        }
    }
    Some(WorkerArgs {
        parent_pid: parent_pid?,
        app_dir: app_dir?,
        payload_dir: payload_dir?,
        from_version: from_version?,
        to_version: to_version?,
    })
}

pub fn spawn_update_worker(
    app_dir: &Path,
    payload_dir: &Path,
    current: &str,
    latest: &str,
) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let from_v = format!("v{current}");
    Command::new(&exe)
        .args([
            "--update-worker",
            "--parent-pid",
            &format!("{}", std::process::id()),
            "--app-dir",
            &app_dir.display().to_string(),
            "--payload-dir",
            &payload_dir.display().to_string(),
            "--from-version",
            &from_v,
            "--to-version",
            latest,
        ])
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .is_ok()
}

pub fn run_update_worker(args: WorkerArgs) -> i32 {
    let result_path = result_path(&args.app_dir);
    let _ = write_result(
        &result_path,
        "started",
        &args.from_version,
        &args.to_version,
        None,
    );
    eprintln!(
        "[AppUpdaterWorker] waiting parent pid {} …",
        args.parent_pid
    );
    if !wait_parent_exit(args.parent_pid, Duration::from_secs(120)) {
        let _ = write_result(
            &result_path,
            "failed",
            &args.from_version,
            &args.to_version,
            Some("wait_timeout"),
        );
        return 1;
    }
    eprintln!("[AppUpdaterWorker] applying payload …");
    if let Err(e) = apply_payload(&args.app_dir, &args.payload_dir) {
        eprintln!("[AppUpdaterWorker] 복사 실패: {e}");
        let _ = write_result(
            &result_path,
            "failed",
            &args.from_version,
            &args.to_version,
            Some("copy_failed"),
        );
        return 1;
    }
    let _ = write_result(
        &result_path,
        "success",
        &args.from_version,
        &args.to_version,
        None,
    );
    let _ = write_applied_tag(&args.app_dir, &args.to_version);
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| args.app_dir.join(n)))
        .unwrap_or_else(|| args.app_dir.join(super::main_exe_name()));
    if !restart_app(&exe) {
        let _ = write_result(
            &result_path,
            "failed",
            &args.from_version,
            &args.to_version,
            Some("restart_failed"),
        );
        return 1;
    }
    0
}

pub fn wait_parent_exit(pid: u32, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !is_process_running(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    false
}

fn is_process_running(pid: u32) -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.contains(&pid.to_string()) && !s.contains("No tasks")
        }
        Err(_) => false,
    }
}

fn apply_payload(app_dir: &Path, payload_dir: &Path) -> io::Result<()> {
    let backup = app_dir
        .join("cache")
        .join("update")
        .join("settings.pre_update.json");
    let source_settings = app_dir.join("settings.json");
    if let Some(p) = backup.parent() {
        fs::create_dir_all(p)?;
    }
    if source_settings.exists() {
        fs::copy(&source_settings, &backup)?;
    }
    copy_tree_skip_worker(payload_dir, app_dir)?;
    if backup.exists() {
        let _ = fs::copy(&backup, &source_settings);
    }
    Ok(())
}

fn copy_tree_skip_worker(src_root: &Path, dst_root: &Path) -> io::Result<()> {
    for entry in walk_recursive(src_root)? {
        let rel = entry
            .strip_prefix(src_root)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        if rel.file_name().and_then(|n| n.to_str()) == Some(WORKER_SKIP_NAME) {
            continue;
        }
        let dst = dst_root.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&dst)?;
            continue;
        }
        if let Some(p) = dst.parent() {
            fs::create_dir_all(p)?;
        }
        fs::copy(&entry, &dst)?;
    }
    Ok(())
}

fn walk_recursive(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir)? {
            let e = e?;
            let p = e.path();
            if p.is_dir() {
                stack.push(p.clone());
            }
            out.push(p);
        }
    }
    Ok(out)
}

fn restart_app(exe: &Path) -> bool {
    Command::new(exe)
        .current_dir(exe.parent().unwrap_or_else(|| Path::new(".")))
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .is_ok()
}
