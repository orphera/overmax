use crate::record_db::RecordDB;
use crate::varchive::VArchiveDB;
use std::cmp::Ordering;

const DIFFICULTIES: &[&str] = &["NM", "HD", "MX", "SC"];
const SC_GROUP: &[&str] = &["SC"];

#[derive(Debug, Clone)]
pub struct RecommendEntry {
    pub song_id: i32,
    pub song_name: String,
    pub composer: String,
    pub button_mode: String,
    pub difficulty: String,
    pub level: Option<u32>,
    pub floor: Option<f64>,
    pub floor_name: Option<String>,
    pub rate: Option<f64>,
    pub is_max_combo: bool,
}

impl RecommendEntry {
    pub fn is_played(&self) -> bool {
        self.rate.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct RecommendResult {
    pub entries: Vec<RecommendEntry>,
    pub avg_rate: f64,
    pub has_record_count: usize,
    pub total_count: usize,
}

impl RecommendResult {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            avg_rate: -1.0,
            has_record_count: 0,
            total_count: 0,
        }
    }
}

pub struct Recommender<'a> {
    vdb: &'a VArchiveDB,
    rdb: &'a RecordDB,
}

impl<'a> Recommender<'a> {
    pub fn new(vdb: &'a VArchiveDB, rdb: &'a RecordDB) -> Self {
        Self { vdb, rdb }
    }

    fn parse_floor_value(floor_name: Option<&String>) -> Option<f64> {
        floor_name.and_then(|s| s.parse::<f64>().ok())
    }

    fn diff_group(diff: &str) -> &'static str {
        if SC_GROUP.contains(&diff) {
            "SC"
        } else {
            "NHM"
        }
    }

    pub fn recommend(
        &self,
        song_id: i32,
        button_mode: &str,
        difficulty: &str,
        floor_range: f64,
        max_results: usize,
        same_mode_only: bool,
    ) -> RecommendResult {
        let current_song = match self.vdb.search_by_id(song_id) {
            Some(s) => s,
            None => return RecommendResult::empty(),
        };

        let current_pattern = current_song
            .patterns
            .get(button_mode)
            .and_then(|m| m.get(difficulty));

        let p = match current_pattern {
            Some(p) => p,
            None => return RecommendResult::empty(),
        };

        let ref_floor = Self::parse_floor_value(p.floor_name.as_ref());
        let use_official = ref_floor.is_none();

        let (final_ref_floor, ref_diff_grp) = if use_official {
            (p.level.unwrap_or(0) as f64, Self::diff_group(difficulty))
        } else {
            (ref_floor.unwrap(), "")
        };

        let mut candidates = self.get_candidates(
            song_id,
            button_mode,
            difficulty,
            final_ref_floor,
            use_official,
            ref_diff_grp,
            floor_range,
            same_mode_only,
        );

        if candidates.is_empty() {
            return RecommendResult::empty();
        }

        self.merge_record_rates(&mut candidates);

        candidates.sort_by(|a, b| {
            if a.is_played() && !b.is_played() {
                Ordering::Less
            } else if !a.is_played() && b.is_played() {
                Ordering::Greater
            } else if a.is_played() && b.is_played() {
                a.rate
                    .unwrap()
                    .partial_cmp(&b.rate.unwrap())
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| {
                        a.floor
                            .unwrap_or(0.0)
                            .partial_cmp(&b.floor.unwrap_or(0.0))
                            .unwrap_or(Ordering::Equal)
                    })
            } else {
                a.floor
                    .unwrap_or(0.0)
                    .partial_cmp(&b.floor.unwrap_or(0.0))
                    .unwrap_or(Ordering::Equal)
            }
        });

        // Skip caching for brevity in Rust port, we just compute stats directly for the returned entries or all candidates
        let total_count = candidates.len();
        let has_record_count = candidates.iter().filter(|c| c.is_played()).count();
        let avg_rate = if has_record_count > 0 {
            let sum: f64 = candidates.iter().filter_map(|c| c.rate).sum();
            sum / has_record_count as f64
        } else {
            -1.0
        };

        candidates.truncate(max_results);

        RecommendResult {
            entries: candidates,
            avg_rate,
            has_record_count,
            total_count,
        }
    }

    fn get_candidates(
        &self,
        target_song_id: i32,
        target_mode: &str,
        target_diff: &str,
        ref_floor: f64,
        use_official: bool,
        ref_diff_grp: &str,
        floor_range: f64,
        same_mode_only: bool,
    ) -> Vec<RecommendEntry> {
        let modes_to_check = if same_mode_only {
            vec![target_mode]
        } else {
            vec!["4B", "5B", "6B", "8B"]
        };

        let mut candidates = Vec::new();

        for song in &self.vdb.songs {
            let sid = match song.title.parse::<i32>() {
                Ok(id) => id,
                Err(_) => continue,
            };

            for mode in &modes_to_check {
                if let Some(mode_patterns) = song.patterns.get(*mode) {
                    for diff in DIFFICULTIES {
                        if let Some(p) = mode_patterns.get(*diff) {
                            let cand_floor_val = Self::parse_floor_value(p.floor_name.as_ref());
                            
                            let final_cand_floor = if use_official {
                                if cand_floor_val.is_some() || Self::diff_group(diff) != ref_diff_grp {
                                    continue;
                                }
                                p.level.unwrap_or(0) as f64
                            } else {
                                match cand_floor_val {
                                    Some(f) => f,
                                    None => continue,
                                }
                            };

                            if (final_cand_floor - ref_floor).abs() > floor_range {
                                continue;
                            }

                            if sid == target_song_id && mode == &target_mode && diff == &target_diff {
                                continue;
                            }

                            candidates.push(RecommendEntry {
                                song_id: sid,
                                song_name: song.name.clone(),
                                composer: song.composer.clone(),
                                button_mode: mode.to_string(),
                                difficulty: diff.to_string(),
                                level: p.level,
                                floor: Some(final_cand_floor),
                                floor_name: p.floor_name.clone(),
                                rate: None,
                                is_max_combo: false,
                            });
                        }
                    }
                }
            }
        }
        candidates
    }

    fn merge_record_rates(&self, candidates: &mut Vec<RecommendEntry>) {
        if !self.rdb.is_ready {
            return;
        }

        let mut unique_ids = Vec::new();
        for c in candidates.iter() {
            if !unique_ids.contains(&c.song_id) {
                unique_ids.push(c.song_id);
            }
        }

        let rate_map = self.rdb.get_rate_map(&unique_ids);

        for entry in candidates.iter_mut() {
            if let Some(&(rate, is_max_combo)) = rate_map.get(&(entry.song_id, entry.button_mode.clone(), entry.difficulty.clone())) {
                entry.rate = Some(rate);
                entry.is_max_combo = is_max_combo;
            }
        }
    }
}
