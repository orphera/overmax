use crate::frame_utils::ImageRegion;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

const LOGO_KEYWORD: &str = "FREESTYLE";

pub struct OcrDetector {
    engine: WindowsOcrEngine,
}

impl OcrDetector {
    pub fn new() -> Self {
        Self { engine: WindowsOcrEngine::new() }
    }

    pub fn is_available(&self) -> bool {
        self.engine.is_available()
    }

    pub fn detect_logo(&self, logo: &ImageRegion) -> (bool, String, String) {
        let text = self.engine.recognize(logo, false).unwrap_or_default();
        let normalized = normalize_alnum(&text);
        let keyword = normalize_alnum(LOGO_KEYWORD);
        (is_logo_keyword_match(&keyword, &normalized), text, normalized)
    }

    pub fn detect_rate(&self, rate: &ImageRegion) -> (Option<f32>, String) {
        let mut text = self.engine.recognize(rate, false).unwrap_or_default();
        let mut value = parse_rate_text(&text);
        if value.is_none() && text.is_empty() {
            text = self.engine.recognize(rate, true).unwrap_or_default();
            value = parse_rate_text(&text);
        }
        (value, text)
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

    fn recognize(&self, image: &ImageRegion, force_invert: bool) -> Result<String, String> {
        let Some(engine) = &self.engine else {
            return Ok(String::new());
        };
        let bmp = preprocess_ocr_bmp(image, force_invert)?;
        recognize_bmp(engine, &bmp).map(|text| text.trim().to_string())
    }
}

fn preprocess_ocr_bmp(image: &ImageRegion, force_invert: bool) -> Result<Vec<u8>, String> {
    if image.width <= 0 || image.height <= 0 {
        return Err("OCR image has invalid dimensions".to_string());
    }
    overmax_cv::preprocess_ocr_bgra(
        &image.bgra,
        image.width as usize,
        image.height as usize,
        force_invert,
    )
}

fn recognize_bmp(engine: &OcrEngine, bmp: &[u8]) -> Result<String, String> {
    let stream = InMemoryRandomAccessStream::new().map_err(to_err)?;
    let writer = DataWriter::CreateDataWriter(&stream).map_err(to_err)?;
    writer.WriteBytes(bmp).map_err(to_err)?;
    writer.StoreAsync().map_err(to_err)?.join().map_err(to_err)?;
    writer.DetachStream().map_err(to_err)?;
    stream.Seek(0).map_err(to_err)?;

    let decoder = BitmapDecoder::CreateAsync(&stream).map_err(to_err)?.join().map_err(to_err)?;
    let bitmap = decoder.GetSoftwareBitmapAsync().map_err(to_err)?.join().map_err(to_err)?;
    let result = engine.RecognizeAsync(&bitmap).map_err(to_err)?.join().map_err(to_err)?;
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
    let value = cleaned.parse::<f32>().ok()?;
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
        assert!(!is_logo_keyword_match("FREESTYLE", "MISSION"));
    }
}
