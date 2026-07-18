use crate::capture::capture_engine::CaptureEngine;
use crate::capture::frame::CapturedFrame;
use crate::capture::window_tracker::{WindowRect, WindowSnapshot};
use memmap2::{MmapMut, MmapOptions};
use std::fs::File;
use x11rb::connection::Connection;
use x11rb::errors::{ConnectionError, ReplyError, ReplyOrIdError};
use x11rb::protocol::composite::{ConnectionExt as _, Redirect};
use x11rb::protocol::shm::ConnectionExt as _;
use x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ConnectionExt as _, EventMask, ImageFormat, ImageOrder, MapState,
    VisualClass, Visualid, Window,
};
use x11rb::protocol::{ErrorKind, Event};
use x11rb::rust_connection::RustConnection;

const SHM_PROBE_SIZE: usize = 4096;
const PAGE_SIZE: usize = 4096;

pub struct AdaptiveCaptureEngine {
    conn: Option<RustConnection>,
    binding: Option<TargetBinding>,
    fatal: Option<String>,
    transient: Option<String>,
}

struct TargetBinding {
    window: Window,
    redirected: bool,
    generation: Option<CaptureGeneration>,
}

struct CaptureGeneration {
    pixmap: u32,
    shmseg: u32,
    map: MmapMut,
    width: u16,
    height: u16,
    depth: u8,
    stride: usize,
    frame_len: usize,
}

