use std::sync::{Arc, RwLock};

/// scRGB 1.0의 물리적 기준 휘도 (80 nits)
#[allow(dead_code)]
pub const SCRGB_REFERENCE_WHITE_NITS: f32 = 80.0;

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
/// Windows DWM scRGB 모드에서의 SDR Reference White 기본 레벨 (1.0 = 80 nits, 5.168 = 413.44 nits).
///
/// 실측 계측된 DJMAX RESPECT V 환경의 기준 화이트 레벨이며, OS API 감지 실패 시의 안전한 폴백으로 사용됩니다.
pub const SCRGB_SDR_WHITE_LEVEL: f32 = 5.168;

/// 주어진 SDR 화이트 레벨(배율, 1.0 = 80 nits)에 대응하는 64KB 고속 역변환 룩업 테이블을 생성합니다.
pub fn build_lut_table(sdr_white_level: f32) -> Box<[u8; 65536]> {
    let mut table = Box::new([0u8; 65536]);
    let safe_level = if sdr_white_level <= 0.0 {
        1.0
    } else {
        sdr_white_level
    };
    for bits in 0..=65535u16 {
        let val_f32 = f16_to_f32(bits);
        let lin = (val_f32 / safe_level).clamp(0.0, 1.0);
        let srgb = strict_srgb_oetf(lin);
        table[bits as usize] = (srgb * 255.0 + 0.5) as u8;
    }
    table
}

/// 활성 64KB 고속 역변환 룩업 테이블.
///
/// 캡처 엔진 초기화 시 감지된 모니터의 SDR 화이트 레벨로 자동 갱신됩니다.
static ACTIVE_HDR_LUT: std::sync::LazyLock<RwLock<Arc<[u8; 65536]>>> =
    std::sync::LazyLock::new(|| {
        let table = build_lut_table(SCRGB_SDR_WHITE_LEVEL);
        RwLock::new(Arc::new(*table))
    });

/// 현재 활성화된 64KB HDR LUT를 반환합니다.
pub fn get_active_lut() -> Arc<[u8; 65536]> {
    ACTIVE_HDR_LUT.read().unwrap().clone()
}

/// 활성화된 전역 HDR LUT를 새로운 SDR 화이트 레벨로 갱신합니다.
pub fn set_active_sdr_white_level(level: f32) {
    let table = build_lut_table(level);
    *ACTIVE_HDR_LUT.write().unwrap() = Arc::new(*table);
}

/// Win32 Connecting and Configuring Displays (CCD) API를 통해
/// 현재 모니터의 OS 설정 SDR 화이트 레벨(배율, 1.0 = 80 nits)을 자동으로 조회합니다.
///
/// `target_device_name`이 제공되면 해당 GDI 디바이스명(예: `\\.\DISPLAY1`)과 일치하는 모니터의 값을 조회하며,
/// `None`이면 첫 번째 활성 디스플레이의 값을 반환합니다.
#[cfg(windows)]
pub fn detect_monitor_sdr_white_level(target_device_name: Option<&[u16]>) -> Option<f32> {
    use windows::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
        DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
        DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SDR_WHITE_LEVEL,
        DISPLAYCONFIG_SOURCE_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
    };
    use windows::Win32::Foundation::WIN32_ERROR;

    unsafe {
        let mut path_count = 0u32;
        let mut mode_count = 0u32;
        let err =
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count);
        if err != WIN32_ERROR(0) || path_count == 0 {
            return None;
        }

        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];

        let err = QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        );
        if err != WIN32_ERROR(0) {
            return None;
        }

        for path in paths.iter().take(path_count as usize) {
            if let Some(target) = target_device_name {
                let mut source_name = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
                source_name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
                source_name.header.size =
                    std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
                source_name.header.adapterId = path.sourceInfo.adapterId;
                source_name.header.id = path.sourceInfo.id;

                let name_err = DisplayConfigGetDeviceInfo(&mut source_name.header);
                if name_err != 0 {
                    continue;
                }

                let target_len = target.iter().position(|&c| c == 0).unwrap_or(target.len());
                let src_len = source_name
                    .viewGdiDeviceName
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(source_name.viewGdiDeviceName.len());

                if target[..target_len] != source_name.viewGdiDeviceName[..src_len] {
                    continue;
                }
            }

            let mut sdr_white = DISPLAYCONFIG_SDR_WHITE_LEVEL::default();
            sdr_white.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL;
            sdr_white.header.size = std::mem::size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32;
            sdr_white.header.adapterId = path.targetInfo.adapterId;
            sdr_white.header.id = path.targetInfo.id;

            let sdr_err = DisplayConfigGetDeviceInfo(&mut sdr_white.header);
            if sdr_err == 0 && sdr_white.SDRWhiteLevel > 0 {
                return Some(sdr_white.SDRWhiteLevel as f32 / 1000.0);
            }
        }
    }

    None
}

/// R16G16B16A16_FLOAT (scRGB) 버퍼를 특정 LUT를 사용하여 디텍션용 BGRA8 버퍼로 초고속 변환합니다.
///
/// # Safety
///
/// `src_row`는 최소 `pixel_count * 8` 바이트, `dst_row`는 최소 `pixel_count * 4` 바이트 쓸 수 있어야 합니다.
#[inline(always)]
pub unsafe fn convert_scrgb_fp16_to_bgra8_with_lut(
    src_row: *const u8,
    dst_row: *mut u8,
    pixel_count: usize,
    lut: &[u8; 65536],
) {
    let src = src_row as *const u16;

    for x in 0..pixel_count {
        let r_bits = std::ptr::read_unaligned(src.add(x * 4));
        let g_bits = std::ptr::read_unaligned(src.add(x * 4 + 1));
        let b_bits = std::ptr::read_unaligned(src.add(x * 4 + 2));

        let dst = dst_row.add(x * 4);
        *dst.add(0) = lut[b_bits as usize];
        *dst.add(1) = lut[g_bits as usize];
        *dst.add(2) = lut[r_bits as usize];
        *dst.add(3) = 255;
    }
}

/// R16G16B16A16_FLOAT (scRGB) 버퍼를 전역 활성 LUT를 사용하여 디텍션용 BGRA8 버퍼로 초고속 변환합니다.
///
/// # Safety
///
/// `src_row`는 최소 `pixel_count * 8` 바이트, `dst_row`는 최소 `pixel_count * 4` 바이트 쓸 수 있어야 합니다.
#[inline]
pub unsafe fn convert_scrgb_fp16_to_bgra8(
    src_row: *const u8,
    dst_row: *mut u8,
    pixel_count: usize,
) {
    let lut = get_active_lut();
    convert_scrgb_fp16_to_bgra8_with_lut(src_row, dst_row, pixel_count, &lut);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_lut_table_consistency() {
        let lut = build_lut_table(SCRGB_SDR_WHITE_LEVEL);
        assert_eq!(lut.len(), 65536);

        // 0.0 (half float 0x0000) -> sRGB 0
        assert_eq!(lut[0], 0);

        // Half float 5.1680 (0x452b) -> sRGB 255
        let half_5168 = 0x452bu16;
        let f = f16_to_f32(half_5168);
        assert!((f - 5.168).abs() < 0.005);
        assert_eq!(lut[half_5168 as usize], 255);
    }

    #[test]
    fn test_detect_monitor_sdr_white_level() {
        let level = detect_monitor_sdr_white_level(None);
        println!("detect_monitor_sdr_white_level(None) = {:?}", level);
        if let Some(val) = level {
            assert!((0.5..=20.0).contains(&val));
        }
    }
}
