use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternSheetMetaItem {
    #[serde(default)]
    pub gold: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub assist_key: String,
}

#[derive(Clone, Debug, Default)]
pub struct PatternSheetMeta {
    items: HashMap<String, PatternSheetMetaItem>,
}

impl PatternSheetMeta {
    pub fn load_cache(path: impl AsRef<Path>) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let items = serde_json::from_str(&text).unwrap_or_default();
        Self { items }
    }

    pub fn get(&self, song_name: &str, mode: &str, diff: &str) -> PatternSheetMetaItem {
        let key = format!("{}|{}|{}", mode, normalize(song_name), normalize(diff));
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
    fn lookup_uses_python_sheet_meta_key_shape() {
        let mut items = HashMap::new();
        items.insert(
            "5B|love☆panic|sc".into(),
            PatternSheetMetaItem {
                gold: "O".into(),
                note: "개인차".into(),
                assist_key: "Y".into(),
            },
        );
        let meta = PatternSheetMeta { items };

        assert_eq!(
            meta.get(" Love ☆ Panic ", "5B", "SC"),
            PatternSheetMetaItem {
                gold: "O".into(),
                note: "개인차".into(),
                assist_key: "Y".into(),
            }
        );
    }
}
