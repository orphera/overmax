use crate::frame_utils::ImageRegion;
use overmax_core::SceneType;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

const KEYWORD_FREESTYLE: &str = "FREESTYLE";
const KEYWORD_ONLINE: &str = "ONLINE";


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
    #[allow(unused_assignments)]
    pub fn detect_logo(&self, logo: &ImageRegion) -> (SceneType, String, String) {
        // Pass 1: Grayscale (no binarization) - extremely robust on bright/fluctuating backgrounds
        let mut text = self.engine.recognize_logo(logo, false, false).unwrap_or_default();
        let mut normalized = normalize_alnum(&text);

        let mut scene = if is_logo_keyword_match(&normalize_alnum(KEYWORD_FREESTYLE), &normalized) {
            SceneType::Freestyle
        } else if is_logo_keyword_match(&normalize_alnum(KEYWORD_ONLINE), &normalized) {
            SceneType::Online
        } else {
            SceneType::Unknown
        };

        // Pass 2: Fallback to binarized normal
        if scene == SceneType::Unknown {
            if let Ok(t) = self.engine.recognize_logo(logo, false, true) {
                let norm = normalize_alnum(&t);
                let sc = if is_logo_keyword_match(&normalize_alnum(KEYWORD_FREESTYLE), &norm) {
                    SceneType::Freestyle
                } else if is_logo_keyword_match(&normalize_alnum(KEYWORD_ONLINE), &norm) {
                    SceneType::Online
                } else {
                    SceneType::Unknown
                };
                if sc != SceneType::Unknown {
                    scene = sc;
                    text = t;
                    normalized = norm;
                }
            }
        }

        // Pass 3: Fallback to binarized inverted (useful for bright BGA flashes that invert the threshold)
        if scene == SceneType::Unknown {
            if let Ok(t) = self.engine.recognize_logo(logo, true, true) {
                let norm = normalize_alnum(&t);
                let sc = if is_logo_keyword_match(&normalize_alnum(KEYWORD_FREESTYLE), &norm) {
                    SceneType::Freestyle
                } else if is_logo_keyword_match(&normalize_alnum(KEYWORD_ONLINE), &norm) {
                    SceneType::Online
                } else {
                    SceneType::Unknown
                };
                if sc != SceneType::Unknown {
                    scene = sc;
                    text = t;
                    normalized = norm;
                }
            }
        }

        let label = match scene {
            SceneType::Freestyle => "FREESTYLE".to_string(),
            SceneType::Online => "ONLINE".to_string(),
            _ => "UNKNOWN".to_string(),
        };

        (scene, text, label)
    }

    pub fn detect_rate(&self, rate: &ImageRegion) -> (Option<f32>, String, Option<OcrTelemetry>) {
        let mut telemetry = None;
        let mut text = String::new();
        let mut value = None;
        
        if let Ok((txt, threshold, bg_mean, use_invert, pixels, w, h)) = self.engine.recognize_with_telemetry(rate, false) {
            text = txt.clone();
            value = parse_rate_text(&text);
            telemetry = Some(OcrTelemetry {
                rate_text: text.clone(),
                threshold,
                bg_mean,
                use_invert,
                image_pixels: pixels,
                image_width: w,
                image_height: h,
            });
        }
        
        if value.is_none() && text.is_empty() {
            if let Ok((txt, threshold, bg_mean, use_invert, pixels, w, h)) = self.engine.recognize_with_telemetry(rate, true) {
                text = txt;
                value = parse_rate_text(&text);
                telemetry = Some(OcrTelemetry {
                    rate_text: text.clone(),
                    threshold,
                    bg_mean,
                    use_invert,
                    image_pixels: pixels,
                    image_width: w,
                    image_height: h,
                });
            }
        }
        
        (value, text, telemetry)
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

    fn recognize_with_telemetry(
        &self,
        image: &ImageRegion,
        force_invert: bool,
    ) -> Result<(String, u8, f32, bool, Vec<u8>, usize, usize), String> {
        let Some(engine) = &self.engine else {
            return Ok((String::new(), 0, 0.0, false, Vec::new(), 0, 0));
        };
        let (bmp, threshold, bg_mean, use_invert, pixels, w, h) = preprocess_ocr_bmp_with_telemetry(image, force_invert)?;
        let text = recognize_bmp(engine, &bmp).map(|t| t.trim().to_string())?;
        Ok((text, threshold, bg_mean, use_invert, pixels, w, h))
    }
}

fn preprocess_ocr_bmp_with_telemetry(
    image: &ImageRegion,
    force_invert: bool,
) -> Result<(Vec<u8>, u8, f32, bool, Vec<u8>, usize, usize), String> {
    if image.width <= 0 || image.height <= 0 {
        return Err("OCR image has invalid dimensions".to_string());
    }
    overmax_cv::preprocess_ocr_bgra_with_telemetry(
        &image.bgra,
        image.width as usize,
        image.height as usize,
        force_invert,
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
