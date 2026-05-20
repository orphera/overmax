"""
ocr.py - OCR 기반 감지 모듈
로고 인식과 Rate(달성률) 인식 등 문자열을 통한 상태 감지를 담당합니다.
"""

import difflib
import re
from typing import Optional
import numpy as np
from detection.ocr_wrapper import WindowsOcrEngine
from constants import LOGO_OCR_KEYWORDS


class OcrDetector:
    """OCR 엔진을 활용해 화면 내 특정 텍스트 정보를 감지합니다."""
    def __init__(self, log_cb=None):
        self._log = log_cb or print
        self.engine = WindowsOcrEngine(log_cb)

    async def detect_logo(self, logo_img: np.ndarray) -> tuple[bool, str, str, Optional[str]]:
        """
        주어진 로고 이미지 영역에서 FREESTYLE 키워드를 감지합니다.
        반환: (is_detected, raw_text, normalized_text, matched_keyword)
        """
        text = await self.engine.recognize(logo_img)
        normalized = self._normalize_alnum(text)
        matched_keyword = None
        for keyword in LOGO_OCR_KEYWORDS:
            normalized_keyword = self._normalize_alnum(keyword)
            if self._is_logo_keyword_match(normalized_keyword, normalized):
                matched_keyword = normalized_keyword
                break
        is_detected = matched_keyword is not None
        return is_detected, text, normalized, matched_keyword

    async def detect_rate(self, rate_img: np.ndarray) -> tuple[Optional[float], str]:
        """
        주어진 Rate(달성률) 이미지 영역에서 퍼센티지를 추출합니다.
        실패 시 흑백 반전(force_invert)하여 1회 재시도합니다.
        반환: (rate_value, raw_text)
        """
        text = await self.engine.recognize(rate_img)
        rate = self._parse_rate_text(text)
        
        # 첫 시도 실패이고 텍스트 자체가 비어있다면 반전해서 재시도
        if rate is None and not text:
            text = await self.engine.recognize(rate_img, force_invert=True)
            rate = self._parse_rate_text(text)
            
        return rate, text

    @staticmethod
    def _parse_rate_text(text: str) -> Optional[float]:
        if not text:
            return None

        cleaned = re.sub(r"[^0-9.]", "", text)
        try:
            if cleaned.count(".") > 1:
                parts = cleaned.split(".")
                cleaned = parts[0] + "." + "".join(parts[1:])

            value = float(cleaned)
            if 0.0 <= value <= 100.0:
                return value
        except ValueError:
            return None
        return None

    @staticmethod
    def _normalize_alnum(text: str) -> str:
        return re.sub(r"[^A-Z0-9]", "", text.upper())

    @staticmethod
    def _is_logo_keyword_match(keyword: str, normalized_ocr: str) -> bool:
        if not keyword or not normalized_ocr:
            return False
        if keyword in normalized_ocr:
            return True

        min_partial_len = min(6, len(keyword))
        for i in range(0, len(keyword) - min_partial_len + 1):
            part = keyword[i:i + min_partial_len]
            if part and part in normalized_ocr:
                return True

        ratio = difflib.SequenceMatcher(None, keyword, normalized_ocr).ratio()
        return ratio >= 0.72


