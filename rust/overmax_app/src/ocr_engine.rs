use crate::frame_utils::ImageRegion;
use overmax_core::SceneType;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};


#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrTelemetry {
    pub rate_text: String,
    pub threshold: u8,
    pub bg_mean: f32,
    pub use_invert: bool,
    pub image_pixels: Vec<u8>,
    pub image_width: usize,
    pub image_height: usize,
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
    pub fn detect_logo(&self, logo: &ImageRegion) -> (SceneType, String, String) {
        // 1. Try Color OCR
        let mut color_txt = String::new();
        if let Ok(t) = self.engine.recognize_logo_color(logo) {
            color_txt = t.clone();
            if let Some((scene, _)) = match_logo_scene(&t) {
                return (scene, t, scene_label(scene));
            }
        }
        // 2. Try Grayscale (no binarization)
        let mut gray_txt = String::new();
        if let Ok(t) = self.engine.recognize_logo(logo, false, false) {
            gray_txt = t.clone();
            if let Some((scene, _)) = match_logo_scene(&t) {
                return (scene, t, scene_label(scene));
            }
        }
        // 3. Try Binarized normal
        let mut bin_txt = String::new();
        if let Ok(t) = self.engine.recognize_logo(logo, false, true) {
            bin_txt = t.clone();
            if let Some((scene, _)) = match_logo_scene(&t) {
                return (scene, t, scene_label(scene));
            }
        }
        // 4. Try Binarized inverted
        let mut inv_txt = String::new();
        if let Ok(t) = self.engine.recognize_logo(logo, true, true) {
            inv_txt = t.clone();
            if let Some((scene, _)) = match_logo_scene(&t) {
                return (scene, t, scene_label(scene));
            }
        }

        println!("    [detect_logo] all passes failed! color='{}', gray='{}', bin='{}', inv='{}'",
                 color_txt.trim(), gray_txt.trim(), bin_txt.trim(), inv_txt.trim());
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

    pub fn detect_rate(&self, rate: &ImageRegion) -> (Option<f32>, String, Option<OcrTelemetry>) {
        let mut fallback = None;
        // Pass 1: Color OCR
        if let Some((val, txt, tel)) = self.attempt_rate_ocr(rate, true, false) {
            if val.is_some() {
                return (val, txt, Some(tel));
            }
            if !txt.is_empty() && fallback.is_none() {
                fallback = Some((val, txt, tel));
            }
        }
        // Pass 2: Grayscale OCR (auto-invert)
        if let Some((val, txt, tel)) = self.attempt_rate_ocr(rate, false, false) {
            if val.is_some() {
                return (val, txt, Some(tel));
            }
            if !txt.is_empty() && fallback.is_none() {
                fallback = Some((val, txt, tel));
            }
        }
        // Pass 3: Grayscale OCR (forced opposite invert)
        if let Some((val, txt, tel)) = self.attempt_rate_ocr(rate, false, true) {
            if val.is_some() {
                return (val, txt, Some(tel));
            }
            if !txt.is_empty() && fallback.is_none() {
                fallback = Some((val, txt, tel));
            }
        }
        if let Some((val, txt, tel)) = fallback {
            (val, txt, Some(tel))
        } else {
            (None, String::new(), None)
        }
    }

    pub fn recognize_text_color(&self, region: &ImageRegion) -> Option<String> {
        self.engine.recognize_logo_color(region).ok()
    }

    pub fn recognize_bottom_half_with_rate_x(&self, region: &ImageRegion) -> Option<(String, Option<f32>)> {
        self.engine.recognize_bottom_half_with_rate_x(region)
    }

    pub fn detect_bottom_guide_space(&self, bottom_guide: &ImageRegion) -> bool {
        let check = |t: &str| {
            let norm = normalize_alnum(t).to_lowercase();
            norm.contains("space")
                || norm.contains("pace")
                || norm.contains("spac")
                || norm.contains("spce")
                || norm.contains("5pace")
                || sequence_ratio("space", &norm) >= 0.70
        };

        if let Ok(t) = self.engine.recognize_logo_color(bottom_guide) {
            if check(&t) { return true; }
        }
        if let Ok(t) = self.engine.recognize_logo(bottom_guide, false, false) {
            if check(&t) { return true; }
        }
        false
    }

    pub fn detect_bottom_guide_f5(&self, bottom_guide: &ImageRegion) -> bool {
        let check = |t: &str| {
            let norm = normalize_alnum(t).to_lowercase();
            norm.contains("f5")
                || norm.contains("fs")
                || norm.contains("es")
                || sequence_ratio("f5", &norm) >= 0.60
        };

        if let Ok(t) = self.engine.recognize_logo_color(bottom_guide) {
            if check(&t) { return true; }
        }
        if let Ok(t) = self.engine.recognize_logo(bottom_guide, false, false) {
            if check(&t) { return true; }
        }
        false
    }
    pub fn classify_fallback_scene(&self, text: &str, rate_x_ratio: Option<f32>) -> Option<SceneType> {
        let norm = text.to_lowercase();
        
        let has_percent = norm.contains('%') || norm.contains("99.") || norm.contains("98.") || norm.contains("97.") || norm.contains("100%");
        let has_judgement = norm.contains("judgement") || norm.contains("details") || norm.contains("restart") || norm.contains("save");
        
        if norm.contains("button") && (norm.contains("tunes") || norm.contains("tune") || has_judgement) {
            return Some(SceneType::ResultFreestyle);
        }
        
        let has_mode = norm.contains("4b") || norm.contains("5b") || norm.contains("6b") || norm.contains("8b");
        let has_high_score = norm.contains("99") || norm.contains("98") || norm.contains("97") || norm.contains("1000000");
        
        if has_percent || norm.contains("tunes") || norm.contains("tune") || (has_mode && has_high_score) {
            // 1. Layout-based classification (ResultOpen3 clear zones: 1st card < 0.15, 2nd+ card >= 0.20)
            if let Some(ratio) = rate_x_ratio {
                if ratio < 0.15 || ratio >= 0.20 {
                    return Some(SceneType::ResultOpen3);
                }
            }

            // 2. Text-based fallback heuristics
            let distinct_scores = count_distinct_scores(&norm);
            let has_bottom_bar = norm.contains("space") || norm.contains("상세정보") || norm.contains("details") ||
                                 norm.contains("상4") || norm.contains("섬보") || norm.contains("출계") || norm.contains("등록") || norm.contains("젤져");
            
            if has_bottom_bar || distinct_scores >= 3 {
                return Some(SceneType::ResultOpen3);
            }

            // 3. If ratio is in the 2-player range (0.15 ~ 0.20) and no other ResultOpen3 clues exist, classify as ResultOpen2
            if let Some(ratio) = rate_x_ratio {
                if ratio >= 0.15 && ratio < 0.20 {
                    return Some(SceneType::ResultOpen2);
                }
            }

            return Some(SceneType::ResultOpen2);
        }
        None
    }
}

