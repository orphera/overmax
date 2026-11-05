use super::{copy_mapped_frame, session::StreamTarget, Mailbox, PortalStatus, StreamCommand};
use crate::capture::frame::CapturedFrame;
use pipewire::{self as pw, properties::properties, spa};
use spa::{
    buffer::meta::{MetaHeader, MetaHeaderFlags, MetaVideoCrop},
    param::video::VideoInfoRaw,
    pod::Pod,
};
use std::{
    cell::RefCell,
    os::fd::OwnedFd,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

struct ReceiverState {
    format: Option<VideoInfoRaw>,
    sequence: Option<u64>,
    scratch: CapturedFrame,
    shared: Arc<Mutex<Mailbox>>,
    demand: Arc<AtomicBool>,
    mainloop: pw::main_loop::MainLoopRc,
    error: Rc<RefCell<Option<String>>>,
}

impl ReceiverState {
    fn fail(&self, error: impl ToString) {
        *self.error.borrow_mut() = Some(error.to_string());
        self.shared
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .invalidate();
        self.mainloop.quit();
    }

    fn format_changed(&mut self, stream: &pw::stream::Stream, param: Option<&Pod>) {
        self.format = None;
        self.sequence = None;
        {
            let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
            shared.invalidate();
            shared.format_generation += 1;
        }
        let Some(param) = param else {
            return;
        };
        let mut format = VideoInfoRaw::new();
        if let Err(e) = format.parse(param) {
            self.fail(e);
            return;
        }
        // SDR packed pixels only. Unknown color metadata is conventional for
        // desktop streams; explicitly different transfer/range is unsupported.
        if !matches!(
            format.transfer_function(),
            spa::sys::SPA_VIDEO_TRANSFER_UNKNOWN | spa::sys::SPA_VIDEO_TRANSFER_SRGB
        ) || !matches!(
            format.color_range(),
            spa::sys::SPA_VIDEO_COLOR_RANGE_UNKNOWN | spa::sys::SPA_VIDEO_COLOR_RANGE_0_255
        ) {
            self.fail("unsupported PipeWire color space");
            return;
        }
        self.format = Some(format);
        let mask = (1 << spa::sys::SPA_DATA_MemFd) | (1 << spa::sys::SPA_DATA_MemPtr);
        let buffers = spa::pod::Object {
            type_: spa::utils::SpaTypes::ObjectParamBuffers.as_raw(),
            id: spa::param::ParamType::Buffers.as_raw(),
            properties: vec![spa::pod::Property {
                key: spa::sys::SPA_PARAM_BUFFERS_dataType,
                flags: spa::pod::PropertyFlags::empty(),
                value: spa::pod::Value::Choice(spa::pod::ChoiceValue::Int(spa::utils::Choice(
                    spa::utils::ChoiceFlags::empty(),
                    spa::utils::ChoiceEnum::Flags {
                        default: mask,
                        flags: vec![],
                    },
                ))),
            }],
        };
        let header = spa::pod::Object {
            type_: spa::utils::SpaTypes::ObjectParamMeta.as_raw(),
            id: spa::param::ParamType::Meta.as_raw(),
            properties: vec![
                spa::pod::Property {
                    key: spa::sys::SPA_PARAM_META_type,
                    flags: spa::pod::PropertyFlags::empty(),
                    value: spa::pod::Value::Id(spa::utils::Id(spa::sys::SPA_META_Header)),
                },
                spa::pod::Property {
                    key: spa::sys::SPA_PARAM_META_size,
                    flags: spa::pod::PropertyFlags::empty(),
                    value: spa::pod::Value::Int(
                        std::mem::size_of::<spa::sys::spa_meta_header>() as i32
                    ),
                },
            ],
        };
        let result = (|| {
            let buffers = serialize(buffers)?;
            let header = serialize(header)?;
            stream
                .update_params(&mut [pod(&buffers)?, pod(&header)?])
                .map_err(|e| e.to_string())
        })();
        if let Err(e) = result {
            self.fail(e);
        }
    }

    fn accept_header(&mut self, sequence: u64, flags: MetaHeaderFlags) -> bool {
        let recoverable = MetaHeaderFlags::DISCONT | MetaHeaderFlags::CORRUPTED;
        if flags.bits() & !recoverable.bits() != 0 {
            self.fail(format!("unsupported PipeWire frame metadata: {flags:?}"));
            return false;
        }
        let corrupt = flags.contains(MetaHeaderFlags::CORRUPTED);
        if !corrupt && self.sequence == Some(sequence) {
            return false;
        }
        if flags.intersects(recoverable) {
            self.sequence = None;
            let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
            shared.invalidate();
            shared.format_generation += 1;
        }
        // The producer can return an unfinished buffer when capture is paused.
        // Reject its pixels and queued history, keeping the same session alive.
        if corrupt {
            return false;
        }
        self.sequence = Some(sequence);
        true
    }

    fn process(&mut self, stream: &pw::stream::Stream) {
        // Retain only the most recent dequeued buffer. Buffer::drop returns all
        // buffers, including every early/error return below, to PipeWire.
        let Some(mut buffer) = stream.dequeue_buffer() else {
            return;
        };
        while let Some(next) = stream.dequeue_buffer() {
            buffer = next;
        }
        let Some(format) = self.format else {
            return;
        };
        let Some(header) = buffer.find_meta::<MetaHeader>() else {
            self.fail("PipeWire omitted required frame sequence metadata");
            return;
        };
        if !self.accept_header(header.seq(), header.flags()) {
            return;
        }
        if !self.demand.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Some(crop) = buffer.find_meta::<MetaVideoCrop>() {
            let crop = crop.meta_region();
            if crop.is_valid()
                && (crop.position().x != 0
                    || crop.position().y != 0
                    || crop.size() != format.size())
            {
                self.fail("cropped PipeWire window buffers are unsupported");
                return;
            }
        }
        if let Err(e) = copy_mapped_frame(&format, buffer.datas_mut(), &mut self.scratch) {
            self.fail(e);
            return;
        }
        let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        if !shared.accepting {
            return;
        }
        std::mem::swap(&mut shared.frame, &mut self.scratch);
        shared.sequence += 1;
        shared.received = Some(Instant::now());
        shared.ready = true;
        shared.status = PortalStatus::Streaming;
    }
}

fn serialize(object: spa::pod::Object) -> Result<Vec<u8>, String> {
    spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .map(|(cursor, _)| cursor.into_inner())
    .map_err(|e| e.to_string())
}

fn pod(bytes: &[u8]) -> Result<&Pod, String> {
    Pod::from_bytes(bytes).ok_or_else(|| "invalid serialized SPA parameter".into())
}

fn format_offer(max_fps: u32) -> Result<Vec<u8>, String> {
    use spa::{
        param::{
            format::{FormatProperties as F, MediaSubtype, MediaType},
            video::VideoFormat as V,
        },
        utils::{Fraction, Rectangle},
    };
    serialize(spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(F::MediaType, Id, MediaType::Video),
        spa::pod::property!(F::MediaSubtype, Id, MediaSubtype::Raw),
        spa::pod::property!(
            F::VideoFormat,
            Choice,
            Enum,
            Id,
            V::BGRA,
            V::BGRA,
            V::BGRx,
            V::RGBA,
            V::RGBx
        ),
        spa::pod::property!(
            F::VideoSize,
            Choice,
            Range,
            Rectangle,
            Rectangle {
                width: 1920,
                height: 1080
            },
            Rectangle {
                width: 1,
                height: 1
            },
            Rectangle {
                width: 8192,
                height: 8192
            }
        ),
        spa::pod::property!(F::VideoFramerate, Fraction, Fraction { num: 0, denom: 1 }),
        spa::pod::property!(
            F::VideoMaxFramerate,
            Choice,
            Range,
            Fraction,
            Fraction {
                num: max_fps,
                denom: 1
            },
            Fraction { num: 1, denom: 1 },
            Fraction {
                num: max_fps,
                denom: 1
            }
        )
    ))
}

