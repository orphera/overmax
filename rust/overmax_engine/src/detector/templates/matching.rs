use crate::capture::frame_utils::ImageView;
use overmax_core::{Difficulty, Mode};

/// Rate 영역을 Pure Rust CV 템플릿 매칭으로 감지합니다.
pub fn detect_rate(rate: &ImageView) -> Option<f32> {
    let (matched_str, _, _, _) =
        match_digits_template(rate, super::digit::DIGIT_TEMPLATES_RATE).ok()?;

    // 템플릿 매칭 결과에서 ?를 제거하고 파싱 시도
    (!matched_str.is_empty())
        .then(|| matched_str.replace('?', ""))
        .and_then(|clean_str| parse_rate_text(&clean_str))
}

/// Score 영역을 템플릿 매칭을 통해 정수로 직접 누적하여 파싱합니다 (Zero String Allocation).
pub fn detect_score(score: &ImageView) -> Option<u32> {
    let w = score.width;
    let h = score.height;
    if w * h == 0 {
        return None;
    }

    let region = score.to_image_region();
    let (binary, _, _) = overmax_cv::binarize_by_global_contrast(
        &region.bgra,
        w,
        h,
        overmax_cv::LumaMethod::Average,
        255,
    )
    .ok()?;

    let segments = overmax_cv::segment_characters(&binary, w, h).ok()?;
    if segments.len() != 6 && segments.len() != 7 {
        return None;
    }

    let mut score_val = 0u32;
    let mut char_bin = Vec::with_capacity(32 * h);

    for &(x1, x2) in &segments {
        let char_w = x2 - x1;
        let char_h = h;
        char_bin.resize(char_w * char_h, 0);
        for y in 0..char_h {
            for x in 0..char_w {
                char_bin[y * char_w + x] = binary[y * w + (x1 + x)];
            }
        }

        let matched = overmax_cv::match_character(
            &char_bin,
            char_w,
            char_h,
            super::digit::DIGIT_TEMPLATES_SCORE,
        )
        .ok()??;
        let digit = matched.0.to_digit(10)?;
        score_val = score_val * 10 + digit;
    }

    if score_val <= 1_000_000 {
        Some(score_val)
    } else {
        None
    }
}

/// Freestyle 결과창 모드 영역을 템플릿 매칭으로 판독합니다.
pub fn detect_freestyle_mode(mode_img: &ImageView) -> Option<Mode> {
    let w = mode_img.width;
    let h = mode_img.height;
    if w * h == 0 {
        return None;
    }

    let region = mode_img.to_image_region();
    let (binary, _, _) = overmax_cv::binarize_by_global_contrast(
        &region.bgra,
        w,
        h,
        overmax_cv::LumaMethod::Average,
        1,
    )
    .ok()?;
    let fg_count = binary.iter().filter(|&&x| x == 1).count();
    if fg_count < 20 {
        return None;
    }
    let (target_w, target_h) = (50usize, 68usize);
    let mut resized_binary = [0u8; 50 * 68];
    overmax_cv::resize_binary_nearest_into(&binary, w, h, &mut resized_binary, target_w, target_h);

    match_best_template(
        &resized_binary,
        target_w,
        target_h,
        super::result_mode::RESULT_MODE_TEMPLATES
            .iter()
            .map(|t| (t.width, t.height, t.mask, t.mode)),
        0.75,
        |_| 0,
    )
}

/// 결과 화면 전용 난이도 패널 영역을 템플릿 매칭으로 감지합니다.
pub fn detect_result_difficulty(diff_img: &ImageView) -> Option<Difficulty> {
    let w = diff_img.width;
    let h = diff_img.height;
    if w * h == 0 {
        return None;
    }

    let region = diff_img.to_image_region();
    let (binary, _, _) = overmax_cv::binarize_by_global_contrast(
        &region.bgra,
        w,
        h,
        overmax_cv::LumaMethod::Average,
        1,
    )
    .ok()?;
    let fg_count = binary.iter().filter(|&&x| x == 1).count();
    if fg_count < 10 {
        return None;
    }
    let (target_w, target_h) = (90usize, 18usize);
    let mut resized_binary = [0u8; 90 * 18];
    overmax_cv::resize_binary_nearest_into(&binary, w, h, &mut resized_binary, target_w, target_h);

    match_best_template(
        &resized_binary,
        target_w,
        target_h,
        super::result_diff::RESULT_DIFF_TEMPLATES
            .iter()
            .map(|t| (t.width, t.height, t.mask, t.diff)),
        0.80,
        |_| 0,
    )
}

/// 오픈매치 결과 화면 전용 난이도 영역을 템플릿 매칭으로 감지합니다. (106x18 해상도 적용)
pub fn detect_openmatch_result_difficulty(diff_img: &ImageView) -> Option<Difficulty> {
    let w = diff_img.width;
    let h = diff_img.height;
    if w * h == 0 {
        return None;
    }

    let region = diff_img.to_image_region();
    let binary = overmax_cv::adaptive_threshold_bradley_roth(
        &region.bgra,
        w,
        h,
        overmax_cv::LumaMethod::Average,
        80,
        0.03,
        1,
    );
    let (target_w, target_h) = (106usize, 18usize);
    let mut resized_binary = [0u8; 106 * 18];
    overmax_cv::resize_binary_nearest_into(&binary, w, h, &mut resized_binary, target_w, target_h);

    match_best_template(
        &resized_binary,
        target_w,
        target_h,
        super::result_diff::RESULT_DIFF_OPEN_TEMPLATES
            .iter()
            .map(|t| (t.width, t.height, t.mask, t.diff)),
        0.80,
        |val| match val {
            Difficulty::NM => 15,
            Difficulty::HD => 35,
            Difficulty::MX => 0,
            Difficulty::SC => 55,
        },
    )
}

