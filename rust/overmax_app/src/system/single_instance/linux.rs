use std::fs::{File, OpenOptions};
use std::path::Path;

const LOCK_FILE: &str = "overmax.lock";

pub struct SingleInstanceGuard {
    _file: File,
}

impl SingleInstanceGuard {
    pub fn try_acquire() -> Option<Self> {
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")?;
        if runtime_dir.is_empty() {
            return None;
        }
        Self::try_acquire_at(Path::new(&runtime_dir).join(LOCK_FILE))
    }

    fn try_acquire_at(path: impl AsRef<Path>) -> Option<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .ok()?;
        file.try_lock().ok()?;
        Some(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::SingleInstanceGuard;

    #[test]
    fn lock_is_exclusive_and_released_with_guard() {
        let path = std::env::temp_dir().join(format!(
            "overmax-single-instance-test-{}.lock",
            std::process::id()
        ));
        let first = SingleInstanceGuard::try_acquire_at(&path).expect("acquire first lock");
        assert!(SingleInstanceGuard::try_acquire_at(&path).is_none());
        drop(first);
        assert!(SingleInstanceGuard::try_acquire_at(&path).is_some());
        let _ = std::fs::remove_file(path);
    }
}
