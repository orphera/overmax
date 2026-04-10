import cv2
import tkinter as tk
from tkinter import filedialog
import sys
import time
import ctypes

# Windows 커서 및 윈도우 제어
ctypes.windll.user32.ShowCursor.argtypes = [ctypes.c_bool]

class BorderlessTester:
    def __init__(self):
        self.win_name = "DJMAX RESPECT V"
        
        # 파일 선택 (취소 시 종료)
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

        # 영상 해상도 정보
        self.width = int(self.cap.get(cv2.CAP_PROP_FRAME_WIDTH))
        self.height = int(self.cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
        self.fps = self.cap.get(cv2.CAP_PROP_FPS) or 30

        self.is_paused = False
        self.cursor_visible = True
        self.last_mouse_move_time = time.time()

        # 1. 윈도우 생성 및 Borderless 설정
        cv2.namedWindow(self.win_name, cv2.WINDOW_NORMAL)
        
        # 핵심: FULLSCREEN 속성을 주면 타이틀 바가 사라집니다.
        cv2.setWindowProperty(self.win_name, cv2.WND_PROP_FULLSCREEN, cv2.WINDOW_FULLSCREEN)
        
        # 2. 그 상태에서 창 크기를 영상 크기로 고정 (화면보다 커도 잘리지 않음)
        cv2.resizeWindow(self.win_name, self.width, self.height)
        
        cv2.setMouseCallback(self.win_name, self.mouse_callback)
        self.run()

    def mouse_callback(self, event, x, y, flags, param):
        if event == cv2.EVENT_MOUSEMOVE:
            self.last_mouse_move_time = time.time()
            if not self.cursor_visible:
                ctypes.windll.user32.ShowCursor(True)
                self.cursor_visible = True

    def run(self):
        while True:
            if not self.is_paused:
                ret, frame = self.cap.read()
                if not ret:
                    self.cap.set(cv2.CAP_PROP_POS_FRAMES, 0)
                    continue
                
                # 마우스 커서 숨김 (2초)
                if self.cursor_visible and (time.time() - self.last_mouse_move_time > 2.0):
                    ctypes.windll.user32.ShowCursor(False)
                    self.cursor_visible = False

                cv2.imshow(self.win_name, frame)

            # 키 입력 (Windows 확장 키 대응)
            key = cv2.waitKeyEx(int(1000 / self.fps))

            if key == 27: # ESC로 종료
                break
            elif key == ord(' '): # 재생/일시정지
                self.is_paused = not self.is_paused
            
            # Seek (Windows 화살표 키 코드)
            elif key == 2424832: # Left
                pos = self.cap.get(cv2.CAP_PROP_POS_MSEC)
                self.cap.set(cv2.CAP_PROP_POS_MSEC, max(0, pos - 5000))
            elif key == 2555904: # Right
                pos = self.cap.get(cv2.CAP_PROP_POS_MSEC)
                self.cap.set(cv2.CAP_PROP_POS_MSEC, pos + 5000)

            # 창이 닫히면 종료
            if cv2.getWindowProperty(self.win_name, cv2.WND_PROP_VISIBLE) < 1:
                break

        if not self.cursor_visible:
            ctypes.windll.user32.ShowCursor(True)
        self.cap.release()
        cv2.destroyAllWindows()

if __name__ == "__main__":
    BorderlessTester()