//! GitHub API, download, manifest SHA-256, zip extraction.

use std::fs::File;
use std::io::{copy, Cursor, Read};
use std::path::{Path, PathBuf};

use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{ACCEPT, CACHE_CONTROL, PRAGMA, USER_AGENT};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::AppUpdateConfig;

const MANIFEST_NAME: &str = "release_manifest.json";
const GITHUB_USER_AGENT: &str = concat!("overmax-rs/", env!("CARGO_PKG_VERSION"));
const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";
const GITHUB_API_VERSION: &str = "2022-11-28";

pub struct LatestRelease {
    pub tag: Option<String>,
    pub asset_url: Option<String>,
    pub manifest_url: Option<String>,
}

pub fn fetch_latest_release(
    cfg: &AppUpdateConfig,
) -> Result<LatestRelease, Box<dyn std::error::Error + Send + Sync>> {
    let url = cfg.latest_release_url.clone().unwrap_or_else(|| {
        format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            cfg.owner, cfg.repo
        )
    });
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?;
    let resp = github_api_get(&client, &url).send()?;
    let data = release_json(resp, &url)?;
    let tag = data
        .get("tag_name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let Some(ref t) = tag else {
        return Ok(LatestRelease {
            tag: None,
            asset_url: None,
            manifest_url: None,
        });
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
        return Ok(LatestRelease {
            tag: Some(t.clone()),
            asset_url: None,
            manifest_url,
        });
    }
    Ok(LatestRelease {
        tag,
        asset_url,
        manifest_url,
    })
}

fn release_json(
    resp: Response,
    url: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!(
            "GitHub latest release request failed: {status} ({url}){}",
            status_body_suffix(&body)
        )
        .into());
    }
    Ok(resp.json()?)
}

fn status_body_suffix(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return String::new();
    }
    let mut short: String = body.chars().take(300).collect();
    if body.chars().count() > short.chars().count() {
        short.push_str("...");
    }
    format!(": {short}")
}

fn github_api_get(client: &Client, url: &str) -> RequestBuilder {
    no_cache_get(client, url)
        .header(ACCEPT, GITHUB_API_ACCEPT)
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
}

fn no_cache_get(client: &Client, url: &str) -> RequestBuilder {
    client
        .get(url)
        .header(USER_AGENT, GITHUB_USER_AGENT)
        .header(CACHE_CONTROL, "no-cache")
        .header(PRAGMA, "no-cache")
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
    let mut resp = no_cache_get(&client, url).send()?.error_for_status()?;
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
    let data: Value = match no_cache_get(&client, manifest_url).send() {
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

#[cfg(test)]
mod tests {
    use super::{
        github_api_get, no_cache_get, status_body_suffix, GITHUB_API_ACCEPT, GITHUB_API_VERSION,
        GITHUB_USER_AGENT,
    };
    use reqwest::blocking::Client;
    use reqwest::header::{ACCEPT, CACHE_CONTROL, PRAGMA, USER_AGENT};

    #[test]
    fn github_api_request_sets_required_headers() {
        let client = Client::new();
        let request = github_api_get(
            &client,
            "https://api.github.com/repos/orphera/overmax/releases/latest",
        )
        .build()
        .unwrap();
        let headers = request.headers();

        assert_eq!(
            headers.get(USER_AGENT).and_then(|v| v.to_str().ok()),
            Some(GITHUB_USER_AGENT)
        );
        assert_eq!(
            headers.get(ACCEPT).and_then(|v| v.to_str().ok()),
            Some(GITHUB_API_ACCEPT)
        );
        assert_eq!(
            headers
                .get("X-GitHub-Api-Version")
                .and_then(|v| v.to_str().ok()),
            Some(GITHUB_API_VERSION)
        );
    }

    #[test]
    fn download_request_sets_user_agent_and_no_cache() {
        let client = Client::new();
        let request = no_cache_get(
            &client,
            "https://github.com/orphera/overmax/releases/download/v1/overmax.zip",
        )
        .build()
        .unwrap();
        let headers = request.headers();

        assert_eq!(
            headers.get(USER_AGENT).and_then(|v| v.to_str().ok()),
            Some(GITHUB_USER_AGENT)
        );
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-cache")
        );
        assert_eq!(
            headers.get(PRAGMA).and_then(|v| v.to_str().ok()),
            Some("no-cache")
        );
    }

    #[test]
    fn status_body_suffix_trims_long_body() {
        let suffix = status_body_suffix(&"x".repeat(400));

        assert!(suffix.starts_with(": "));
        assert!(suffix.ends_with("..."));
        assert!(suffix.len() < 310);
    }
}
