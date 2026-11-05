use crate::capture::frame::CapturedFrame;
use pipewire::spa::{
    buffer::{Data, DataFlags, DataType},
    param::video::{VideoFlags, VideoFormat, VideoInfoRaw, VideoInterlaceMode},
};

/// Copy one dequeued, CPU-mapped PipeWire buffer to tightly packed BGRA.
///
/// The caller owns the dequeued buffer and must return it to PipeWire after this
/// call (including on error). No mapped pointers escape. Only a successful call
/// produces a usable frame; failure clears the output, retaining its allocation.
/// This does not establish freshness, target identity or detection verification.
pub fn copy_mapped_frame(
    format: &VideoInfoRaw,
    planes: &mut [Data],
    output: &mut CapturedFrame,
) -> Result<(), &'static str> {
    output.width = 0;
    output.height = 0;
    output.bgra.clear();

    let pixel_format = format.format();
    if !matches!(
        pixel_format,
        VideoFormat::BGRA | VideoFormat::BGRx | VideoFormat::RGBA | VideoFormat::RGBx
    ) {
        return Err("unsupported PipeWire pixel format (expected BGRA/BGRx/RGBA/RGBx)");
    }
    // Variable FPS affects delivery, not pixel layout. Reject other flags,
    // including premultiplied alpha and DMA-BUF format modifiers.
    if !(format.flags() & !VideoFlags::VARIABLE_FPS).is_empty()
        || format.modifier() != 0
        || format.interlace_mode() != VideoInterlaceMode::Progressive
        || format.views() > 1
    {
        return Err("unsupported PipeWire video layout");
    }
    let size = format.size();
    let width = i32::try_from(size.width).map_err(|_| "PipeWire width exceeds frame contract")?;
    let height =
        i32::try_from(size.height).map_err(|_| "PipeWire height exceeds frame contract")?;
    if width == 0 || height == 0 {
        return Err("empty PipeWire frame dimensions");
    }
    let [plane] = planes else {
        return Err("expected a single packed PipeWire plane");
    };
    if !matches!(plane.type_(), DataType::MemPtr | DataType::MemFd) {
        return Err("unsupported PipeWire memory type (expected mapped MemPtr/MemFd)");
    }
    if !plane.flags().contains(DataFlags::READABLE) {
        return Err("PipeWire plane is not readable");
    }
    if plane.as_raw().chunk.is_null() {
        return Err("missing PipeWire chunk");
    }
    let chunk = plane.chunk();
    // A flagged chunk can be corrupt or represent neutral/empty content. It
    // must not be treated as a new game observation, even if memory is present.
    if !chunk.flags().is_empty() {
        return Err("flagged PipeWire chunk");
    }
    let offset = chunk.offset() as usize;
    let valid_size = chunk.size() as usize;
    let stride = usize::try_from(chunk.stride()).map_err(|_| "negative PipeWire stride")?;
    let row_bytes = (width as usize)
        .checked_mul(4)
        .ok_or("PipeWire row size overflow")?;
    if stride < row_bytes {
        return Err("PipeWire stride is shorter than a pixel row");
    }
    let required = (height as usize - 1)
        .checked_mul(stride)
        .and_then(|bytes| bytes.checked_add(row_bytes))
        .ok_or("PipeWire image extent overflow")?;
    let packed_size = (height as usize)
        .checked_mul(row_bytes)
        .ok_or("PipeWire packed size overflow")?;
    let data = plane.data().ok_or("PipeWire plane is not CPU-mapped")?;
    if data.is_empty() {
        return Err("empty PipeWire allocation");
    }
    // spa_chunk defines offset modulo maxsize, and size clamped to maxsize.
    // Ring-wrapped images are deliberately unsupported; never stitch together
    // an apparent frame from bytes outside a contiguous valid chunk.
    let offset = offset % data.len();
    let valid_size = valid_size.min(data.len());
    let end = offset
        .checked_add(valid_size)
        .ok_or("PipeWire chunk extent overflow")?;
    if end > data.len() || required > valid_size {
        return Err("truncated or wrapped PipeWire image");
    }
    output
        .bgra
        .try_reserve(packed_size)
        .map_err(|_| "could not allocate PipeWire frame")?;
    output.bgra.resize(packed_size, 0);
    let source = &data[offset..offset + required];
    for (src, dst) in source
        .chunks(stride)
        .zip(output.bgra.chunks_exact_mut(row_bytes))
    {
        let src = &src[..row_bytes];
        if pixel_format == VideoFormat::BGRA {
            dst.copy_from_slice(src);
        } else {
            let swap_rb = matches!(pixel_format, VideoFormat::RGBA | VideoFormat::RGBx);
            let has_alpha = pixel_format == VideoFormat::RGBA;
            for (src, dst) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                dst.copy_from_slice(&[
                    src[if swap_rb { 2 } else { 0 }],
                    src[1],
                    src[if swap_rb { 0 } else { 2 }],
                    if has_alpha { src[3] } else { 255 },
                ]);
            }
        }
    }
    output.width = width;
    output.height = height;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipewire::spa::{sys, utils::Rectangle};

    fn format(pixel_format: VideoFormat, width: u32, height: u32) -> VideoInfoRaw {
        let mut format = VideoInfoRaw::new();
        format.set_format(pixel_format);
        format.set_size(Rectangle { width, height });
        format
    }

    struct Buffer {
        bytes: Vec<u8>,
        chunk: sys::spa_chunk,
        memory: DataType,
        readable: bool,
        mapped: bool,
        has_chunk: bool,
    }

    impl Buffer {
        fn new(bytes: Vec<u8>, stride: i32) -> Self {
            Self {
                chunk: sys::spa_chunk {
                    offset: 0,
                    size: bytes.len() as u32,
                    stride,
                    flags: 0,
                },
                bytes,
                memory: DataType::MemFd,
                readable: true,
                mapped: true,
                has_chunk: true,
            }
        }

        fn copy(
            &mut self,
            format: &VideoInfoRaw,
            out: &mut CapturedFrame,
        ) -> Result<(), &'static str> {
            let mut data = sys::spa_data {
                type_: self.memory.as_raw(),
                flags: if self.readable {
                    DataFlags::READABLE.bits()
                } else {
                    0
                },
                fd: -1,
                mapoffset: 0,
                maxsize: self.bytes.len() as u32,
                data: if self.mapped {
                    self.bytes.as_mut_ptr().cast()
                } else {
                    std::ptr::null_mut()
                },
                chunk: if self.has_chunk {
                    &mut self.chunk
                } else {
                    std::ptr::null_mut()
                },
            };
            // Data is repr(transparent) over spa_data. Both pointers remain valid
            // for this call, and the fixture retains exclusive access to them.
            let data = unsafe { &mut *(&mut data as *mut sys::spa_data).cast::<Data>() };
            copy_mapped_frame(format, std::slice::from_mut(data), out)
        }
    }

    #[test]
    fn converts_negotiated_channel_orders() {
        for (pixel_format, expected) in [
            (VideoFormat::BGRA, vec![10, 20, 30, 40]),
            (VideoFormat::BGRx, vec![10, 20, 30, 255]),
            (VideoFormat::RGBA, vec![30, 20, 10, 40]),
            (VideoFormat::RGBx, vec![30, 20, 10, 255]),
        ] {
            for memory in [DataType::MemPtr, DataType::MemFd] {
                let mut buffer = Buffer::new(vec![10, 20, 30, 40], 4);
                buffer.memory = memory;
                let mut out = CapturedFrame::default();
                buffer.copy(&format(pixel_format, 1, 1), &mut out).unwrap();
                assert_eq!(out.bgra, expected);
                assert_eq!((out.width, out.height), (1, 1));
            }
        }
    }

    #[test]
    fn strips_row_padding_and_chunk_prefix_without_requiring_trailing_padding() {
        let mut buffer = Buffer::new(vec![99, 99, 1, 2, 3, 4, 88, 88, 5, 6, 7, 8], 6);
        buffer.chunk.offset = 2;
        buffer.chunk.size = 10;
        let mut out = CapturedFrame::default();
        buffer
            .copy(&format(VideoFormat::BGRA, 1, 2), &mut out)
            .unwrap();
        assert_eq!(out.bgra, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn applies_spa_offset_modulo_and_size_clamp() {
        let mut buffer = Buffer::new(vec![1, 2, 3, 4], 4);
        buffer.chunk.offset = 8;
        buffer.chunk.size = u32::MAX;
        let mut out = CapturedFrame::default();
        buffer
            .copy(&format(VideoFormat::BGRA, 1, 1), &mut out)
            .unwrap();
        assert_eq!(out.bgra, [1, 2, 3, 4]);
    }

    #[test]
    fn resize_replaces_dimensions_and_truncates_previous_pixels() {
        let mut buffer = Buffer::new(vec![1; 16], 8);
        let mut out = CapturedFrame::default();
        buffer
            .copy(&format(VideoFormat::BGRA, 2, 2), &mut out)
            .unwrap();
        let capacity = out.bgra.capacity();
        buffer.chunk.stride = 4;
        buffer
            .copy(&format(VideoFormat::BGRA, 1, 1), &mut out)
            .unwrap();
        assert_eq!((out.width, out.height, out.bgra.len()), (1, 1, 4));
        assert_eq!(out.bgra.capacity(), capacity);
    }

    #[test]
    fn rejects_bad_chunks_and_invalidates_old_output() {
        for (offset, size, stride, flags) in [
            (0, 0, 4, 0),
            (0, 7, 4, 0),
            (1, 8, 4, 0),
            (0, 8, 0, 0),
            (0, 8, -4, 0),
            (0, 8, 3, 0),
            (0, 8, i32::MAX, 0),
            (0, 8, 4, 1),
            (0, 8, 4, 2),
        ] {
            let mut buffer = Buffer::new(vec![1; 8], stride);
            buffer.chunk.offset = offset;
            buffer.chunk.size = size;
            buffer.chunk.flags = flags;
            let mut out = CapturedFrame {
                width: 1,
                height: 1,
                bgra: vec![42; 4],
            };
            assert!(buffer
                .copy(&format(VideoFormat::BGRA, 1, 2), &mut out)
                .is_err());
            assert_eq!(out, CapturedFrame::default());
        }
    }

    #[test]
    fn rejects_dma_buf_unmapped_unreadable_and_missing_chunk() {
        for case in 0..4 {
            let mut buffer = Buffer::new(vec![1; 4], 4);
            match case {
                0 => buffer.memory = DataType::DmaBuf,
                1 => buffer.mapped = false,
                2 => buffer.readable = false,
                _ => buffer.has_chunk = false,
            }
            assert!(buffer
                .copy(
                    &format(VideoFormat::BGRA, 1, 1),
                    &mut CapturedFrame::default()
                )
                .is_err());
        }
    }

    #[test]
    fn rejects_unsupported_formats_dimensions_and_layouts() {
        let mut formats = vec![
            format(VideoFormat::NV12, 1, 1),
            format(VideoFormat::BGRA, 0, 1),
            format(VideoFormat::BGRA, 1, 0),
            format(VideoFormat::BGRA, u32::MAX, 1),
            format(VideoFormat::BGRA, 1, u32::MAX),
            format(VideoFormat::BGRA, i32::MAX as u32, i32::MAX as u32),
        ];
        let mut modified = format(VideoFormat::BGRA, 1, 1);
        modified.set_modifier(1);
        formats.push(modified);
        let mut premultiplied = format(VideoFormat::BGRA, 1, 1);
        premultiplied.set_flags(VideoFlags::PREMULTIPLIED_ALPHA);
        formats.push(premultiplied);
        let mut interlaced = format(VideoFormat::BGRA, 1, 1);
        interlaced.set_interlace_mode(VideoInterlaceMode::Interleaved);
        formats.push(interlaced);
        let mut multiview = format(VideoFormat::BGRA, 1, 1);
        multiview.set_views(2);
        formats.push(multiview);
        for format in formats {
            assert!(Buffer::new(vec![1; 4], 4)
                .copy(&format, &mut CapturedFrame::default())
                .is_err());
        }
    }

    #[test]
    fn rejects_missing_plane_and_empty_allocation() {
        let format = format(VideoFormat::BGRA, 1, 1);
        assert!(copy_mapped_frame(&format, &mut [], &mut CapturedFrame::default()).is_err());
        assert!(Buffer::new(vec![], 4)
            .copy(&format, &mut CapturedFrame::default())
            .is_err());
    }
}
