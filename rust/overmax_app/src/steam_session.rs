//! Read most-recent Steam login id from `loginusers.vdf` (same logic as Python `steam_session.py`).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

pub fn most_recent_steam_id() -> Option<String> {
    let vdf = read_loginusers_vdf()?;
    let data = parse_vdf(&vdf);
    let users = match data.get("users")? {
        VdfVal::Obj(m) => m,
        _ => return None,
    };
    for (steam_id, user_data) in users {
        let attrs = match user_data {
            VdfVal::Obj(m) => m,
            _ => continue,
        };
        if let Some(VdfVal::Str(s)) = attrs.get("MostRecent") {
            if s == "1" {
                return Some(steam_id.clone());
            }
        }
    }
    None
}

fn read_loginusers_vdf() -> Option<String> {
    let steam = find_steam_path()?;
    let path = Path::new(&steam).join("config").join("loginusers.vdf");
    fs::read_to_string(path).ok()
}

fn find_steam_path() -> Option<String> {
    for path in [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"] {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(r"Software\Valve\Steam").ok()?;
    let steam_path: String = key.get_value("SteamPath").ok()?;
    let trimmed = steam_path
        .trim()
        .trim_end_matches('/')
        .trim_end_matches('\\');
    if !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }
    None
}

#[derive(Debug, Clone)]
enum VdfVal {
    Str(String),
    Obj(HashMap<String, VdfVal>),
}

/// Minimal VDF parser (mirrors Python `parse_vdf` in `steam_session.py`).
fn parse_vdf(content: &str) -> HashMap<String, VdfVal> {
    let mut map_stack: Vec<HashMap<String, VdfVal>> = vec![HashMap::new()];
    let mut key_stack: Vec<String> = Vec::new();
    let mut pending_key: Option<String> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        if line == "{" {
            if let Some(k) = pending_key.take() {
                key_stack.push(k);
                map_stack.push(HashMap::new());
            }
            continue;
        }
        if line == "}" {
            if map_stack.len() > 1 {
                if let (Some(done), Some(key)) = (map_stack.pop(), key_stack.pop()) {
                    if let Some(parent) = map_stack.last_mut() {
                        parent.insert(key, VdfVal::Obj(done));
                    }
                }
            }
            continue;
        }
        if let Some((k, v)) = parse_quoted_key_value_line(line) {
            if let Some(parent) = map_stack.last_mut() {
                parent.insert(k, VdfVal::Str(v));
            }
            pending_key = None;
        } else if let Some(k) = parse_quoted_key_open_brace(line) {
            key_stack.push(k);
            map_stack.push(HashMap::new());
            pending_key = None;
        } else if let Some(k) = parse_quoted_key_only_line(line) {
            pending_key = Some(k);
        }
    }
    map_stack.into_iter().next().unwrap_or_default()
}

fn parse_quoted_key_value_line(line: &str) -> Option<(String, String)> {
    let s = line.trim();
    let rest = s.strip_prefix('"')?;
    let (key, rest) = take_until_quote(rest)?;
    let rest = rest.trim();
    let rest = rest.strip_prefix('"')?;
    let (value, tail) = take_until_quote(rest)?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some((key, value))
}

fn parse_quoted_key_only_line(line: &str) -> Option<String> {
    let s = line.trim();
    if s.contains('{') {
        return None;
    }
    let rest = s.strip_prefix('"')?;
    let (key, tail) = take_until_quote(rest)?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some(key)
}

fn parse_quoted_key_open_brace(line: &str) -> Option<String> {
    let s = line.trim();
    if !s.contains('{') {
        return None;
    }
    let before = s.split('{').next()?.trim();
    let rest = before.strip_prefix('"')?;
    let (key, tail) = take_until_quote(rest)?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some(key)
}

fn take_until_quote(s: &str) -> Option<(String, &str)> {
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'"' {
            return Some((s[..i].to_string(), &s[i + 1..]));
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{parse_vdf, VdfVal};

    #[test]
    fn parse_sample_loginusers() {
        let sample = r#""users"
{
    "76561198000000001"
    {
        "AccountName" "test"
        "MostRecent" "0"
    }
    "76561198000000002"
    {
        "AccountName" "main"
        "MostRecent" "1"
    }
}
"#;
        let m = parse_vdf(sample);
        let users = match m.get("users") {
            Some(VdfVal::Obj(u)) => u,
            _ => panic!("users"),
        };
        let u2 = match users.get("76561198000000002") {
            Some(VdfVal::Obj(b)) => b,
            _ => panic!("user2"),
        };
        assert!(matches!(
            u2.get("MostRecent"),
            Some(VdfVal::Str(s)) if s == "1"
        ));
    }
}
