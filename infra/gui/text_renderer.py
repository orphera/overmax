"""DirectWrite-backed text drawing for Win32 GUI surfaces."""

from __future__ import annotations

import ctypes
from ctypes import wintypes
from dataclasses import dataclass
from typing import Callable

HRESULT = ctypes.c_long
UINT32 = ctypes.c_uint32
FLOAT = ctypes.c_float

D2D1_FACTORY_TYPE_SINGLE_THREADED = 0
D2D1_RENDER_TARGET_TYPE_DEFAULT = 0
D2D1_RENDER_TARGET_USAGE_NONE = 0
D2D1_FEATURE_LEVEL_DEFAULT = 0
D2D1_ALPHA_MODE_PREMULTIPLIED = 1
DXGI_FORMAT_B8G8R8A8_UNORM = 87
D2D1_DRAW_TEXT_OPTIONS_CLIP = 0x2
DWRITE_FACTORY_TYPE_SHARED = 0
DWRITE_FONT_STYLE_NORMAL = 0
DWRITE_FONT_STRETCH_NORMAL = 5
DWRITE_MEASURING_MODE_NATURAL = 0
DWRITE_TEXT_ALIGNMENT_LEADING = 0
DWRITE_TEXT_ALIGNMENT_TRAILING = 1
DWRITE_TEXT_ALIGNMENT_CENTER = 2
DWRITE_PARAGRAPH_ALIGNMENT_CENTER = 2
DWRITE_WORD_WRAPPING_NO_WRAP = 1


class D2D1_PIXEL_FORMAT(ctypes.Structure):
    _fields_ = (("format", UINT32), ("alphaMode", UINT32))


class D2D1_RENDER_TARGET_PROPERTIES(ctypes.Structure):
    _fields_ = (
        ("type", UINT32),
        ("pixelFormat", D2D1_PIXEL_FORMAT),
        ("dpiX", FLOAT),
        ("dpiY", FLOAT),
        ("usage", UINT32),
        ("minLevel", UINT32),
    )


class D2D1_COLOR_F(ctypes.Structure):
    _fields_ = (("r", FLOAT), ("g", FLOAT), ("b", FLOAT), ("a", FLOAT))


class D2D1_RECT_F(ctypes.Structure):
    _fields_ = (("left", FLOAT), ("top", FLOAT), ("right", FLOAT), ("bottom", FLOAT))


@dataclass(frozen=True)
class TextDiagnostics:
    directwrite_available: bool
    directwrite_used: bool


class DirectWriteTextRenderer:
    def __init__(
        self,
        scale: float = 1.0,
        target_size: tuple[int, int] = (1, 1),
        font_cell_height: Callable[[int], int] | None = None,
        font_weight: Callable[[int], int] | None = None,
    ) -> None:
        self._scale = scale
        self._target_size = target_size
        self._font_cell_height = font_cell_height or _default_font_cell_height
        self._font_weight = font_weight or _default_font_weight
        self._factory = 0
        self._d2d_factory = 0
        self._target = 0
        self._formats: dict[tuple[str, int, int, str], int] = {}
        self._used = False
        self._initialize()

    @property
    def available(self) -> bool:
        return bool(self._factory and self._d2d_factory and self._target)

    @property
    def used(self) -> bool:
        return self._used

    def set_scale(self, scale: float) -> None:
        if abs(self._scale - scale) < 0.001:
            return
        self._scale = max(0.1, scale)
        self.destroy()
        self._target = _create_dc_render_target(self._d2d_factory)

    def set_target_size(self, target_size: tuple[int, int]) -> None:
        self._target_size = target_size

    def draw_text(
        self,
        hdc: int,
        text: str,
        rect: tuple[int, int, int, int],
        color: int,
        size: int,
        weight: int,
        face: str,
        align_right: bool = False,
        align_center: bool = False,
    ) -> bool:
        if not self.available:
            return False
        try:
            self._draw_text(
                hdc, text, rect, color, size, weight, face, align_right, align_center
            )
            self._used = True
            return True
        except OSError:
            return False

    def diagnostics(self) -> TextDiagnostics:
        return TextDiagnostics(self.available, self.used)

    def destroy(self) -> None:
        for text_format in self._formats.values():
            _release(text_format)
        self._formats.clear()
        if self._target:
            _release(self._target)
            self._target = 0

    def close(self) -> None:
        self.destroy()
        if self._factory:
            _release(self._factory)
            self._factory = 0
        if self._d2d_factory:
            _release(self._d2d_factory)
            self._d2d_factory = 0

    def _initialize(self) -> None:
        try:
            self._factory = _create_dwrite_factory()
            self._d2d_factory = _create_d2d_factory()
            self._target = _create_dc_render_target(self._d2d_factory)
        except OSError:
            self.close()

    def _draw_text(
        self,
        hdc: int,
        text: str,
        rect: tuple[int, int, int, int],
        color: int,
        size: int,
        weight: int,
        face: str,
        align_right: bool,
        align_center: bool,
    ) -> None:
        text_format = self._text_format(face, size, weight, align_right, align_center)
        brush = _create_solid_brush(self._target, color)
        try:
            _bind_dc(self._target, hdc, _target_rect(self._scale, self._target_size))
            _begin_draw(self._target)
            _draw_text(self._target, text, text_format, _rect_f(rect), brush)
            _end_draw(self._target)
        finally:
            _release(brush)

    def _text_format(
        self,
        face: str,
        size: int,
        weight: int,
        align_right: bool,
        align_center: bool,
    ) -> int:
        align = _text_alignment(align_right, align_center)
        key = (face, size, weight, str(align))
        if key not in self._formats:
            font_size = _font_size(size, self._scale, self._font_cell_height)
            self._formats[key] = _create_text_format(
                self._factory, face, font_size, self._font_weight(weight), align
            )
        return self._formats[key]


