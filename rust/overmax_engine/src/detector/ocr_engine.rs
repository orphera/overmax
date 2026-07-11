use crate::capture::frame_utils::ImageRegion;
use overmax_core::SceneType;
use std::fmt;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux::OcrEngine as PlatformOcrEngine;
#[cfg(target_os = "windows")]
use windows::OcrEngine as PlatformOcrEngine;

#[derive(Clone, Default, PartialEq)]
pub struct OcrTelemetry {
    pub rate_text: String,
    pub threshold: u8,
    pub bg_mean: f32,
    pub use_invert: bool,
    pub image_pixels: Vec<u8>,
    pub image_width: usize,
    pub image_height: usize,
}

impl From<(String, overmax_cv::OcrPreprocessResult)> for OcrTelemetry {
    fn from((rate_text, result): (String, overmax_cv::OcrPreprocessResult)) -> Self {
        Self {
            rate_text,
            threshold: result.threshold,
            bg_mean: result.bg_mean,
            use_invert: result.use_invert,
            image_pixels: result.padded_pixels,
            image_width: result.padded_width,
            image_height: result.padded_height,
        }
    }
}

impl fmt::Debug for OcrTelemetry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OcrTelemetry")
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

pub struct OcrDetector {
    engine: PlatformOcrEngine,
}

