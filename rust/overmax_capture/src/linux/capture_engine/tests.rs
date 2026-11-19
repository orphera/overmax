use super::AdaptiveCaptureEngine;
use super::{calculate_layout, connection_failure, pixel_format_supported, CaptureFailure};
use crate::capture_engine::{CaptureEngine, CaptureErrorAction};
use crate::window_tracker::{WindowRect, WindowSnapshot};
use overmax_frame::CapturedFrame;
use x11rb::connection::Connection;
use x11rb::errors::ConnectionError;
use x11rb::protocol::xproto::{
    ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, ImageOrder, VisualClass, Window,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;

#[test]
fn calculates_checked_bgra_layout() {
    let layout = calculate_layout(1920, 1080, 32, 32).unwrap();
    assert_eq!(layout.stride, 7680);
    assert_eq!(layout.size, 8_294_400);
    assert_eq!(layout.frame_len, 8_294_400);
    assert!(calculate_layout(u32::MAX, 1080, 32, 32).is_none());
    assert!(calculate_layout(1920, 1080, 24, 32).is_none());
}

#[test]
fn accepts_only_native_bgra_truecolor() {
    assert!(pixel_format_supported(
        24,
        32,
        ImageOrder::LSB_FIRST,
        VisualClass::TRUE_COLOR,
        0x00ff_0000,
        0x0000_ff00,
        0x0000_00ff,
    ));
    assert!(!pixel_format_supported(
        24,
        32,
        ImageOrder::MSB_FIRST,
        VisualClass::TRUE_COLOR,
        0x00ff_0000,
        0x0000_ff00,
        0x0000_00ff,
    ));
}

#[test]
fn reconnects_transport_errors_but_stops_on_capability_errors() {
    assert!(matches!(
        connection_failure("test", ConnectionError::UnknownError),
        CaptureFailure::Reconnect(_)
    ));
    assert!(matches!(
        connection_failure("test", ConnectionError::UnsupportedExtension),
        CaptureFailure::Permanent(_)
    ));
}

#[test]
#[ignore = "requires DISPLAY without Composite 0.2 or MIT-SHM 1.2"]
fn unavailable_capture_backend_is_deferred_to_set_target() {
    let mut capture = AdaptiveCaptureEngine::new().expect("construct capture engine");
    let error = capture
        .set_target(None)
        .expect_err("unsupported capture backend must fail closed");
    assert!(error.contains("Composite") || error.contains("MIT-SHM"));
    assert_eq!(capture.error_action(), CaptureErrorAction::Stop);
}

#[test]
#[ignore = "requires DISPLAY with Composite >= 0.2 and MIT-SHM >= 1.2"]
fn xcomposite_shm_lifecycle() {
    let (client, screen_num) = x11rb::connect(None).expect("connect test X11 client");
    let first = create_test_window(&client, screen_num, 65, 37, 0x0011_2233);
    let mut capture = AdaptiveCaptureEngine::new().expect("construct capture engine");
    let mut frame = CapturedFrame::default();

    capture
        .set_target(Some(snapshot(first, 65, 37)))
        .expect("bind initial target");
    capture
        .capture_bgra_inplace(rect(65, 37), &mut frame)
        .expect("capture initial target");
    assert_frame(&frame, 65, 37, [0x33, 0x22, 0x11, 255], "initial");

    client
        .configure_window(first, &ConfigureWindowAux::new().width(73).height(47))
        .expect("send resize")
        .check()
        .expect("resize target");
    clear_window(&client, first, 73, 47);
    capture
        .set_target(Some(snapshot(first, 73, 47)))
        .expect("keep resized target");
    capture
        .capture_bgra_inplace(rect(73, 47), &mut frame)
        .expect("capture resized target");
    assert_frame(&frame, 73, 47, [0x33, 0x22, 0x11, 255], "resize");

    client
        .unmap_window(first)
        .expect("send unmap")
        .check()
        .expect("unmap target");
    client
        .map_window(first)
        .expect("send remap")
        .check()
        .expect("remap target");
    clear_window(&client, first, 73, 47);
    capture
        .set_target(Some(snapshot(first, 73, 47)))
        .expect("keep remapped target");
    capture
        .capture_bgra_inplace(rect(73, 47), &mut frame)
        .expect("capture remapped target");
    assert_frame(&frame, 73, 47, [0x33, 0x22, 0x11, 255], "remap");

    client
        .destroy_window(first)
        .expect("send destroy")
        .check()
        .expect("destroy target");
    let second = create_test_window(&client, screen_num, 41, 29, 0x0044_5566);
    assert_ne!(first, second);
    capture
        .set_target(Some(snapshot(second, 41, 29)))
        .expect("bind recreated target");
    capture
        .capture_bgra_inplace(rect(41, 29), &mut frame)
        .expect("capture recreated target");
    assert_frame(&frame, 41, 29, [0x66, 0x55, 0x44, 255], "recreate");

    capture.set_target(None).expect("release target");
    client
        .destroy_window(second)
        .expect("send final destroy")
        .check()
        .expect("destroy final target");
}

fn create_test_window(
    conn: &RustConnection,
    screen_num: usize,
    width: u16,
    height: u16,
    background: u32,
) -> Window {
    let screen = &conn.setup().roots[screen_num];
    let window = conn.generate_id().expect("allocate test window ID");
    conn.create_window(
        screen.root_depth,
        window,
        screen.root,
        0,
        0,
        width,
        height,
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        &CreateWindowAux::new()
            .background_pixel(background)
            .override_redirect(1u32),
    )
    .expect("send create test window")
    .check()
    .expect("create test window");
    conn.map_window(window)
        .expect("send map test window")
        .check()
        .expect("map test window");
    clear_window(conn, window, width, height);
    window
}

fn clear_window(conn: &RustConnection, window: Window, width: u16, height: u16) {
    conn.clear_area(false, window, 0, 0, width, height)
        .expect("send clear test window")
        .check()
        .expect("clear test window");
}

fn snapshot(window: Window, width: i32, height: i32) -> WindowSnapshot {
    WindowSnapshot {
        window: u64::from(window),
        rect: rect(width, height),
        foreground: true,
        fullscreen: true,
    }
}

fn rect(width: i32, height: i32) -> WindowRect {
    WindowRect {
        left: 0,
        top: 0,
        width,
        height,
    }
}

fn assert_frame(frame: &CapturedFrame, width: usize, height: usize, bgra: [u8; 4], stage: &str) {
    assert_eq!(
        (frame.width, frame.height),
        (width as i32, height as i32),
        "{stage} dimensions"
    );
    assert_eq!(frame.bgra.len(), width * height * 4, "{stage} length");
    let center = ((height / 2) * width + width / 2) * 4;
    assert_eq!(&frame.bgra[center..center + 4], &bgra, "{stage} pixel");
}
