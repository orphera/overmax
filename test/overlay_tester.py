import cv2
import tkinter as tk
from tkinter import filedialog, simpledialog
import sys
import time
import ctypes
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
from capture.roi_manager import ROIManager
from detection.image_db import ImageDB
from settings import SETTINGS

ctypes.windll.user32.ShowCursor.argtypes = [ctypes.c_bool]

JACKET_SAVE_DIR = Path(__file__).parent / "jackets"


class BorderlessTester:
    VK_ESCAPE = 0x1B
    VK_SPACE = 0x20
    VK_LEFT = 0x25
    VK_RIGHT = 0x27
    VK_A = 0x41
    VK_C = 0x43
    VK_R = 0x52
    VK_S = 0x53
    VK_X = 0x58

    def __init__(self):
        self.win_name = "DJMAX RESPECT V"

        self.root = tk.Tk()
        self.root.withdraw()
        self.video_path = filedialog.askopenfilename(
            title="테스트할 게임 영상 선택",
            filetypes=[("Video files", "*.mp4 *.avi *.mkv *.mov")]
        )
        if not self.video_path:
            sys.exit()

        self.cap = cv2.VideoCapture(self.video_path)
        if not self.cap.isOpened():
            sys.exit()

        self.width  = int(self.cap.get(cv2.CAP_PROP_FRAME_WIDTH))
        self.height = int(self.cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
        self.fps    = self.cap.get(cv2.CAP_PROP_FPS) or 30
        self.effective_fps = min(max(float(self.fps), 1.0), 240.0)
        self.frame_interval = 1.0 / self.effective_fps
        self.key_poll_ms = 10
        self.last_frame_ts = 0.0
        self.key_state: dict[int, bool] = {}
        self.toggle_cooldown_sec = 0.20
        self.last_toggle_ts = 0.0

        self.is_paused = False
        self.cursor_visible = True
        self.last_mouse_move_time = time.time()
        self.current_frame = None
        self.show_roi = False
        self.selection_mode = False
        self.is_selecting = False
        self.drag_start: tuple[int, int] | None = None
        self.drag_current: tuple[int, int] | None = None
        self.selected_rect: tuple[int, int, int, int] | None = None

        self.roiman = ROIManager(self.width, self.height)
        JACKET_SAVE_DIR.mkdir(parents=True, exist_ok=True)
        self.image_db = self._load_image_db()

        cv2.namedWindow(self.win_name, cv2.WINDOW_NORMAL)
        cv2.setWindowProperty(self.win_name, cv2.WND_PROP_FULLSCREEN, cv2.WINDOW_FULLSCREEN)
        cv2.resizeWindow(self.win_name, self.width, self.height)
        cv2.setMouseCallback(self.win_name, self.mouse_callback)

        print(f"[Tester] 영상: {self.video_path}")
        print(f"[Tester] 해상도: {self.width}x{self.height}")
        print(f"[Tester] FPS: 원본={self.fps:.3f} / 재생기준={self.effective_fps:.3f}")
        print(f"[Tester] 재킷 저장 경로: {JACKET_SAVE_DIR}")
        print(
            "[Tester] 단축키: Space=일시정지  C=재킷캡쳐  R=ROI표시토글  "
            "S=선택모드  X=선택영역삭제  A=선택영역분석  ←/→=5초 이동  ESC=종료"
        )

        self.run()

    def mouse_callback(self, event, x, y, flags, param):
        if event == cv2.EVENT_MOUSEMOVE:
            self.last_mouse_move_time = time.time()
            if not self.cursor_visible:
                ctypes.windll.user32.ShowCursor(True)
                self.cursor_visible = True
            if self.selection_mode and self.is_selecting:
                self.drag_current = (x, y)

        if not self.selection_mode:
            return

        if event == cv2.EVENT_LBUTTONDOWN:
            self.selected_rect = None
            self.is_selecting = True
            self.drag_start = (x, y)
            self.drag_current = (x, y)
        elif event == cv2.EVENT_LBUTTONUP and self.is_selecting:
            self.is_selecting = False
            self.drag_current = (x, y)
            rect = self._build_rect(self.drag_start, self.drag_current)
            self.selected_rect = rect if rect and rect[0] < rect[2] and rect[1] < rect[3] else None
            if self.selected_rect:
                x1, y1, x2, y2 = self.selected_rect
                print(f"[Tester] 선택 완료: ({x1}, {y1}) ~ ({x2}, {y2})")
            else:
                print("[Tester] 선택 취소: 유효한 영역이 아님")
            self.drag_start = None
            self.drag_current = None

    def _load_image_db(self) -> ImageDB:
        cfg = SETTINGS["jacket_matcher"]
        db = ImageDB(
            db_path=str(cfg["db_path"]),
            similarity_threshold=float(cfg["similarity_threshold"]),
        )
        if db.initialize():
            db.load()
            print(f"[Tester] ImageDB 로드 완료: {db.song_count}곡")
        else:
            print("[Tester] ImageDB 로드 실패 - song_id 자동완성 비활성")
        return db

    def _search_song_id(self, jacket: cv2.Mat) -> tuple[str, float] | None:
        """재킷 이미지로 ImageDB 검색. 결과 없으면 None."""
        if not self.image_db.is_ready or self.image_db.song_count == 0:
            return None
        result = self.image_db.search(jacket)
        return result  # (song_id, score) | None

    def _capture_jacket(self, frame):
        """현재 프레임에서 재킷 ROI를 크롭하고 song_id 입력 후 저장."""
        x1, y1, x2, y2 = self.roiman.get_roi("jacket")
        jacket = frame[y1:y2, x1:x2]

        if jacket.size == 0:
            print("[Tester] 재킷 ROI가 비어있음")
            return

        # ImageDB 검색 → 기본값 준비
        search_result = self._search_song_id(jacket)
        if search_result:
            default_id, score = search_result
            hint = f"DB 매칭: {default_id}  (유사도 {score:.3f})"
        else:
            default_id, hint = "", "DB 매칭 없음"
        print(f"[Tester] {hint}")

        # 미리보기
        preview = cv2.resize(jacket, (240, 240), interpolation=cv2.INTER_NEAREST)
        cv2.imshow("재킷 미리보기 (아무 키나 누르면 닫힘)", preview)
        cv2.waitKey(1)

        # song_id 입력 (기본값 주입)
        song_id = simpledialog.askstring(
            "song_id 입력",
            f"이 재킷의 song_id (숫자)를 입력하세요.\n비워두면 저장하지 않습니다.\n{hint}",
            initialvalue=default_id,
            parent=self.root,
        )
        cv2.destroyWindow("재킷 미리보기 (아무 키나 누르면 닫힘)")

        if not song_id or not song_id.strip().isdigit():
            print("[Tester] 저장 취소 (입력 없음 또는 숫자 아님)")
            return

        song_id = song_id.strip()
        save_path = JACKET_SAVE_DIR / f"{song_id}.png"

        # 이미 있으면 확인
        if save_path.exists():
            overwrite = simpledialog.askstring(
                "덮어쓰기 확인",
                f"{song_id}.png 이미 존재합니다. 덮어쓸까요? (y/N)",
                parent=self.root,
            )
            if not overwrite or overwrite.strip().lower() != "y":
                print(f"[Tester] 저장 취소: {save_path}")
                return

        cv2.imwrite(str(save_path), jacket)
        print(f"[Tester] 저장 완료: {save_path}  (크기: {jacket.shape[1]}x{jacket.shape[0]})")

    def _build_rect(
        self,
        start: tuple[int, int] | None,
        end: tuple[int, int] | None,
    ) -> tuple[int, int, int, int] | None:
        if start is None or end is None:
            return None
        x1, y1 = start
        x2, y2 = end
        left, right = sorted((max(0, x1), max(0, x2)))
        top, bottom = sorted((max(0, y1), max(0, y2)))
        right = min(self.width - 1, right)
        bottom = min(self.height - 1, bottom)
        return (left, top, right, bottom)

    def _clear_selection(self):
        self.is_selecting = False
        self.drag_start = None
        self.drag_current = None
        self.selected_rect = None
        print("[Tester] 선택 영역 삭제")

    def _analyze_selection_text(self) -> str | None:
        if self.current_frame is None:
            return None
        if self.selected_rect is None:
            return None

        x1, y1, x2, y2 = self.selected_rect
        roi = self.current_frame[y1:y2, x1:x2]
        if roi.size == 0:
            return None

        gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
        rgb = cv2.cvtColor(roi, cv2.COLOR_BGR2RGB)
        mean_brightness = float(gray.mean())
        mean_r, mean_g, mean_b = rgb.reshape(-1, 3).mean(axis=0)
        text = (
            "=== Overlay Selection Analysis ===\n"
            f"start: ({x1}, {y1})\n"
            f"end: ({x2}, {y2})\n"
            f"size: {x2 - x1} x {y2 - y1}\n"
            f"pixels: {(x2 - x1) * (y2 - y1)}\n"
            f"avg_brightness(gray): {mean_brightness:.2f}\n"
            f"avg_rgb: ({mean_r:.2f}, {mean_g:.2f}, {mean_b:.2f})"
        )
        return text

    def _show_analysis_result(self, text: str):
        self.root.clipboard_clear()
        self.root.clipboard_append(text)
        self.root.update_idletasks()

        win = tk.Toplevel(self.root)
        win.title("선택 영역 분석 결과")
        win.geometry("520x260")
        win.attributes("-topmost", True)

        msg = tk.Label(win, text="분석 결과 (복사 가능). 클립보드에도 자동 복사됨.")
        msg.pack(padx=10, pady=(10, 4), anchor="w")

        txt = tk.Text(win, wrap="none", height=10)
        txt.insert("1.0", text)
        txt.pack(fill="both", expand=True, padx=10, pady=4)

        def _copy():
            value = txt.get("1.0", "end-1c")
            self.root.clipboard_clear()
            self.root.clipboard_append(value)
            self.root.update_idletasks()

        btn = tk.Button(win, text="클립보드 복사", command=_copy)
        btn.pack(padx=10, pady=(2, 10), anchor="e")
        close_btn = tk.Button(win, text="닫기", command=win.destroy)
        close_btn.pack(padx=10, pady=(0, 10), anchor="e")
        win.grab_set()
        win.wait_window()

    def _update_frame(self):
        if self.is_paused:
            return
        now = time.perf_counter()
        if now - self.last_frame_ts < self.frame_interval:
            return
        self.last_frame_ts = now
        ret, frame = self.cap.read()
        if not ret:
            self.cap.set(cv2.CAP_PROP_POS_FRAMES, 0)
            self.last_frame_ts = 0.0
            return
        self.current_frame = frame
        if self.cursor_visible and (time.time() - self.last_mouse_move_time > 2.0):
            ctypes.windll.user32.ShowCursor(False)
            self.cursor_visible = False

    def _draw_overlay(self, display):
        if self.show_roi:
            x1, y1, x2, y2 = self.roiman.get_roi("jacket")
            cv2.rectangle(display, (x1, y1), (x2, y2), (0, 0, 255), 2)
            cv2.putText(
                display, "JACKET ROI",
                (x1, max(12, y1 - 6)),
                cv2.FONT_HERSHEY_SIMPLEX, 0.5, (0, 0, 255), 1,
            )

        preview_rect = self._build_rect(self.drag_start, self.drag_current)
        target_rect = preview_rect if self.is_selecting else self.selected_rect
        if target_rect:
            x1, y1, x2, y2 = target_rect
            cv2.rectangle(display, (x1, y1), (x2, y2), (0, 255, 255), 2)
            cv2.putText(
                display, f"SEL ({x1},{y1})-({x2},{y2})",
                (x1, max(12, y1 - 6)),
                cv2.FONT_HERSHEY_SIMPLEX, 0.5, (0, 255, 255), 1,
            )

    def _is_pressed_once(self, vk: int) -> bool:
        is_down = bool(ctypes.windll.user32.GetAsyncKeyState(vk) & 0x8000)
        was_down = self.key_state.get(vk, False)
        self.key_state[vk] = is_down
        return is_down and not was_down

    def _handle_pause_toggle(self):
        now = time.perf_counter()
        if now - self.last_toggle_ts < self.toggle_cooldown_sec:
            return
        self.last_toggle_ts = now
        self.is_paused = not self.is_paused
        if not self.is_paused:
            # 재생 재개 직후 첫 프레임을 즉시 읽어 반응성 개선
            self.last_frame_ts = 0.0
        print(f"[Tester] 일시정지: {'ON' if self.is_paused else 'OFF'}")

    def _seek_ms(self, delta_ms: int):
        pos = self.cap.get(cv2.CAP_PROP_POS_MSEC)
        self.cap.set(cv2.CAP_PROP_POS_MSEC, max(0, pos + delta_ms))
        self.last_frame_ts = 0.0

    def _analyze_selection(self):
        text = self._analyze_selection_text()
        if text is None:
            print("[Tester] 분석할 선택 영역이 없음")
            return
        print(text)
        self._show_analysis_result(text)

    def _handle_hotkeys(self) -> bool:
        if self._is_pressed_once(self.VK_ESCAPE):
            return True
        if self._is_pressed_once(self.VK_SPACE):
            self._handle_pause_toggle()
        if self._is_pressed_once(self.VK_C):
            if self.current_frame is not None:
                self._capture_jacket(self.current_frame)
            else:
                print("[Tester] 캡처할 프레임 없음")
        if self._is_pressed_once(self.VK_R):
            self.show_roi = not self.show_roi
            print(f"[Tester] ROI 표시: {'ON' if self.show_roi else 'OFF'}")
        if self._is_pressed_once(self.VK_S):
            self.selection_mode = not self.selection_mode
            self._clear_selection()
            print(f"[Tester] 선택 모드: {'ON' if self.selection_mode else 'OFF'}")
        if self._is_pressed_once(self.VK_X):
            self._clear_selection()
        if self._is_pressed_once(self.VK_A):
            self._analyze_selection()
        if self._is_pressed_once(self.VK_LEFT):
            self._seek_ms(-5000)
        if self._is_pressed_once(self.VK_RIGHT):
            self._seek_ms(5000)
        return False

    def run(self):
        while True:
            self._update_frame()
            display = self.current_frame.copy() if self.current_frame is not None else None
            if display is not None:
                self._draw_overlay(display)
                cv2.imshow(self.win_name, display)

            # OpenCV 윈도우 이벤트 펌프용. 실제 키 처리는 GetAsyncKeyState 사용.
            cv2.waitKeyEx(1)
            if self._handle_hotkeys():
                break
            if cv2.getWindowProperty(self.win_name, cv2.WND_PROP_VISIBLE) < 1:
                break
            time.sleep(self.key_poll_ms / 1000.0)

        if not self.cursor_visible:
            ctypes.windll.user32.ShowCursor(True)
        self.cap.release()
        cv2.destroyAllWindows()


if __name__ == "__main__":
    BorderlessTester()