impl Default for OcrDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrDetector {
    pub fn new() -> Self {
        Self {
            engine: PlatformOcrEngine::new(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.engine.is_available()
    }
    /// 상단 로고 영역을 단일 패스(Color)로 감지하여 씬을 판별합니다.
    pub fn detect_logo(&self, logo: &ImageRegion) -> (SceneType, String, String) {
        // 단일 패스: Color OCR (성능 향상을 위한 1-pass 단일화)
        if let Ok(t) = self.engine.recognize_logo_color(logo) {
            if let Some((scene, _)) = match_logo_scene(&t) {
                return (scene, t, scene_label(scene));
            }
        }
        (SceneType::Unknown, String::new(), "UNKNOWN".to_string())
    }

    fn attempt_rate_ocr(
        &self,
        rate: &ImageRegion,
        color: bool,
        force_invert: bool,
    ) -> Option<(Option<f32>, String, OcrTelemetry)> {
        let res = if color {
            self.engine.recognize_color_with_telemetry(rate)
        } else {
            self.engine.recognize_with_telemetry(rate, force_invert, false)
        };
        let (txt, preprocess_res) = res.ok()?;
        let val = parse_rate_text(&txt);
        let telemetry = OcrTelemetry::from((txt.clone(), preprocess_res));
        Some((val, txt, telemetry))
    }

    /// Rate 영역을 감지합니다.
    ///
    /// # 가드레일 (CRITICAL GUARDRAIL)
    /// 인게임 실시간 성능 보호를 위해 **반드시 단일 패스(1-Pass) 실행만 수행**해야 합니다.
    /// 절대로 3-pass 등의 다중 패스 루프를 이곳에 재도입하지 마십시오. (AGENTS.md 및 CONTEXT.md 제약 조건)
    /// OCR 인식 실패나 오작동 대응은 `PlayStateDetector`의 히스토리 버퍼(`stable_raw` 다수결)를 통해 해결합니다.
    pub fn detect_rate(&self, rate: &ImageRegion) -> (Option<f32>, String, Option<OcrTelemetry>) {
        let cv_templates = get_digit_templates();
        let matched = match match_digits_template(rate, &cv_templates) {
            Ok(m) => m,
            Err(_) => return (None, String::new(), None),
        };
        let (matched_str, binary, threshold, max_y) = matched;

        // 우선 템플릿 매칭 결과에서 ?를 제거하고 파싱을 시도
        let mut rate_val = None;
        let mut template_success = false;
        if !matched_str.is_empty() {
            let clean_str = matched_str.replace('?', "");
            if let Some(val) = parse_rate_text(&clean_str) {
                rate_val = Some(val);
                template_success = true;
            }
        }

        if template_success {
            let telemetry = OcrTelemetry {
                rate_text: matched_str.clone(),
                threshold,
                bg_mean: max_y as f32,
                use_invert: false,
                image_pixels: binary,
                image_width: rate.width as usize,
                image_height: rate.height as usize,
            };
            return (rate_val, matched_str, Some(telemetry));
        }

        // 템플릿 매칭 실패 시 Windows OCR fallback으로 전환
        if let Some((val, txt, tel)) = self.attempt_rate_ocr(rate, true, false) {
            (val, txt, Some(tel))
        } else {
            (None, String::new(), None)
        }
    }

    /// Score 영역을 템플릿 매칭 또는 OCR을 통해 정수로 파싱합니다.
    pub fn detect_score(&self, score: &ImageRegion) -> Option<u32> {
        let cv_templates = get_digit_templates();
        let matched = match match_digits_template(score, &cv_templates) {
            Ok(m) => m,
            Err(_) => return None,
        };
        let (matched_str, _binary, _threshold, _max_y) = matched;

        // 실패나 오독이 포함되면 Windows OCR로 즉각 안전 폴백
        if matched_str.is_empty() || matched_str.contains('?') {
            return if let Ok(text) = self.engine.recognize_logo_color(score) {
                parse_score_text(&text)
            } else {
                None
            };
        }

        parse_score_text(&matched_str)
    }

    /// 결과창 뱃지 이미지로부터 모드(4B~8B)와 난이도(NM~SC)를 감지합니다.
    /// Freestyle 결과창 모드 영역을 템플릿 매칭으로 판독합니다.
    pub fn detect_freestyle_mode(&self, mode_img: &ImageRegion) -> Option<String> {
        let w = mode_img.width as usize;
        let h = mode_img.height as usize;
        if w * h == 0 {
            return None;
        }

        let binary = overmax_cv::adaptive_threshold_bradley_roth(
            &mode_img.bgra,
            w,
            h,
            overmax_cv::LumaMethod::Average,
            16,
            0.15,
            1,
        );
        let (target_w, target_h) = (50usize, 68usize);
        let resized_binary = resize_binary(&binary, w, h, target_w, target_h);

        let t_infos: Vec<MatchTemplateInfo> = crate::detector::templates::result_mode::RESULT_MODE_TEMPLATES
            .iter()
            .map(|t| MatchTemplateInfo {
                width: t.width,
                height: t.height,
                mask: t.mask,
                label: t.mode_label,
            })
            .collect();

        match_best_template(&resized_binary, target_w, target_h, &t_infos, 0.80, |_| 0)
    }

    /// 결과 화면 전용 난이도 패널 영역을 템플릿 매칭으로 감지합니다.
    pub fn detect_result_difficulty(&self, diff_img: &ImageRegion) -> Option<String> {
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
        let (target_w, target_h) = (90usize, 18usize);
        let resized_binary = resize_binary(&binary, w, h, target_w, target_h);

        let t_infos: Vec<MatchTemplateInfo> = crate::detector::templates::result_diff::RESULT_DIFF_TEMPLATES
            .iter()
            .map(|t| MatchTemplateInfo {
                width: t.width,
                height: t.height,
                mask: t.mask,
                label: t.name,
            })
            .collect();

        match_best_template(&resized_binary, target_w, target_h, &t_infos, 0.80, |_| 0)
    }

    /// 오픈매치 결과 화면 전용 난이도 영역을 템플릿 매칭으로 감지합니다. (106x18 해상도 적용)
    pub fn detect_openmatch_result_difficulty(&self, diff_img: &ImageRegion) -> Option<String> {
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
        let (target_w, target_h) = (106usize, 18usize);
        let resized_binary = resize_binary(&binary, w, h, target_w, target_h);

        let t_infos: Vec<MatchTemplateInfo> = crate::detector::templates::result_diff::RESULT_DIFF_OPEN_TEMPLATES
            .iter()
            .map(|t| MatchTemplateInfo {
                width: t.width,
                height: t.height,
                mask: t.mask,
                label: t.name,
            })
            .collect();

        match_best_template(&resized_binary, target_w, target_h, &t_infos, 0.80, |label| {
            match label {
                "NM" => 15,
                "HD" => 35,
                "MX" => 0,
                "SC" => 55,
                _ => 0,
            }
        })
    }

    pub fn recognize_text_color(&self, region: &ImageRegion) -> Option<String> {
        self.engine.recognize_logo_color(region).ok()
    }

    pub fn recognize_text_binarized(&self, region: &ImageRegion, force_invert: bool) -> Option<String> {
        self.engine.recognize_logo(region, force_invert, true).ok()
    }

    /// 텍스트 내에 유효한 버튼 모드 키워드가 포함되어 있는지 판단합니다.
    pub fn contains_mode_keyword(&self, text: &str) -> bool {
        let norm = text.to_lowercase();
        norm.contains("4b") || norm.contains("5b") || norm.contains("6b") || norm.contains("8b")
    }

    /// 텍스트에서 매칭되는 버튼 모드를 문자열로 파싱합니다.
    pub fn parse_mode_from_text(&self, text: &str) -> Option<String> {
        let norm = text.to_lowercase();
        if norm.contains("4b") || norm.contains('4') { Some("4B".to_string()) }
        else if norm.contains("5b") || norm.contains('5') { Some("5B".to_string()) }
        else if norm.contains("6b") || norm.contains('6') { Some("6B".to_string()) }
        else if norm.contains("8b") || norm.contains('8') { Some("8B".to_string()) }
        else { None }
    }

    pub fn recognize_text_all_passes(&self, region: &ImageRegion) -> Option<String> {
        // 단일 패스: Color OCR (인게임 성능 및 제약 조건을 준수하기 위해 단일 패스로 강제 유지합니다.)
        if let Ok(t) = self.engine.recognize_logo_color(region) {
            if !t.trim().is_empty() {
                return Some(t);
            }
        }
        None
    }


}

fn match_logo_scene(text: &str) -> Option<(SceneType, String)> {
    let normalized = normalize_alnum(text).to_lowercase();
    if normalized.contains("buttontunes") || normalized.contains("button") {
        Some((SceneType::ResultFreestyle, normalized))
    } else if normalized.contains("freestyle") {
        Some((SceneType::Freestyle, normalized))
    } else if normalized.contains("online") {
        if normalized.contains("open") || normalized.contains("openmatch") {
            Some((SceneType::OpenMatch, normalized))
        } else if normalized.contains("ladder") || normalized.contains("laddermatch") {
            Some((SceneType::LadderMatch, normalized))
        } else {
            Some((SceneType::Online, normalized))
        }
    } else if normalized.contains("tunes") || normalized.contains("tune") {
        let has_number = normalized.chars().any(|c| c.is_ascii_digit());
        if has_number {
            Some((SceneType::ResultOpen2, normalized))
        } else {
            None
        }
    } else {
        None
    }
}

fn scene_label(scene: SceneType) -> String {
    match scene {
        SceneType::Freestyle => "FREESTYLE".to_string(),
        SceneType::Online => "ONLINE".to_string(),
        SceneType::OpenMatch => "OPEN_MATCH".to_string(),
        SceneType::LadderMatch => "LADDER_MATCH".to_string(),
        SceneType::ResultFreestyle => "RESULT_FREESTYLE".to_string(),
        SceneType::ResultOpen3 => "RESULT_OPEN3".to_string(),
        SceneType::ResultOpen2 => "RESULT_OPEN2".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}

fn match_digits_template(
    img: &ImageRegion,
    cv_templates: &[overmax_cv::CvTemplate],
) -> Result<(String, Vec<u8>, u8, u8), String> {
    let w = img.width as usize;
    let h = img.height as usize;

    // 1. 고휘도 이진화 전처리
    let (binary, threshold, max_y) = overmax_cv::binarize_by_global_contrast(
        &img.bgra,
        w,
        h,
        overmax_cv::LumaMethod::Average,
        255,
    ).map_err(|e| e.to_string())?;

    // 2. 수직 투영 분할
    let segments = overmax_cv::segment_characters(&binary, w, h)
        .map_err(|e| e.to_string())?;

    // 3. 템플릿 매칭 판독
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

        if let Ok(Some((ch, _score))) = overmax_cv::match_character(&char_bin, char_w, char_h, cv_templates) {
            if ch.is_ascii_digit() || ch == '.' || ch == '%' {
                matched_str.push(ch);
            }
        } else {
            matched_str.push('?');
        }
    }

    Ok((matched_str, binary, threshold, max_y))
}

fn resize_binary(
    binary: &[u8],
    w: usize,
    h: usize,
    target_w: usize,
    target_h: usize,
) -> Vec<u8> {
    if w == target_w && h == target_h {
        return binary.to_vec();
    }
    let mut dst = vec![0u8; target_w * target_h];
    for dy in 0..target_h {
        let sy = (dy * h) / target_h;
        let sy_clamped = sy.min(h - 1);
        for dx in 0..target_w {
            let sx = (dx * w) / target_w;
            let sx_clamped = sx.min(w - 1);
            dst[dy * target_w + dx] = binary[sy_clamped * w + sx_clamped];
        }
    }
    dst
}

fn get_digit_templates() -> Vec<overmax_cv::CvTemplate<'static>> {
    crate::detector::templates::digit::DIGIT_TEMPLATES
        .iter()
        .map(|t| overmax_cv::CvTemplate {
            char_val: t.char_val,
            width: t.width,
            height: t.height,
            mask: t.mask,
        })
        .collect()
}

struct MatchTemplateInfo<'a> {
    width: usize,
    height: usize,
    mask: &'a [u8],
    label: &'a str,
}

