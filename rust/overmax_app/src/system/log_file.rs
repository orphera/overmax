//! 콘솔이 없는 실행 환경에서 stdout/stderr를 로그 파일로 돌린다.
//!
//! 릴리스 빌드(Windows `windows_subsystem = "windows"`, 데스크톱 엔트리로 실행한 Linux)는
//! 표준 출력이 연결되어 있지 않아 기존 `eprintln!`/`println!` 진단 출력이 모두 버려진다.
//! 프로세스 표준 핸들을 파일로 교체해 기존 호출부를 바꾸지 않고 기록을 남긴다.

use std::fs::{self, File};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

const LOG_DIR: &str = "logs";
const LOG_PREFIX: &str = "overmax-";
const LOG_SUFFIX: &str = ".log";
/// 이번 실행 파일을 포함해 남겨 둘 로그 파일 수
const KEEP_FILES: usize = 10;

/// stderr가 터미널에 연결되어 있지 않으면 `<data_dir>/logs/overmax-<시각>.log`로 리다이렉트한다.
///
/// 터미널에서 실행한 경우(개발 빌드, 터미널 실행)는 아무것도 하지 않고 `None`을 돌려준다.
pub fn redirect_std_streams(data_dir: &Path) -> io::Result<Option<PathBuf>> {
    if io::stderr().is_terminal() {
        return Ok(None);
    }
    let dir = data_dir.join(LOG_DIR);
    fs::create_dir_all(&dir)?;
    prune_old_logs(&dir, KEEP_FILES - 1);

    let path = dir.join(format!("{LOG_PREFIX}{}{LOG_SUFFIX}", file_stamp()));
    let file = File::create(&path)?;
    platform::redirect(file)?;
    eprintln!(
        "overmax {} ({} {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    Ok(Some(path))
}

/// panic 발생 시각·스레드·위치와 backtrace를 stderr(리다이렉트되었다면 로그 파일)에 남긴다.
///
/// `RUST_BACKTRACE` 설정과 무관하게 backtrace를 강제로 수집한다. panic은 드문 경로라 비용은 문제되지 않는다.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::force_capture();
        eprintln!(
            "[PANIC] {} thread '{}' {info}
{backtrace}",
            file_stamp(),
            thread.name().unwrap_or("<unnamed>")
        );
    }));
}

/// 이름순(=시각순)으로 오래된 로그를 지워 `keep`개만 남긴다.
fn prune_old_logs(dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| is_log_file(path))
        .collect();
    if logs.len() <= keep {
        return;
    }
    logs.sort();
    for path in &logs[..logs.len() - keep] {
        let _ = fs::remove_file(path);
    }
}

fn is_log_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(LOG_PREFIX) && name.ends_with(LOG_SUFFIX))
}

fn file_stamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

#[cfg(windows)]
mod platform {
    use std::fs::File;
    use std::io;
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};

    /// Rust std는 쓰기마다 `GetStdHandle`을 조회하므로 표준 핸들 교체만으로 충분하다.
    /// 핸들은 프로세스 종료까지 유지되도록 소유권을 넘긴다.
    pub(super) fn redirect(file: File) -> io::Result<()> {
        let handle = file.into_raw_handle();
        // SAFETY: handle은 방금 연 유효한 파일 핸들이며 프로세스 수명 동안 닫지 않는다.
        unsafe {
            if SetStdHandle(STD_ERROR_HANDLE, handle) == 0
                || SetStdHandle(STD_OUTPUT_HANDLE, handle) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::fs::File;
    use std::io;
    use std::os::fd::AsFd;

    pub(super) fn redirect(file: File) -> io::Result<()> {
        rustix::stdio::dup2_stderr(file.as_fd())?;
        rustix::stdio::dup2_stdout(file.as_fd())?;
        Ok(())
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use std::fs::File;
    use std::io;

    pub(super) fn redirect(_file: File) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_keeps_newest_logs_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        for stamp in ["20261101-000000", "20261102-000000", "20261103-000000"] {
            File::create(temp.path().join(format!("{LOG_PREFIX}{stamp}{LOG_SUFFIX}")))
                .expect("create log");
        }
        File::create(temp.path().join("other.txt")).expect("create other");

        prune_old_logs(temp.path(), 2);

        let mut names: Vec<String> = fs::read_dir(temp.path())
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().into_string().expect("utf8"))
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "other.txt",
                "overmax-20261102-000000.log",
                "overmax-20261103-000000.log"
            ]
        );
    }
}
