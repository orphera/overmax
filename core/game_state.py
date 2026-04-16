from dataclasses import dataclass
from typing import Optional

@dataclass(frozen=True)
class GameSessionState:
    """
    Overmax의 통합 게임 상태 스냅샷.
    곡 ID, 버튼 모드, 난이도를 하나의 원자 단위로 묶어 전달하여 
    각 정보 간의 타이밍 어긋남을 방지함.
    """
    song_id: Optional[int]
    mode: Optional[str]
    diff: Optional[str]
    is_stable: bool = False
    
    # 헬퍼 프로퍼티
    @property
    def is_valid(self) -> bool:
        """모든 필수 정보가 존재하고 안정화된 상태인지 여부"""
        return all([self.song_id, self.mode, self.diff]) and self.is_stable

    def __str__(self):
        status = "STABLE" if self.is_stable else "DETECTING"
        return f"[{status}] {self.song_id} | {self.mode} | {self.diff}"
