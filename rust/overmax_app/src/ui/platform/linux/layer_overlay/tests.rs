use super::{
    calculate_margin, game_anchor_on_output, output_render_scale, overlay_props, panel_margin,
    panel_size, parse_foreign_toplevel_states, physical_size, retained_panel_size,
    select_committed_output, select_output_by_overlap, unique_matching_toplevel,
    uses_manual_position, ForeignToplevelState, LinuxLayerOverlayHandle, LinuxOverlaySnapshot,
    PublishedSnapshots,
};
use overmax_capture::{FocusState, GameGeometry, PresentationObservation};
use overmax_core::{GameSessionState, SceneType};
use overmax_data::{RecommendResult, RecordDB, RecordManager};
use overmax_frame::{WindowRect, WindowSnapshot};
use std::io::Read as _;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[test]
fn calculates_snap_and_clamps_manual_position() {
    let rect = WindowRect {
        left: 0,
        top: 0,
        width: 1920,
        height: 1080,
    };
    assert_eq!(
        calculate_margin("bottom_right", None, Some(rect), (360, 380)),
        (1544, 684)
    );
    assert_eq!(
        calculate_margin(
            "bottom_right",
            None,
            Some(WindowRect {
                left: 0,
                top: 0,
                width: 2560,
                height: 1440,
            }),
            (360, 380),
        ),
        (2184, 1044)
    );
    assert_eq!(
        calculate_margin("manual", Some((-20, 30)), Some(rect), (360, 380)),
        (0, 30)
    );
    assert_eq!(panel_size(None), (320, 116));
}

#[test]
fn scales_fractional_buffers() {
    assert_eq!(physical_size((360, 380), 1.25), (450, 475));
    assert_eq!(output_render_scale(1.0, Some(1.25), true, false), 1.25);
    assert_eq!(output_render_scale(1.5, Some(1.25), true, true), 1.5);
}

#[test]
fn selects_only_unambiguous_committed_output() {
    assert_eq!(select_committed_output(&[1], None), Some(&1));
    assert_eq!(select_committed_output(&[1, 2], Some(&2)), Some(&2));
    assert_eq!(select_committed_output(&[1, 2], Some(&3)), None);
    assert_eq!(select_committed_output::<i32>(&[], Some(&1)), None);
}

#[test]
fn falls_back_to_the_output_with_the_largest_game_overlap() {
    let left = WindowRect {
        left: 0,
        top: 0,
        width: 1920,
        height: 1080,
    };
    let right = WindowRect {
        left: 1920,
        top: 0,
        width: 2560,
        height: 1440,
    };
    assert_eq!(
        select_output_by_overlap(right, [("left", left), ("right", right)].into_iter()),
        Some("right")
    );
    assert_eq!(
        select_output_by_overlap(
            WindowRect {
                left: -500,
                top: -500,
                width: 100,
                height: 100,
            },
            [("left", left), ("right", right)].into_iter(),
        ),
        None
    );
}

#[test]
fn delayed_repaint_becomes_a_poll_timeout() {
    let now = Instant::now();
    let timeout = super::repaint_timeout(Some(now + Duration::from_millis(25)), now)
        .expect("scheduled repaint must set a timeout");
    assert_eq!(
        Duration::try_from(timeout).unwrap(),
        Duration::from_millis(25)
    );

    let due = super::repaint_timeout(Some(now), now).expect("due repaint uses zero timeout");
    assert!(Duration::try_from(due).unwrap().is_zero());
    assert!(super::repaint_timeout(None, now).is_none());
}

#[test]
fn commits_exact_unique_foreign_toplevel_state() {
    let mut target = ForeignToplevelState::default();
    target.pending.title = Some("DJMAX RESPECT V".into());
    target.pending.activated = true;

    assert!(unique_matching_toplevel("DJMAX RESPECT V", [&target].into_iter()).is_none());
    target.committed = Some(target.pending.clone());
    assert!(
        unique_matching_toplevel("DJMAX RESPECT V", [&target].into_iter())
            .is_some_and(|snapshot| snapshot.activated)
    );

    let duplicate = ForeignToplevelState {
        committed: target.committed.clone(),
        ..Default::default()
    };
    assert!(
        unique_matching_toplevel("DJMAX RESPECT V", [&target, &duplicate].into_iter()).is_none()
    );
}

#[test]
fn presentation_epoch_survives_focus_events_but_not_lost_or_replaced_targets() {
    let mut target = super::PresentationTarget::default();
    let first = target.observe(Some(1));
    assert_eq!(target.observe(Some(1)), first);
    target.observe(None); // Even if the worker never sees this intermediate state.
    let restored = target.observe(Some(1));
    assert_ne!(restored, first);
    assert_ne!(target.observe(Some(2)), restored);
}

