//! Manual smoke test: choose a window, receive three fresh frames, then close.
//! No images or restore tokens are written. --cancel tests picker cancellation.
#[cfg(all(target_os = "linux", feature = "portal-capture"))]
fn main() -> Result<(), String> {
    use overmax_engine::capture::{
        frame::CapturedFrame,
        portal::{PortalCapture, PortalStatus},
    };
    use std::time::{Duration, Instant};
    let cancel = std::env::args().any(|arg| arg == "--cancel");
    let capture = PortalCapture::start(10)?;
    let start = Instant::now();
    let mut frame = CapturedFrame::default();
    let mut received = 0;
    eprintln!("Choose one window in the Portal picker (timeout: 30 seconds).");
    while start.elapsed() < Duration::from_secs(30) {
        if cancel && start.elapsed() >= Duration::from_secs(1) {
            break;
        }
        if let Some(stamp) = capture.take_frame(&mut frame) {
            eprintln!(
                "frame={} generation={} size={}x{} bytes={}",
                stamp.sequence,
                stamp.format_generation,
                frame.width,
                frame.height,
                frame.bgra.len()
            );
            received += 1;
            if received == 3 {
                break;
            }
            capture.request_frame();
        }
        match capture.status() {
            PortalStatus::Failed(error) => return Err(error),
            PortalStatus::Stopped => return Err("Session stopped".into()),
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let close_started = Instant::now();
    drop(capture);
    eprintln!("Session released in {:?}", close_started.elapsed());
    if !cancel && received != 3 {
        return Err("Timed out waiting for three frames".into());
    }
    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "portal-capture")))]
fn main() {
    eprintln!("This probe requires Linux and --features portal-capture");
}
