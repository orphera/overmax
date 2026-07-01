use crate::frame_utils::ImageRegion;
use overmax_core::SceneType;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};


use std::fmt;

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
    engine: WindowsOcrEngine,
}

impl OcrDetector {
    pub fn new() -> Self {
        Self {
            engine: WindowsOcrEngine::new(),
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
        let (txt, threshold, bg_mean, use_invert, pixels, w, h) = res.ok()?;
        let val = parse_rate_text(&txt);
        let telemetry = OcrTelemetry {
            rate_text: txt.clone(),
            threshold,
            bg_mean,
            use_invert,
            image_pixels: pixels,
            image_width: w,
            image_height: h,
        };
        Some((val, txt, telemetry))
    }

    /// Rate 영역을 감지합니다.
    ///
    /// # 가드레일 (CRITICAL GUARDRAIL)
    /// 인게임 실시간 성능 보호를 위해 **반드시 단일 패스(1-Pass) 실행만 수행**해야 합니다.
    /// 절대로 3-pass 등의 다중 패스 루프를 이곳에 재도입하지 마십시오. (AGENTS.md 및 CONTEXT.md 제약 조건)
    /// OCR 인식 실패나 오작동 대응은 `PlayStateDetector`의 히스토리 버퍼(`stable_raw` 다수결)를 통해 해결합니다.
    pub fn detect_rate(&self, rate: &ImageRegion) -> (Option<f32>, String, Option<OcrTelemetry>) {
        // 단일 패스: Color OCR (사용자 경험 및 테스트 결과 가장 우수한 텍스트 품질 확보)
        if let Some((val, txt, tel)) = self.attempt_rate_ocr(rate, true, false) {
            (val, txt, Some(tel))
        } else {
            (None, String::new(), None)
        }
    }

    /// Score 영역을 단일 패스(Color)로 감지하여 정수로 파싱합니다.
    pub fn detect_score(&self, score: &ImageRegion) -> Option<u32> {
        // Color 1-pass 호출 (Rate와 동일하게 Color 채널을 유지하여 인식 성능 극대화)
        if let Ok(text) = self.engine.recognize_logo_color(score) {
            parse_score_text(&text)
        } else {
            None
        }
    }

    pub fn recognize_text_color(&self, region: &ImageRegion) -> Option<String> {
        self.engine.recognize_logo_color(region).ok()
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

struct WindowsOcrEngine {
    engine: Option<OcrEngine>,
}

impl WindowsOcrEngine {
    fn new() -> Self {
        Self {
            engine: OcrEngine::TryCreateFromUserProfileLanguages().ok(),
        }
    }

    fn is_available(&self) -> bool {
        self.engine.is_some()
    }

    #[allow(dead_code)]
    fn recognize_logo(&self, image: &ImageRegion, force_invert: bool, binarize: bool) -> Result<String, String> {
        let Some(engine) = &self.engine else {
            return Ok(String::new());
        };
        let bmp = overmax_cv::preprocess_ocr_bgra(
            &image.bgra,
            image.width as usize,
            image.height as usize,
            force_invert,
            binarize,
        )
        .map_err(|e| e.to_string())?;
        recognize_bmp(engine, &bmp).map(|text| text.trim().to_string())
    }

    fn recognize_logo_color(&self, image: &ImageRegion) -> Result<String, String> {
        let Some(engine) = &self.engine else {
            return Ok(String::new());
        };
        if image.width <= 0 || image.height <= 0 {
            return Err("OCR image has invalid dimensions".to_string());
        }
        let bmp = overmax_cv::preprocess_ocr_color_bgra(
            &image.bgra,
            image.width as usize,
            image.height as usize,
        )
        .map_err(|e| e.to_string())?;
        recognize_bmp(engine, &bmp).map(|text| text.trim().to_string())
    }

    fn recognize_with_telemetry(
        &self,
        image: &ImageRegion,
        force_invert: bool,
        binarize: bool,
    ) -> Result<(String, u8, f32, bool, Vec<u8>, usize, usize), String> {
        let Some(engine) = &self.engine else {
            return Ok((String::new(), 0, 0.0, false, Vec::new(), 0, 0));
        };
        let (bmp, threshold, bg_mean, use_invert, pixels, w, h) =
            preprocess_ocr_bmp_with_telemetry(image, force_invert, binarize)?;
        let text = recognize_bmp(engine, &bmp).map(|t| t.trim().to_string())?;
        Ok((text, threshold, bg_mean, use_invert, pixels, w, h))
    }

    fn recognize_color_with_telemetry(
        &self,
        image: &ImageRegion,
    ) -> Result<(String, u8, f32, bool, Vec<u8>, usize, usize), String> {
        let Some(engine) = &self.engine else {
            return Ok((String::new(), 0, 0.0, false, Vec::new(), 0, 0));
        };
        if image.width <= 0 || image.height <= 0 {
            return Err("OCR image has invalid dimensions".to_string());
        }
        let (bmp, threshold, bg_mean, use_invert, pixels, w, h) =
            overmax_cv::preprocess_ocr_color_bgra_with_telemetry(
                &image.bgra,
                image.width as usize,
                image.height as usize,
            )
            .map_err(|e| e.to_string())?;
        let text = recognize_bmp(engine, &bmp).map(|t| t.trim().to_string())?;
        Ok((text, threshold, bg_mean, use_invert, pixels, w, h))
    }

}

fn preprocess_ocr_bmp_with_telemetry(
    image: &ImageRegion,
    force_invert: bool,
    binarize: bool,
) -> Result<(Vec<u8>, u8, f32, bool, Vec<u8>, usize, usize), String> {
    if image.width <= 0 || image.height <= 0 {
        return Err("OCR image has invalid dimensions".to_string());
    }
    overmax_cv::preprocess_ocr_bgra_with_telemetry(
        &image.bgra,
        image.width as usize,
        image.height as usize,
        force_invert,
        binarize,
    )
    .map_err(|e| e.to_string())
}

fn recognize_bmp(engine: &OcrEngine, bmp: &[u8]) -> Result<String, String> {
    let stream = InMemoryRandomAccessStream::new().map_err(to_err)?;
    let writer = DataWriter::CreateDataWriter(&stream).map_err(to_err)?;
    writer.WriteBytes(bmp).map_err(to_err)?;
    writer
        .StoreAsync()
        .map_err(to_err)?
        .join()
        .map_err(to_err)?;
    writer.DetachStream().map_err(to_err)?;
    stream.Seek(0).map_err(to_err)?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(to_err)?
        .join()
        .map_err(to_err)?;
    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .map_err(to_err)?
        .join()
        .map_err(to_err)?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(to_err)?
        .join()
        .map_err(to_err)?;
    let text = result.Text().map_err(to_err)?.to_string_lossy();
    stream.Close().map_err(to_err)?;
    Ok(text)
}

fn parse_score_text(text: &str) -> Option<u32> {
    let clean = text.chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>();
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
    if !dot_seen && value >= (crate::play_state::MIN_VALID_RATE * 100.0) {
        value /= 100.0;
    }

    // 소수점 셋째 자리 이하 무조건 버림(Truncate) 보정 적용하여 반올림 차단
    value = (value * 100.0).floor() / 100.0;

    // 유효한 실시간 기록으로 처리할 수 있는 최소 범위(MIN_VALID_RATE = 80.0%) 이상인 경우만 유효값으로 반환하고,
    // 0.00%가 4.00% 또는 9.00% 노이즈로 완벽하게 잘못 오인식되는 수치 등은 스캔 시점에 원천 배제합니다.
    (crate::play_state::MIN_VALID_RATE..=100.0).contains(&value).then_some(value)
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

fn to_err(err: windows::core::Error) -> String {
    err.message().to_string()
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
        use crate::frame_utils::crop_roi;
        use crate::roi::RoiManager;
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
            let img = image::io::Reader::open(&path).expect("Failed to open file")
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
            let frame = crate::screen_capture::CapturedFrame {
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
