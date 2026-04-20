"""
ocr_wrapper.py - Windows 10/11 OCR API Wrapper
"""

import numpy as np
import cv2
from typing import Optional

try:
    import winrt.windows.media.ocr as ocr
    import winrt.windows.graphics.imaging as imaging
    import winrt.windows.storage.streams as streams
    WINDOWS_OCR_AVAILABLE = True
except ImportError:
    WINDOWS_OCR_AVAILABLE = False

from capture.helpers import preprocess_for_ocr


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

    async def recognize(self, img_bgra: np.ndarray, force_invert: bool = False) -> str:
        if not self.is_available:
            return ""
        try:
            thresh = preprocess_for_ocr(img_bgra, force_invert=force_invert)
            if thresh is None:
                return ""

            success, encoded = cv2.imencode(".bmp", thresh)
            if not success:
                return ""

            stream = streams.InMemoryRandomAccessStream()
            data_writer = streams.DataWriter(stream)
            data_writer.write_bytes(encoded.tobytes())
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
