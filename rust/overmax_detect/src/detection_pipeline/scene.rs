//! 씬 판별 (정적 씬 파싱, 결과·자켓 게이트, 엣지/색 검사).

use super::SceneMissDiag;
use crate::jacket_index::JacketIndex;
use crate::roi::RoiManager;
use crate::templates::FREESTYLE_RESULT_MODE_COLORS;
use overmax_core::SceneType;
use overmax_cv::Bgr;
use overmax_frame::CapturedFrame;

const STRICT_EDGE_THRESHOLD: f32 = 25.0;

fn get_min_color_distance(mean: Bgr, colors: &[Bgr]) -> f32 {
    let mut min_dist = f32::MAX;
    for color in colors {
        let dist = mean.distance_f32(*color);
        if dist < min_dist {
            min_dist = dist;
        }
    }
    min_dist
}

pub fn detect_freestyle_result_colorbar_match(mean: Bgr) -> bool {
    get_min_color_distance(mean, &FREESTYLE_RESULT_MODE_COLORS) <= 40.0f32
}

pub fn check_open_match_badge(frame: &CapturedFrame, rois: &RoiManager) -> Option<SceneType> {
    // PlayerPanel ROI 엣지 확인
    let edge_strength_result_open3 = rois
        .get_roi_for_scene("player_panel", SceneType::ResultOpen3)
        .and_then(|roi| detect_rect_edges(frame, roi));

    let edge_strength_result_open2 = rois
        .get_roi_for_scene("player_panel", SceneType::ResultOpen2)
        .and_then(|roi| detect_rect_edges(frame, roi));

    match (edge_strength_result_open2, edge_strength_result_open3) {
        (Some(strength2), Some(strength3)) => {
            if strength2 >= STRICT_EDGE_THRESHOLD && strength3 >= STRICT_EDGE_THRESHOLD {
                return Some(if strength2 > strength3 {
                    SceneType::ResultOpen2
                } else {
                    SceneType::ResultOpen3
                });
            } else if strength2 >= STRICT_EDGE_THRESHOLD {
                return Some(SceneType::ResultOpen2);
            } else if strength3 >= STRICT_EDGE_THRESHOLD {
                return Some(SceneType::ResultOpen3);
            }
        }
        (Some(strength2), None) => {
            if strength2 >= STRICT_EDGE_THRESHOLD {
                return Some(SceneType::ResultOpen2);
            }
        }
        (None, Some(strength3)) => {
            if strength3 >= STRICT_EDGE_THRESHOLD {
                return Some(SceneType::ResultOpen3);
            }
        }
        (None, None) => {}
    }

    None
}

fn detect_result_scene_via_edge(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &impl JacketIndex,
    is_unknown: bool,
) -> Option<(SceneType, i32)> {
    // ResultFreestyle, ResultOpen3, ResultOpen2 재킷 ROI는 같은 위치를 공유함
    let jacket_roi = rois.get_roi_for_scene("jacket", SceneType::ResultFreestyle)?;
    let colorbar_roi = rois.get_roi_for_scene("mode_colorbar", SceneType::ResultFreestyle)?;
    let mean = overmax_frame::region_mean_bgr(frame, colorbar_roi);

    let is_freestyle_result = detect_freestyle_result_colorbar_match(mean);
    let open_match_scene = check_open_match_badge(frame, rois);
    let gate_ok = is_freestyle_result || open_match_scene.is_some();

    if gate_ok {
        let jacket = jacket_roi.and_then(frame, |jacket_img| Some(jacket_img.to_image_region()))?;

        // 1차 초고속 게이트: Centroid Kernel 사전 검사 (결과창 노이즈로 인한 98ms 스파이크 차단)
        let kernel_ok = matcher.check_centroid_kernel(
            &jacket.bgra,
            jacket.width as usize,
            jacket.height as usize,
            4,
        );

        if !kernel_ok {
            if is_unknown {
                debug_println!(
                    "    [scene_gate] Rejected by 1st Centroid Kernel Gate for result scene"
                );
            }
            return None;
        }

        if is_unknown {
            debug_println!(
                "    [telemetry] match_jacket triggered while scene=Unknown, candidate=result"
            );
        }
        // 결과창 재킷 매칭 시도
        let mut song_id = None;
        if let Some(match_res) = matcher.match_jacket(
            &jacket.bgra,
            jacket.width as usize,
            jacket.height as usize,
            4,
        ) {
            let threshold = matcher.similarity_threshold();
            if match_res.similarity >= threshold {
                if let Ok(id) = match_res.image_id.parse::<i32>() {
                    song_id = Some(id);
                    debug_println!(
                        "    [detect_result_scene_via_edge] Result screen jacket verified. SongID={}, Similarity={}",
                        id, match_res.similarity
                    );
                }
            }
        }

        // 재킷 매칭이 확실히 성공한 경우에만 결과창 씬으로 반환
        if let Some(id) = song_id {
            if is_freestyle_result {
                debug_println!("    [detect_result_scene_via_edge] Result screen detected via freestyle colorbar!");
                return Some((SceneType::ResultFreestyle, id));
            }

            if let Some(fallback_scene) = open_match_scene {
                debug_println!("    [detect_result_scene_via_edge] Result screen detected via openmatch badge!");
                return Some((fallback_scene, id));
            }
        }
    }
    None
}

