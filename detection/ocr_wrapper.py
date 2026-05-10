"""
ocr_wrapper.py - Windows 10/11 OCR API Wrapper
"""

import numpy as np
from typing import Optional
from overmax_cv import ocr_preprocess_bgra

try:
    import winrt.windows.media.ocr as ocr
    import winrt.windows.graphics.imaging as imaging
    import winrt.windows.storage.streams as streams
    WINDOWS_OCR_AVAILABLE = True
except ImportError:
    WINDOWS_OCR_AVAILABLE = False


class WindowsOcrEngine:
    def __init__(self, log_cb=None):
        self._log = log_cb or print
        self.engine = self._create_engine()

    @property
    def is_available(self) -> bool:
        return WINDOWS_OCR_AVAILABLE and self.engine is not None

    def _create_engine(self):
        if not WINDOWS_OCR_AVAILABLE:
            return None
        try:
            supported_langs = ocr.OcrEngine.available_recognizer_languages
            target_lang = next((lang for lang in supported_langs if "ko" in lang.language_tag.lower()), None)
            if target_lang is None and supported_langs:
                target_lang = supported_langs[0]

            if target_lang:
                self._log(f"[OCR] 엔진 언어: {target_lang.language_tag}")
                return ocr.OcrEngine.try_create_from_language(target_lang)

            self._log("[OCR] 엔진: user profile 언어 사용")
            return ocr.OcrEngine.try_create_from_user_profile_languages()
        except Exception as exc:
            self._log(f"[OCR] 엔진 초기화 실패: {exc}")
            return None

    def _preprocess(self, img_bgra: np.ndarray, force_invert: bool = False) -> Optional[bytes]:
        h, w = img_bgra.shape[:2]
        if w == 0 or h == 0:
            return None
        data = np.ascontiguousarray(img_bgra, dtype=np.uint8).tobytes()
        return ocr_preprocess_bgra(data, w, h, force_invert)

    async def recognize(self, img_bgra: np.ndarray, force_invert: bool = False) -> str:
        if not self.is_available:
            return ""
        try:
            thresh = self._preprocess(img_bgra, force_invert=force_invert)
            if thresh is None:
                return ""

            stream = streams.InMemoryRandomAccessStream()
            data_writer = streams.DataWriter(stream)
            data_writer.write_bytes(thresh)
            await data_writer.store_async()
            data_writer.detach_stream()
            stream.seek(0)

            decoder = await imaging.BitmapDecoder.create_async(stream)
            software_bitmap = await decoder.get_software_bitmap_async()
            result = await self.engine.recognize_async(software_bitmap)

            stream.close()

            return result.text.strip()
        except Exception as e:
            self._log(f"[OCR] 실행 오류: {e}")
            return ""
