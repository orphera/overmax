use crate::capture::frame_utils::ImageRegion;
use overmax_core::{Difficulty, Mode};
use std::fmt;

#[derive(Clone, Default, PartialEq)]
pub struct RateTelemetry {
    pub rate_text: String,
    pub threshold: u8,
    pub bg_mean: f32,
    pub use_invert: bool,
    pub image_pixels: Vec<u8>,
    pub image_width: usize,
    pub image_height: usize,
}

impl fmt::Debug for RateTelemetry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RateTelemetry")
            .field("rate_text", &self.rate_text)
            .field("threshold", &self.threshold)
            .field("bg_mean", &self.bg_mean)
            .field("use_invert", &self.use_invert)
            .field("image_pixels_len", &self.image_pixels.len())
            .field("image_width", &self.image_width)
            .field("image_height", &self.image_height)
            .finish()
    }
}

/// Rate 영역을 Pure Rust CV 템플릿 매칭으로 감지합니다.
pub fn detect_rate(rate: &ImageRegion) -> (Option<f32>, String, Option<RateTelemetry>) {
    detect_rate_with_min(rate, crate::detector::play_state::MIN_VALID_RATE)
}

pub fn detect_result_rate(rate: &ImageRegion) -> (Option<f32>, String, Option<RateTelemetry>) {
    detect_rate_with_min(rate, 0.01)
}

fn detect_rate_with_min(
    rate: &ImageRegion,
    min_valid_rate: f32,
) -> (Option<f32>, String, Option<RateTelemetry>) {
    let cv_templates = get_digit_templates();
    let matched = match match_digits_template(rate, &cv_templates) {
        Ok(m) => m,
        Err(_) => return (None, String::new(), None),
    };
    let (matched_str, binary, threshold, max_y) = matched;

    // 템플릿 매칭 결과에서 ?를 제거하고 파싱 시도
    let rate_val = (!matched_str.is_empty())
        .then(|| matched_str.replace('?', ""))
        .and_then(|clean_str| parse_rate_text(&clean_str, min_valid_rate));

    if let Some(val) = rate_val {
        let telemetry = RateTelemetry {
            rate_text: matched_str.clone(),
            threshold,
            bg_mean: max_y as f32,
            use_invert: false,
            image_pixels: binary,
            image_width: rate.width as usize,
            image_height: rate.height as usize,
        };
        (Some(val), matched_str, Some(telemetry))
    } else {
        (None, String::new(), None)
    }
}

/// Score 영역을 템플릿 매칭을 통해 정수로 파싱합니다.
pub fn detect_score(score: &ImageRegion) -> Option<u32> {
    let cv_templates = get_digit_templates();
    match match_digits_template(score, &cv_templates) {
        Ok((matched_str, _, _, _)) => {
            let parsed = parse_score_text(&matched_str)
                .filter(|_| !matched_str.contains('?'))
                .or_else(|| parse_score_rate_prefix(&matched_str))
                .or_else(|| parse_score_dot_as_one(&matched_str));
            if parsed.is_none() {
                println!(
                    "      [Debug Score] Template matching failed/invalid. Matched String: '{}', Parsed: {:?}",
                    matched_str, parsed
                );
                None
            } else {
                parsed
            }
        }
        Err(e) => {
            println!(
                "      [Debug Score] match_digits_template failed with error: {}",
                e
            );
            None
        }
    }
}

/// Freestyle 결과창 모드 영역을 템플릿 매칭으로 판독합니다.
pub fn detect_freestyle_mode(mode_img: &ImageRegion) -> Option<Mode> {
    let w = mode_img.width as usize;
    let h = mode_img.height as usize;
    if w * h == 0 {
        return None;
    }

    let (binary, _, _) = match overmax_cv::binarize_by_global_contrast(
        &mode_img.bgra,
        w,
        h,
        overmax_cv::LumaMethod::Average,
        1,
    ) {
        Ok(b) => b,
        Err(_) => return None,
    };
    let fg_count = binary.iter().filter(|&&x| x == 1).count();
    if fg_count < 20 {
        return None;
    }
    let t_infos: Vec<MatchTemplateInfo<Mode>> =
        super::result_mode::RESULT_MODE_TEMPLATES
            .iter()
            .map(|t| MatchTemplateInfo {
                width: t.width,
                height: t.height,
                mask: t.mask,
                value: t.mode,
            })
            .collect();

    match_best_template(&binary, w, h, &t_infos, 0.75, |_| 0)
}

