use crate::capture::frame_utils::ImageRegion;
use ::windows::Graphics::Imaging::BitmapDecoder;
use ::windows::Media::Ocr::OcrEngine as WindowsOcrEngine;
use ::windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

pub(super) struct OcrEngine {
    engine: Option<WindowsOcrEngine>,
}

impl OcrEngine {
    pub(super) fn new() -> Self {
        Self {
            engine: WindowsOcrEngine::TryCreateFromUserProfileLanguages().ok(),
        }
    }

    pub(super) fn is_available(&self) -> bool {
        self.engine.is_some()
    }

    pub(super) fn recognize_logo(&self, image: &ImageRegion, force_invert: bool, binarize: bool) -> Result<String, String> {
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

    pub(super) fn recognize_logo_color(&self, image: &ImageRegion) -> Result<String, String> {
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

    pub(super) fn recognize_with_telemetry(
        &self,
        image: &ImageRegion,
        force_invert: bool,
        binarize: bool,
    ) -> Result<(String, overmax_cv::OcrPreprocessResult), String> {
        let Some(engine) = &self.engine else {
            return Ok((String::new(), overmax_cv::OcrPreprocessResult::default()));
        };
        let preprocess = preprocess_ocr_bmp_with_telemetry(image, force_invert, binarize)?;
        let text = recognize_bmp(engine, &preprocess.bmp).map(|t| t.trim().to_string())?;
        Ok((text, preprocess))
    }

    pub(super) fn recognize_color_with_telemetry(
        &self,
        image: &ImageRegion,
    ) -> Result<(String, overmax_cv::OcrPreprocessResult), String> {
        let Some(engine) = &self.engine else {
            return Ok((String::new(), overmax_cv::OcrPreprocessResult::default()));
        };
        if image.width <= 0 || image.height <= 0 {
            return Err("OCR image has invalid dimensions".to_string());
        }
        let preprocess = overmax_cv::preprocess_ocr_color_bgra_with_telemetry(
            &image.bgra,
            image.width as usize,
            image.height as usize,
        )
        .map_err(|e| e.to_string())?;
        let text = recognize_bmp(engine, &preprocess.bmp).map(|t| t.trim().to_string())?;
        Ok((text, preprocess))
    }
}

fn preprocess_ocr_bmp_with_telemetry(
    image: &ImageRegion,
    force_invert: bool,
    binarize: bool,
) -> Result<overmax_cv::OcrPreprocessResult, String> {
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

fn recognize_bmp(engine: &WindowsOcrEngine, bmp: &[u8]) -> Result<String, String> {
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

fn to_err(err: ::windows::core::Error) -> String {
    err.message().to_string()
}
