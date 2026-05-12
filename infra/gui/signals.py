"""Thread-safe signal implementation to replace pyqtSignal."""

from __future__ import annotations

import threading
from typing import Any, Callable, Generic, TypeVar

T = TypeVar("T")


class Signal:
    """Simple thread-safe signal for project-wide event handling."""

    def __init__(self) -> None:
        self._callbacks: list[Callable[..., Any]] = []
        self._lock = threading.Lock()

    def connect(self, callback: Callable[..., Any]) -> None:
        with self._lock:
            if callback not in self._callbacks:
                self._callbacks.append(callback)

    def disconnect(self, callback: Callable[..., Any]) -> None:
        with self._lock:
            if callback in self._callbacks:
                self._callbacks.remove(callback)

    def emit(self, *args: Any, **kwargs: Any) -> None:
        # Create a copy to avoid holding the lock during callback execution
        with self._lock:
            callbacks = list(self._callbacks)

        for callback in callbacks:
            try:
                callback(*args, **kwargs)
            except Exception as e:
                print(f"[Signal] Error in callback {callback}: {e}")
