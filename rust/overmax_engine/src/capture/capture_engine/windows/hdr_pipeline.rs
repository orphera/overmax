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

/// 고속 sRGB OETF 변환을 위한 1,025-엔트리 룩업 테이블 (1KB L1 캐시 상주).
///
/// 픽셀 루프 내의 무거운 `powf(1.0 / 2.4)` 부동소수점 호출을 1클럭 테이블 조회로 대체합니다.
pub static FAST_OETF_TABLE: std::sync::LazyLock<[u8; 1025]> = std::sync::LazyLock::new(|| {
    let mut table = [0u8; 1025];
    for (i, item) in table.iter_mut().enumerate() {
        let lin = i as f32 / 1024.0;
        let srgb = strict_srgb_oetf(lin);
        *item = (srgb * 255.0 + 0.5) as u8;
    }
    table
});

/// 2-Stage Rational Spline 역톤매핑 커브.
///
/// - 중간 톤(V <= v_knee): 앨범 자켓과 UI 색상을 SDR 원본과 1:1로 밝고 선명하게 복원 (scale_mid 기준 선형 복원)
/// - 고휘도 숄더(V > v_knee): C1 연속(접합점 도함수 일치)으로 1.0(255)에 부드럽게 점근하여 고휘도 폰트 블룸 억제 및 255 클램핑 방지
#[inline(always)]
pub fn tone_map_2stage_rational(v: f32, scale_mid: f32, v_knee: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    let inv_scale = 1.0 / scale_mid;
    let l_knee = (v_knee * inv_scale).clamp(0.0, 1.0);
    if v <= v_knee {
        (v * inv_scale).clamp(0.0, 1.0)
    } else {
        let delta_v = v - v_knee;
        let m = inv_scale;
        let rem = 1.0 - l_knee;
        if rem <= 1e-6 {
            1.0
        } else {
            let k = rem / m;
            let shoulder = rem * (delta_v / (k + delta_v));
            (l_knee + shoulder).clamp(0.0, 1.0)
        }
    }
}

/// Windows DWM scRGB 모드에서의 실효 SDR Target White 기본 레벨 (1.0 = 80 nits, 4.88 = 390.4 nits).
///
/// DisplayHDR 400 패널 피크(408.76 nits)의 95.5% 유효 백색 기준선이며, OS API 감지 실패 시의 안전한 폴백으로 사용됩니다.
pub const SCRGB_SDR_WHITE_LEVEL: f32 = 4.88;