/// 선곡 계열 씬의 공통 자켓 게이트 체인:
/// ROI 크롭 → Centroid Kernel 게이트 → 카테고리 띠 단색성 검사 → 자켓 유사도 매칭.
/// 성공 시 (song_id, similarity), 실패 시 거절 단계가 기록된 SceneMissDiag를 반환한다.
fn run_jacket_match_gate(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &impl JacketIndex,
    scene: SceneType,
    is_unknown: bool,
    #[allow(unused_variables)] gate_label: &str,
) -> (Option<(i32, f32)>, SceneMissDiag) {
    let mut diag = SceneMissDiag::default();
    let Some(jacket_roi) = rois.get_roi_for_scene("jacket", scene) else {
        return (None, diag);
    };
    let Some(jacket) = jacket_roi.and_then(frame, |jacket_img| Some(jacket_img.to_image_region()))
    else {
        return (None, diag);
    };

    // 1차 게이트: Centroid Kernel 사전 검사
    if !matcher.check_centroid_kernel(
        &jacket.bgra,
        jacket.width as usize,
        jacket.height as usize,
        4,
    ) {
        if is_unknown {
            debug_println!(
                "    [scene_gate] Rejected by 1st Centroid Kernel Gate for {gate_label}"
            );
        }
        diag.centroid_rejected = true;
        return (None, diag);
    }

    // 2차 게이트: 카테고리 띠 단색성 검사
    if !check_category_band_solid(frame, jacket_roi, rois.scale()) {
        diag.band_rejected = true;
        return (None, diag);
    }

    if is_unknown {
        debug_println!(
            "    [telemetry] match_jacket triggered while scene=Unknown, candidate={gate_label}"
        );
    }

    let Some(match_res) = matcher.match_jacket(
        &jacket.bgra,
        jacket.width as usize,
        jacket.height as usize,
        4,
    ) else {
        return (None, diag);
    };
    let threshold = matcher.similarity_threshold();
    if match_res.similarity < threshold {
        diag.top_similarity = Some(match_res.similarity);
        return (None, diag);
    }
    match match_res.image_id.parse::<i32>() {
        Ok(song_id) => (Some((song_id, match_res.similarity)), diag),
        Err(_) => (None, diag),
    }
}

fn detect_freestyle_scene_via_edge(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &impl JacketIndex,
    is_unknown: bool,
) -> (Option<(SceneType, i32, f32)>, SceneMissDiag) {
    let (res, diag) = run_jacket_match_gate(
        frame,
        rois,
        matcher,
        SceneType::Freestyle,
        is_unknown,
        "freestyle",
    );
    (res.map(|(id, sim)| (SceneType::Freestyle, id, sim)), diag)
}

fn detect_openmatch_scene_via_edge(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &impl JacketIndex,
    is_unknown: bool,
) -> (Option<(SceneType, i32, f32)>, SceneMissDiag) {
    let (res, diag) = run_jacket_match_gate(
        frame,
        rois,
        matcher,
        SceneType::OpenMatch,
        is_unknown,
        "openmatch",
    );
    (res.map(|(id, sim)| (SceneType::OpenMatch, id, sim)), diag)
}