pub(super) fn run(
    fd: OwnedFd,
    target: StreamTarget,
    commands: pw::channel::Receiver<StreamCommand>,
    shared: Arc<Mutex<Mailbox>>,
    demand: Arc<AtomicBool>,
    max_fps: u32,
) -> Result<(), String> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| e.to_string())?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(|e| e.to_string())?;
    let core = context.connect_fd_rc(fd, None).map_err(|e| e.to_string())?;
    let error = Rc::new(RefCell::new(None));
    let _core_listener = core
        .add_listener_local()
        .error({
            let error = error.clone();
            let mainloop = mainloop.clone();
            move |_, _, _, message| {
                *error.borrow_mut() = Some(format!("PipeWire connection: {message}"));
                mainloop.quit();
            }
        })
        .register();
    let mut properties = properties! { *pw::keys::MEDIA_TYPE => "Video", *pw::keys::MEDIA_CATEGORY => "Capture", *pw::keys::MEDIA_ROLE => "Screen" };
    if let Some(serial) = target.serial {
        properties.insert("target.object", serial.to_string());
    }
    let stream = pw::stream::StreamRc::new(core, "Overmax window capture", properties)
        .map_err(|e| e.to_string())?;
    let state = ReceiverState {
        format: None,
        sequence: None,
        scratch: CapturedFrame::default(),
        shared: shared.clone(),
        demand,
        mainloop: mainloop.clone(),
        error: error.clone(),
    };
    let _listener = stream
        .add_local_listener_with_user_data(state)
        .state_changed(|_, data, old, new| match new {
            pw::stream::StreamState::Error(message) => data.fail(message),
            pw::stream::StreamState::Unconnected if old != pw::stream::StreamState::Unconnected => {
                data.fail("PipeWire stream disconnected")
            }
            _ => {}
        })
        .param_changed(|stream, data, id, param| {
            if id == spa::param::ParamType::Format.as_raw() {
                data.format_changed(stream, param);
            }
        })
        .process(|stream, data| data.process(stream))
        .register()
        .map_err(|e| e.to_string())?;
    let _commands = commands.attach(mainloop.loop_(), {
        let stream = stream.clone();
        let mainloop = mainloop.clone();
        let error = error.clone();
        let shared = shared.clone();
        move |command| match command {
            StreamCommand::Stop => mainloop.quit(),
            StreamCommand::Active(active) => {
                // Drain frames queued before this activation boundary.
                while stream.dequeue_buffer().is_some() {}
                let mut mailbox = shared.lock().unwrap_or_else(|p| p.into_inner());
                mailbox.invalidate();
                mailbox.accepting = active;
                drop(mailbox);
                if let Err(e) = stream.set_active(active) {
                    *error.borrow_mut() = Some(e.to_string());
                    mainloop.quit();
                }
            }
        }
    });
    let offer = format_offer(max_fps)?;
    stream
        .connect(
            spa::utils::Direction::Input,
            target.serial.is_none().then_some(target.node),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut [pod(&offer)?],
        )
        .map_err(|e| e.to_string())?;
    mainloop.run();
    let result = error.borrow_mut().take().map_or(Ok(()), Err);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receiver() -> ReceiverState {
        pw::init();
        ReceiverState {
            format: None,
            sequence: Some(20),
            scratch: CapturedFrame::default(),
            shared: Arc::new(Mutex::new(Mailbox::new())),
            demand: Arc::new(AtomicBool::new(true)),
            mainloop: pw::main_loop::MainLoopRc::new(None).unwrap(),
            error: Rc::new(RefCell::new(None)),
        }
    }

    #[test]
    fn corrupt_resume_frame_is_dropped_without_ending_the_session() {
        // A bad buffer must invalidate a queued frame even if its sequence repeats.
        for sequence in [21, 20] {
            let mut receiver = receiver();
            {
                let mut mailbox = receiver.shared.lock().unwrap();
                mailbox.status = PortalStatus::Streaming;
                mailbox.ready = true;
                mailbox.received = Some(Instant::now());
                mailbox.format_generation = 4;
            }
            assert!(!receiver.accept_header(sequence, MetaHeaderFlags::CORRUPTED));
            assert!(
                receiver.error.borrow().is_none(),
                "one bad frame must not end the session"
            );
            {
                let mailbox = receiver.shared.lock().unwrap();
                assert!(!mailbox.ready && mailbox.received.is_none());
                assert_eq!(mailbox.status, PortalStatus::Streaming);
                assert!(mailbox.format_generation > 4);
            }
            assert!(receiver.demand.load(Ordering::Acquire));
            assert!(receiver.accept_header(22, MetaHeaderFlags::empty()));
            assert!(!receiver.accept_header(22, MetaHeaderFlags::empty()));
            assert!(receiver.error.borrow().is_none());
        }
    }

    #[test]
    fn unsupported_header_flags_still_end_the_session() {
        for flags in [
            MetaHeaderFlags::HEADER,
            MetaHeaderFlags::DELTA_UNIT,
            MetaHeaderFlags::from_bits_retain(1 << 31),
        ] {
            let mut receiver = receiver();
            assert!(!receiver.accept_header(21, flags));
            assert!(receiver.error.borrow().is_some());
        }
    }

    #[test]
    fn duplicate_discontinuity_is_not_a_new_observation() {
        let mut receiver = receiver();
        assert!(receiver.accept_header(21, MetaHeaderFlags::DISCONT));
        let generation = receiver.shared.lock().unwrap().format_generation;
        assert!(!receiver.accept_header(21, MetaHeaderFlags::DISCONT));
        assert_eq!(
            receiver.shared.lock().unwrap().format_generation,
            generation
        );
    }
}
