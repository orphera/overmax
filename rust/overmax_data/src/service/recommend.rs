use crate::community::client::VArchiveDB;
use crate::service::record_manager::{RecordManager, RecordSource};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

const DIFFICULTIES: &[&str] = &["NM", "HD", "MX", "SC"];
const SC_GROUP: &[&str] = &["SC"];
const MODES: &[&str] = &["4B", "5B", "6B", "8B"];

type RecordKey = (i32, String, String);

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FloorCacheKey {
    pub button_mode: String,
    pub scale_type: String,
    pub floor_millis: i64,
}

#[derive(Debug, Clone)]
pub struct FloorRateSummary {
    pub total_count: usize,
    pub has_record_count: usize,
    pub rate_sum: f64,
}

impl FloorRateSummary {
    pub fn new(total_count: usize) -> Self {
        Self {
            total_count,
            has_record_count: 0,
            rate_sum: 0.0,
        }
    }

    pub fn avg_rate(&self) -> f64 {
        if self.has_record_count == 0 {
            return -1.0;
        }
        self.rate_sum / self.has_record_count as f64
    }
}

pub struct Recommender {
    vdb: Arc<VArchiveDB>,
    rdb: Arc<RecordManager>,
    floor_rate_cache: Mutex<HashMap<FloorCacheKey, FloorRateSummary>>,
    floor_rate_dirty: Mutex<HashMap<FloorCacheKey, bool>>,
    floor_patterns: Mutex<HashMap<FloorCacheKey, Vec<RecordKey>>>,
    record_to_floor_key: Mutex<HashMap<RecordKey, FloorCacheKey>>,
    cache_index_ready: AtomicBool,
}

struct CandidateSearchParams<'a> {
    target_song_id: i32,
    target_mode: &'a str,
    target_diff: &'a str,
    ref_floor: f64,
    use_official: bool,
    ref_diff_grp: &'a str,
    floor_range: f64,
    same_mode_only: bool,
}

impl Recommender {
    pub fn new(vdb: Arc<VArchiveDB>, rdb: Arc<RecordManager>) -> Self {
        Self {
            vdb,
            rdb,
            floor_rate_cache: Mutex::new(HashMap::new()),
            floor_rate_dirty: Mutex::new(HashMap::new()),
            floor_patterns: Mutex::new(HashMap::new()),
            record_to_floor_key: Mutex::new(HashMap::new()),
            cache_index_ready: AtomicBool::new(false),
        }
    }

    fn parse_floor_value(floor_name: Option<&String>) -> Option<f64> {
        floor_name.and_then(|s| s.parse::<f64>().ok())
    }

