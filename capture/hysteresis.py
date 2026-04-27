from collections import deque

class HysteresisBuffer:
    """
    OCR 히트 기록을 바탕으로 상태(선곡화면 진입/이탈)와 신뢰도를 계산합니다.
    """
    def __init__(self, history_size: int, on_ratio: float, on_min_samples: int, off_ratio: float, off_min_samples: int):
        self.history_size = max(1, history_size)
        self.on_ratio = on_ratio
        self.on_min_samples = max(1, on_min_samples)
        self.off_ratio = off_ratio
        self.off_min_samples = max(1, off_min_samples)
        
        self.history = deque(maxlen=self.history_size)
        self.is_active = False
        self.is_leaving = False
        self.confidence = 0.0
        
        # 로그 및 외부 접근용
        self.hit_count = 0
        self.sample_count = 0
        self.ratio = 0.0

    def update(self, is_hit: bool) -> tuple[bool, bool, float]:
        self.history.append(is_hit)
        self.sample_count = len(self.history)
        self.hit_count = sum(1 for v in self.history if v)
        self.ratio = (self.hit_count / self.sample_count) if self.sample_count > 0 else 0.0

        # 진입/이탈 상태 판정
        if self.is_active:
            should_turn_off = (
                self.sample_count >= self.off_min_samples
                and self.ratio <= self.off_ratio
            )
            self.is_active = not should_turn_off
        else:
            self.is_active = (
                self.sample_count >= self.on_min_samples
                and self.ratio >= self.on_ratio
            )

        # 이탈 중(fade-out) 감지
        self.is_leaving = False
        if self.is_active and self.sample_count >= 4:
            half = self.sample_count // 2
            history_list = list(self.history)
            first_half_ratio = sum(history_list[:half]) / half
            second_half_ratio = sum(history_list[half:]) / (self.sample_count - half)
            if second_half_ratio < first_half_ratio:
                self.is_leaving = True

        # 신뢰도 계산
        self.confidence = self.ratio * (0.5 if self.is_leaving else 1.0)
        return self.is_active, self.is_leaving, self.confidence
