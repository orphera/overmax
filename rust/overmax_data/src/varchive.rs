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
pub struct Song {
    pub title: String, // Actually song_id
    pub name: String,
    pub composer: String,
    #[serde(default)]
    pub patterns: HashMap<String, HashMap<String, PatternInfo>>,
}

pub struct VArchiveDB {
    pub songs: Vec<Song>,
    title_map: HashMap<String, Vec<Song>>,
}

impl VArchiveDB {
    pub fn new() -> Self {
        Self {
            songs: Vec::new(),
            title_map: HashMap::new(),
        }
    }

    pub fn load_from_file(&mut self, path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        self.songs = serde_json::from_str(&content)?;
        self.build_index();
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
        value.to_lowercase().replace(char::is_whitespace, "")
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
            } else if !query.is_empty() && !comp_norm.is_empty() && (comp_norm.contains(&query) || query.contains(&comp_norm)) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_song(title: &str, name: &str, composer: &str) -> Song {
        Song {
            title: title.to_string(),
            name: name.to_string(),
            composer: composer.to_string(),
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
        db.songs.push(create_mock_song("1", "End of the Moonlight", "Forte Escape"));
        db.songs.push(create_mock_song("2", "End of the Moonlight", "BEXTER"));
        db.build_index();

        let song1 = db.find_exact("End of the Moonlight", "bexter");
        assert_eq!(song1.unwrap().title, "2");

        let song2 = db.find_exact("End of the Moonlight", "forte");
        assert_eq!(song2.unwrap().title, "1");
    }
}
