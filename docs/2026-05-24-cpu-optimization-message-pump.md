# Task Specification: CPU Optimization via State-Driven Message Pumping

## 1. Objective

Current codebase suffers from high CPU usage. The main root cause is that the background threads (`DetectionWorker` and async tasks) unconditionally trigger `ctx.request_repaint()` on the `egui` UI thread even when no UI-visible data or states have changed.

We will transition the application into a highly efficient, event-driven message pumping structure where `request_repaint()` is strictly throttled and only called when an actual visual or state update occurs.

---

## 2. Targeted Files & Current Code Analysis

### A. `rust/overmax_app/src/detection_worker.rs`

* **Current Issue:** Inside `DetectionWorker::tick`, after running `pipeline.detect(&frame, ...)`, `self.request_repaint()` is called unconditionally every **120ms** (`ACTIVE_SLEEP`) while the game window is active.
* **Impact:** `egui` is forced to re-layout and re-render the entire viewport continuously, consuming heavy CPU cycles even when the user is sitting on the same song or screen.

### B. `rust/overmax_app/src/native_app.rs`

* **Current Issue:** Inside background worker threads like `spawn_fetch` or `spawn_upload`, `ctx.request_repaint()` is called inside a loop or redundant scopes (e.g., calling repaint for every single button configuration loop).
* **Impact:** Unnecessary redundant repaint requests during batch operations.

---

## 3. Detailed Refactoring Instructions

### Task 1: Add State Tracking to `DetectionWorker`

Modify `DetectionWorker` to track the *previous* detection state and only invoke `request_repaint()` if the essential fields of `DetectionOutput` change.

1. Open `rust/overmax_app/src/detection_worker.rs`.
2. Update the `DetectionWorker` struct to hold state cache fields:
```rust
struct DetectionWorker {
    // ... existing fields ...
    last_song_id: Option<u32>,
    last_is_song_select: bool,
    last_logo_detected: bool,
    last_jacket_status: crate::detection_pipeline::JacketMatchStatus,
}

```


3. Initialize these fields in `DetectionWorker::new()` with default/empty values.
4. Modify `DetectionWorker::tick` to compare the new `DetectionOutput` with the cached fields before calling `request_repaint()`:
```rust
// Inside tick() -> match capturer.capture_bgra(rect) -> Ok(frame)
let out = pipeline.detect(&frame, self.start.elapsed().as_secs_f64());

// Determine if visual/state changes occurred
let state_changed = out.current_song_id != self.last_song_id
    || out.is_song_select != self.last_is_song_select
    || out.logo_detected != self.last_logo_detected
    || std::mem::discriminant(&out.jacket_status) != std::mem::discriminant(&self.last_jacket_status);

// Update cache
self.last_song_id = out.current_song_id;
self.last_is_song_select = out.is_song_select;
self.last_logo_detected = out.logo_detected;
self.last_jacket_status = out.jacket_status.clone();

// Send to channel
let _ = self.detection_tx.send(out);

// CRITICAL: Only repaint when state actually changes!
if state_changed {
    self.request_repaint();
}

```



### Task 2: Optimize Batch Redundant Repaints in `native_app.rs`

Throttling `request_repaint` inside background async loop scopes.

1. Open `rust/overmax_app/src/native_app.rs`.
2. Locate `fn spawn_fetch(&self, steam_id: String, v_id: String, button: i32, ctx: egui::Context)`.
3. Move `ctx.request_repaint()` **outside** the `for b in buttons` loop so that it is called exactly **once** after all configurations are fetched and saved to cache.
```rust
std::thread::spawn(move || {
    let buttons = if button == 0 { vec![4, 5, 6, 8] } else { vec![button] };
    let mut any_success = false;
    for b in buttons {
        // ... fetch and save logic ...
        if success { any_success = true; }
    }
    // Repaint once after the entire batch operation is done
    if any_success {
        ctx.request_repaint();
    }
});

```



### Task 3: Continuous Profiling Verification

* Ensure no `ctx.request_repaint()` calls are placed inside the main UI loop (`NativeApp::update` or draining functions) without an explicit incoming message event matching it.
* Ensure `while let Ok(...) = rx.try_recv()` blocks completely empty the queue cleanly when the UI thread is woken up by the worker threads.

---

## 4. Acceptance Criteria

1. **Compilation:** The project must compile successfully (`cargo build`) without any data race or ownership errors.
2. **Idle CPU Usage:** When the game window is active but sitting idle on a single song, the Overmax app's CPU usage should drop to **near 0%** (Reactive Mode).
3. **Responsiveness:** When the user switches songs in the game, the overlay UI must immediately catch up and re-render without any perceived lag or missed frames.