    fn floor_to_millis(floor: f64) -> i64 {
        (floor * 1000.0).round() as i64
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

        let current_pattern = crate::community::client::Mode::from_str(button_mode)
            .zip(crate::community::client::Difficulty::from_str(difficulty))
            .and_then(|(m, d)| current_song.patterns[m as usize][d as usize].as_ref());

        let p = match current_pattern {
            Some(p) => p,
            None => return RecommendResult::empty(),
        };

        let ref_floor = Self::parse_floor_value(p.floor_name.as_ref());
        let use_official = ref_floor.is_none();

        let (final_ref_floor, ref_diff_grp) = if let Some(floor) = ref_floor {
            (floor, "")
        } else {
            (p.level.unwrap_or(0) as f64, Self::diff_group(difficulty))
        };

        let mut candidates = self.get_candidates(CandidateSearchParams {
            target_song_id: song_id,
            target_mode: button_mode,
            target_diff: difficulty,
            ref_floor: final_ref_floor,
            use_official,
            ref_diff_grp,
            floor_range,
            same_mode_only,
        });

        self.merge_record_rates(&mut candidates);

        candidates.sort_by(|a, b| {
            if a.is_played() && !b.is_played() {
                Ordering::Less
            } else if !a.is_played() && b.is_played() {
                Ordering::Greater
            } else if let (Some(ra), Some(rb)) = (a.rate, b.rate) {
                ra.partial_cmp(&rb)
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

        let summary = self.get_summary_from_cache(
            button_mode,
            difficulty,
            final_ref_floor,
            use_official,
            floor_range,
            same_mode_only,
        );

        candidates.truncate(max_results);

        RecommendResult {
            entries: candidates,
            avg_rate: summary.avg_rate(),
            has_record_count: summary.has_record_count,
            total_count: summary.total_count,
        }
    }

    fn get_candidates(&self, params: CandidateSearchParams) -> Vec<RecommendEntry> {
        let modes_to_check = if params.same_mode_only {
            vec![params.target_mode]
        } else {
            MODES.to_vec()
        };

        let mut candidates = Vec::new();

        for song in &self.vdb.songs {
            let sid = match song.title.parse::<i32>() {
                Ok(id) => id,
                Err(_) => continue,
            };

            for mode in &modes_to_check {
                if let (Some(m), Some(d_list)) = (
                    crate::community::client::Mode::from_str(mode),
                    Some(DIFFICULTIES),
                ) {
                    for diff in d_list {
                        if let Some(d) = crate::community::client::Difficulty::from_str(diff) {
                            if let Some(p) = &song.patterns[m as usize][d as usize] {
                                let cand_floor_val = Self::parse_floor_value(p.floor_name.as_ref());

                                let final_cand_floor = if params.use_official {
                                    if cand_floor_val.is_some()
                                        || Self::diff_group(diff) != params.ref_diff_grp
                                    {
                                        continue;
                                    }
                                    p.level.unwrap_or(0) as f64
                                } else {
                                    match cand_floor_val {
                                        Some(f) => f,
                                        None => continue,
                                    }
                                };

                                if (final_cand_floor - params.ref_floor).abs() > params.floor_range
                                {
                                    continue;
                                }

                                if sid == params.target_song_id
                                    && mode == &params.target_mode
                                    && diff == &params.target_diff
                                {
                                    continue;
                                }

                                candidates.push(RecommendEntry {
                                    song_id: sid,
                                    song_name: song.name.clone(),
                                    composer: song.composer.to_string(),
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
        }
        candidates
    }

    fn merge_record_rates(&self, candidates: &mut [RecommendEntry]) {
        if !self.rdb.is_ready() {
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
            if let Some(&(rate, is_max_combo)) = rate_map.get(&(
                entry.song_id,
                entry.button_mode.clone(),
                entry.difficulty.clone(),
            )) {
                entry.rate = Some(rate as f64);
                entry.is_max_combo = is_max_combo;
            }
        }
    }

    fn build_floor_cache_index(&self) {
        let mut floor_patterns = HashMap::new();
        let mut record_to_floor_key = HashMap::new();

        for song in &self.vdb.songs {
            let song_id = match song.title.parse::<i32>() {
                Ok(id) => id,
                Err(_) => continue,
            };
            for (m_idx, &mode) in MODES.iter().enumerate() {
                for d_idx in 0..4 {
                    let diff = match d_idx {
                        0 => "NM",
                        1 => "HD",
                        2 => "MX",
                        3 => "SC",
                        _ => unreachable!(),
                    };
                    if let Some(p) = &song.patterns[m_idx][d_idx] {
                        let floor_val;
                        let scale_type;
                        if let Some(f) = Self::parse_floor_value(p.floor_name.as_ref()) {
                            floor_val = f;
                            scale_type = "UNOFFICIAL".to_string();
                        } else {
                            if let Some(level) = p.level {
                                floor_val = level as f64;
                            } else {
                                continue;
                            }
                            scale_type = if SC_GROUP.contains(&diff) {
                                "OFFICIAL_SC".to_string()
                            } else {
                                "OFFICIAL_NHM".to_string()
                            };
                        }

                        let key = FloorCacheKey {
                            button_mode: mode.to_string(),
                            scale_type,
                            floor_millis: Self::floor_to_millis(floor_val),
                        };
                        let record_key = (song_id, mode.to_string(), diff.to_string());
                        floor_patterns
                            .entry(key.clone())
                            .or_insert_with(Vec::new)
                            .push(record_key.clone());
                        record_to_floor_key.insert(record_key, key);
                    }
                }
            }
        }

        let mut cache_guard = overmax_core::lock_or_recover(&self.floor_rate_cache);
        let mut dirty_guard = overmax_core::lock_or_recover(&self.floor_rate_dirty);
        let mut patterns_guard = overmax_core::lock_or_recover(&self.floor_patterns);
        let mut record_to_key_guard = overmax_core::lock_or_recover(&self.record_to_floor_key);

        *patterns_guard = floor_patterns;
        *record_to_key_guard = record_to_floor_key;

        cache_guard.clear();
        dirty_guard.clear();
        for (key, entries) in patterns_guard.iter() {
            cache_guard.insert(key.clone(), FloorRateSummary::new(entries.len()));
            dirty_guard.insert(key.clone(), true);
        }

        self.cache_index_ready.store(true, AtomicOrdering::SeqCst);
    }

    fn ensure_floor_rate_cache(&self) {
        if !self.cache_index_ready.load(AtomicOrdering::SeqCst) {
            self.build_floor_cache_index();
        }

        let (full_dirty, dirty_keys) = self.rdb.consume_dirty_info();
        {
            let mut dirty_guard = overmax_core::lock_or_recover(&self.floor_rate_dirty);
            let patterns_guard = overmax_core::lock_or_recover(&self.floor_patterns);
            let record_to_key_guard = overmax_core::lock_or_recover(&self.record_to_floor_key);

            if full_dirty {
                for key in patterns_guard.keys() {
                    dirty_guard.insert(key.clone(), true);
                }
            } else {
                for record_key in &dirty_keys {
                    if let Some(floor_key) = record_to_key_guard.get(record_key) {
                        dirty_guard.insert(floor_key.clone(), true);
                    }
                }
            }
        }

        let dirty_floor_keys: Vec<FloorCacheKey> = {
            let dirty_guard = overmax_core::lock_or_recover(&self.floor_rate_dirty);
            dirty_guard
                .iter()
                .filter(|(_, &is_dirty)| is_dirty)
                .map(|(k, _)| k.clone())
                .collect()
        };

        if dirty_floor_keys.is_empty() {
            return;
        }

        let mut all_song_ids = Vec::new();
        for song in &self.vdb.songs {
            if let Ok(song_id) = song.title.parse::<i32>() {
                all_song_ids.push(song_id);
            }
        }
        all_song_ids.sort_unstable();
        all_song_ids.dedup();

        let rate_map = self.rdb.get_rate_map(&all_song_ids);

        let mut cache_guard = overmax_core::lock_or_recover(&self.floor_rate_cache);
        let mut dirty_guard = overmax_core::lock_or_recover(&self.floor_rate_dirty);
        let patterns_guard = overmax_core::lock_or_recover(&self.floor_patterns);

        for key in &dirty_floor_keys {
            let entries = patterns_guard.get(key).cloned().unwrap_or_default();
            let mut summary = FloorRateSummary::new(entries.len());
            for record_key in &entries {
                if let Some(&(rate, _)) = rate_map.get(record_key) {
                    if rate > 0.0 {
                        summary.has_record_count += 1;
                        summary.rate_sum += rate as f64;
                    }
                }
            }
            cache_guard.insert(key.clone(), summary);
            dirty_guard.insert(key.clone(), false);
        }
    }

    fn get_summary_from_cache(
        &self,
        button_mode: &str,
        difficulty: &str,
        ref_floor: f64,
        use_official: bool,
        floor_range: f64,
        same_mode_only: bool,
    ) -> FloorRateSummary {
        self.ensure_floor_rate_cache();

        let scale_type = if use_official {
            if SC_GROUP.contains(&difficulty) {
                "OFFICIAL_SC"
            } else {
                "OFFICIAL_NHM"
            }
        } else {
            "UNOFFICIAL"
        };

        let modes = if same_mode_only {
            vec![button_mode]
        } else {
            MODES.to_vec()
        };

        let mut total = 0;
        let mut has_record = 0;
        let mut rate_sum = 0.0;

        let cache_guard = overmax_core::lock_or_recover(&self.floor_rate_cache);
        for (key, summary) in cache_guard.iter() {
            if !modes.contains(&key.button_mode.as_str()) {
                continue;
            }
            if key.scale_type != scale_type {
                continue;
            }

            let key_floor = key.floor_millis as f64 / 1000.0;
            if (key_floor - ref_floor).abs() > floor_range {
                continue;
            }

            total += summary.total_count;
            has_record += summary.has_record_count;
            rate_sum += summary.rate_sum;
        }

        FloorRateSummary {
            total_count: total,
            has_record_count: has_record,
            rate_sum,
        }
    }
}
