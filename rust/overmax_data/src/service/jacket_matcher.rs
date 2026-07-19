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
        top_k: usize,
    ) -> Option<ImageMatch> {
        if self.entries.is_empty() || top_k == 0 {
            return None;
        }

        // 1단계: 해시 특징량 계산
        let (q_phash, q_dhash, q_ahash) =
            overmax_cv::compute_image_hashes(data, width, height, channels).ok()?;

        // 오염이 집중되는 상단 테두리(y=0), 우측 테두리(x=7), 좌상단 즐겨찾기(y=1, x=0) 비트들을 무력화
        let mut mask_bits: u64 = 0;
        for x in 0..8 {
            mask_bits |= 1 << x; // y = 0
        }
        for y in 0..8 {
            mask_bits |= 1 << (y * 8 + 7); // x = 7
        }
        mask_bits |= 1 << 8; // y = 1, x = 0

        let hash_mask: u64 = !mask_bits;
        let compare_bits = hash_mask.count_ones() as f32; // 유효 비트 수 (48개)

        // 2단계: 전체 DB 곡에 대해 해시 유사도 스코어링 (마스크 반영)
        let mut candidates = self
            .entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let p_dist = (entry.phash ^ q_phash).count_ones(); // phash는 전역 변환이므로 마스크 없음
                let d_dist = ((entry.dhash ^ q_dhash) & hash_mask).count_ones();
                let a_dist = ((entry.ahash ^ q_ahash) & hash_mask).count_ones();

                let p_sim = 1.0 - (p_dist as f32 / 64.0);
                let d_sim = 1.0 - (d_dist as f32 / compare_bits);
                let a_sim = 1.0 - (a_dist as f32 / compare_bits);

                let hash_sim = 0.5 * p_sim + 0.3 * d_sim + 0.2 * a_sim;
                (idx, hash_sim)
            })
            .collect::<Vec<_>>();

        // 해시 유사도 정렬 (내림차순, 높을수록 가까움)
        candidates.sort_by(|a, b| b.1.total_cmp(&a.1));

        if candidates.is_empty() {
            return None;
        }

        let first_idx = candidates[0].0;
        let first_hash_sim = candidates[0].1;

        // 3단계: HOG 연산 스킵 여부 판정 (유사도 차이가 크거나 HOG 데이터가 없으면 스킵)
        let skip_hog = if self.config.disable_hog || self.entries[first_idx].hog.is_empty() {
            true
        } else if candidates.len() > 1 {
            let second_hash_sim = candidates[1].1;
            let margin = first_hash_sim - second_hash_sim;
            margin >= self.config.margin_threshold * 0.1 || first_hash_sim >= 0.99
        } else {
            true
        };

        if skip_hog {
            let similarity = first_hash_sim;
            if similarity >= self.config.similarity_threshold {
                self.update_cache(first_idx);
                return Some(ImageMatch {
                    image_id: self.entries[first_idx].image_id.clone(),
                    similarity,
                });
            }
            return None;
        }

        // 4단계: HOG 정밀 매칭 (상단/우측 테두리 + 좌상단 HOG 블록 성분 마스킹 적용)
        let q_hog = overmax_cv::compute_image_hog(data, width, height, channels).ok()?;
        let mut q_hog_masked = q_hog.clone();
        apply_hog_mask(&mut q_hog_masked);
        let q_hog_norm = vector_norm(&q_hog_masked).max(1.0);

        // 상위 top_k개 후보군에 대해서만 HOG Dot product 연산 적용
        let mut final_candidates = candidates
            .into_iter()
            .take(top_k.min(self.entries.len()))
            .map(|(idx, hash_sim)| {
                let entry = &self.entries[idx];
                let mut db_hog_masked = entry.hog.clone();
                apply_hog_mask(&mut db_hog_masked);
                let db_hog_norm = vector_norm(&db_hog_masked).max(1.0);

                let hog_sim = dot(&db_hog_masked, &q_hog_masked) / (db_hog_norm * q_hog_norm);
                let similarity = 0.45 * hash_sim + 0.55 * hog_sim;
                (idx, similarity)
            })
            .collect::<Vec<_>>();

        // 최종 유사도 기준 내림차순 정렬
        final_candidates.sort_by(|a, b| b.1.total_cmp(&a.1));

        final_candidates
            .into_iter()
            .next()
            .and_then(|(idx, similarity)| {
                if similarity >= self.config.similarity_threshold {
                    self.update_cache(idx);
                    Some(ImageMatch {
                        image_id: self.entries[idx].image_id.clone(),
                        similarity,
                    })
                } else {
                    None
                }
            })
    }
}

fn dot(left: &[f32], right: &[f32]) -> f32 {
    let len = left.len();
    assert_eq!(len, right.len());
    let mut sum = 0.0;
    for i in 0..len {
        sum += left[i] * right[i];
    }
    sum
}

fn vector_norm(values: &[f32]) -> f32 {
    values.iter().map(|&v| v * v).sum::<f32>().sqrt()
}

fn apply_hog_mask(hog: &mut [f32]) {
    // 7x7 블록 중 block_y = 0 (상단) 및 block_x = 6 (우측), 그리고 좌상단 (0,1), (1,1) 무력화
    for block_x in 0..7 {
        let start = block_x * 252; // block_y = 0
        for i in 0..36 {
            hog[start + i] = 0.0;
        }
    }
    for block_y in 0..7 {
        let start = 6 * 252 + block_y * 36; // block_x = 6
        for i in 0..36 {
            hog[start + i] = 0.0;
        }
    }
    // 좌상단 추가 블록: (0,1)
    let start_0_1 = 36;
    for i in 0..36 {
        hog[start_0_1 + i] = 0.0;
    }
    // 좌상단 추가 블록: (1,1)
    let start_1_1 = 252 + 36;
    for i in 0..36 {
        hog[start_1_1 + i] = 0.0;
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
