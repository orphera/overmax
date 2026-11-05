//! Session-scoped, user-confirmed native Wayland input. The worker owns capture;
//! UI commands and a single preview are bounded and never invoke Portal on the UI.

use super::{FrameStamp, PortalCapture, PortalStatus};
use crate::capture::{
    frame::CapturedFrame,
    window_tracker::{FocusState, PresentationObservation},
};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Idle,
    Selecting,
    Preview,
    WaitingForFrame,
    Connected,
    Paused,
    XWayland,
    TargetUnavailable,
    TargetChanged,
    Stopped,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Confirmation {
    session: u64,
    target_epoch: u64,
}

pub struct Preview {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

#[derive(Clone)]
pub struct ConnectionView {
    pub status: ConnectionStatus,
    pub preview: Option<Arc<Preview>>,
    pub confirmation: Option<Confirmation>,
    /// Only populated for a confirmed session and its current observed target.
    pub presentation: Option<PresentationObservation>,
}

impl Default for ConnectionView {
    fn default() -> Self {
        Self {
            status: ConnectionStatus::Idle,
            preview: None,
            confirmation: None,
            presentation: None,
        }
    }
}

enum Command {
    Connect,
    Confirm(Confirmation),
    Disconnect,
}

#[derive(Clone)]
pub struct PortalControl {
    commands: mpsc::SyncSender<Command>,
    view: Arc<Mutex<ConnectionView>>,
}

impl PortalControl {
    pub fn view(&self) -> ConnectionView {
        self.view.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn connect(&self) -> bool {
        self.commands.try_send(Command::Connect).is_ok()
    }

    pub fn confirm(&self, confirmation: Confirmation) -> bool {
        self.commands
            .try_send(Command::Confirm(confirmation))
            .is_ok()
    }

    pub fn disconnect(&self) -> bool {
        self.commands.try_send(Command::Disconnect).is_ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetScope {
    /// The same confirmed session and target; all visual evidence is stale.
    Observations,
    /// Confirmation or target/session continuity ended; discard completion state too.
    Session,
}

pub struct Poll {
    pub handling: bool,
    pub reset: Option<ResetScope>,
    pub changed: bool,
    pub frame: Option<(FrameStamp, PresentationObservation)>,
}

pub struct PortalConnection {
    commands: mpsc::Receiver<Command>,
    shared: Arc<Mutex<ConnectionView>>,
    view: ConnectionView,
    capture: Option<PortalCapture>,
    binding: Option<Confirmation>,
    confirmed: bool,
    session: u64,
    last_x11_check: Option<Instant>,
    last_frame: Option<FrameStamp>,
    format: Option<(u64, i32, i32)>,
}

impl PortalConnection {
    pub fn new() -> (PortalControl, Self) {
        let (tx, commands) = mpsc::sync_channel(4);
        let shared = Arc::new(Mutex::new(ConnectionView::default()));
        (
            PortalControl {
                commands: tx,
                view: shared.clone(),
            },
            Self {
                commands,
                shared,
                view: ConnectionView::default(),
                capture: None,
                binding: None,
                confirmed: false,
                session: 0,
                last_x11_check: None,
                last_frame: None,
                format: None,
            },
        )
    }

    fn stop(&mut self, status: ConnectionStatus) {
        self.capture = None; // Joins and releases the session on the worker.
        self.binding = None;
        self.confirmed = false;
        self.last_frame = None;
        self.format = None;
        self.view = ConnectionView {
            status,
            ..Default::default()
        };
    }

    /// The factories are only called when needed. Starting must release the X11
    /// capture target first; an absent X11 window alone never starts Portal.
    pub fn poll(
        &mut self,
        mut x11_present: impl FnMut() -> Result<bool, String>,
        start: impl FnOnce() -> Result<PortalCapture, String>,
        observation: Option<PresentationObservation>,
        output: &mut CapturedFrame,
        now: Instant,
    ) -> Poll {
        let old_status = self.view.status.clone();
        let old_presentation = self.view.presentation;
        let old_confirmation = self.view.confirmation;
        let old_binding = self.binding.filter(|_| self.confirmed);
        let mut reset = false;
        let mut connect = false;
        let mut confirm = None;
        // A bounded batch: a later disconnect supersedes earlier queued actions.
        for _ in 0..4 {
            match self.commands.try_recv() {
                Ok(Command::Connect) => {
                    connect = true;
                    confirm = None;
                }
                Ok(Command::Confirm(value)) => confirm = Some(value),
                Ok(Command::Disconnect) => {
                    self.stop(ConnectionStatus::Idle);
                    connect = false;
                    confirm = None;
                    reset = true;
                }
                Err(_) => break,
            }
        }
        if connect {
            self.stop(ConnectionStatus::Selecting);
            reset = true;
            let eligible = observation.filter(|o| o.focus != FocusState::Unknown);
            match x11_present() {
                Ok(true) => self.stop(ConnectionStatus::XWayland),
                Ok(false) if eligible.is_some() => {
                    self.session += 1;
                    self.binding = Some(Confirmation {
                        session: self.session,
                        target_epoch: eligible.unwrap().target_epoch,
                    });
                    match start() {
                        Ok(capture) => {
                            self.capture = Some(capture);
                            self.last_x11_check = Some(now);
                        }
                        Err(error) => self.stop(ConnectionStatus::Failed(error)),
                    }
                }
                Ok(false) => self.stop(ConnectionStatus::TargetUnavailable),
                Err(error) => self.stop(ConnectionStatus::Failed(error)),
            }
        }

        if self.capture.is_some() {
            let same_target = self
                .binding
                .zip(observation)
                .is_some_and(|(binding, observed)| {
                    binding.target_epoch == observed.target_epoch
                        && observed.focus != FocusState::Unknown
                });
            if !same_target {
                self.stop(ConnectionStatus::TargetChanged);
                reset = true;
            } else if self
                .last_x11_check
                .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
            {
                self.last_x11_check = Some(now);
                match x11_present() {
                    Ok(false) => {}
                    Ok(true) => {
                        self.stop(ConnectionStatus::XWayland);
                        reset = true;
                    }
                    Err(error) => {
                        self.stop(ConnectionStatus::Failed(error));
                        reset = true;
                    }
                }
            }
        }

        let mut frame = None;
        if let Some(capture) = self.capture.as_mut() {
            match capture.status() {
                PortalStatus::Stopped => {
                    self.stop(ConnectionStatus::Stopped);
                    reset = true;
                }
                PortalStatus::Failed(error) => {
                    self.stop(ConnectionStatus::Failed(error));
                    reset = true;
                }
                PortalStatus::Selecting => {}
                PortalStatus::Streaming => {
                    if !self.confirmed {
                        if let Some(ticket) = confirm {
                            if self.view.confirmation == Some(ticket)
                                && self.binding == Some(ticket)
                                && self.view.preview.is_some()
                            {
                                self.confirmed = true;
                                self.view.preview = None;
                                self.view.confirmation = None;
                                reset = true;
                            }
                        }
                        if !self.confirmed && self.view.preview.is_none() {
                            if capture.take_frame(output).is_some() {
                                self.view.preview = Some(Arc::new(make_preview(output)));
                                self.view.confirmation = self.binding;
                                self.view.status = ConnectionStatus::Preview;
                                capture.set_active(false);
                            } else {
                                capture.request_frame();
                            }
                        }
                    }
                    if self.confirmed {
                        let observed = observation.expect("target checked above");
                        self.view.presentation = Some(observed);
                        let focused = observed.focus == FocusState::Focused;
                        capture.set_active(focused);
                        self.view.status = if focused {
                            ConnectionStatus::Connected
                        } else {
                            ConnectionStatus::Paused
                        };
                        if !focused {
                            reset |= self.last_frame.take().is_some()
                                || old_status != ConnectionStatus::Paused;
                        } else if let Some(stamp) = capture.take_frame(output) {
                            reset |= self.last_frame.is_some_and(|previous| {
                                stamp.received.saturating_duration_since(previous.received)
                                    > Duration::from_secs(2)
                            });
                            let format = (stamp.format_generation, output.width, output.height);
                            reset |= self.format.is_some_and(|old| old != format);
                            self.format = Some(format);
                            self.last_frame = Some(stamp);
                            frame = Some((stamp, observed));
                            capture.request_frame();
                        } else {
                            if self.last_frame.is_some_and(|stamp| {
                                now.saturating_duration_since(stamp.received)
                                    > Duration::from_secs(2)
                            }) {
                                self.last_frame = None;
                                reset = true;
                            }
                            capture.request_frame();
                            if self.last_frame.is_none() {
                                self.view.status = ConnectionStatus::WaitingForFrame;
                            }
                        }
                    }
                }
            }
        }
        let changed = old_status != self.view.status
            || old_presentation != self.view.presentation
            || old_confirmation != self.view.confirmation;
        if changed {
            *self.shared.lock().unwrap_or_else(|p| p.into_inner()) = self.view.clone();
        }
        Poll {
            handling: self.capture.is_some(),
            reset: reset.then_some(
                if old_binding.is_some() && old_binding == self.binding.filter(|_| self.confirmed) {
                    ResetScope::Observations
                } else {
                    ResetScope::Session
                },
            ),
            changed,
            frame,
        }
    }
}

fn make_preview(frame: &CapturedFrame) -> Preview {
    let mut rgba = frame.bgra.clone();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    let source = image::RgbaImage::from_raw(frame.width as u32, frame.height as u32, rgba)
        .expect("validated packed frame");
    let thumbnail = image::DynamicImage::ImageRgba8(source)
        .thumbnail(640.min(frame.width as u32), 360.min(frame.height as u32))
        .into_rgba8();
    Preview {
        width: thumbnail.width() as usize,
        height: thumbnail.height() as usize,
        rgba: thumbnail.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::Mailbox;
    use super::*;

    fn observed(epoch: u64, focused: bool) -> PresentationObservation {
        PresentationObservation {
            target_epoch: epoch,
            focus: if focused {
                FocusState::Focused
            } else {
                FocusState::Background
            },
            fullscreen: Some(true),
            generation: 1,
            committed_at: Instant::now(),
        }
    }

    fn stream() -> (PortalCapture, Arc<Mutex<Mailbox>>) {
        let shared = Arc::new(Mutex::new(Mailbox::new()));
        let (stop, _) = tokio::sync::watch::channel(false);
        let (commands, _) = pipewire::channel::channel();
        let capture = PortalCapture {
            shared: shared.clone(),
            demand: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            stop,
            commands,
            thread: None,
            active: true,
        };
        deliver(&shared, 1, 1);
        (capture, shared)
    }

    fn deliver(shared: &Arc<Mutex<Mailbox>>, sequence: u64, generation: u64) {
        let mut mailbox = shared.lock().unwrap();
        mailbox.status = PortalStatus::Streaming;
        mailbox.frame = CapturedFrame {
            width: 1,
            height: 1,
            bgra: vec![1, 2, 3, 255],
        };
        mailbox.ready = true;
        mailbox.accepting = true; // Receiver has acknowledged activation.
        mailbox.sequence = sequence;
        mailbox.format_generation = generation;
        mailbox.received = Some(Instant::now());
    }

    #[test]
    fn preview_requires_confirmation_and_alt_tab_preserves_only_the_same_session() {
        let (control, mut connection) = PortalConnection::new();
        let (capture, shared) = stream();
        let mut output = CapturedFrame::default();
        control.connect();
        let preview = connection.poll(
            || Ok(false),
            || Ok(capture),
            Some(observed(1, false)),
            &mut output,
            Instant::now(),
        );
        assert!(preview.handling && preview.frame.is_none());
        assert_eq!(preview.reset, Some(ResetScope::Session));
        let view = control.view();
        assert_eq!(view.status, ConnectionStatus::Preview);
        assert_eq!(view.preview.unwrap().rgba, [3, 2, 1, 255]);
        let ticket = view.confirmation.unwrap();
        control.confirm(Confirmation {
            session: ticket.session + 1,
            ..ticket
        });
        let rejected = connection.poll(
            || Ok(false),
            || panic!("no new session"),
            Some(observed(1, true)),
            &mut output,
            Instant::now(),
        );
        assert!(rejected.frame.is_none());
        assert_eq!(control.view().status, ConnectionStatus::Preview);

        control.confirm(ticket);
        let confirmed = connection.poll(
            || Ok(false),
            || panic!(),
            Some(observed(1, false)),
            &mut output,
            Instant::now(),
        );
        assert_eq!(confirmed.reset, Some(ResetScope::Session));
        assert_eq!(control.view().status, ConnectionStatus::Paused);
        assert!(control.view().preview.is_none());
        connection.poll(
            || Ok(false),
            || panic!(),
            Some(observed(1, true)),
            &mut output,
            Instant::now(),
        );
        deliver(&shared, 2, 1);
        let delivered = connection.poll(
            || Ok(false),
            || panic!(),
            Some(observed(1, true)),
            &mut output,
            Instant::now(),
        );
        assert_eq!(delivered.frame.unwrap().0.sequence, 2);
        assert!(connection
            .poll(
                || Ok(false),
                || panic!(),
                Some(observed(1, true)),
                &mut output,
                Instant::now()
            )
            .frame
            .is_none());
        assert_eq!(
            connection
                .poll(
                    || Ok(false),
                    || panic!(),
                    Some(observed(1, false)),
                    &mut output,
                    Instant::now()
                )
                .reset,
            Some(ResetScope::Observations)
        );
        connection.poll(
            || Ok(false),
            || panic!(),
            Some(observed(1, true)),
            &mut output,
            Instant::now(),
        );
        deliver(&shared, 3, 2);
        let resized = connection.poll(
            || Ok(false),
            || panic!(),
            Some(observed(1, true)),
            &mut output,
            Instant::now(),
        );
        assert_eq!(resized.reset, Some(ResetScope::Observations));
        assert!(resized.frame.is_some());
        assert_eq!(control.view().status, ConnectionStatus::Connected);

        let replaced = connection.poll(
            || Ok(false),
            || panic!(),
            Some(observed(2, true)),
            &mut output,
            Instant::now(),
        );
        assert_eq!(replaced.reset, Some(ResetScope::Session));
        assert!(!replaced.handling && replaced.frame.is_none());
        assert_eq!(control.view().status, ConnectionStatus::TargetChanged);
        control.confirm(ticket);
        assert!(
            !connection
                .poll(
                    || panic!(),
                    || panic!(),
                    Some(observed(2, true)),
                    &mut output,
                    Instant::now()
                )
                .handling
        );
    }

    #[test]
    fn target_loss_and_session_close_invalidate_preview() {
        for closed in [false, true] {
            let (control, mut connection) = PortalConnection::new();
            let (capture, shared) = stream();
            let mut output = CapturedFrame::default();
            control.connect();
            connection.poll(
                || Ok(false),
                || Ok(capture),
                Some(observed(1, false)),
                &mut output,
                Instant::now(),
            );
            let ticket = control.view().confirmation.unwrap();
            if closed {
                shared.lock().unwrap().finish(Ok(()));
            }
            control.confirm(ticket);
            let poll = connection.poll(
                || Ok(false),
                || panic!(),
                closed.then(|| observed(1, true)),
                &mut output,
                Instant::now(),
            );
            assert_eq!(poll.reset, Some(ResetScope::Session));
            assert!(!poll.handling && poll.frame.is_none());
            assert!(control.view().preview.is_none());
            assert!(control.view().presentation.is_none());
        }
    }

    #[test]
    fn stalled_stream_resets_history_and_waits_for_a_fresh_frame() {
        let (control, mut connection) = PortalConnection::new();
        let (capture, shared) = stream();
        let mut output = CapturedFrame::default();
        control.connect();
        connection.poll(
            || Ok(false),
            || Ok(capture),
            Some(observed(1, true)),
            &mut output,
            Instant::now(),
        );
        control.confirm(control.view().confirmation.unwrap());
        connection.poll(
            || Ok(false),
            || panic!(),
            Some(observed(1, true)),
            &mut output,
            Instant::now(),
        );
        assert_eq!(control.view().status, ConnectionStatus::WaitingForFrame);
        deliver(&shared, 2, 1);
        let delivered = connection.poll(
            || Ok(false),
            || panic!(),
            Some(observed(1, true)),
            &mut output,
            Instant::now(),
        );
        let received = delivered.frame.unwrap().0.received;
        let stalled = connection.poll(
            || Ok(false),
            || panic!(),
            Some(observed(1, true)),
            &mut output,
            received + Duration::from_secs(3),
        );
        assert_eq!(stalled.reset, Some(ResetScope::Observations));
        assert!(stalled.handling && stalled.frame.is_none());
        assert_eq!(control.view().status, ConnectionStatus::WaitingForFrame);
        assert!(control.view().confirmation.is_none());
        deliver(&shared, 3, 1);
        let resumed = connection.poll(
            || Ok(false),
            || panic!(),
            Some(observed(1, true)),
            &mut output,
            received + Duration::from_secs(3),
        );
        assert_eq!(resumed.frame.unwrap().0.sequence, 3);
        assert_eq!(control.view().status, ConnectionStatus::Connected);
    }

    #[test]
    fn default_and_xwayland_never_start_portal() {
        let (control, mut connection) = PortalConnection::new();
        let mut frame = CapturedFrame::default();
        assert!(
            !connection
                .poll(
                    || panic!("no default query"),
                    || panic!("no default start"),
                    None,
                    &mut frame,
                    Instant::now()
                )
                .handling
        );
        assert!(control.connect());
        assert!(
            !connection
                .poll(
                    || Ok(true),
                    || panic!("XWayland stays automatic"),
                    None,
                    &mut frame,
                    Instant::now()
                )
                .handling
        );
        assert_eq!(control.view().status, ConnectionStatus::XWayland);
        assert!(control.connect());
        connection.poll(
            || Ok(false),
            || panic!("missing target is not native evidence"),
            None,
            &mut frame,
            Instant::now(),
        );
        assert_eq!(control.view().status, ConnectionStatus::TargetUnavailable);
    }

    #[test]
    fn cancellation_wins_over_queued_start_and_confirmation() {
        let (control, mut connection) = PortalConnection::new();
        control.connect();
        control.confirm(Confirmation {
            session: 1,
            target_epoch: 1,
        });
        control.disconnect();
        let poll = connection.poll(
            || panic!("cancelled"),
            || panic!("cancelled"),
            None,
            &mut CapturedFrame::default(),
            Instant::now(),
        );
        assert!(!poll.handling);
        assert_eq!(poll.reset, Some(ResetScope::Session));
        assert_eq!(control.view().status, ConnectionStatus::Idle);
    }

    #[test]
    fn xwayland_appearance_releases_portal_and_preserves_explicit_reconnect() {
        let (control, mut connection) = PortalConnection::new();
        let (capture, _) = stream();
        let mut output = CapturedFrame::default();
        let now = Instant::now();
        control.connect();
        connection.poll(
            || Ok(false),
            || Ok(capture),
            Some(observed(1, true)),
            &mut output,
            now,
        );
        let old_ticket = control.view().confirmation.unwrap();
        control.confirm(old_ticket);
        let switched = connection.poll(
            || Ok(true),
            || panic!("must not open a second session"),
            Some(observed(1, true)),
            &mut output,
            now + Duration::from_secs(1),
        );
        assert_eq!(switched.reset, Some(ResetScope::Session));
        assert!(!switched.handling && switched.frame.is_none());
        assert_eq!(control.view().status, ConnectionStatus::XWayland);
        assert!(control.view().preview.is_none());
        assert!(control.view().presentation.is_none());
        control.confirm(old_ticket);
        let idle = connection.poll(
            || panic!("no automatic reconnect query"),
            || panic!("no automatic selector"),
            Some(observed(1, true)),
            &mut output,
            now + Duration::from_secs(2),
        );
        assert!(!idle.handling);
    }
}