fn match_best_template(
    resized_binary: &[u8],
    target_w: usize,
    target_h: usize,
    templates: &[MatchTemplateInfo],
    min_score: f32,
    safe_x_calc: impl Fn(&str) -> usize,
) -> Option<String> {
    let mut best_score = 0.0f32;
    let mut best_label: Option<String> = None;
    let compare_total = target_w * target_h;

    for t in templates {
        if t.width != target_w || t.height != target_h {
            continue;
        }
        let safe_x = safe_x_calc(t.label);
        let mut matches = 0usize;
        for dy in 0..target_h {
            for dx in 0..target_w {
                let i = dy * target_w + dx;
                if dx < safe_x {
                    matches += 1;
                } else if resized_binary[i] == t.mask[i] {
                    matches += 1;
                }
            }
        }
        let score = matches as f32 / compare_total as f32;
        if score > min_score && score > best_score {
            best_score = score;
            best_label = Some(t.label.to_string());
        }
    }
    best_label
}

fn parse_score_text(text: &str) -> Option<u32> {
    let clean = text.chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>();
    if clean.len() != 6 && clean.len() != 7 {
        return None;
    }
    clean.parse::<u32>().ok()
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

    // Windows OCR 오인식 대응:
    // "94.12%"를 "9412%"와 같이 소수점(.)을 누락하여 인식하는 경우가 존재합니다.
    // DJMAX RESPECT V의 Rate는 항상 소수점 둘째 자리까지 표기되므로, 
    // 문자열에 소수점이 감지되지 않았고 파싱 결과가 MIN_VALID_RATE(80.0%) 이상인 경우
    // 소수점 이하 2자리가 정수로 취급되었다고 가정하고 100.0으로 나누어 보정합니다.
    // 이를 통해 0.00%가 900% 노이즈로 튈 때 9.00% 등으로 오보정되어 통과하는 부작용을 예방합니다.
    if !dot_seen && value >= (crate::detector::play_state::MIN_VALID_RATE * 100.0) {
        value /= 100.0;
    }

    // 소수점 셋째 자리 이하 무조건 버림(Truncate) 보정 적용하여 반올림 차단
    value = (value * 100.0).floor() / 100.0;

    // 유효한 실시간 기록으로 처리할 수 있는 최소 범위(MIN_VALID_RATE = 80.0%) 이상인 경우만 유효값으로 반환하고,
    // 0.00%가 4.00% 또는 9.00% 노이즈로 완벽하게 잘못 오인식되는 수치 등은 스캔 시점에 원천 배제합니다.
    (crate::detector::play_state::MIN_VALID_RATE..=100.0).contains(&value).then_some(value)
}

