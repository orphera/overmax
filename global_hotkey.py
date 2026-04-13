# global_hotkey.py
import ctypes
import threading
from typing import Callable, Optional
import win32con
import win32api

class GlobalHotkey:
    """Windows RegisterHotKey 기반 전역 단축키"""

    # F9 = 0x78, F10 = 0x79 ...
    VK_MAP = {
        "F1": 0x70, "F2": 0x71, "F3": 0x72, "F4": 0x73,
        "F5": 0x74, "F6": 0x75, "F7": 0x76, "F8": 0x77,
        "F9": 0x78, "F10": 0x79, "F11": 0x7A, "F12": 0x7B,
    }

    def __init__(self):
        self._hotkeys: dict[int, Callable] = {}   # id → callback
        self._next_id = 1
        self._thread: Optional[threading.Thread] = None
        self._running = False

    def register(self, key: str, callback: Callable, modifiers: int = 0) -> bool:
        vk = self.VK_MAP.get(key.upper())
        if vk is None:
            print(f"[GlobalHotkey] 지원하지 않는 키: {key}")
            return False
        hk_id = self._next_id
        self._next_id += 1
        self._hotkeys[hk_id] = (vk, modifiers, callback)
        return True

    def start(self):
        self._running = True
        self._thread = threading.Thread(target=self._loop, daemon=True)
        self._thread.start()

    def stop(self):
        self._running = False

    def _loop(self):
        user32 = ctypes.WinDLL("user32", use_last_error=True)
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

        registered = []
        for hk_id, (vk, mod, _) in self._hotkeys.items():
            ok = user32.RegisterHotKey(None, hk_id, mod, vk)
            if ok:
                registered.append(hk_id)
                print(f"[GlobalHotkey] 등록 완료: id={hk_id}, vk=0x{vk:02X}")
            else:
                print(f"[GlobalHotkey] 등록 실패: id={hk_id} (이미 점유됐을 수 있음)")

        msg = ctypes.wintypes.MSG()
        while self._running:
            # PeekMessage로 WM_HOTKEY 수신
            if user32.PeekMessageW(ctypes.byref(msg), None, 0, 0, 1):
                if msg.message == 0x0312:  # WM_HOTKEY
                    hk_id = msg.wParam
                    if hk_id in self._hotkeys:
                        _, _, callback = self._hotkeys[hk_id]
                        callback()
            kernel32.Sleep(10)

        for hk_id in registered:
            user32.UnregisterHotKey(None, hk_id)