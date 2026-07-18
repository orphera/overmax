use crate::store::image_index::{ImageEntry, ImageMatch};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct JacketMatcherConfig {
    pub similarity_threshold: f32,
    pub margin_threshold: f32,
    pub disable_hog: bool,
}

#[derive(Debug)]
struct MatchCache {
    recent_indices: Vec<usize>,
}

pub struct JacketMatcher {
    entries: Arc<Vec<ImageEntry>>,
    config: JacketMatcherConfig,
    cache: std::sync::Mutex<MatchCache>,
}

impl JacketMatcher {
    pub fn new(entries: Vec<ImageEntry>, config: JacketMatcherConfig) -> Self {
        Self {
            entries: Arc::new(entries),
            config,
            cache: std::sync::Mutex::new(MatchCache {
                recent_indices: Vec::new(),
            }),
        }
    }

    pub fn similarity_threshold(&self) -> f32 {
        self.config.similarity_threshold
    }

    pub fn match_jacket(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
    ) -> Option<ImageMatch> {
        self.match_jacket_with_top_k(data, width, height, channels, 10)
    }

    fn update_cache(&self, idx: usize) {
        if let Ok(mut guard) = self.cache.lock() {
            if let Some(pos) = guard.recent_indices.iter().position(|&x| x == idx) {
                guard.recent_indices.remove(pos);
            }
            guard.recent_indices.insert(0, idx);
            if guard.recent_indices.len() > 8 {
                guard.recent_indices.truncate(8);
            }
        }
    }

    pub fn match_jacket_with_top_k(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
        _top_k: usize,
    ) -> Option<ImageMatch> {
        if self.entries.is_empty() {
            return None;
        }

        // 1. 3종 해시 추출
        let (q_phash, q_dhash, q_ahash) =
            overmax_cv::compute_image_hashes(data, width, height, channels).ok()?;

        // 2. 2x2 분할 그리드 히스토그램 추출을 위한 그레이스케일 전처리 및 대비 스트레칭
        let mut gray = overmax_cv::to_gray(data, channels);
        overmax_cv::stretch_contrast(&mut gray, width, height);
        let q_grid_hist = overmax_cv::compute_grid_histogram(&gray, width, height);

        // 오염 영역 비트 마스킹 (상단 y=0, 우측 x=7, 즐겨찾기 y=1, x=0)
        let mut mask_bits: u64 = 0;
        for x in 0..8 {
            mask_bits |= 1 << x; // y = 0
        }
        for y in 0..8 {
            mask_bits |= 1 << (y * 8 + 7); // x = 7
        }
        mask_bits |= 1 << 8; // y = 1, x = 0

        let hash_mask: u64 = !mask_bits;
        let compare_bits = hash_mask.count_ones() as f32; // 48.0
        let total_compare_bits = 64.0 + compare_bits * 2.0; // 160.0

        // 3. 싱글 스레드 순차 최적화 매칭 순회 (1차 Early Exit + 2차 WTA 유사도 계산)
        let matched = self.entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                let p_dist = (entry.phash ^ q_phash).count_ones();
                let d_dist = ((entry.dhash ^ q_dhash) & hash_mask).count_ones();
                let a_dist = ((entry.ahash ^ q_ahash) & hash_mask).count_ones();
                
                let hamming_sum = p_dist + d_dist + a_dist;
                
                // 1차 필터: Early Exit (임계치 42)
                if hamming_sum > 42 {
                    return None;
                }

                // 2차 필터: 히스토그램 L1 유사도 산출 (레거시 DB 하위 호환 보장)
                let hist_sim = if let Some(e_hist) = entry.grid_hist {
                    let mut hist_diff = 0u32;
                    for (&e_h, &q_h) in e_hist.iter().zip(q_grid_hist.iter()) {
                        hist_diff += (e_h as i32 - q_h as i32).unsigned_abs();
                    }
                    1.0 - (hist_diff as f32 / 256.0).clamp(0.0, 1.0)
                } else {
                    1.0 // 히스토그램이 없는 레거시 DB는 해시 유사도로만 판단
                };

                let hash_sim = 1.0 - (hamming_sum as f32 / total_compare_bits);
                
                // 가중합 유사도 산출
                let similarity = if entry.grid_hist.is_some() {
                    0.5 * hash_sim + 0.5 * hist_sim
                } else {
                    hash_sim
                };

                let sim_key = (similarity * 1000000.0) as u32;
                Some((idx, sim_key, similarity))
            })
            .max_by_key(|&(_, sim_key, _)| sim_key);

        if let Some((idx, _, similarity)) = matched {
            if similarity >= self.config.similarity_threshold {
                self.update_cache(idx);
                return Some(ImageMatch {
                    image_id: self.entries[idx].image_id.clone(),
                    similarity,
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_entry(image_id: &str, phash: u64, hog_val: f32) -> ImageEntry {
        let hog = vec![hog_val; 1764];
        let hog_norm = (1764.0 * hog_val * hog_val).sqrt().max(1.0);
        ImageEntry {
            image_id: image_id.to_string(),
            phash,
            dhash: phash,
            ahash: phash,
            hog,
            hog_norm,
            grid_hist: None,
        }
    }

    #[test]
    fn test_jacket_matcher_basic_match() {
        let entries = vec![
            dummy_entry("song-a", 0x0000_0000_0000_0000, 0.1),
            dummy_entry("song-b", 0xFFFF_FFFF_FFFF_FFFF, 0.2),
        ];
        let config = JacketMatcherConfig {
            similarity_threshold: 0.75,
            margin_threshold: 3.0,
            disable_hog: false,
        };
        let matcher = JacketMatcher::new(entries, config);

        // 8x8 그레이스케일 이미지 모킹 (전부 0)
        let query_data = vec![0u8; 64];

        let matched = matcher.match_jacket(&query_data, 8, 8, 1).unwrap();
        assert_eq!(matched.image_id, "song-a");
        assert!(matched.similarity >= 0.9);
    }
}
