//! ScreenCast/PipeWire input preparation. The session and worker connection are
//! separate steps; enabling this feature does not start a capture session.

mod pixels;

pub use pixels::copy_mapped_frame;