#[derive(Clone, Copy)]
struct WindowInfo {
    window: Window,
    visual: Visualid,
    width: u16,
    height: u16,
    depth: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PixelLayout {
    stride: usize,
    size: usize,
    frame_len: usize,
}

#[derive(Debug)]
enum CaptureFailure {
    Transient(String),
    Permanent(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetTransition {
    Keep,
    Clear,
    Replace(Window),
}

impl AdaptiveCaptureEngine {
    pub fn new() -> Result<Self, String> {
        let (conn, fatal) = match initialize() {
            Ok(conn) => (Some(conn), None),
            Err(error) => (None, Some(error)),
        };
        Ok(Self {
            conn,
            binding: None,
            fatal,
            transient: None,
        })
    }

    fn latch(&mut self, message: String) -> String {
        if let (Some(conn), Some(binding)) = (self.conn.as_ref(), self.binding.take()) {
            release_binding_best_effort(conn, binding);
        }
        self.transient = None;
        self.fatal = Some(message.clone());
        message
    }

    fn fatal_error(&self) -> Option<String> {
        self.fatal.clone()
    }
}

impl CaptureEngine for AdaptiveCaptureEngine {
    fn set_target(&mut self, target: Option<WindowSnapshot>) -> Result<(), String> {
        if let Some(error) = self.fatal_error() {
            return Err(error);
        }
        let requested = match snapshot_window(target) {
            Ok(window) => window,
            Err(error) => return Err(self.latch(error)),
        };
        let result = apply_target(
            self.conn.as_ref().expect("initialized X11 connection"),
            &mut self.binding,
            requested,
        );
        match result {
            Ok(transient) => {
                self.transient = transient;
                Ok(())
            }
            Err(CaptureFailure::Transient(error)) => {
                self.transient = Some(error);
                Ok(())
            }
            Err(CaptureFailure::Permanent(error)) => Err(self.latch(error)),
        }
    }

    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String> {
        let mut frame = CapturedFrame::default();
        self.capture_bgra_inplace(rect, &mut frame)?;
        Ok(frame)
    }

    fn capture_bgra_inplace(
        &mut self,
        _rect: WindowRect,
        out_frame: &mut CapturedFrame,
    ) -> Result<(), String> {
        if let Some(error) = self.fatal_error() {
            return Err(error);
        }
        let result = capture_frame(
            self.conn.as_ref().expect("initialized X11 connection"),
            &mut self.binding,
            out_frame,
        );
        match result {
            Ok(()) => {
                self.transient = None;
                Ok(())
            }
            Err(CaptureFailure::Transient(error)) => {
                let error = self.transient.clone().unwrap_or(error);
                self.transient = Some(error.clone());
                Err(error)
            }
            Err(CaptureFailure::Permanent(error)) => Err(self.latch(error)),
        }
    }
}

impl Drop for AdaptiveCaptureEngine {
    fn drop(&mut self) {
        if let (Some(conn), Some(binding)) = (self.conn.as_ref(), self.binding.take()) {
            release_binding_best_effort(conn, binding);
        }
    }
}

fn initialize() -> Result<RustConnection, String> {
    let (conn, _) = x11rb::connect(None).map_err(|error| format!("X11 connect failed: {error}"))?;
    let composite = conn
        .composite_query_version(0, 2)
        .map_err(|error| format!("Composite query failed: {error}"))?
        .reply()
        .map_err(|error| format!("Composite query failed: {error}"))?;
    if (composite.major_version, composite.minor_version) < (0, 2) {
        return Err(format!(
            "Composite 0.2 required, server has {}.{}",
            composite.major_version, composite.minor_version
        ));
    }
    let shm = conn
        .shm_query_version()
        .map_err(|error| format!("MIT-SHM query failed: {error}"))?
        .reply()
        .map_err(|error| format!("MIT-SHM query failed: {error}"))?;
    if (shm.major_version, shm.minor_version) < (1, 2) {
        return Err(format!(
            "MIT-SHM 1.2 required, server has {}.{}",
            shm.major_version, shm.minor_version
        ));
    }
    probe_shm_segment(&conn)?;
    Ok(conn)
}

fn probe_shm_segment(conn: &RustConnection) -> Result<(), String> {
    let shmseg = conn
        .generate_id()
        .map_err(|error| format!("MIT-SHM probe ID failed: {error}"))?;
    let reply = conn
        .shm_create_segment(shmseg, SHM_PROBE_SIZE as u32, false)
        .map_err(|error| format!("MIT-SHM CreateSegment probe failed: {error}"))?
        .reply()
        .map_err(|error| format!("MIT-SHM CreateSegment probe failed: {error}"))?;
    let file = File::from(reply.shm_fd);
    let map_result = unsafe { MmapOptions::new().len(SHM_PROBE_SIZE).map_mut(&file) };
    let mut map = match map_result {
        Ok(map) => map,
        Err(error) => {
            if let Ok(cookie) = conn.shm_detach(shmseg) {
                let _ = cookie.check();
            }
            return Err(format!("MIT-SHM mmap probe failed: {error}"));
        }
    };
    map[0] = 0;
    conn.shm_detach(shmseg)
        .map_err(|error| format!("MIT-SHM detach probe failed: {error}"))?
        .check()
        .map_err(|error| format!("MIT-SHM detach probe failed: {error}"))?;
    Ok(())
}

fn snapshot_window(target: Option<WindowSnapshot>) -> Result<Option<Window>, String> {
    target
        .map(|snapshot| {
            let window = u32::try_from(snapshot.window)
                .map_err(|_| format!("X11 window ID out of range: {}", snapshot.window))?;
            if window == 0 {
                return Err("X11 window ID must be non-zero".to_string());
            }
            Ok(window)
        })
        .transpose()
}

fn apply_target(
    conn: &RustConnection,
    binding: &mut Option<TargetBinding>,
    requested: Option<Window>,
) -> Result<Option<String>, CaptureFailure> {
    poll_lifecycle_events(conn, binding)?;
    let transition = target_transition(binding.as_ref().map(|bound| bound.window), requested);
    match transition {
        TargetTransition::Keep => {
            let Some(bound) = binding.as_mut() else {
                return Ok(None);
            };
            match ensure_generation(conn, bound) {
                Ok(()) => Ok(None),
                Err(CaptureFailure::Transient(error)) => Ok(Some(error)),
                Err(error) => Err(error),
            }
        }
        TargetTransition::Clear => {
            if let Some(bound) = binding.take() {
                release_binding_checked(conn, bound)?;
            }
            Ok(None)
        }
        TargetTransition::Replace(window) => {
            if let Some(bound) = binding.take() {
                release_binding_checked(conn, bound)?;
            }
            match create_binding(conn, window) {
                Ok((bound, transient)) => {
                    *binding = Some(bound);
                    Ok(transient)
                }
                Err(CaptureFailure::Transient(error)) => Ok(Some(error)),
                Err(error) => Err(error),
            }
        }
    }
}

fn target_transition(current: Option<Window>, requested: Option<Window>) -> TargetTransition {
    match (current, requested) {
        (current, requested) if current == requested => TargetTransition::Keep,
        (_, None) => TargetTransition::Clear,
        (_, Some(window)) => TargetTransition::Replace(window),
    }
}

fn create_binding(
    conn: &RustConnection,
    window: Window,
) -> Result<(TargetBinding, Option<String>), CaptureFailure> {
    let info = query_window_info(conn, window)?;
    conn.change_window_attributes(
        window,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::STRUCTURE_NOTIFY),
    )
    .map_err(|error| connection_failure("select StructureNotify", error))?
    .check()
    .map_err(|error| reply_failure("select StructureNotify", error))?;
    conn.composite_redirect_window(window, Redirect::AUTOMATIC)
        .map_err(|error| connection_failure("redirect window", error))?
        .check()
        .map_err(|error| reply_failure("redirect window", error))?;

    let mut binding = TargetBinding {
        window,
        redirected: true,
        generation: None,
    };
    match create_generation(conn, info) {
        Ok(generation) => {
            binding.generation = Some(generation);
            Ok((binding, None))
        }
        Err(CaptureFailure::Transient(error)) => Ok((binding, Some(error))),
        Err(error) => {
            let cleanup = release_binding_checked(conn, binding);
            Err(prefer_cleanup_error(error, cleanup))
        }
    }
}

fn ensure_generation(
    conn: &RustConnection,
    binding: &mut TargetBinding,
) -> Result<(), CaptureFailure> {
    if binding.generation.is_some() {
        return Ok(());
    }
    let info = query_window_info(conn, binding.window)?;
    binding.generation = Some(create_generation(conn, info)?);
    Ok(())
}

fn query_window_info(conn: &RustConnection, window: Window) -> Result<WindowInfo, CaptureFailure> {
    let attrs = conn
        .get_window_attributes(window)
        .map_err(|error| connection_failure("get window attributes", error))?
        .reply()
        .map_err(|error| reply_failure("get window attributes", error))?;
    if attrs.map_state != MapState::VIEWABLE {
        return Err(CaptureFailure::Transient(
            "capture target is not viewable".to_string(),
        ));
    }
    let geometry = conn
        .get_geometry(window)
        .map_err(|error| connection_failure("get window geometry", error))?
        .reply()
        .map_err(|error| reply_failure("get window geometry", error))?;
    if geometry.border_width != 0 {
        return Err(CaptureFailure::Transient(format!(
            "capture target border width is {}",
            geometry.border_width
        )));
    }
    if geometry.width == 0 || geometry.height == 0 {
        return Err(CaptureFailure::Transient(
            "capture target has zero size".to_string(),
        ));
    }
    Ok(WindowInfo {
        window,
        visual: attrs.visual,
        width: geometry.width,
        height: geometry.height,
        depth: geometry.depth,
    })
}

fn create_generation(
    conn: &RustConnection,
    info: WindowInfo,
) -> Result<CaptureGeneration, CaptureFailure> {
    let pixmap = create_named_pixmap(conn, info.window)?;
    let result = create_generation_for_pixmap(conn, info, pixmap);
    match result {
        Ok(mut generation) => {
            if let Err(error) = warmup(conn, &mut generation) {
                let cleanup = release_generation_checked(conn, generation);
                return Err(prefer_cleanup_error(error, cleanup));
            }
            Ok(generation)
        }
        Err(error) => {
            let cleanup = release_xids_checked(conn, pixmap, None);
            Err(prefer_cleanup_error(error, cleanup))
        }
    }
}

fn create_named_pixmap(conn: &RustConnection, window: Window) -> Result<u32, CaptureFailure> {
    let pixmap = conn
        .generate_id()
        .map_err(|error| id_failure("allocate named pixmap ID", error))?;
    let result = conn
        .composite_name_window_pixmap(window, pixmap)
        .map_err(|error| connection_failure("name window pixmap", error))?
        .check()
        .map_err(|error| reply_failure("name window pixmap", error));
    match result {
        Ok(()) => Ok(pixmap),
        Err(error) => {
            let cleanup = release_xids_checked(conn, pixmap, None);
            Err(prefer_cleanup_error(error, cleanup))
        }
    }
}

fn create_generation_for_pixmap(
    conn: &RustConnection,
    info: WindowInfo,
    pixmap: u32,
) -> Result<CaptureGeneration, CaptureFailure> {
    let geometry = conn
        .get_geometry(pixmap)
        .map_err(|error| connection_failure("get named pixmap geometry", error))?
        .reply()
        .map_err(|error| reply_failure("get named pixmap geometry", error))?;
    if (geometry.width, geometry.height, geometry.depth) != (info.width, info.height, info.depth) {
        return Err(CaptureFailure::Transient(format!(
            "named pixmap {}x{} depth {} != window {}x{} depth {}",
            geometry.width, geometry.height, geometry.depth, info.width, info.height, info.depth
        )));
    }
    let layout = pixel_layout(conn, info)?;
    let shmseg = conn
        .generate_id()
        .map_err(|error| id_failure("allocate MIT-SHM segment ID", error))?;
    let size = u32::try_from(layout.size).map_err(|_| {
        CaptureFailure::Permanent(format!("capture buffer too large: {} bytes", layout.size))
    })?;
    let reply = conn
        .shm_create_segment(shmseg, size, false)
        .map_err(|error| connection_failure("create MIT-SHM segment", error))?
        .reply()
        .map_err(|error| reply_failure("create MIT-SHM segment", error))?;
    let file = File::from(reply.shm_fd);
    let map = match unsafe { MmapOptions::new().len(layout.size).map_mut(&file) } {
        Ok(map) => map,
        Err(error) => {
            let cleanup = release_shm_checked(conn, shmseg);
            let failure = CaptureFailure::Permanent(format!("map MIT-SHM segment: {error}"));
            return Err(prefer_cleanup_error(failure, cleanup));
        }
    };
    Ok(CaptureGeneration {
        pixmap,
        shmseg,
        map,
        width: info.width,
        height: info.height,
        depth: info.depth,
        stride: layout.stride,
        frame_len: layout.frame_len,
    })
}

fn pixel_layout(conn: &RustConnection, info: WindowInfo) -> Result<PixelLayout, CaptureFailure> {
    let setup = conn.setup();
    let format = setup
        .pixmap_formats
        .iter()
        .find(|format| format.depth == info.depth && format.bits_per_pixel == 32)
        .ok_or_else(|| {
            CaptureFailure::Permanent(format!(
                "unsupported pixel format: depth {} without 32 bpp",
                info.depth
            ))
        })?;
    let visual = setup
        .roots
        .iter()
        .flat_map(|screen| screen.allowed_depths.iter())
        .filter(|depth| depth.depth == info.depth)
        .flat_map(|depth| depth.visuals.iter())
        .find(|visual| visual.visual_id == info.visual)
        .ok_or_else(|| {
            CaptureFailure::Permanent(format!("X11 visual {} not found", info.visual))
        })?;
    if !pixel_format_supported(
        info.depth,
        format.bits_per_pixel,
        setup.image_byte_order,
        visual.class,
        visual.red_mask,
        visual.green_mask,
        visual.blue_mask,
    ) {
        return Err(CaptureFailure::Permanent(format!(
            "unsupported X11 pixel layout: depth={} bpp={} order={:?} class={:?} masks={:#010x}/{:#010x}/{:#010x}",
            info.depth,
            format.bits_per_pixel,
            setup.image_byte_order,
            visual.class,
            visual.red_mask,
            visual.green_mask,
            visual.blue_mask
        )));
    }
    calculate_layout(
        u32::from(info.width),
        u32::from(info.height),
        format.bits_per_pixel,
        format.scanline_pad,
    )
    .ok_or_else(|| {
        CaptureFailure::Permanent(format!(
            "capture layout overflow: {}x{} pad {}",
            info.width, info.height, format.scanline_pad
        ))
    })
}

fn pixel_format_supported(
    depth: u8,
    bits_per_pixel: u8,
    image_order: ImageOrder,
    visual_class: VisualClass,
    red_mask: u32,
    green_mask: u32,
    blue_mask: u32,
) -> bool {
    matches!(depth, 24 | 32)
        && bits_per_pixel == 32
        && image_order == ImageOrder::LSB_FIRST
        && visual_class == VisualClass::TRUE_COLOR
        && red_mask == 0x00ff_0000
        && green_mask == 0x0000_ff00
        && blue_mask == 0x0000_00ff
}

fn calculate_layout(
    width: u32,
    height: u32,
    bits_per_pixel: u8,
    scanline_pad: u8,
) -> Option<PixelLayout> {
    if width == 0 || height == 0 || bits_per_pixel != 32 || !matches!(scanline_pad, 8 | 16 | 32) {
        return None;
    }
    let pad = u32::from(scanline_pad);
    let row_bits = width.checked_mul(u32::from(bits_per_pixel))?;
    let stride_bits = row_bits.checked_add(pad - 1)?.checked_div(pad)? * pad;
    let stride = usize::try_from(stride_bits.checked_div(8)?).ok()?;
    let row = usize::try_from(width).ok()?.checked_mul(4)?;
    if stride < row {
        return None;
    }
    let height = usize::try_from(height).ok()?;
    Some(PixelLayout {
        stride,
        size: stride.checked_mul(height)?,
        frame_len: row.checked_mul(height)?,
    })
}

fn warmup(conn: &RustConnection, generation: &mut CaptureGeneration) -> Result<(), CaptureFailure> {
    capture_raw(conn, generation)?;
    let mut touched = 0u8;
    for index in (0..generation.map.len()).step_by(PAGE_SIZE) {
        touched ^= generation.map[index];
    }
    if let Some(last) = generation.map.last() {
        touched ^= *last;
    }
    std::hint::black_box(touched);
    Ok(())
}

fn capture_frame(
    conn: &RustConnection,
    binding: &mut Option<TargetBinding>,
    out_frame: &mut CapturedFrame,
) -> Result<(), CaptureFailure> {
    poll_lifecycle_events(conn, binding)?;
    let Some(bound) = binding.as_mut() else {
        return Err(CaptureFailure::Transient(
            "capture target is not bound".to_string(),
        ));
    };
    ensure_generation(conn, bound)?;
    let result = capture_into(
        conn,
        bound.generation.as_mut().expect("capture generation"),
        out_frame,
    );
    if let Err(error) = result {
        let cleanup = match bound.generation.take() {
            Some(generation) => release_generation_checked(conn, generation),
            None => Ok(()),
        };
        return Err(prefer_cleanup_error(error, cleanup));
    }
    Ok(())
}

fn capture_into(
    conn: &RustConnection,
    generation: &mut CaptureGeneration,
    out_frame: &mut CapturedFrame,
) -> Result<(), CaptureFailure> {
    capture_raw(conn, generation)?;
    if out_frame.bgra.capacity() < generation.frame_len {
        out_frame
            .bgra
            .try_reserve_exact(generation.frame_len - out_frame.bgra.len())
            .map_err(|error| {
                CaptureFailure::Permanent(format!("allocate capture frame: {error}"))
            })?;
    }
    out_frame.bgra.resize(generation.frame_len, 0);
    let row = usize::from(generation.width) * 4;
    for (source, destination) in generation
        .map
        .chunks_exact(generation.stride)
        .zip(out_frame.bgra.chunks_exact_mut(row))
    {
        destination.copy_from_slice(&source[..row]);
        for alpha in destination[3..].iter_mut().step_by(4) {
            *alpha = 255;
        }
    }
    out_frame.width = i32::from(generation.width);
    out_frame.height = i32::from(generation.height);
    Ok(())
}

fn capture_raw(
    conn: &RustConnection,
    generation: &CaptureGeneration,
) -> Result<(), CaptureFailure> {
    let reply = conn
        .shm_get_image(
            generation.pixmap,
            0,
            0,
            generation.width,
            generation.height,
            u32::MAX,
            ImageFormat::Z_PIXMAP.into(),
            generation.shmseg,
            0,
        )
        .map_err(|error| connection_failure("MIT-SHM GetImage", error))?
        .reply()
        .map_err(|error| reply_failure("MIT-SHM GetImage", error))?;
    if reply.depth != generation.depth
        || usize::try_from(reply.size).ok() != Some(generation.map.len())
    {
        return Err(CaptureFailure::Permanent(format!(
            "MIT-SHM reply layout mismatch: depth {} size {}, expected depth {} size {}",
            reply.depth,
            reply.size,
            generation.depth,
            generation.map.len()
        )));
    }
    Ok(())
}

fn poll_lifecycle_events(
    conn: &RustConnection,
    binding: &mut Option<TargetBinding>,
) -> Result<(), CaptureFailure> {
    while let Some(event) = conn
        .poll_for_event()
        .map_err(|error| connection_failure("poll X11 events", error))?
    {
        let Some(window) = binding.as_ref().map(|bound| bound.window) else {
            continue;
        };
        let action = match event {
            Event::ConfigureNotify(event)
                if event.window == window
                    && binding
                        .as_ref()
                        .and_then(|bound| bound.generation.as_ref())
                        .is_some_and(|generation| {
                            (event.width, event.height) != (generation.width, generation.height)
                        }) =>
            {
                LifecycleAction::Invalidate
            }
            Event::UnmapNotify(event) if event.window == window => LifecycleAction::Invalidate,
            Event::MapNotify(event) if event.window == window => LifecycleAction::Invalidate,
            Event::DestroyNotify(event) if event.window == window => LifecycleAction::Destroy,
            _ => LifecycleAction::None,
        };
        match action {
            LifecycleAction::None => {}
            LifecycleAction::Invalidate => {
                if let Some(generation) = binding.as_mut().and_then(|bound| bound.generation.take())
                {
                    release_generation_checked(conn, generation)?;
                }
            }
            LifecycleAction::Destroy => {
                if let Some(mut bound) = binding.take() {
                    bound.redirected = false;
                    if let Some(generation) = bound.generation.take() {
                        release_generation_checked(conn, generation)?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum LifecycleAction {
    None,
    Invalidate,
    Destroy,
}

fn release_binding_checked(
    conn: &RustConnection,
    mut binding: TargetBinding,
) -> Result<(), CaptureFailure> {
    let mut failure = None;
    if let Some(generation) = binding.generation.take() {
        remember_failure(&mut failure, release_generation_checked(conn, generation));
    }
    if binding.redirected {
        let result = conn
            .composite_unredirect_window(binding.window, Redirect::AUTOMATIC)
            .map_err(|error| connection_failure("unredirect window", error))
            .and_then(|cookie| cleanup_reply("unredirect window", cookie.check()));
        remember_failure(&mut failure, result);
    }
    remember_failure(
        &mut failure,
        conn.flush()
            .map_err(|error| connection_failure("flush X11 cleanup", error)),
    );
    failure.map_or(Ok(()), Err)
}

fn release_generation_checked(
    conn: &RustConnection,
    generation: CaptureGeneration,
) -> Result<(), CaptureFailure> {
    release_xids_checked(conn, generation.pixmap, Some(generation.shmseg))
}

fn release_xids_checked(
    conn: &RustConnection,
    pixmap: u32,
    shmseg: Option<u32>,
) -> Result<(), CaptureFailure> {
    let mut failure = None;
    if let Some(shmseg) = shmseg {
        let result = conn
            .shm_detach(shmseg)
            .map_err(|error| connection_failure("detach MIT-SHM segment", error))
            .and_then(|cookie| cleanup_reply("detach MIT-SHM segment", cookie.check()));
        remember_failure(&mut failure, result);
    }
    let result = conn
        .free_pixmap(pixmap)
        .map_err(|error| connection_failure("free named pixmap", error))
        .and_then(|cookie| cleanup_reply("free named pixmap", cookie.check()));
    remember_failure(&mut failure, result);
    failure.map_or(Ok(()), Err)
}

fn release_shm_checked(conn: &RustConnection, shmseg: u32) -> Result<(), CaptureFailure> {
    conn.shm_detach(shmseg)
        .map_err(|error| connection_failure("detach MIT-SHM segment", error))
        .and_then(|cookie| cleanup_reply("detach MIT-SHM segment", cookie.check()))
}

fn release_binding_best_effort(conn: &RustConnection, mut binding: TargetBinding) {
    if let Some(generation) = binding.generation.take() {
        let _ = conn.shm_detach(generation.shmseg);
        let _ = conn.free_pixmap(generation.pixmap);
    }
    if binding.redirected {
        let _ = conn.composite_unredirect_window(binding.window, Redirect::AUTOMATIC);
    }
    let _ = conn.flush();
}

fn remember_failure(first: &mut Option<CaptureFailure>, result: Result<(), CaptureFailure>) {
    if first.is_none() {
        if let Err(error) = result {
            *first = Some(error);
        }
    }
}

fn prefer_cleanup_error(
    original: CaptureFailure,
    cleanup: Result<(), CaptureFailure>,
) -> CaptureFailure {
    cleanup.err().unwrap_or(original)
}

fn connection_failure(context: &str, error: ConnectionError) -> CaptureFailure {
    CaptureFailure::Permanent(format!("{context}: {error}"))
}

fn id_failure(context: &str, error: ReplyOrIdError) -> CaptureFailure {
    match error {
        ReplyOrIdError::X11Error(error) if transient_error_kind(error.error_kind) => {
            CaptureFailure::Transient(format!("{context}: {error:?}"))
        }
        error => CaptureFailure::Permanent(format!("{context}: {error}")),
    }
}

fn reply_failure(context: &str, error: ReplyError) -> CaptureFailure {
    match error {
        ReplyError::X11Error(error) if transient_error_kind(error.error_kind) => {
            CaptureFailure::Transient(format!("{context}: {error:?}"))
        }
        error => CaptureFailure::Permanent(format!("{context}: {error}")),
    }
}

fn cleanup_reply(context: &str, result: Result<(), ReplyError>) -> Result<(), CaptureFailure> {
    match result {
        Ok(()) => Ok(()),
        Err(ReplyError::X11Error(error)) if transient_error_kind(error.error_kind) => Ok(()),
        Err(error) => Err(CaptureFailure::Permanent(format!("{context}: {error}"))),
    }
}

fn transient_error_kind(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::Window | ErrorKind::Drawable | ErrorKind::Pixmap | ErrorKind::Match
    )
}

#[cfg(test)]
mod tests {
    use super::{calculate_layout, pixel_format_supported, target_transition, TargetTransition};
    use crate::capture::capture_engine::{AdaptiveCaptureEngine, CaptureEngine};
    use crate::capture::frame::CapturedFrame;
    use crate::capture::window_tracker::{WindowRect, WindowSnapshot};
    use x11rb::connection::Connection;
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
    fn same_target_is_idempotent() {
        assert_eq!(target_transition(Some(7), Some(7)), TargetTransition::Keep);
        assert_eq!(target_transition(None, None), TargetTransition::Keep);
        assert_eq!(target_transition(Some(7), None), TargetTransition::Clear);
        assert_eq!(
            target_transition(Some(7), Some(8)),
            TargetTransition::Replace(8)
        );
    }

    #[test]
    #[ignore = "requires DISPLAY without Composite 0.2 or MIT-SHM 1.2"]
    fn unavailable_capture_backend_is_deferred_to_set_target() {
        let mut capture = AdaptiveCaptureEngine::new().expect("construct capture engine");
        let error = capture
            .set_target(None)
            .expect_err("unsupported capture backend must fail closed");
        assert!(error.contains("Composite") || error.contains("MIT-SHM"));
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

    fn assert_frame(
        frame: &CapturedFrame,
        width: usize,
        height: usize,
        bgra: [u8; 4],
        stage: &str,
    ) {
        assert_eq!(
            (frame.width, frame.height),
            (width as i32, height as i32),
            "{stage} dimensions"
        );
        assert_eq!(frame.bgra.len(), width * height * 4, "{stage} length");
        let center = ((height / 2) * width + width / 2) * 4;
        assert_eq!(&frame.bgra[center..center + 4], &bgra, "{stage} pixel");
    }
}