#[test]
fn parses_v1_focus_and_v2_fullscreen_states() {
    let raw = [2u32.to_ne_bytes(), 3u32.to_ne_bytes(), 99u32.to_ne_bytes()].concat();
    assert_eq!(parse_foreign_toplevel_states(&raw, false), (true, None));
    assert_eq!(
        parse_foreign_toplevel_states(&raw, true),
        (true, Some(true))
    );
}

#[test]
fn publish_skips_equal_display_state_and_tracks_atomic_values() {
    let settings_open = Arc::new(AtomicBool::new(false));
    let sync_open = Arc::new(AtomicBool::new(false));
    let record_db = Arc::new(RecordDB::new("unused-record.db", None));
    let record_manager = Arc::new(RecordManager::new(record_db));
    let snapshot = LinuxOverlaySnapshot {
        state: GameSessionState::detecting(),
        song_label: String::new(),
        pattern_tabs: Vec::new(),
        recommendations: RecommendResult::empty(),
        settings_open: settings_open.clone(),
        sync_open,
        scale: 1.0,
        opacity: 0.8,
        varchive_upload_needed: false,
        varchive_upload_progress: None,
        varchive_account_configured: false,
        lite_mode: false,
        always_visible: false,
        snap: "manual".to_string(),
        position: None,
        record_manager,
        session_initial_record: None,
        toast: None,
        window_snapshot: None,
        portal_presentation: None,
        game_geometry: None,
        capture_fatal: None,
        overlay_visible_override: None,
        #[cfg(any(debug_assertions, feature = "telemetry"))]
        delivery_telemetry: None,
    };
    assert_eq!(panel_size(Some(&snapshot)), (320, 116));
    let mut native = snapshot.clone();
    native.portal_presentation = Some(PresentationObservation {
        target_epoch: 1,
        focus: FocusState::Focused,
        fullscreen: Some(true),
        generation: 1,
        committed_at: Instant::now(),
    });
    native.state.scene = SceneType::Freestyle;
    native.snap = "bottom_right".into();
    assert!(native.window_snapshot.is_none());
    assert!(!super::is_degraded(&native));
    assert!(!super::is_hidden(&native, None));
    assert_eq!(panel_size(Some(&native)), (360, 406));
    assert!(!uses_manual_position(&native));
    assert!(!super::should_switch_drag_to_manual_snap(&native));
    native.portal_presentation.as_mut().unwrap().focus = FocusState::Background;
    assert!(super::is_hidden(&native, None));
    native.portal_presentation.as_mut().unwrap().fullscreen = Some(false);
    assert!(uses_manual_position(&native));
    // A windowed Portal drag must persist manual snap like the XWayland path.
    assert!(super::should_switch_drag_to_manual_snap(&native));
    native.portal_presentation = None;
    assert!(super::is_degraded(&native));
    let mut background = snapshot.clone();
    background.window_snapshot = Some(WindowSnapshot {
        window: 7,
        rect: WindowRect {
            left: -1920,
            top: -200,
            width: 1920,
            height: 1080,
        },
        foreground: false,
        fullscreen: true,
    });
    assert_eq!(panel_size(Some(&background)), (360, 406));
    assert_eq!(
        retained_panel_size((420, 240), &background, None),
        (420, 240)
    );
    background.window_snapshot.as_mut().unwrap().foreground = true;
    assert_eq!(
        retained_panel_size((420, 240), &background, None),
        (420, 240)
    );
    background.state.scene = SceneType::Freestyle;
    assert_eq!(panel_size(Some(&background)), (360, 406));
    assert_eq!(
        retained_panel_size((420, 240), &background, None),
        (360, 406)
    );
    for scene in [SceneType::Gameplay, SceneType::Paused] {
        background.state.scene = scene;
        assert!(super::is_hidden(&background, None));
        background.always_visible = true;
        assert!(!super::is_hidden(&background, None));
        background.always_visible = false;
    }
    // IPC set_overlay_visibility(false) 는 씬 상태와 무관하게 숨긴다.
    background.state.scene = SceneType::Freestyle;
    background.always_visible = true;
    background.window_snapshot.as_mut().unwrap().foreground = true;
    assert!(!super::is_hidden(&background, None));
    background.overlay_visible_override = Some(false);
    assert!(super::is_hidden(&background, None));
    background.overlay_visible_override = Some(true);
    assert!(!super::is_hidden(&background, None));
    background.overlay_visible_override = None;
    assert!(!super::is_hidden(&background, None));

    // Foreign-toplevel Done can precede the worker's debounced snapshot.
    // A stale foreground snapshot (even always_visible / IPC show) must hide.
    background.overlay_visible_override = Some(true);
    for always_visible in [false, true] {
        background.always_visible = always_visible;
        assert!(super::is_hidden(&background, Some(false)));
    }
    // Returning focus must still wait for the worker's foreground snapshot.
    background.window_snapshot.as_mut().unwrap().foreground = false;
    assert!(super::is_hidden(&background, Some(true)));
    background.window_snapshot.as_mut().unwrap().foreground = true;
    assert!(!super::is_hidden(&background, Some(true)));
    // Missing / ambiguous foreign-toplevel state retains the existing fallback.
    assert!(!super::is_hidden(&background, None));
    background.overlay_visible_override = None;
    background.state.scene = SceneType::Freestyle;
    background.window_snapshot.as_mut().unwrap().fullscreen = false;
    background.snap = "bottom_right".to_string();
    background.position = Some((25, 35));
    assert!(super::should_switch_drag_to_manual_snap(&background));
    assert_eq!(panel_size(Some(&background)), (360, 406));
    assert!(uses_manual_position(&background));
    assert!(overlay_props(&background).is_snap_manual);
    assert_eq!(panel_margin(&background, (360, 406), None, None), (25, 35));
    let anchor = game_anchor_on_output(
        &GameGeometry {
            output: "DP-1".to_string(),
            rect: WindowRect {
                left: 2120,
                top: 100,
                width: 1280,
                height: 720,
            },
            generation: 1,
        },
        "DP-1",
        WindowRect {
            left: 1920,
            top: 0,
            width: 2560,
            height: 1440,
        },
    );
    assert_eq!(
        anchor,
        Some(WindowRect {
            left: 200,
            top: 100,
            width: 1280,
            height: 720,
        })
    );
    assert_eq!(
        panel_margin(&background, (360, 406), Some((2560, 1440)), anchor),
        (1104, 398)
    );
    assert_eq!(
        game_anchor_on_output(
            &GameGeometry {
                output: "HDMI-A-1".to_string(),
                rect: WindowRect {
                    left: 0,
                    top: 0,
                    width: 1280,
                    height: 720,
                },
                generation: 1,
            },
            "DP-1",
            WindowRect {
                left: 0,
                top: 0,
                width: 1920,
                height: 1080,
            },
        ),
        None
    );
    background.window_snapshot.as_mut().unwrap().fullscreen = true;
    assert!(!uses_manual_position(&background));
    assert!(!super::should_switch_drag_to_manual_snap(&background));
    assert_eq!(
        panel_margin(&background, (360, 406), Some((1920, 1080)), None),
        (1544, 658)
    );

    let (mut reader, writer) = UnixStream::pair().expect("UnixStream pair");
    reader
        .set_nonblocking(true)
        .expect("nonblocking wake reader");
    let handle = LinuxLayerOverlayHandle {
        published: Arc::new(Mutex::new(PublishedSnapshots::default())),
        wake_writer: Arc::new(writer),
        runtime_failure: Arc::new(Mutex::new(None)),
        presentation_observation: Arc::new(Mutex::new(None)),
        runtime_telemetry: None,
    };
    let mut wake = [0u8; 8];

    handle.publish(snapshot.clone());
    assert!(
        reader
            .read(&mut wake)
            .expect("first snapshot wakes backend")
            > 0
    );
    handle.publish(snapshot.clone());
    assert_eq!(
        reader
            .read(&mut wake)
            .expect_err("duplicate display state must not wake backend")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );

    let mut changed_opacity = snapshot.clone();
    changed_opacity.opacity = 0.7;
    handle.publish(changed_opacity);
    assert!(
        reader
            .read(&mut wake)
            .expect("changed opacity wakes backend")
            > 0
    );

    settings_open.store(true, Ordering::Relaxed);
    handle.publish(snapshot);
    assert!(
        reader
            .read(&mut wake)
            .expect("changed atomic display value wakes backend")
            > 0
    );

    assert!(
        super::drain_wake_socket(&reader),
        "no wake pending, handle still connected"
    );
    drop(handle);
    assert!(
        wait_for_eof(&reader, Duration::from_millis(200)),
        "dropping every handle signals shutdown via EOF"
    );
}

fn wait_for_eof(reader: &UnixStream, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !super::drain_wake_socket(reader) {
            return true; // EOF 관측됨
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
