# Decision: Qt Runtime & UI Structural Cleanup

## Background
The project aimed to decouple core logic from the Qt framework to prepare for a native Win32 transition and reduce binary size.

## Phases

### Phase 0-1: Runtime Cleanup
- [x] Investigate PyQt6 usage and footprint.
- [x] Optimize `overmax.spec` to exclude unused Qt modules.
- [x] Establish baseline for distribution size.

### Phase 2: UI Structure Refactoring
- [x] Decompose large files (`settings_window.py`, `sync_window.py`).
- [x] Clarify `controller.py` orchestration boundaries.
- [x] Enforce 500-line limit per file.

### Phase 3: Boundary Decoupling
- [x] Move UI-independent logic (verified state -> payload) to a separate layer.
- [x] Isolate Qt signal bridges to the display layer.
- [x] Verify no Qt imports in `detection`, `capture`, or `core`.

### Phase 4: Alternative UI Spike (Win32 Transition)
- [x] Evaluate PySide6 vs. Win32 native.
- [x] Decision: Transition to Win32 native for main overlay due to size and performance.
- [x] Smoke test Win32 topmost/layered/capture exclusion properties.