/// 결과 화면 전용 난이도 패널 영역을 템플릿 매칭으로 감지합니다.
pub fn detect_result_difficulty(diff_img: &ImageRegion) -> Option<Difficulty> {
    let w = diff_img.width as usize;
    let h = diff_img.height as usize;
    if w * h == 0 {
        return None;
    }

    let (binary, _, _) = match overmax_cv::binarize_by_global_contrast(
        &diff_img.bgra,
        w,
        h,
        overmax_cv::LumaMethod::Average,
        1,
    ) {
        Ok(b) => b,
        Err(_) => return None,
    };
    let fg_count = binary.iter().filter(|&&x| x == 1).count();
    if fg_count < 10 {
        return None;
    }
    let first_foreground = (0..w).find(|&x| (0..h).any(|y| binary[y * w + x] != 0));
    let last_foreground = (0..w)
        .rev()
        .find(|&x| (0..h).any(|y| binary[y * w + x] != 0));
    let wide_text = first_foreground
        .zip(last_foreground)
        .is_some_and(|(first, last)| last - first + 1 > w / 2);

    let t_infos: Vec<MatchTemplateInfo<Difficulty>> =
        super::result_diff::RESULT_DIFF_TEMPLATES
            .iter()
            .filter(|t| {
                w >= 90
                    || !wide_text
                    || matches!(t.diff, Difficulty::NM | Difficulty::MX)
            })
            .map(|t| MatchTemplateInfo {
                width: t.width,
                height: t.height,
                mask: t.mask,
                value: t.diff,
            })
            .collect();

    let min_score = if w < 90 { 0.70 } else { 0.80 };
    match_best_template(&binary, w, h, &t_infos, min_score, |_| 0)
}

/// 오픈매치 결과 화면 전용 난이도 영역을 템플릿 매칭으로 감지합니다. (106x18 해상도 적용)
pub fn detect_openmatch_result_difficulty(diff_img: &ImageRegion) -> Option<Difficulty> {
    let w = diff_img.width as usize;
    let h = diff_img.height as usize;
    if w * h == 0 {
        return None;
    }

    let binary = overmax_cv::adaptive_threshold_bradley_roth(
        &diff_img.bgra,
        w,
        h,
        overmax_cv::LumaMethod::Average,
        80,
        0.03,
        1,
    );
    let t_infos: Vec<MatchTemplateInfo<Difficulty>> =
        super::result_diff::RESULT_DIFF_OPEN_TEMPLATES
            .iter()
            .map(|t| MatchTemplateInfo {
                width: t.width,
                height: t.height,
                mask: t.mask,
                value: t.diff,
            })
            .collect();

    match_best_template(
        &binary,
        w,
        h,
        &t_infos,
        0.80,
        |val| match val {
            Difficulty::NM => 15 * w / 106,
            Difficulty::HD => 35 * w / 106,
            Difficulty::MX => 0,
            Difficulty::SC => 55 * w / 106,
        },
    )
}

fn match_digits_template(
    img: &ImageRegion,
    cv_templates: &[overmax_cv::CvTemplate],
) -> Result<(String, Vec<u8>, u8, u8), String> {
    let w = img.width as usize;
    let h = img.height as usize;

    let (binary, threshold, max_y) = overmax_cv::binarize_by_global_contrast(
        &img.bgra,
        w,
        h,
        overmax_cv::LumaMethod::Average,
        255,
    )
    .map_err(|e| e.to_string())?;

    let segments = overmax_cv::segment_characters(&binary, w, h).map_err(|e| e.to_string())?;

    let mut matched_str = String::new();
    for &(x1, x2) in &segments {
        let char_w = x2 - x1;
        let char_h = h;
        let mut char_bin = vec![0u8; char_w * char_h];
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

fn get_digit_templates() -> Vec<overmax_cv::CvTemplate<'static>> {
    super::digit::DIGIT_TEMPLATES
        .iter()
        .map(|t| overmax_cv::CvTemplate {
            char_val: t.char_val,
            width: t.width,
            height: t.height,
            mask: t.mask,
        })
        .collect()
}

struct MatchTemplateInfo<'a, T> {
    width: usize,
    height: usize,
    mask: &'a [u8],
    value: T,
}