fn count_distinct_scores(text: &str) -> usize {
    use std::collections::HashSet;
    let mut scores = HashSet::new();
    for word in text.split_whitespace() {
        let clean: String = word.chars().filter(|c| c.is_ascii_digit()).collect();
        if clean.len() == 6 && (clean.starts_with('9') || clean.starts_with('8')) {
            scores.insert(clean);
        } else if clean == "1000000" {
            scores.insert(clean);
        }
    }
    scores.len()
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

    fn recognize_bottom_half_with_rate_x(&self, region: &ImageRegion) -> Option<(String, Option<f32>)> {
        let engine = self.engine.as_ref()?;
        if region.width <= 0 || region.height <= 0 {
            return None;
        }
        let bmp = overmax_cv::preprocess_ocr_color_bgra(
            &region.bgra,
            region.width as usize,
            region.height as usize,
        ).ok()?;

        let stream = InMemoryRandomAccessStream::new().ok()?;
        let writer = DataWriter::CreateDataWriter(&stream).ok()?;
        writer.WriteBytes(&bmp).ok()?;
        writer.StoreAsync().ok()?.join().ok()?;
        writer.DetachStream().ok()?;
        stream.Seek(0).ok()?;

        let decoder = BitmapDecoder::CreateAsync(&stream).ok()?.join().ok()?;
        let bitmap = decoder.GetSoftwareBitmapAsync().ok()?.join().ok()?;
        let result = engine.RecognizeAsync(&bitmap).ok()?.join().ok()?;

        let full_text = result.Text().ok()?.to_string_lossy();
        let width = bitmap.PixelWidth().ok()? as f32;

        let mut rate_x_ratio = None;

        if let Ok(lines) = result.Lines() {
            for line in lines {
                if let Ok(words) = line.Words() {
                    for word in words {
                        let Ok(w_text) = word.Text() else { continue; };
                        let text_str = w_text.to_string_lossy().to_lowercase();
                        
                        let clean_digits: String = text_str.chars().filter(|c| c.is_ascii_digit()).collect();
                        let is_score = (clean_digits.len() == 6 && (clean_digits.starts_with('9') || clean_digits.starts_with('8')))
                            || clean_digits == "1000000";
                        let is_rate = text_str.contains('%') 
                            || text_str.contains("99.") 
                            || text_str.contains("98.") 
                            || text_str.contains("97.") 
                            || text_str.contains("100%");

                        if is_score || is_rate {
                            if let Ok(rect) = word.BoundingRect() {
                                let ratio = rect.X as f32 / width;
                                if ratio < 0.5 {
                                    rate_x_ratio = Some(ratio);
                                    break;
                                }
                            }
                        }
                    }
                }
                if rate_x_ratio.is_some() {
                    break;
                }
            }
        }

        let _ = stream.Close();
        Some((full_text, rate_x_ratio))
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
    // 문자열에 소수점이 감지되지 않았고 파싱 결과가 100.0을 초과하는 경우(예: 9412.0)
    // 소수점 이하 2자리가 정수로 취급되었다고 가정하고 100.0으로 나누어 보정합니다.
    if !dot_seen && value > 100.0 {
        value /= 100.0;
    }

    (0.0..=100.0).contains(&value).then_some(value)
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
    use super::{is_logo_keyword_match, normalize_alnum, parse_rate_text};

    #[test]
    fn parses_rate_text_like_python_path() {
        assert_eq!(parse_rate_text("99.43%"), Some(99.43));
        assert_eq!(parse_rate_text("100.00"), Some(100.0));
        assert_eq!(parse_rate_text("101.0"), None);
        // 소수점 누락 보정 테스트
        assert_eq!(parse_rate_text("9412%"), Some(94.12));
        assert_eq!(parse_rate_text("10000"), Some(100.0));
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
