//! Explicitly started ScreenCast sessions. No X11 target or global coordinates
//! are fabricated: frames describe only the selected stream's pixel dimensions.

pub mod connection;
mod pixels;
mod receiver;
mod session;

pub use pixels::copy_mapped_frame;

use crate::capture::frame::CapturedFrame;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortalStatus {
    Selecting,
    Streaming,
    Stopped,
    Failed(String),
}

pub(super) struct Mailbox {
    status: PortalStatus,
    frame: CapturedFrame,
    ready: bool,
    received: Option<Instant>,
    sequence: u64,
    format_generation: u64,
    accepting: bool,
}

impl Mailbox {
    fn new() -> Self {
        Self {
            status: PortalStatus::Selecting,
            frame: CapturedFrame::default(),
            ready: false,
            received: None,
            sequence: 0,
            format_generation: 0,
            accepting: true,
        }
    }

    fn invalidate(&mut self) {
        self.ready = false;
        self.received = None;
    }

    fn finish(&mut self, result: Result<(), String>) {
        self.invalidate();
        self.status = match result {
            Ok(()) => PortalStatus::Stopped,
            Err(error) => PortalStatus::Failed(error),
        };
    }

    fn take(&mut self, output: &mut CapturedFrame, now: Instant) -> Option<FrameStamp> {
        if self.status != PortalStatus::Streaming || !self.ready || !self.accepting {
            return None;
        }
        self.ready = false;
        let received = self.received?;
        if now.saturating_duration_since(received) > Duration::from_secs(2) {
            return None;
        }
        std::mem::swap(&mut self.frame, output);
        Some(FrameStamp {
            sequence: self.sequence,
            format_generation: self.format_generation,
            received,
        })
    }
}

/// Each successful take consumes a distinct delivered frame. Generation changes
/// require the caller to reset detection history before using the new frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameStamp {
    pub sequence: u64,
    pub format_generation: u64,
    pub received: Instant,
}

pub(super) enum StreamCommand {
    Active(bool),
    Stop,
}

/// One session, one bounded frame slot, and an explicitly cancellable receiver.
/// Dropping joins on the owning worker, never on the UI thread.
pub struct PortalCapture {
    shared: Arc<Mutex<Mailbox>>,
    demand: Arc<AtomicBool>,
    stop: tokio::sync::watch::Sender<bool>,
    commands: pipewire::channel::Sender<StreamCommand>,
    thread: Option<std::thread::JoinHandle<()>>,
    active: bool,
}

impl PortalCapture {
    pub fn start(max_fps: u32) -> Result<Self, String> {
        if !(1..=30).contains(&max_fps) {
            return Err("Portal FPS must be in 1..=30".into());
        }
        let shared = Arc::new(Mutex::new(Mailbox::new()));
        let demand = Arc::new(AtomicBool::new(true));
        let (stop, stop_rx) = tokio::sync::watch::channel(false);
        let (commands, commands_rx) = pipewire::channel::channel();
        let thread_shared = shared.clone();
        let thread_demand = demand.clone();
        let stream_stop = commands.clone();
        let thread = std::thread::Builder::new()
            .name("portal-session".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())
                    .and_then(|runtime| {
                        runtime.block_on(session::run(
                            stop_rx,
                            commands_rx,
                            stream_stop,
                            thread_shared.clone(),
                            thread_demand,
                            max_fps,
                        ))
                    });
                thread_shared
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .finish(result);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            shared,
            demand,
            stop,
            commands,
            thread: Some(thread),
            active: true,
        })
    }

    pub fn status(&self) -> PortalStatus {
        self.shared
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .status
            .clone()
    }

    pub fn request_frame(&self) {
        self.demand.store(true, Ordering::Release);
    }

    pub fn take_frame(&self, output: &mut CapturedFrame) -> Option<FrameStamp> {
        self.shared
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take(output, Instant::now())
    }

    /// Suspend the PipeWire consumer on focus loss or while awaiting confirmation.
    /// This is a request to the server, not a claim about compositor workload.
    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        {
            let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
            shared.accepting = false;
            shared.invalidate();
        }
        self.demand.store(active, Ordering::Release);
        let _ = self.commands.send(StreamCommand::Active(active));
    }
}

impl Drop for PortalCapture {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        let _ = self.commands.send(StreamCommand::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delivered() -> Mailbox {
        Mailbox {
            status: PortalStatus::Streaming,
            frame: CapturedFrame {
                width: 1,
                height: 1,
                bgra: vec![1, 2, 3, 255],
            },
            ready: true,
            received: Some(Instant::now()),
            sequence: 7,
            format_generation: 2,
            accepting: true,
        }
    }

    #[test]
    fn delivered_frame_is_consumed_once() {
        let mut mailbox = delivered();
        let mut frame = CapturedFrame::default();
        let stamp = mailbox.take(&mut frame, Instant::now()).unwrap();
        assert_eq!(stamp.sequence, 7);
        assert_eq!(stamp.format_generation, 2);
        assert_eq!(frame.bgra, [1, 2, 3, 255]);
        assert!(mailbox.take(&mut frame, Instant::now()).is_none());
    }

    #[test]
    fn stale_paused_and_closed_frames_are_not_observations() {
        let mut frame = CapturedFrame::default();
        let mut mailbox = delivered();
        assert!(mailbox
            .take(&mut frame, Instant::now() + Duration::from_secs(3))
            .is_none());
        let mut mailbox = delivered();
        mailbox.invalidate();
        assert!(mailbox.take(&mut frame, Instant::now()).is_none());
        for result in [Ok(()), Err("closed".into())] {
            let mut mailbox = delivered();
            mailbox.finish(result);
            assert!(mailbox.take(&mut frame, Instant::now()).is_none());
        }
        assert!(Mailbox::new().take(&mut frame, Instant::now()).is_none());
    }
}
