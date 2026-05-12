"""Win32 thread-safe signal bridge for the sync window."""

from __future__ import annotations

import queue
from typing import Any, Callable

import win32api
import win32con
import win32gui

# Custom window messages for sync worker updates
WM_SYNC_SCAN_FINISHED = win32con.WM_USER + 101
WM_SYNC_ROW_STATUS = win32con.WM_USER + 102
WM_SYNC_ACTION_FINISHED = win32con.WM_USER + 103


class Win32SyncSignals:
    """Compatibility layer for SyncActionsMixin using Win32 PostMessage."""

    def __init__(self, hwnd: int) -> None:
        self.hwnd = hwnd
        self.scan_finished = _Win32Signal(hwnd, WM_SYNC_SCAN_FINISHED)
        self.row_status_changed = _Win32Signal(hwnd, WM_SYNC_ROW_STATUS)
        self.action_finished = _Win32Signal(hwnd, WM_SYNC_ACTION_FINISHED)


class _Win32Signal:
    def __init__(self, hwnd: int, msg: int) -> None:
        self.hwnd = hwnd
        self.msg = msg
        self._queue: queue.Queue[tuple[Any, ...]] = queue.Queue()

    def emit(self, *args: Any) -> None:
        """Thread-safe emit: put args in queue and notify UI thread."""
        self._queue.put(args)
        if self.hwnd:
            win32gui.PostMessage(self.hwnd, self.msg, 0, 0)

    def connect(self, callback: Callable[..., Any]) -> None:
        """Dummy connect for mixin compatibility - real dispatch in wnd_proc."""
        pass

    def pull(self) -> tuple[Any, ...] | None:
        """Retrieve the next set of arguments from the queue."""
        try:
            return self._queue.get_nowait()
        except queue.Empty:
            return None
