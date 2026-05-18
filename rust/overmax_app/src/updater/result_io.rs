//! `update_result.txt` key=value format + cleanup.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::paths::{result_path, update_root};

pub fn peek_result(app_dir: &Path) -> Option<HashMap<String, String>> {
    read_result_file(&result_path(app_dir))
}

pub fn consume_result(app_dir: &Path) -> Option<HashMap<String, String>> {
    let p = result_path(app_dir);
    let m = read_result_file(&p)?;
    if m.get("status").map(|s| s.as_str()) != Some("started") {
        let _ = fs::remove_file(&p);
    }
    Some(m)
}

pub fn cleanup_artifacts(app_dir: &Path) {
    let stage = update_root(app_dir).join("stage");
    if stage.exists() {
        let _ = fs::remove_dir_all(&stage);
    }
}

pub fn write_applied_tag(app_dir: &Path, tag: &str) -> std::io::Result<()> {
    let p = super::paths::applied_tag_path(app_dir);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(p, tag.trim())
}

pub fn write_result(
    path: &Path,
    status: &str,
    from_ver: &str,
    to_ver: &str,
    reason: Option<&str>,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut body = format!("status={status}\nfrom={from_ver}\nto={to_ver}\n");
    if let Some(r) = reason {
        body.push_str(&format!("reason={r}\n"));
    }
    fs::write(path, body)
}

fn read_result_file(path: &Path) -> Option<HashMap<String, String>> {
    let f = fs::File::open(path).ok()?;
    let mut m = HashMap::new();
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() || !line.contains('=') {
            continue;
        }
        let (k, v) = line.split_once('=')?;
        m.insert(k.trim().to_string(), v.trim().to_string());
    }
    if m.is_empty() {
        None
    } else {
        Some(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_result_file() {
        let dir = std::env::temp_dir().join("overmax_updater_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("update_result.txt");
        write_result(&p, "success", "v0.1.0", "v0.1.1", None).unwrap();
        let m = read_result_file(&p).unwrap();
        assert_eq!(m.get("status").map(String::as_str), Some("success"));
        assert_eq!(m.get("to").map(String::as_str), Some("v0.1.1"));
    }
}
