use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternSheetMetaItem {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gold: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub assist_key: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub keypart: bool,
}

fn is_false(val: &bool) -> bool {
    !*val
}

#[derive(Clone, Debug, Default)]
pub struct PatternSheetMeta {
    items: HashMap<String, PatternSheetMetaItem>,
}

impl PatternSheetMeta {
    pub fn load_cache(path: impl AsRef<Path>, varchive_db: &crate::varchive::VArchiveDB) -> Self {
        let path_ref = path.as_ref();
        let Ok(text) = std::fs::read_to_string(path_ref) else {
            return Self::default();
        };
        let raw_items: HashMap<String, PatternSheetMetaItem> = serde_json::from_str(&text).unwrap_or_default();
        let mut items = HashMap::new();
        let mut dirty = false;

        for (key, item) in raw_items {
            let parts: Vec<&str> = key.split('|').collect();
            if parts.len() == 3 {
                let first = parts[0].to_lowercase();
                if first == "4b" || first == "5b" || first == "6b" || first == "8b" {
                    // Old format: mode|title|diff
                    let mode = parts[0].to_uppercase();
                    let title = parts[1];
                    let diff = parts[2].to_uppercase();
                    
                    if let Some(song) = varchive_db.find_best_match(title, &mode, &diff, None, "", &item.note) {
                        let new_key = format!("{}|{}|{}", song.title, mode, normalize(&diff));
                        items.insert(new_key, item);
                        dirty = true;
                    } else {
                        items.insert(key, item);
                    }
                } else {
                    // New format: song_id|mode|diff
                    items.insert(key, item);
                }
            } else {
                items.insert(key, item);
            }
        }

        if dirty {
            if let Ok(serialized) = serde_json::to_string_pretty(&items) {
                let _ = std::fs::write(path_ref, serialized);
            }
        }

        Self { items }
    }

    pub fn get(&self, song_id: &str, mode: &str, diff: &str) -> PatternSheetMetaItem {
        let key = format!("{}|{}|{}", song_id, mode, normalize(diff));
        self.items.get(&key).cloned().unwrap_or_default()
    }
}

fn normalize(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_uses_new_sheet_meta_key_shape() {
        let mut items = HashMap::new();
        items.insert(
            "123|5B|sc".into(),
            PatternSheetMetaItem {
                gold: "O".into(),
                note: "개인차".into(),
                assist_key: "Y".into(),
                keypart: false,
            },
        );
        let meta = PatternSheetMeta { items };

        assert_eq!(
            meta.get("123", "5B", "SC"),
            PatternSheetMetaItem {
                gold: "O".into(),
                note: "개인차".into(),
                assist_key: "Y".into(),
                keypart: false,
            }
        );
    }
}