fn match_digits_template(
    img: &ImageView,
    cv_templates: &[overmax_cv::CvTemplate],
) -> Result<(String, Vec<u8>, u8, u8), String> {
    let w = img.width;
    let h = img.height;

    let region = img.to_image_region();
    let (binary, threshold, max_y) = overmax_cv::binarize_by_global_contrast(
        &region.bgra,
        w,
        h,
        overmax_cv::LumaMethod::Average,
        255,
    )
    .map_err(|e| e.to_string())?;

    let segments = overmax_cv::segment_characters(&binary, w, h).map_err(|e| e.to_string())?;

    let mut matched_str = String::with_capacity(segments.len());
    let mut char_bin = Vec::with_capacity(32 * h);
    for &(x1, x2) in &segments {
        let char_w = x2 - x1;
        let char_h = h;
        char_bin.resize(char_w * char_h, 0);
        for y in 0..char_h {
            for x in 0..char_w {
                char_bin[y * char_w + x] = binary[y * w + (x1 + x)];
            }
        }

        if let Ok(Some((ch, _score))) =
            overmax_cv::match_character(&char_bin, char_w, char_h, cv_templates)
        {
            if ch.is_ascii_digit() || ch == '.' || ch == '%' {
                matched_str.push(ch);
            }
        } else {
            matched_str.push('?');
        }
    }

    Ok((matched_str, binary, threshold, max_y))
}

fn match_best_template<T: Copy + std::fmt::Display>(
    resized_binary: &[u8],
    target_w: usize,
    target_h: usize,
    templates: impl IntoIterator<Item = (usize, usize, &'static [u8], T)>,
    min_score: f32,
    safe_x_calc: impl Fn(T) -> usize,
) -> Option<T> {
    let mut best_score = 0.0f32;
    let mut best_val: Option<T> = None;
    let mut max_candidate_score = 0.0f32;
    let mut max_candidate_val: Option<T> = None;
    let compare_total = target_w * target_h;
    if compare_total == 0 {
        return None;
    }

    for (w, h, mask, val) in templates {
        if w != target_w || h != target_h || mask.len() < compare_total {
            continue;
        }
        let safe_x = safe_x_calc(val);
        let mut matches = 0usize;
        for dy in 0..target_h {
            let row_offset = dy * target_w;
            for dx in 0..target_w {
                let i = row_offset + dx;
                if dx < safe_x || resized_binary[i] == mask[i] {
                    matches += 1;
                }
            }
        }
        let score = matches as f32 / compare_total as f32;
        if score > max_candidate_score {
            max_candidate_score = score;
            max_candidate_val = Some(val);
        }
        if score > min_score && score > best_score {
            best_score = score;
            best_val = Some(val);
        }
    }

    if best_val.is_none() {
        if let Some(cand) = max_candidate_val {
            debug_println!(
                "      [Debug Result Mode/Diff] Match failed (min_score: {}). Best candidate was '{}' with score {:.3}",
                min_score, cand, max_candidate_score
            );
        } else {
            debug_println!(
                "      [Debug Result Mode/Diff] Match failed (min_score: {}). No candidates matched size",
                min_score
            );
        }
    }
    best_val
}

fn parse_rate_text(text: &str) -> Option<f32> {
    let mut cleaned = String::new();
    let mut dot_seen = false;
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            cleaned.push(ch);
        } else if ch == '.' && !dot_seen {
            cleaned.push(ch);
            dot_seen = true;
        }
    }
    let mut value = cleaned.parse::<f32>().ok()?;

    if !dot_seen && value >= (crate::detector::play_state::MIN_VALID_RATE * 100.0) {
        value /= 100.0;
    }

    value = (value * 100.0).floor() / 100.0;

    (crate::detector::play_state::MIN_VALID_RATE..=100.0)
        .contains(&value)
        .then_some(value)
}

#[cfg(test)]
mod tests {
    use super::parse_rate_text;

    #[test]
    fn parses_rate_text_like_python_path() {
        assert_eq!(parse_rate_text("99.43%"), Some(99.43));
        assert_eq!(parse_rate_text("100.00"), Some(100.0));
        assert_eq!(parse_rate_text("101.0"), None);
        assert_eq!(parse_rate_text("9412%"), Some(94.12));
        assert_eq!(parse_rate_text("10000"), Some(100.0));
        assert_eq!(parse_rate_text("99.289%"), Some(99.28));
        assert_eq!(parse_rate_text("99.281"), Some(99.28));
        assert_eq!(parse_rate_text("99.280"), Some(99.28));
    }

    #[test]
    fn matches_digit_templates_accurately() {
        for &template_set in &[
            crate::detector::templates::digit::DIGIT_TEMPLATES_SCORE,
            crate::detector::templates::digit::DIGIT_TEMPLATES_RATE,
        ] {
            for t in template_set {
                let res = overmax_cv::match_character(t.mask, t.width, t.height, template_set);
                assert!(res.is_ok(), "Failed to call match_character: '{}'", t.char_val);
                let matched = res.unwrap();
                assert!(matched.is_some(), "Failed to match digit template: '{}'", t.char_val);
                let (matched_char, score) = matched.unwrap();
                assert_eq!(matched_char, t.char_val, "Mismatched char for template '{}'", t.char_val);
                assert!((score - 1.0).abs() < 1e-4, "Score for perfect template should be 1.0");
            }
        }
    }
}