fn match_best_template<T: Copy + std::fmt::Display>(
    resized_binary: &[u8],
    target_w: usize,
    target_h: usize,
    templates: &[MatchTemplateInfo<'_, T>],
    min_score: f32,
    safe_x_calc: impl Fn(T) -> usize,
) -> Option<T> {
    let mut best_score = 0.0f32;
    let mut best_val: Option<T> = None;

    for t in templates {
        let resized_template;
        let template = if t.width == target_w && t.height == target_h {
            t.mask
        } else {
            resized_template = overmax_cv::resize_binary_nearest(
                t.mask,
                t.width,
                t.height,
                target_w,
                target_h,
            );
            &resized_template
        };
        let safe_x = safe_x_calc(t.value).min(target_w);
        let compare_total = (target_w - safe_x) * target_h;
        if compare_total == 0 {
            continue;
        }
        let mut matches = 0usize;
        for dy in 0..target_h {
            for dx in safe_x..target_w {
                let i = dy * target_w + dx;
                if resized_binary[i] == template[i] {
                    matches += 1;
                }
            }
        }
        let score = matches as f32 / compare_total as f32;
        if score > min_score && score > best_score {
            best_score = score;
            best_val = Some(t.value);
        }
    }
    if best_val.is_none() {
        let mut max_candidate_score = 0.0f32;
        let mut max_candidate_val: Option<T> = None;
        for t in templates {
            let resized_template;
            let template = if t.width == target_w && t.height == target_h {
                t.mask
            } else {
                resized_template = overmax_cv::resize_binary_nearest(
                    t.mask,
                    t.width,
                    t.height,
                    target_w,
                    target_h,
                );
                &resized_template
            };
            let safe_x = safe_x_calc(t.value).min(target_w);
            let compare_total = (target_w - safe_x) * target_h;
            if compare_total == 0 {
                continue;
            }
            let mut matches = 0usize;
            for dy in 0..target_h {
                for dx in safe_x..target_w {
                    let i = dy * target_w + dx;
                    if resized_binary[i] == template[i] {
                        matches += 1;
                    }
                }
            }
            let score = matches as f32 / compare_total as f32;
            if score > max_candidate_score {
                max_candidate_score = score;
                max_candidate_val = Some(t.value);
            }
        }
        if let Some(cand) = max_candidate_val {
            println!(
                "      [Debug Result Mode/Diff] Match failed (min_score: {}). Best candidate was '{}' with score {:.3}",
                min_score, cand, max_candidate_score
            );
        } else {
            println!(
                "      [Debug Result Mode/Diff] Match failed (min_score: {}). No candidates matched size",
                min_score
            );
        }
    }
    best_val
}

fn parse_score_text(text: &str) -> Option<u32> {
    let clean = text
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>();
    if clean.len() != 6 && clean.len() != 7 {
        return None;
    }
    clean.parse::<u32>().ok()
}

fn parse_score_rate_prefix(text: &str) -> Option<u32> {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() != 6 || !chars[..4].iter().all(char::is_ascii_digit) {
        return None;
    }
    chars[..4]
        .iter()
        .collect::<String>()
        .parse::<u32>()
        .ok()
        .map(|prefix| prefix * 100)
}

fn parse_score_dot_as_one(text: &str) -> Option<u32> {
    (text.len() == 6 && text.chars().filter(|&ch| ch == '.').count() == 1)
        .then(|| text.replace('.', "1"))
        .filter(|value| value.chars().all(|ch| ch.is_ascii_digit()))
        .and_then(|value| value.parse::<u32>().ok())
}

fn parse_rate_text(text: &str, min_valid_rate: f32) -> Option<f32> {
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

    if !dot_seen && value >= min_valid_rate * 100.0 {
        value /= 100.0;
    }

    value = (value * 100.0).floor() / 100.0;

    (min_valid_rate..=100.0).contains(&value).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::{
        match_best_template, parse_rate_text, parse_score_dot_as_one, parse_score_rate_prefix,
        parse_score_text, MatchTemplateInfo,
    };

    #[test]
    fn ignored_template_prefix_does_not_inflate_match_score() {
        let target = [0; 10];
        let mut mask = [0; 10];
        mask[9] = 1;
        let templates = [MatchTemplateInfo {
            width: 10,
            height: 1,
            mask: &mask,
            value: "SC",
        }];

        assert_eq!(
            match_best_template(&target, 10, 1, &templates, 0.80, |_| 6),
            None
        );
    }

    #[test]
    fn score_rate_prefix_ignores_only_uncertain_tail_digits() {
        assert_eq!(parse_score_rate_prefix("98545."), Some(985_400));
        assert_eq!(parse_score_rate_prefix("9990.4"), Some(999_000));
        assert_eq!(parse_score_rate_prefix("?990.4"), None);
    }

    #[test]
    fn score_recovers_one_misread_as_dot() {
        assert_eq!(parse_score_dot_as_one("6.9874"), Some(619_874));
        assert_eq!(parse_score_dot_as_one("993.64"), Some(993_164));
        assert_eq!(parse_score_dot_as_one("99.3.4"), None);
    }

    #[test]
    fn parses_score_text_correctly() {
        assert_eq!(parse_score_text("999,800"), Some(999800));
        assert_eq!(parse_score_text("1,000,000"), Some(1000000));
        assert_eq!(parse_score_text("abc"), None);
    }

    #[test]
    fn parses_rate_text_like_python_path() {
        let min = crate::detector::play_state::MIN_VALID_RATE;
        assert_eq!(parse_rate_text("99.43%", min), Some(99.43));
        assert_eq!(parse_rate_text("100.00", min), Some(100.0));
        assert_eq!(parse_rate_text("101.0", min), None);
        assert_eq!(parse_rate_text("9412%", min), Some(94.12));
        assert_eq!(parse_rate_text("10000", min), Some(100.0));
        assert_eq!(parse_rate_text("99.289%", min), Some(99.28));
        assert_eq!(parse_rate_text("99.281", min), Some(99.28));
        assert_eq!(parse_rate_text("99.280", min), Some(99.28));
        assert_eq!(parse_rate_text("48.26%", 0.01), Some(48.26));
        assert_eq!(parse_rate_text("6198", 0.01), Some(61.98));
        assert_eq!(parse_rate_text("0.00%", 0.01), None);
    }
}