def _create_dwrite_factory() -> int:
    factory = ctypes.c_void_p()
    hr = ctypes.windll.dwrite.DWriteCreateFactory(
        DWRITE_FACTORY_TYPE_SHARED,
        ctypes.byref(_guid("b859ee5a-d838-4b5b-a2e8-1adc7d93db48")),
        ctypes.byref(factory),
    )
    _check_hr(hr)
    return factory.value or 0


def _create_d2d_factory() -> int:
    factory = ctypes.c_void_p()
    hr = ctypes.windll.d2d1.D2D1CreateFactory(
        D2D1_FACTORY_TYPE_SINGLE_THREADED,
        ctypes.byref(_guid("06152247-6f50-465a-9245-118bfd3b6007")),
        None,
        ctypes.byref(factory),
    )
    _check_hr(hr)
    return factory.value or 0


def _create_dc_render_target(factory: int) -> int:
    if not factory:
        return 0
    props = D2D1_RENDER_TARGET_PROPERTIES(
        D2D1_RENDER_TARGET_TYPE_DEFAULT,
        D2D1_PIXEL_FORMAT(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED),
        96.0,
        96.0,
        D2D1_RENDER_TARGET_USAGE_NONE,
        D2D1_FEATURE_LEVEL_DEFAULT,
    )
    target = ctypes.c_void_p()
    hr = _com_call(
        factory, 16, HRESULT, ctypes.c_void_p, ctypes.POINTER(D2D1_RENDER_TARGET_PROPERTIES),
        ctypes.POINTER(ctypes.c_void_p),
    )(factory, ctypes.byref(props), ctypes.byref(target))
    _check_hr(hr)
    return target.value or 0


def _create_text_format(factory: int, face: str, size: float, weight: int, align: int) -> int:
    text_format = ctypes.c_void_p()
    hr = _com_call(
        factory, 15, HRESULT, ctypes.c_void_p, wintypes.LPCWSTR, ctypes.c_void_p,
        UINT32, UINT32, UINT32, FLOAT, wintypes.LPCWSTR,
        ctypes.POINTER(ctypes.c_void_p),
    )(
        factory, face, None, weight, DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL, FLOAT(size), "ko-kr", ctypes.byref(text_format),
    )
    _check_hr(hr)
    _set_text_format_options(text_format.value, align)
    return text_format.value or 0


