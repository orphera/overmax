use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use strsim::normalized_damerau_levenshtein;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternInfo {
    pub level: Option<u32>,
    pub floor: Option<f64>,
    #[serde(rename = "floorName")]
    pub floor_name: Option<String>,
    pub rating: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dlc {
    #[serde(rename = "dlcCode")]
    pub dlc_code: String,
    #[serde(rename = "dlcName")]
    pub dlc_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    #[serde(deserialize_with = "deserialize_string_id")]
    pub title: String, // Actually song_id
    pub name: String,
    pub composer: String,
    #[serde(default, rename = "dlcCode")]
    pub dlc_code: String,
    #[serde(default)]
    pub patterns: HashMap<String, HashMap<String, PatternInfo>>,
}

pub struct VArchiveDB {
    pub songs: Vec<Song>,
    pub dlcs: Vec<Dlc>,
    title_map: HashMap<String, Vec<Song>>,
}

impl Default for VArchiveDB {
    fn default() -> Self {
        Self::new()
    }
}

impl VArchiveDB {
    pub fn new() -> Self {
        Self {
            songs: Vec::new(),
            dlcs: Vec::new(),
            title_map: HashMap::new(),
        }
    }

    pub fn load_from_file(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        self.songs = serde_json::from_str(&content)?;
        self.build_index();
        Ok(())
    }

    pub fn load_dlcs_from_file(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        self.dlcs = serde_json::from_str(&content)?;
        Ok(())
    }

    fn build_index(&mut self) {
        self.title_map.clear();
        for song in &self.songs {
            let key = song.name.to_lowercase().trim().to_string();
            self.title_map.entry(key).or_default().push(song.clone());
        }
    }

    fn normalize_text(value: &str) -> String {
        value
            .to_lowercase()
            .replace("腦", "脳")
            .replace("擊", "撃")
            .replace(char::is_whitespace, "")
    }

    fn pick_by_composer(&self, songs: &[Song], composer: &str) -> Option<Song> {
        if songs.is_empty() {
            return None;
        }
        if composer.is_empty() {
            return Some(songs[0].clone());
        }

        let query = Self::normalize_text(composer);
        if query.is_empty() {
            return Some(songs[0].clone());
        }

        let mut best_song = None;
        let mut best_score = -1.0;

        for song in songs {
            let comp_norm = Self::normalize_text(&song.composer);
            let score: f64;

            if query == comp_norm {
                score = 200.0;
            } else if !query.is_empty()
                && !comp_norm.is_empty()
                && (comp_norm.contains(&query) || query.contains(&comp_norm))
            {
                score = 150.0;
            } else {
                score = normalized_damerau_levenshtein(&query, &comp_norm) * 100.0;
            }

            if score > best_score {
                best_score = score;
                best_song = Some(song.clone());
            }
        }

        best_song.or_else(|| Some(songs[0].clone()))
    }

    pub fn find_exact(&self, title: &str, composer: &str) -> Option<Song> {
        let key = title.to_lowercase().trim().to_string();
        if let Some(songs) = self.title_map.get(&key) {
            self.pick_by_composer(songs, composer)
        } else {
            None
        }
    }

    pub fn find_fuzzy(&self, title: &str, composer: &str, threshold: u32) -> Option<Song> {
        if self.title_map.is_empty() {
            return None;
        }

        let query = title.to_lowercase().trim().to_string();
        let threshold_f64 = threshold as f64 / 100.0;

        let mut best_match: Option<String> = None;
        let mut best_score = 0.0;

        for candidate in self.title_map.keys() {
            let score = normalized_damerau_levenshtein(&query, candidate);
            if score >= threshold_f64 && score > best_score {
                best_score = score;
                best_match = Some(candidate.clone());
            }
        }

        if let Some(matched_key) = best_match {
            if let Some(songs) = self.title_map.get(&matched_key) {
                return self.pick_by_composer(songs, composer);
            }
        }

        None
    }

    pub fn search_by_id(&self, song_id: i32) -> Option<Song> {
        let song_id_str = song_id.to_string();
        self.songs.iter().find(|s| s.title == song_id_str).cloned()
    }

    pub fn search(&self, title: &str, composer: &str, threshold: u32) -> Option<Song> {
        self.find_exact(title, composer)
            .or_else(|| self.find_fuzzy(title, composer, threshold))
    }

    pub fn find_best_match(
        &self,
        title: &str,
        mode: &str,
        diff: &str,
        level: Option<u32>,
        category: &str,
        note: &str,
    ) -> Option<Song> {
        if self.songs.is_empty() {
            return None;
        }

        // Try matching full title first
        if let Some(song) = self.find_best_match_internal(title, mode, diff, level, category, note) {
            return Some(song);
        }

        // For DPC patterns or composite titles, try matching the part before '/'
        if title.contains('/') {
            if let Some(first_part) = title.split('/').next() {
                if let Some(song) = self.find_best_match_internal(first_part, mode, diff, level, category, note) {
                    return Some(song);
                }
            }
        }

        None
    }

    fn find_best_match_internal(
        &self,
        title: &str,
        mode: &str,
        diff: &str,
        level: Option<u32>,
        category: &str,
        note: &str,
    ) -> Option<Song> {
        let query_norm = Self::normalize_text(title);
        if query_norm.is_empty() {
            return None;
        }

        let mut best_song = None;
        let mut best_score = -1000.0;

        for song in &self.songs {
            let song_name_norm = Self::normalize_text(&song.name);
            let mut score = 0.0;

            // 1. Title match
            if query_norm == song_name_norm {
                score += 100.0;
            } else if query_norm.starts_with(&song_name_norm) || song_name_norm.starts_with(&query_norm) {
                if query_norm.len() >= 5 && song_name_norm.len() >= 5 {
                    score += 80.0;
                } else {
                    continue;
                }
            } else {
                let dist = normalized_damerau_levenshtein(&query_norm, &song_name_norm);
                if dist >= 0.8 {
                    score += 50.0;
                } else {
                    continue; // Skip if title is too different
                }
            }

            // 2. Pattern (mode, diff) & Level check
            if let Some(modes) = song.patterns.get(mode) {
                if let Some(p_info) = modes.get(diff) {
                    score += 50.0;
                    if let Some(target_lvl) = level {
                        if p_info.level == Some(target_lvl) {
                            score += 100.0;
                        } else {
                            score -= 50.0; // Level mismatch penalty
                        }
                    }
                }
            }

            // 3. Category / DLC match
            if category_matches_dlc(category, &song.dlc_code, &self.dlcs) {
                score += 80.0;
            }

            // 4. Composer in note check
            let note_lower = note.to_lowercase();
            let comp_lower = song.composer.to_lowercase();
            if !note_lower.is_empty() && !comp_lower.is_empty() {
                if note_lower.contains(&comp_lower) || comp_lower.contains(&note_lower) {
                    score += 150.0;
                }
            }

            if score > best_score {
                best_score = score;
                best_song = Some(song.clone());
            }
        }

        best_song
    }
}

fn category_matches_dlc(category: &str, dlc_code: &str, dlcs: &[Dlc]) -> bool {
    let cat = category.to_lowercase().replace(char::is_whitespace, "");
    let dlc = dlc_code.to_lowercase().replace(char::is_whitespace, "");

    if cat.contains(&dlc) || dlc.contains(&cat) {
        return true;
    }

    // Try matching using dlcs.json dynamic mapping
    for d in dlcs {
        if d.dlc_code.to_lowercase() == dlc {
            let name_norm = d.dlc_name.to_lowercase().replace(char::is_whitespace, "");
            if name_norm == cat || cat.contains(&name_norm) || name_norm.contains(&cat) {
                return true;
            }
        }
    }

    match cat.as_str() {
        "respect/v" | "respect" => dlc == "rv" || dlc == "r",
        "emotional.s" | "emotionals." => dlc == "es",
        "vextension1" | "vextension" => dlc == "ve",
        "trilogy" => dlc == "tr",
        "blacksquare" => dlc == "bs",
        "clazziquai" => dlc == "ce",
        "technika3" => dlc == "t3",
        "technika2" => dlc == "t2",
        "technika1" | "technika" => dlc == "t1",
        "portable3" | "pli3" => dlc == "pli3" || dlc == "p3",
        "portable2" | "pli" | "pli(상)" | "pli(下)" => dlc == "pli2" || dlc == "p2",
        "portable1" | "pli1" => dlc == "pli1" || dlc == "p1",
        "vextension2" => dlc == "ve2",
        "vextension3" => dlc == "ve3",
        "ez2on" => dlc == "ez2",
        "vextension4" => dlc == "ve4",
        "vextension5" => dlc == "ve5",
        "vliberty" => dlc == "vl",
        "vliberty2" => dlc == "vl2",
        "vliberty3" => dlc == "vl3",
        "vliberty4" => dlc == "vl4",
        _ => false,
    }
}

fn deserialize_string_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(value) => Ok(value),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        _ => Err(serde::de::Error::custom(
            "song title must be string or number",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_song(title: &str, name: &str, composer: &str) -> Song {
        Song {
            title: title.to_string(),
            name: name.to_string(),
            composer: composer.to_string(),
            dlc_code: String::new(),
            patterns: HashMap::new(),
        }
    }

    #[test]
    fn exact_match_finds_song() {
        let mut db = VArchiveDB::new();
        db.songs.push(create_mock_song("1", "Kamui", "tj.hangneil"));
        db.build_index();

        let song = db.find_exact("kamui", "");
        assert!(song.is_some());
        assert_eq!(song.unwrap().name, "Kamui");
    }

    #[test]
    fn fuzzy_match_finds_typo() {
        let mut db = VArchiveDB::new();
        db.songs.push(create_mock_song("1", "OBLIVION", "ESTi"));
        db.build_index();

        // typo "oblvion"
        let song = db.find_fuzzy("oblvion", "", 80);
        assert!(song.is_some());
        assert_eq!(song.unwrap().name, "OBLIVION");
    }

    #[test]
    fn composer_disambiguation() {
        let mut db = VArchiveDB::new();
        db.songs.push(create_mock_song(
            "1",
            "End of the Moonlight",
            "Forte Escape",
        ));
        db.songs
            .push(create_mock_song("2", "End of the Moonlight", "BEXTER"));
        db.build_index();

        let song1 = db.find_exact("End of the Moonlight", "bexter");
        assert_eq!(song1.unwrap().title, "2");

        let song2 = db.find_exact("End of the Moonlight", "forte");
        assert_eq!(song2.unwrap().title, "1");
    }

    #[test]
    fn parses_numeric_song_id_title_from_cache_json() {
        let song: Song = serde_json::from_str(
            r#"{
                "title": 0,
                "name": "비상 ~Stay With Me~",
                "composer": "Mycin.T",
                "patterns": {}
            }"#,
        )
        .unwrap();

        assert_eq!(song.title, "0");
        assert_eq!(song.name, "비상 ~Stay With Me~");
    }

    #[test]
    fn test_find_best_match_disambiguation() {
        let mut db = VArchiveDB::new();
        
        // Setup mock songs for Alone duplicates
        let mut s1 = create_mock_song("2", "Alone", "Marshmello");
        s1.dlc_code = "RV".into();
        s1.patterns.insert("5B".into(), [
            ("SC".into(), PatternInfo { level: Some(5), floor: None, floor_name: None, rating: None })
        ].into_iter().collect());
        
        let mut s2 = create_mock_song("441", "Alone", "Nauts");
        s2.dlc_code = "RV".into();
        s2.patterns.insert("5B".into(), [
            ("SC".into(), PatternInfo { level: Some(6), floor: None, floor_name: None, rating: None })
        ].into_iter().collect());
        
        db.songs.push(s1);
        db.songs.push(s2);
        db.build_index();

        // 1. Marshmello (SC level 5, note has Marshmello)
        let match1 = db.find_best_match("Alone", "5B", "SC", Some(5), "RESPECT/V", "Marshmello 작곡");
        assert!(match1.is_some());
        assert_eq!(match1.unwrap().title, "2");

        // 2. Nauts (SC level 6, note has Nauts)
        let match2 = db.find_best_match("Alone", "5B", "SC", Some(6), "RESPECT/V", "Nauts 작곡");
        assert!(match2.is_some());
        assert_eq!(match2.unwrap().title, "441");
    }
}