fn normalize_alnum(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect()
}

#[allow(dead_code)]
fn is_logo_keyword_match(keyword: &str, normalized_ocr: &str) -> bool {
    if keyword.is_empty() || normalized_ocr.is_empty() {
        return false;
    }
    if normalized_ocr.contains(keyword) {
        return true;
    }

    let min_partial_len = keyword.len().min(6);
    for idx in 0..=keyword.len().saturating_sub(min_partial_len) {
        if normalized_ocr.contains(&keyword[idx..idx + min_partial_len]) {
            return true;
        }
    }
    sequence_ratio(keyword, normalized_ocr) >= 0.72
}

fn sequence_ratio(left: &str, right: &str) -> f32 {
    let lcs = lcs_len(left.as_bytes(), right.as_bytes()) as f32;
    2.0 * lcs / (left.len() + right.len()) as f32
}

fn lcs_len(left: &[u8], right: &[u8]) -> usize {
    let mut prev = vec![0; right.len() + 1];
    let mut curr = vec![0; right.len() + 1];
    for &a in left {
        for (idx, &b) in right.iter().enumerate() {
            curr[idx + 1] = if a == b {
                prev[idx] + 1
            } else {
                curr[idx].max(prev[idx + 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[right.len()]
}

#[cfg(test)]
mod tests {
    use super::{is_logo_keyword_match, normalize_alnum, parse_rate_text, parse_score_text};

    #[test]
    fn parses_score_text_correctly() {
        assert_eq!(parse_score_text("999,800"), Some(999800));
        assert_eq!(parse_score_text("1,000,000"), Some(1000000));
        assert_eq!(parse_score_text("abc"), None);
    }

    #[test]
    #[ignore]
    fn test_color_vs_grayscale_ocr() {
        use image::GenericImageView;
        use crate::capture::frame_utils::crop_roi;
        use crate::detector::roi::RoiManager;
        use overmax_core::SceneType;

        let scratch_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch");
        let test_cases = [
            ("hd_test_1.png", SceneType::ResultFreestyle),
            ("hd_test_2.png", SceneType::ResultFreestyle),
            ("hd_test_3.png", SceneType::ResultFreestyle),
            ("hd_test_4.png", SceneType::ResultFreestyle),
            ("hd_test_5.png", SceneType::ResultFreestyle),
            ("hd_test_2p_1.png", SceneType::ResultOpen2),
            ("hd_test_2p_2.png", SceneType::ResultOpen2),
        ];

        let detector = super::OcrDetector::new();
        println!("\n=== OCR COLOR VS GRAYSCALE COMPARISON ===");

        for (img_name, scene) in &test_cases {
            let path = scratch_dir.join(img_name);
            if !path.exists() {
                continue;
            }
            let img = image::ImageReader::open(&path).expect("Failed to open file")
                .with_guessed_format().expect("Failed to guess format")
                .decode().expect("Failed to decode image");
            let (w, h) = img.dimensions();
            let mut bgra = vec![0u8; (w * h * 4) as usize];
            for (x, y, pixel) in img.pixels() {
                let idx = ((y * w + x) * 4) as usize;
                bgra[idx] = pixel[2];     // B
                bgra[idx + 1] = pixel[1]; // G
                bgra[idx + 2] = pixel[0]; // R
                bgra[idx + 3] = pixel[3]; // A
            }
            let frame = crate::capture::frame::CapturedFrame {
                width: w as i32,
                height: h as i32,
                bgra,
            };

            let mut rois = RoiManager::new(w as i32, h as i32);
            rois.set_scene(*scene);

            println!("IMAGE: {}", img_name);

            // Rate OCR 비교
            if let Some(rate_roi) = rois.get_roi("rate") {
                if let Some(rate_img) = crop_roi(&frame, rate_roi) {
                    let color_res = detector.attempt_rate_ocr(&rate_img, true, false);
                    let gray_res = detector.attempt_rate_ocr(&rate_img, false, false);
                    
                    println!("  [Rate-Color]     Parsed: {:?}, Text: '{}'", 
                        color_res.as_ref().and_then(|r| r.0),
                        color_res.as_ref().map(|r| r.1.trim()).unwrap_or("FAILED")
                    );
                    println!("  [Rate-Grayscale] Parsed: {:?}, Text: '{}'", 
                        gray_res.as_ref().and_then(|r| r.0),
                        gray_res.as_ref().map(|r| r.1.trim()).unwrap_or("FAILED")
                    );
                }
            }

            // Score OCR 비교
            if let Some(score_roi) = rois.get_roi("score") {
                if let Some(score_img) = crop_roi(&frame, score_roi) {
                    let color_txt = detector.engine.recognize_logo_color(&score_img).unwrap_or_default();
                    let gray_txt = detector.engine.recognize_logo(&score_img, false, false).unwrap_or_default();
                    
                    let color_score = super::parse_score_text(&color_txt);
                    let gray_score = super::parse_score_text(&gray_txt);

                    println!("  [Score-Color]    Parsed: {:?}, Text: '{}'", color_score, color_txt.trim());
                    println!("  [Score-Gray]     Parsed: {:?}, Text: '{}'", gray_score, gray_txt.trim());
                }
            }
        }
    }

    #[test]
    fn parses_rate_text_like_python_path() {
        assert_eq!(parse_rate_text("99.43%"), Some(99.43));
        assert_eq!(parse_rate_text("100.00"), Some(100.0));
        assert_eq!(parse_rate_text("101.0"), None);
        // 소수점 누락 보정 테스트
        assert_eq!(parse_rate_text("9412%"), Some(94.12));
        assert_eq!(parse_rate_text("10000"), Some(100.0));
        // 소수점 셋째 자리 버림(Truncate) 보정 테스트
        assert_eq!(parse_rate_text("99.289%"), Some(99.28));
        assert_eq!(parse_rate_text("99.281"), Some(99.28));
        assert_eq!(parse_rate_text("99.280"), Some(99.28));
    }

    #[test]
    fn normalizes_logo_text_to_alnum_uppercase() {
        assert_eq!(normalize_alnum("free style!"), "FREESTYLE");
    }

    #[test]
    fn matches_logo_keyword_by_substring_partial_or_ratio() {
        assert!(is_logo_keyword_match("FREESTYLE", "DJMAXFREESTYLE"));
        assert!(is_logo_keyword_match("FREESTYLE", "FREEST"));
        assert!(is_logo_keyword_match("FREESTYLE", "FREESTY1E"));
        assert!(is_logo_keyword_match("ONLINE", "DJMAXONLINE"));
        assert!(is_logo_keyword_match("ONLINE", "ONL1NE"));
        assert!(!is_logo_keyword_match("FREESTYLE", "MISSION"));
    }
}