def _set_text_format_options(text_format: int, align: int) -> None:
    _check_hr(_com_call(text_format, 3, HRESULT, ctypes.c_void_p, UINT32)(text_format, align))
    _check_hr(
        _com_call(text_format, 4, HRESULT, ctypes.c_void_p, UINT32)(
            text_format, DWRITE_PARAGRAPH_ALIGNMENT_CENTER
        )
    )
    _check_hr(
        _com_call(text_format, 5, HRESULT, ctypes.c_void_p, UINT32)(
            text_format, DWRITE_WORD_WRAPPING_NO_WRAP
        )
    )


def _create_solid_brush(target: int, color: int) -> int:
    brush = ctypes.c_void_p()
    color_f = _color_f(color)
    hr = _com_call(
        target, 8, HRESULT, ctypes.c_void_p, ctypes.POINTER(D2D1_COLOR_F),
        ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p),
    )(target, ctypes.byref(color_f), None, ctypes.byref(brush))
    _check_hr(hr)
    return brush.value or 0


def _bind_dc(target: int, hdc: int, rect: tuple[int, int, int, int]) -> None:
    bind_rect = wintypes.RECT(rect[0], rect[1], rect[2], rect[3])
    hr = _com_call(
        target, 57, HRESULT, ctypes.c_void_p, wintypes.HDC, ctypes.POINTER(wintypes.RECT)
    )(target, hdc, ctypes.byref(bind_rect))
    _check_hr(hr)


def _begin_draw(target: int) -> None:
    _com_call(target, 48, None, ctypes.c_void_p)(target)


def _draw_text(
    target: int, text: str, text_format: int, rect: D2D1_RECT_F, brush: int
) -> None:
    _com_call(
        target, 27, None, ctypes.c_void_p, wintypes.LPCWSTR, UINT32,
        ctypes.c_void_p, ctypes.POINTER(D2D1_RECT_F), ctypes.c_void_p,
        UINT32, UINT32,
    )(
        target, text, len(text), text_format, ctypes.byref(rect), brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL,
    )


def _end_draw(target: int) -> None:
    tag1 = ctypes.c_uint64()
    tag2 = ctypes.c_uint64()
    hr = _com_call(
        target, 49, HRESULT, ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_uint64), ctypes.POINTER(ctypes.c_uint64),
    )(target, ctypes.byref(tag1), ctypes.byref(tag2))
    _check_hr(hr)


def _release(ptr: int) -> None:
    if ptr:
        _com_call(ptr, 2, UINT32, ctypes.c_void_p)(ptr)


def _com_call(ptr: int, index: int, result: object, *args: object) -> object:
    vtable = ctypes.cast(ptr, ctypes.POINTER(ctypes.POINTER(ctypes.c_void_p))).contents
    return ctypes.WINFUNCTYPE(result, *args)(vtable[index])


def _check_hr(hr: int) -> None:
    if hr < 0:
        raise ctypes.WinError(ctypes.c_long(hr).value)


def _guid(value: str) -> ctypes.c_byte * 16:
    guid = (ctypes.c_byte * 16)()
    hr = ctypes.windll.ole32.CLSIDFromString(str("{" + value + "}"), ctypes.byref(guid))
    _check_hr(hr)
    return guid


def _rect_f(rect: tuple[int, int, int, int]) -> D2D1_RECT_F:
    return D2D1_RECT_F(float(rect[0]), float(rect[1]), float(rect[2]), float(rect[3]))


def _target_rect(scale: float, target_size: tuple[int, int]) -> tuple[int, int, int, int]:
    width, height = target_size
    return 0, 0, max(1, round(width * scale)), max(1, round(height * scale))


def _color_f(color: int) -> D2D1_COLOR_F:
    red = (color & 0xFF) / 255.0
    green = ((color >> 8) & 0xFF) / 255.0
    blue = ((color >> 16) & 0xFF) / 255.0
    return D2D1_COLOR_F(red, green, blue, 1.0)


def _font_size(size: int, scale: float, cell_height: Callable[[int], int]) -> float:
    return max(1.0, float(cell_height(size)) * scale)


def _default_font_cell_height(size: int) -> int:
    return size


def _default_font_weight(weight: int) -> int:
    return weight


def _text_alignment(align_right: bool, align_center: bool) -> int:
    if align_right:
        return DWRITE_TEXT_ALIGNMENT_TRAILING
    if align_center:
        return DWRITE_TEXT_ALIGNMENT_CENTER
    return DWRITE_TEXT_ALIGNMENT_LEADING