pub(super) fn parse_static_scene(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &impl JacketIndex,
    is_unknown: bool,
) -> (Option<(SceneType, Option<i32>)>, SceneMissDiag) {
    // 1. 결과창 감지 우선 (Native CV)
    if let Some((scene, song_id)) = detect_result_scene_via_edge(frame, rois, matcher, is_unknown) {
        return (Some((scene, Some(song_id))), SceneMissDiag::default());
    }

    // 2. 프리스타일 및 오픈매치 선곡창 자켓 매칭 동시 비교하여 경합 (Native CV)
    let (freestyle_res, fs_diag) =
        detect_freestyle_scene_via_edge(frame, rois, matcher, is_unknown);
    let (openmatch_res, om_diag) =
        detect_openmatch_scene_via_edge(frame, rois, matcher, is_unknown);

    // 실패 원인 진단: 두 후보 중 관측된 최악의 게이트 상태를 병합
    let diag = SceneMissDiag {
        centroid_rejected: fs_diag.centroid_rejected && om_diag.centroid_rejected,
        band_rejected: fs_diag.band_rejected && om_diag.band_rejected,
        top_similarity: match (fs_diag.top_similarity, om_diag.top_similarity) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        },
    };

    let result = match (freestyle_res, openmatch_res) {
        (Some((f_scene, f_id, f_sim)), Some((o_scene, o_id, o_sim))) => {
            // 둘 다 임계치를 넘었을 경우, 유사도(Similarity)가 더 높은 씬을 승자로 채택
            if f_sim >= o_sim {
                debug_println!("    [parse_static_scene] Both matched. Freestyle selected by similarity ({:.4} vs {:.4})", f_sim, o_sim);
                Some((f_scene, Some(f_id)))
            } else {
                debug_println!("    [parse_static_scene] Both matched. OpenMatch selected by similarity ({:.4} vs {:.4})", o_sim, f_sim);
                Some((o_scene, Some(o_id)))
            }
        }
        (Some((f_scene, f_id, _)), None) => Some((f_scene, Some(f_id))),
        (None, Some((o_scene, o_id, _))) => Some((o_scene, Some(o_id))),
        (None, None) => None,
    };
    (result, diag)
}

pub fn detect_static_scene(
    frame: &CapturedFrame,
    rois: &RoiManager,
    matcher: &impl JacketIndex,
) -> SceneType {
    parse_static_scene(frame, rois, matcher, true)
        .0
        .map(|(scene, _)| scene)
        .unwrap_or(SceneType::Unknown)
}

fn detect_rect_edges(frame: &CapturedFrame, roi: overmax_frame::RoiRect) -> Option<f32> {
    let margin = 8;
    roi.with_margin(margin)
        .and_then(frame, |ext_img| ext_img.detect_edges(margin as usize).ok())
}

fn check_category_band_solid(
    frame: &CapturedFrame,
    jacket_roi: overmax_frame::RoiRect,
    scale: f32,
) -> bool {
    let width = ((4.0 * scale).round() as i32).max(4);
    let band_roi = overmax_frame::RoiRect {
        x1: jacket_roi.x2,
        y1: jacket_roi.y1,
        x2: jacket_roi.x2 + width,
        y2: jacket_roi.y2,
    };

    // 띠 경계선 엣지 검사 우회 (자켓과 띠의 색상이 유사해 엣지가 뭉개지는 케이스 방지)
    // 3단계 통계 기준(최소 밝기 >= 60, 수직 편차 <= 20, 채도/무채색 정합성)으로 띠를 판별

    band_roi
        .and_then(frame, |band_img| {
            let total_pixels = band_img.width * band_img.height ;
            if total_pixels == 0 {
                return Some(false);
            }

            let mut sum_bgr = overmax_cv::Bgr::new(0.0, 0.0, 0.0);

            for y in 0..band_img.height {
                let row = band_img.row(y);
                for pixel in row.chunks_exact(4) {
                    sum_bgr += overmax_cv::Bgr::from_bgra_slice_f64(pixel);
                }
            }

            let mean = sum_bgr / total_pixels as f64;

            // 1. Brightness 가드 (>= 60.0)
            let brightness = mean.luma(overmax_cv::LumaMethod::Weighted);
            if brightness < 60.0 {
                return Some(false);
            }

            // 2. AvgDiff 수직 단색성 가드 (<= 20.0)
            let mut diff_sum = 0.0;
            for y in 0..band_img.height {
                let row = band_img.row(y);
                for pixel in row.chunks_exact(4) {
                    let px = overmax_cv::Bgr::from_bgra_slice_f64(pixel);
                    diff_sum += px.abs_diff(mean).sum_channels();
                }
            }

            let avg_diff = diff_sum / (total_pixels * 3) as f64;
            if avg_diff > 20.0 {
                return Some(false);
            }

            // 3. Saturation 및 무채색(Gray) 채널 균등성 검증
            let max_c = mean.max_channel();
            let min_c = mean.min_channel();
            let saturation = if max_c > 0.0 {
                (max_c - min_c) / max_c
            } else {
                0.0
            };

            if saturation < 0.15
                && mean.max_channel_diff() > 15.0 {
                    debug_println!(
                        "    [check_category_band_solid] Category band rejected: channel diff {:.1} > 15.0",
                        mean.max_channel_diff()
                    );
                    return Some(false);
                }

            debug_println!(
                "    [check_category_band_solid] Category band solid detected! brightness={:.1}, avg_diff={:.2}",
                brightness, avg_diff
            );
            Some(true)
        })
        .unwrap_or(false)
}
