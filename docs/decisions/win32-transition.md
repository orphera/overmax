# Decision: Win32 Native Overlay Transition

## Background
To achieve a "zero-dependency" look and feel and minimize resource usage, the main overlay and auxiliary windows were transitioned from PyQt6 to native Win32.

## Phases

### Phase 5-6: Main Overlay Production & Look-and-Feel
- [x] Win32 rendering engine implementation (GDI+ then DirectWrite).
- [x] Capture exclusion (`SetWindowDisplayAffinity`) implementation.
- [x] Visual alignment with PyQt6 (fonts, colors, spacing).
- [x] Double buffering for flicker-free rendering.

### Phase 7: UI Infrastructure Setup
- [x] Extract reusable Win32 logic (DPI, windowing, input).
- [x] Create `infra/gui` for project-neutral helpers.

### Phase 8-11: Auxiliary Window Migration
- [x] Phase 8: Status/Update Window migration.
- [x] Phase 9: Debug Window migration (log bridge, native controls).
- [x] Phase 10: Settings Window migration (Tab control, native sliders).
- [x] Phase 11: Sync Window migration (Thread-safe bridge, candidate rows).

### Phase 12: UI Polish & Layout Engine
- [x] Introduction of `LayoutContext` for adaptive positioning.
- [x] Card-based layouts and native Tab controls for improved aesthetics.

## Design Principles

### IDE-Friendly Color Definitions
- Win32 API는 `0xBBGGRR` 형식을 사용하지만, IDE(VS Code 등)의 컬러 프리뷰 기능을 활용하기 위해 `0xRRGGBB` 또는 `#RRGGBB` 주석을 적극적으로 사용한다.
- `infra/gui/theme.py`에 `hex_rgb(0xRRGGBB)`와 같은 변환 헬퍼를 두어 의도를 명확히 하고 시각적 확인을 용이하게 한다.
- 예: `TEXT_COLOR = hex_rgb(0x1F2937)  # #1F2937`
