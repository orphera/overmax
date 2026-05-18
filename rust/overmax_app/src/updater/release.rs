//! GitHub API, download, manifest SHA-256, zip extraction.

use std::fs::File;
use std::io::{copy, Cursor, Read};
use std::path::{Path, PathBuf};

use reqwest::blocking::Client;
use reqwest::header::{CACHE_CONTROL, PRAGMA};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::AppUpdateConfig;

const MANIFEST_NAME: &str = "release_manifest.json";

pub fn fetch_latest_release(
    cfg: &AppUpdateConfig,
) -> Result<
    (Option<String>, Option<String>, Option<String>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let url = cfg.latest_release_url.clone().unwrap_or_else(|| {
        format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            cfg.owner, cfg.repo
        )
    });
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?;
    let resp = client
        .get(&url)
        .header(CACHE_CONTROL, "no-cache")
        .header(PRAGMA, "no-cache")
        .send()?;
    resp.error_for_status_ref()?;
    let data: Value = resp.json()?;
    let tag = data
        .get("tag_name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let Some(ref t) = tag else {
        return Ok((None, None, None));
    };
    let mut asset_url = None;
    let mut manifest_url = None;
    if let Some(assets) = data.get("assets").and_then(|a| a.as_array()) {
        for a in assets {
            let name = a.get("name").and_then(|n| n.as_str());
            let url = a.get("browser_download_url").and_then(|u| u.as_str());
            match name {
                Some(n) if n == cfg.asset_name.as_str() => asset_url = url.map(String::from),
                Some(n) if n == MANIFEST_NAME => manifest_url = url.map(String::from),
                _ => {}
            }
        }
    }
    if asset_url.is_none() {
        eprintln!(
            "[AppUpdater] 릴리즈에서 '{}' asset을 찾을 수 없음",
            cfg.asset_name
        );
        return Ok((Some(t.clone()), None, manifest_url));
    }
    Ok((tag, asset_url, manifest_url))
}

pub fn download_and_verify(
    asset_url: &str,
    manifest_url: Option<&str>,
    asset_name: &str,
    zip_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    download_file(asset_url, zip_path)?;
    if let Some(murl) = manifest_url {
        verify_manifest(murl, asset_name, zip_path)?;
    }
    Ok(())
}

fn download_file(url: &str, dest: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let mut resp = client
        .get(url)
        .header(CACHE_CONTROL, "no-cache")
        .header(PRAGMA, "no-cache")
        .send()?
        .error_for_status()?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("tmp");
    let mut f = File::create(&tmp)?;
    copy(&mut resp, &mut f)?;
    drop(f);
    std::fs::rename(&tmp, dest)?;
    eprintln!(
        "[AppUpdater] 다운로드 완료: {}",
        dest.file_name().unwrap_or_default().to_string_lossy()
    );
    Ok(())
}

fn verify_manifest(
    manifest_url: &str,
    asset_name: &str,
    zip_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?;
    let data: Value = match client
        .get(manifest_url)
        .header(CACHE_CONTROL, "no-cache")
        .header(PRAGMA, "no-cache")
        .send()
    {
        Ok(r) if r.status().is_success() => r.json().unwrap_or(Value::Null),
        Ok(_) | Err(_) => {
            eprintln!("[AppUpdater] 매니페스트 조회 실패 (검증 생략)");
            return Ok(());
        }
    };
    let Some(expected) = extract_expected_sha256(&data, asset_name) else {
        return Ok(());
    };
    let actual = sha256_file(zip_path)?;
    if actual != expected {
        return Err("release_manifest.json sha256 불일치".into());
    }
    eprintln!("[AppUpdater] 해시 검증 완료");
    Ok(())
}

fn extract_expected_sha256(manifest: &Value, asset_name: &str) -> Option<String> {
    let assets = manifest.get("assets")?.as_array()?;
    for a in assets {
        let name = a.get("name")?.as_str()?;
        if name != asset_name {
            continue;
        }
        let sha = a.get("sha256")?.as_str()?;
        if !sha.is_empty() {
            return Some(sha.to_ascii_lowercase());
        }
    }
    None
}

fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

pub fn extract_zip(
    zip_path: &Path,
    stage_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if stage_dir.exists() {
        std::fs::remove_dir_all(stage_dir)?;
    }
    std::fs::create_dir_all(stage_dir)?;
    let data = std::fs::read(zip_path)?;
    let mut archive = ZipArchive::new(Cursor::new(data))?;
    archive.extract(stage_dir)?;
    Ok(())
}

pub fn resolve_payload_dir(stage_dir: &Path) -> Option<PathBuf> {
    let names = ["overmax-rs.exe", "overmax.exe"];
    for exe in names {
        if stage_dir.join(exe).is_file() {
            return Some(stage_dir.to_path_buf());
        }
    }
    for child in std::fs::read_dir(stage_dir).ok()? {
        let child = child.ok()?;
        if !child.file_type().ok()?.is_dir() {
            continue;
        }
        let p = child.path();
        for exe in names {
            if p.join(exe).is_file() {
                return Some(p);
            }
        }
        let nested = p.join("overmax");
        for exe in names {
            if nested.join(exe).is_file() {
                return Some(nested);
            }
        }
    }
    None
}
