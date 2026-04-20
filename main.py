"""
Overmax - DJMAX Respect V 비공식 난이도 오버레이
메인 진입점 — 모든 컴포넌트를 조립하고 실행
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from core.app import OvermaxApp

def main():
    try:
        app = OvermaxApp()
        app.run()
    except KeyboardInterrupt:
        print("\n[Main] 종료 중...")

if __name__ == "__main__":
    main()
