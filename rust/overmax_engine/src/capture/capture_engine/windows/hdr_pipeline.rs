use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// scRGB 1.0의 물리적 기준 휘도 (80 nits)
#[allow(dead_code)]
pub const SCRGB_REFERENCE_WHITE_NITS: f32 = 80.0;

/// HDR 프레임 1회성 덤프 제어 플래그
static HDR_DUMPED: AtomicBool = AtomicBool::new(false);

/// FP16 (half-precision IEEE 754) 비트를 f32로 변환합니다.
///
/// NaN이나 무한대(Infinity) 등 비정상 픽셀은 0.0으로 안전하게 차단합니다.
#[inline(always)]
pub fn f16_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) & 0x1) as u32;
    let exp = ((bits >> 10) & 0x1f) as u32;
    let frac = (bits & 0x03ff) as u32;

    let f32_bits: u32;

    if exp == 0 {
        if frac == 0 {
            f32_bits = sign << 31;
        } else {
            let mut frac = frac;
            let mut exp_unbiased = -14i32;
            while (frac & 0x0400) == 0 {
                frac <<= 1;
                exp_unbiased -= 1;
            }
            frac &= 0x03ff;
            let exp32 = (exp_unbiased + 127) as u32;
            f32_bits = (sign << 31) | (exp32 << 23) | (frac << 13);
        }
    } else if exp == 0x1f {
        // NaN 또는 Infinity 방어: 디텍션 파이프라인 오염 방지
        return 0.0;
    } else {
        let exp32 = exp + (127 - 15);
        f32_bits = (sign << 31) | (exp32 << 23) | (frac << 13);
    }

    let val = f32::from_bits(f32_bits);
    if val.is_nan() || val.is_infinite() {
        0.0
    } else {
        val
    }
}

/// 표준 sRGB OETF (Opto-Electronic Transfer Function) 감마 곡선.
#[inline(always)]
pub fn strict_srgb_oetf(linear: f32) -> f32 {
    if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// R16G16B16A16_FLOAT (scRGB) 버퍼를 디텍션용 BGRA8 버퍼로 변환합니다.
///
/// 현재는 개발자님이 튜닝하신 정규화 및 감마 매핑 파이프라인을 온전히 보존하며,
/// 향후 덤프 파일 분석을 통해 최적의 역변환 모델로 점진적으로 고도화됩니다.
///
/// # Safety
///
/// `src_row`는 최소 `pixel_count * 8` 바이트 유효한 메모리를 가리켜야 하며,
/// `dst_row`는 최소 `pixel_count * 4` 바이트 쓸 수 있는 메모리를 가리켜야 합니다.
#[inline]
pub unsafe fn convert_scrgb_fp16_to_bgra8(
    src_row: *const u8,
    dst_row: *mut u8,
    pixel_count: usize,
) {
    const SCRGB_MAX: f32 = 5.168;
    const SCRGB_MIN: f32 = -0.5;
    const SCRGB_RANGE: f32 = SCRGB_MAX - SCRGB_MIN;

    for x in 0..pixel_count {
        let src = src_row.add(x * 8);

        let r_bits = std::ptr::read_unaligned(src as *const u16);
        let g_bits = std::ptr::read_unaligned(src.add(2) as *const u16);
        let b_bits = std::ptr::read_unaligned(src.add(4) as *const u16);

        let r_raw = f16_to_f32(r_bits);
        let g_raw = f16_to_f32(g_bits);
        let b_raw = f16_to_f32(b_bits);

        let r_lin = ((r_raw - SCRGB_MIN) / SCRGB_RANGE).clamp(0.0, 1.0);
        let g_lin = ((g_raw - SCRGB_MIN) / SCRGB_RANGE).clamp(0.0, 1.0);
        let b_lin = ((b_raw - SCRGB_MIN) / SCRGB_RANGE).clamp(0.0, 1.0);

        let mut r_srgb = strict_srgb_oetf(r_lin);
        let g_srgb = strict_srgb_oetf(g_lin);
        let b_srgb = strict_srgb_oetf(b_lin);

        // Cyan UI 채도 사수 가드
        if g_lin > 0.6 && b_lin > 0.6 && r_lin < 0.25 {
            r_srgb *= r_lin / 0.25;
        }

        let dst = dst_row.add(x * 4);
        *dst.add(0) = (b_srgb * 255.0 + 0.5) as u8;
        *dst.add(1) = (g_srgb * 255.0 + 0.5) as u8;
        *dst.add(2) = (r_srgb * 255.0 + 0.5) as u8;
        *dst.add(3) = 255;
    }
}

/// HDR 모드에서 수신된 첫 번째 프레임을 `cache/hdr_snapshot.raw`에 1회 한정으로 자동 덤프합니다.
///
/// 덤프된 파일은 SDR 개발 환경으로 가져와 오프라인 역변환 수식 분석 및 단위 테스트에 사용됩니다.
///
/// # Safety
///
/// `data_ptr`는 `height * row_pitch` 바이트 이상의 유효하게 매핑된 DXGI 텍스처 버퍼 메모리를 가리켜야 합니다.
pub unsafe fn maybe_dump_hdr_frame(
    data_ptr: *const u8,
    width: usize,
    height: usize,
    row_pitch: usize,
    is_atlas: bool,
) {
    if HDR_DUMPED.swap(true, Ordering::SeqCst) {
        return;
    }

    let cache_dir = Path::new("cache");
    if !cache_dir.exists() {
        let _ = std::fs::create_dir_all(cache_dir);
    }

    let raw_path = cache_dir.join("hdr_snapshot.raw");
    let json_path = cache_dir.join("hdr_snapshot.json");

    // 1. RAW 버퍼 저장 (row pitch 패딩을 제거한 순수 w * h * 8바이트)
    if let Ok(mut file) = File::create(&raw_path) {
        let line_bytes = width * 8;
        let mut total_written = 0;
        for y in 0..height {
            let row_src = data_ptr.add(y * row_pitch);
            let slice = std::slice::from_raw_parts(row_src, line_bytes);
            if file.write_all(slice).is_err() {
                eprintln!("[HDR DUMP] Failed writing row {}", y);
                return;
            }
            total_written += line_bytes;
        }

        eprintln!(
            "[HDR DUMP] Successfully dumped 1st HDR frame: {}x{} ({} bytes) -> {:?}",
            width, height, total_written, raw_path
        );
    }

    // 2. 메타데이터 JSON 저장
    let metadata = format!(
        "{{\n  \"width\": {},\n  \"height\": {},\n  \"channels\": 4,\n  \"format\": \"R16G16B16A16_FLOAT\",\n  \"is_atlas\": {},\n  \"timestamp_unix\": {}\n}}\n",
        width,
        height,
        is_atlas,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    let _ = std::fs::write(&json_path, metadata);
}
