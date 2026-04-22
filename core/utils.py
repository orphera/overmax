"""
Utility functions for Overmax.
"""

import ctypes
import os

_MB_OK = 0x0
_MB_OKCANCEL = 0x1
_MB_ICONERROR = 0x10
_MB_ICONQUESTION = 0x20
_MB_ICONINFORMATION = 0x40
_MB_TOPMOST = 0x00040000
_IDOK = 1


def show_error_message(message: str, title: str = "Overmax Error"):
    """
    Displays a Windows message box with an error icon.
    
    Args:
        message (str): The message to display.
        title (str): The title of the message box.
    """
    if os.name == "nt":
        flags = _MB_OK | _MB_ICONERROR | _MB_TOPMOST
        ctypes.windll.user32.MessageBoxW(0, message, title, flags)
    else:
        # Fallback to console print for non-Windows platforms
        print(f"[{title}] {message}")


def show_info_message(message: str, title: str = "Overmax"):
    """
    Displays a Windows message box with an information icon.
    """
    if os.name == "nt":
        flags = _MB_OK | _MB_ICONINFORMATION | _MB_TOPMOST
        ctypes.windll.user32.MessageBoxW(0, message, title, flags)
    else:
        print(f"[{title}] {message}")


def ask_ok_cancel(message: str, title: str = "Overmax") -> bool:
    if os.name != "nt":
        print(f"[{title}] {message}")
        return True
    flags = _MB_OKCANCEL | _MB_ICONQUESTION | _MB_TOPMOST
    clicked = ctypes.windll.user32.MessageBoxW(0, message, title, flags)
    return clicked == _IDOK


def show_info_message_timeout(message: str, title: str = "Overmax", timeout_ms: int = 0):
    if os.name != "nt":
        print(f"[{title}] {message}")
        return
    if timeout_ms <= 0:
        show_info_message(message, title=title)
        return
    _show_messagebox_timeout(message, title, timeout_ms)


def _show_messagebox_timeout(message: str, title: str, timeout_ms: int):
    flags = _MB_OK | _MB_ICONINFORMATION | _MB_TOPMOST
    try:
        fn = ctypes.windll.user32.MessageBoxTimeoutW
        fn(0, message, title, flags, 0, int(timeout_ms))
    except Exception:
        show_info_message(message, title=title)


def check_environment():
    """
    Checks if the current environment is supported.
    If not, shows an error message and exits the program.
    """
    import sys
    
    # Minimum required Windows 10 Build (1803 / Anniversary Update is 14393, 1803 is 17134)
    # Build 17134 is the recommended minimum for stable WinRT OCR interop in Python.
    MIN_WIN10_BUILD = 17134
    
    # 1. OS Check
    if os.name != "nt":
        show_error_message("이 프로그램은 Windows 전용입니다.")
        sys.exit(1)
        
    # 2. Windows Version & Build Check
    version = sys.getwindowsversion()
    
    # Major version must be at least 10
    if version.major < 10:
        show_error_message(
            "Windows 10 이상의 OS가 필요합니다.\n\n"
            f"현재 유저님의 버전: Windows {version.major}.{version.minor}"
        )
        sys.exit(1)
        
    # Build version must meet the minimum for OCR reliability
    if version.build < MIN_WIN10_BUILD:
        show_error_message(
            "Windows 10 버전 1803 (Build 17134) 이상의 OS가 필요합니다.\n"
            "(Windows OCR API 연동 및 시스템 안정성을 위함입니다.)\n\n"
            f"현재 유저님의 빌드: {version.build}\n"
            "Windows 업데이트를 진행해 주세요."
        )
        sys.exit(1)