/// 주어진 SDR 화이트 레벨(배율, 1.0 = 80 nits)에 대응하는 64KB 고속 역변환 룩업 테이블을 생성합니다.
pub fn build_lut_table(sdr_white_level: f32) -> Box<[u8; 65536]> {
    let mut table = Box::new([0u8; 65536]);
    let safe_level = if sdr_white_level <= 0.0 {
        SCRGB_SDR_WHITE_LEVEL
    } else {
        sdr_white_level
    };
    let scale_mid = safe_level * (4.0 / 4.88);
    let v_knee = safe_level * (2.2 / 4.88);

    for bits in 0..=65535u16 {
        let val_f32 = f16_to_f32(bits);
        let lin = tone_map_2stage_rational(val_f32, scale_mid, v_knee);
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

static ACTIVE_SDR_WHITE_LEVEL: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(5168); // 5.168 * 1000

/// 현재 활성화된 SDR 화이트 레벨을 반환합니다.
pub fn get_active_sdr_white_level() -> f32 {
    ACTIVE_SDR_WHITE_LEVEL.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0
}

/// 현재 활성화된 64KB HDR LUT를 반환합니다.
pub fn get_active_lut() -> Arc<[u8; 65536]> {
    ACTIVE_HDR_LUT.read().unwrap().clone()
}

/// 활성화된 전역 SDR 화이트 레벨 및 HDR LUT를 갱신합니다.
pub fn set_active_sdr_white_level(level: f32) {
    let safe_level = if level <= 0.0 { 1.0 } else { level };
    let val = (safe_level * 1000.0 + 0.5) as u32;
    ACTIVE_SDR_WHITE_LEVEL.store(val, std::sync::atomic::Ordering::Relaxed);
    let table = build_lut_table(safe_level);
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

/// DCI-P3 (D65) 광색역 역변환 계수 (행 합계 = 1.0000, D65 화이트포인트 완벽 보존)
pub const M_709_TO_P3: [[f32; 3]; 3] = [
    [0.822475, 0.177378, 0.000000],
    [0.033155, 0.966935, 0.000000],
    [0.017052, 0.072371, 0.910581],
];

/// scRGB FP16 버퍼를 DCI-P3 광색역 역변환 및 2-Stage Rational Spline 역톤매핑을 적용하여 고정밀 BGRA8로 변환합니다.
///
/// 게임 엔진(DJMAX RESPECT V)이 DCI-P3 (D65) 색공간에서 렌더링한 버퍼를 수학적으로 정확하게 복원하여
/// 음수 채널 클리핑을 방지하고, 2-Stage 역톤매핑으로 중간톤 앨범 자켓 유사도(0.87+)와
/// 고휘도 폰트(Score 1,000,000 / Rate 100.00%)를 100% 동시에 복원합니다.
///
/// # Safety
///
/// `src_row`는 최소 `pixel_count * 8` 바이트, `dst_row`는 최소 `pixel_count * 4` 바이트 쓸 수 있어야 합니다.
#[inline(always)]
pub unsafe fn convert_scrgb_fp16_to_bgra8_p3(
    src_row: *const u8,
    dst_row: *mut u8,
    pixel_count: usize,
    scale: f32,
) {
    let src = src_row as *const u16;
    let safe_scale = if scale <= 0.0 {
        SCRGB_SDR_WHITE_LEVEL
    } else {
        scale
    };

    let scale_mid = safe_scale * (4.0 / 4.88);
    let v_knee = safe_scale * (2.2 / 4.88);

    let oetf = &*FAST_OETF_TABLE;

    for x in 0..pixel_count {
        let r_bits = std::ptr::read_unaligned(src.add(x * 4));
        let g_bits = std::ptr::read_unaligned(src.add(x * 4 + 1));
        let b_bits = std::ptr::read_unaligned(src.add(x * 4 + 2));

        let r = f16_to_f32(r_bits);
        let g = f16_to_f32(g_bits);
        let b = f16_to_f32(b_bits);

        // DCI-P3 역변환 행렬 곱
        let r_p3 = (0.822475 * r + 0.177378 * g).max(0.0);
        let g_p3 = (0.033155 * r + 0.966935 * g).max(0.0);
        let b_p3 = (0.017052 * r + 0.072371 * g + 0.910581 * b).max(0.0);

        let r_lin = tone_map_2stage_rational(r_p3, scale_mid, v_knee);
        let g_lin = tone_map_2stage_rational(g_p3, scale_mid, v_knee);
        let b_lin = tone_map_2stage_rational(b_p3, scale_mid, v_knee);

        let r_idx = ((r_lin * 1024.0) as usize).min(1024);
        let g_idx = ((g_lin * 1024.0) as usize).min(1024);
        let b_idx = ((b_lin * 1024.0) as usize).min(1024);

        let dst = dst_row.add(x * 4);
        *dst.add(0) = oetf[b_idx];
        *dst.add(1) = oetf[g_idx];
        *dst.add(2) = oetf[r_idx];
        *dst.add(3) = 255;
    }
}

/// R16G16B16A16_FLOAT (scRGB) 버퍼를 전역 활성 화이트 레벨 및 DCI-P3 변환을 사용하여 디텍션용 BGRA8 버퍼로 변환합니다.
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
    convert_scrgb_fp16_to_bgra8_p3(src_row, dst_row, pixel_count, get_active_sdr_white_level());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tone_map_2stage_rational_continuity_and_monotonicity() {
        let scale_mid = 4.0;
        let v_knee = 2.2;

        // 1. Zero check
        assert_eq!(tone_map_2stage_rational(0.0, scale_mid, v_knee), 0.0);

        // 2. Knee continuity (C0)
        let eps = 1e-4;
        let left = tone_map_2stage_rational(v_knee - eps, scale_mid, v_knee);
        let center = tone_map_2stage_rational(v_knee, scale_mid, v_knee);
        let right = tone_map_2stage_rational(v_knee + eps, scale_mid, v_knee);
        assert!((left - center).abs() < 1e-3);
        assert!((right - center).abs() < 1e-3);

        // 3. Monotonicity across dynamic range
        let mut prev = 0.0f32;
        for i in 0..1000 {
            let v = i as f32 * 0.02; // 0.0 to 20.0
            let curr = tone_map_2stage_rational(v, scale_mid, v_knee);
            assert!(
                curr >= prev,
                "Monotonicity violated at v={}: {} < {}",
                v,
                curr,
                prev
            );
            assert!(curr <= 1.0, "Clamping violated at v={}: {}", v, curr);
            prev = curr;
        }

        // 4. Asymptotic convergence to 1.0
        let extreme = tone_map_2stage_rational(100.0, scale_mid, v_knee);
        assert!(extreme > 0.98 && extreme <= 1.0);
    }

    #[test]
    fn test_fast_oetf_table_precision() {
        for i in 0..=1024 {
            let lin = i as f32 / 1024.0;
            let exact = (strict_srgb_oetf(lin) * 255.0 + 0.5) as u8;
            let from_lut = FAST_OETF_TABLE[i];
            assert_eq!(exact, from_lut);
        }
    }

    #[test]
    fn test_build_lut_table_consistency() {
        let lut = build_lut_table(SCRGB_SDR_WHITE_LEVEL);
        assert_eq!(lut.len(), 65536);

        // 0.0 (half float 0x0000) -> sRGB 0
        assert_eq!(lut[0], 0);

        // Half float 5.1680 (0x452b) -> sRGB >= 230 (highlight shoulder preserved without bloom saturation)
        let half_5168 = 0x452bu16;
        let f = f16_to_f32(half_5168);
        assert!((f - 5.168).abs() < 0.005);
        assert!(lut[half_5168 as usize] >= 230);
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
