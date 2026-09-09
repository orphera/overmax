use crate::capture::capture_engine::CaptureEngine;
use crate::capture::frame::CapturedFrame;
use crate::capture::window_tracker::WindowRect;
use crate::detector::atlas_layout::{ATLAS_HEIGHT, ATLAS_SLOTS, ATLAS_WIDTH};

use windows::core::Interface;
use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter, IDXGIDevice, IDXGIFactory1, IDXGIOutput1, IDXGIOutput5,
    IDXGIOutput6, IDXGIOutputDuplication, DXGI_OUTDUPL_FRAME_INFO,
};

pub struct DxgiCaptureEngine {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    adapter: IDXGIAdapter,
    duplication: IDXGIOutputDuplication,
    staging_texture: Option<ID3D11Texture2D>,
    staging_atlas_textures: [Option<ID3D11Texture2D>; 2],
    atlas_write_idx: usize,
    atlas_frames_captured: usize,
    normalizer: Option<super::normalizer::D3d11Normalizer>,
    enable_gpu_atlas: bool,
    width: u32,
    height: u32,
    output_bounds: RECT,
    is_hdr_format: bool,
    device_name: [u16; 32],
    configured_sdr_white_level: Option<f32>,
    active_sdr_white_level: f32,
    hdr_lut: std::sync::Arc<[u8; 65536]>,
}

unsafe impl Send for DxgiCaptureEngine {}
unsafe impl Sync for DxgiCaptureEngine {}

unsafe fn create_device_and_adapter(
) -> Result<(ID3D11Device, ID3D11DeviceContext, IDXGIAdapter), String> {
    // 1. 실제 디스플레이 출력이 연결된 하드웨어 어댑터를 우선 탐색 (듀얼 GPU 환경: iGPU vs 외장 그래픽 충돌 방지)
    if let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() {
        let mut adapter_idx = 0;
        while let Ok(adapter1) = factory.EnumAdapters1(adapter_idx) {
            if adapter1.EnumOutputs(0).is_ok() {
                let mut device = None;
                let mut context = None;
                let mut level = D3D_FEATURE_LEVEL_11_0;
                let adapter: IDXGIAdapter = match adapter1.cast() {
                    Ok(a) => a,
                    Err(_) => {
                        adapter_idx += 1;
                        continue;
                    }
                };

                let res = D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_FLAG(0),
                    Some(&[D3D_FEATURE_LEVEL_11_0]),
                    windows::Win32::Graphics::Direct3D11::D3D11_SDK_VERSION,
                    Some(&mut device),
                    Some(&mut level),
                    Some(&mut context),
                );

                if res.is_ok() {
                    if let (Some(device), Some(context)) = (device, context) {
                        return Ok((device, context, adapter));
                    }
                }
            }
            adapter_idx += 1;
        }
    }

    // 2. 어댑터 열거 실패 시 기본 어댑터로 폴백
    let mut device = None;
    let mut context = None;
    let mut level = D3D_FEATURE_LEVEL_11_0;

    D3D11CreateDevice(
        None,
        D3D_DRIVER_TYPE_HARDWARE,
        HMODULE::default(),
        D3D11_CREATE_DEVICE_FLAG(0),
        Some(&[D3D_FEATURE_LEVEL_11_0]),
        windows::Win32::Graphics::Direct3D11::D3D11_SDK_VERSION,
        Some(&mut device),
        Some(&mut level),
        Some(&mut context),
    )
    .map_err(|e| format!("D3D11CreateDevice failed: {e}"))?;

    let device = device.ok_or("D3D11 device not created")?;
    let context = context.ok_or("D3D11 context not created")?;
    let dxgi_device: IDXGIDevice = device
        .cast()
        .map_err(|e| format!("Query IDXGIDevice failed: {e}"))?;
    let adapter = dxgi_device
        .GetAdapter()
        .map_err(|e| format!("GetAdapter failed: {e}"))?;

    Ok((device, context, adapter))
}

impl DxgiCaptureEngine {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let (device, context, adapter) = create_device_and_adapter()?;

            let (duplication, width, height, output_bounds, is_hdr_format, device_name) =
                Self::find_output(&adapter, &device, None)?;

            let active_sdr_white_level = if is_hdr_format {
                let level = Self::resolve_sdr_white_level(None, Some(&device_name));
                super::hdr_pipeline::set_active_sdr_white_level(level);
                level
            } else {
                1.0
            };
            let hdr_lut = std::sync::Arc::new(*super::hdr_pipeline::build_lut_table(
                active_sdr_white_level,
            ));

            Ok(Self {
                device,
                context,
                adapter,
                duplication,
                staging_texture: None,
                staging_atlas_textures: [None, None],
                atlas_write_idx: 0,
                atlas_frames_captured: 0,
                normalizer: None,
                enable_gpu_atlas: false,
                width,
                height,
                output_bounds,
                is_hdr_format,
                device_name,
                configured_sdr_white_level: None,
                active_sdr_white_level,
                hdr_lut,
            })
        }
    }

    fn resolve_sdr_white_level(configured: Option<f32>, device_name: Option<&[u16]>) -> f32 {
        if let Some(cfg) = configured {
            if cfg > 0.0 {
                eprintln!(
                    "[DXGI HDR] Using configured SDR White Level: {:.4} ({:.1} nits)",
                    cfg,
                    cfg * 80.0
                );
                return cfg;
            }
        }
        if let Some(detected) = super::hdr_pipeline::detect_monitor_sdr_white_level(device_name) {
            eprintln!(
                "[DXGI HDR] Auto-detected OS SDR White Level: {:.4} ({:.1} nits)",
                detected,
                detected * 80.0
            );
            return detected;
        }
        eprintln!(
            "[DXGI HDR] SDR White Level fallback to default: {:.4} ({:.1} nits)",
            super::hdr_pipeline::SCRGB_SDR_WHITE_LEVEL,
            super::hdr_pipeline::SCRGB_SDR_WHITE_LEVEL * 80.0
        );
        super::hdr_pipeline::SCRGB_SDR_WHITE_LEVEL
    }

    pub fn set_hdr_sdr_white_level(&mut self, level: Option<f32>) {
        if self.configured_sdr_white_level == level {
            return;
        }
        self.configured_sdr_white_level = level;
        if self.is_hdr_format {
            let new_level = Self::resolve_sdr_white_level(level, Some(&self.device_name));
            self.active_sdr_white_level = new_level;
            self.hdr_lut = std::sync::Arc::new(*super::hdr_pipeline::build_lut_table(new_level));
            super::hdr_pipeline::set_active_sdr_white_level(new_level);
        }
    }

    #[allow(dead_code)]
    pub fn active_sdr_white_level(&self) -> f32 {
        self.active_sdr_white_level
    }

    fn find_output(
        adapter: &IDXGIAdapter,
        device: &ID3D11Device,
        rect_opt: Option<WindowRect>,
    ) -> Result<(IDXGIOutputDuplication, u32, u32, RECT, bool, [u16; 32]), String> {
        unsafe {
            let mut best_output = None;
            let mut first_output = None;

            let mut i = 0;
            while let Ok(output) = adapter.EnumOutputs(i) {
                if let Ok(desc) = output.GetDesc() {
                    let bounds = desc.DesktopCoordinates;
                    if first_output.is_none() {
                        first_output = Some((output.clone(), bounds, desc.DeviceName));
                    }
                    if let Some(rect) = rect_opt {
                        let center_x = rect.left + rect.width / 2;
                        let center_y = rect.top + rect.height / 2;
                        if center_x >= bounds.left
                            && center_x < bounds.right
                            && center_y >= bounds.top
                            && center_y < bounds.bottom
                        {
                            best_output = Some((output, bounds, desc.DeviceName));
                            break;
                        }
                    }
                }
                i += 1;
            }

            let (output, bounds, device_name) =
                best_output.or(first_output).ok_or("No DXGI output found")?;

            // HDR 모니터 여부 감지 (DXGI 1.6 IDXGIOutput6::GetDesc1)
            let is_output_hdr = if let Ok(output6) = output.cast::<IDXGIOutput6>() {
                if let Ok(desc1) = output6.GetDesc1() {
                    let hdr_active = desc1.ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020
                        || desc1.ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709
                        || desc1.BitsPerColor > 8;
                    eprintln!(
                        "[DXGI HDR] Monitor ColorSpace: {:?}, BitsPerColor: {}, is_hdr: {}",
                        desc1.ColorSpace, desc1.BitsPerColor, hdr_active
                    );
                    hdr_active
                } else {
                    false
                }
            } else {
                false
            };

            // HDR/SDR 최적 포맷 협상: IDXGIOutput5::DuplicateOutput1
            // HDR 모니터인 경우 scRGB FP16 우선, SDR 모니터인 경우 네이티브 B8G8R8A8 우선 요청하여
            // 불필요한 DWM 변환 및 색 왜곡을 원천 차단함
            let duplication = if let Ok(output5) = output.cast::<IDXGIOutput5>() {
                let formats_to_try = if is_output_hdr {
                    [DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_B8G8R8A8_UNORM]
                } else {
                    [DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT]
                };

                let mut dup_result = None;
                for (fmt_idx, fmt) in formats_to_try.iter().enumerate() {
                    match output5.DuplicateOutput1(device, 0, &[*fmt]) {
                        Ok(dup) => {
                            eprintln!(
                                "[DXGI HDR] DuplicateOutput1 success with format {:?} (attempt #{})",
                                fmt, fmt_idx
                            );
                            dup_result = Some(dup);
                            break;
                        }
                        Err(e) => {
                            eprintln!("[DXGI HDR] Format {:?} failed: 0x{:X}", fmt, e.code().0);
                        }
                    }
                }

                if let Some(dup) = dup_result {
                    // DuplicateOutput1 성공
                    dup
                } else {
                    // 모든 DuplicateOutput1 시도 실패 → IDXGIOutput1 폴백
                    eprintln!("[DXGI HDR] All DuplicateOutput1 attempts failed, trying DuplicateOutput fallback");
                    let output1: IDXGIOutput1 = output
                        .cast()
                        .map_err(|e| format!("Query IDXGIOutput1 failed: {e}"))?;
                    output1
                        .DuplicateOutput(device)
                        .map_err(|e| format!("DuplicateOutput fallback failed: {e}"))?
                }
            } else {
                // IDXGIOutput5 미지원 → 직접 IDXGIOutput1 사용
                eprintln!("[DXGI HDR] IDXGIOutput5 not available, using IDXGIOutput1");
                let output1: IDXGIOutput1 = output
                    .cast()
                    .map_err(|e| format!("Query IDXGIOutput1 failed: {e}"))?;
                output1
                    .DuplicateOutput(device)
                    .map_err(|e| format!("DuplicateOutput failed: {e}"))?
            };

            let desc = duplication.GetDesc();
            let is_hdr = desc.ModeDesc.Format.0 == DXGI_FORMAT_R16G16B16A16_FLOAT.0; // R16G16B16A16

            Ok((
                duplication,
                desc.ModeDesc.Width,
                desc.ModeDesc.Height,
                bounds,
                is_hdr,
                device_name,
            ))
        }
    }

    fn ensure_output_for_rect(&mut self, rect: WindowRect) -> Result<(), String> {
        let center_x = rect.left + rect.width / 2;
        let center_y = rect.top + rect.height / 2;
        let inside = center_x >= self.output_bounds.left
            && center_x < self.output_bounds.right
            && center_y >= self.output_bounds.top
            && center_y < self.output_bounds.bottom;

        if !inside {
            if let Ok((duplication, width, height, bounds, is_hdr, device_name)) =
                Self::find_output(&self.adapter, &self.device, Some(rect))
            {
                self.duplication = duplication;
                self.width = width;
                self.height = height;
                self.output_bounds = bounds;
                self.is_hdr_format = is_hdr;
                self.staging_texture = None;
                self.staging_atlas_textures = [None, None];
                self.normalizer = None;
                self.atlas_frames_captured = 0;
                self.atlas_write_idx = 0;
                self.device_name = device_name;
                if is_hdr {
                    let level = Self::resolve_sdr_white_level(
                        self.configured_sdr_white_level,
                        Some(&self.device_name),
                    );
                    self.active_sdr_white_level = level;
                    self.hdr_lut =
                        std::sync::Arc::new(*super::hdr_pipeline::build_lut_table(level));
                    super::hdr_pipeline::set_active_sdr_white_level(level);
                } else {
                    self.active_sdr_white_level = 1.0;
                }
            }
        }
        Ok(())
    }

    fn ensure_staging_texture(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.staging_texture.is_none() {
            unsafe {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: if self.is_hdr_format {
                        DXGI_FORMAT_R16G16B16A16_FLOAT
                    } else {
                        DXGI_FORMAT_B8G8R8A8_UNORM
                    },
                    SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                };
                let mut texture = None;
                self.device
                    .CreateTexture2D(&desc, None, Some(&mut texture))
                    .map_err(|e| format!("Create staging texture failed: {e}"))?;
                self.staging_texture = Some(texture.ok_or("Staging texture is None")?);
            }
        }
        Ok(())
    }

    pub fn set_enable_gpu_atlas(&mut self, enable: bool) {
        self.enable_gpu_atlas = enable;
    }

    #[allow(dead_code)]
    pub fn enable_gpu_atlas(&self) -> bool {
        self.enable_gpu_atlas
    }

    fn ensure_staging_atlas_textures(&mut self) -> Result<(), String> {
        unsafe {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: ATLAS_WIDTH,
                Height: ATLAS_HEIGHT,
                MipLevels: 1,
                ArraySize: 1,
                Format: if self.is_hdr_format {
                    DXGI_FORMAT_R16G16B16A16_FLOAT
                } else {
                    DXGI_FORMAT_B8G8R8A8_UNORM
                },
                SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            for slot in &mut self.staging_atlas_textures {
                if slot.is_none() {
                    let mut texture = None;
                    self.device
                        .CreateTexture2D(&desc, None, Some(&mut texture))
                        .map_err(|e| format!("Create staging atlas texture failed: {e}"))?;
                    *slot = Some(texture.ok_or("Staging atlas texture is None")?);
                }
            }
        }
        Ok(())
    }

    fn ensure_normalizer(&mut self) -> Result<(), String> {
        if self.normalizer.is_none() {
            let format = if self.is_hdr_format {
                DXGI_FORMAT_R16G16B16A16_FLOAT
            } else {
                DXGI_FORMAT_B8G8R8A8_UNORM
            };
            self.normalizer = Some(super::normalizer::D3d11Normalizer::new_with_format(
                &self.device,
                format,
            )?);
        }
        Ok(())
    }
}

impl CaptureEngine for DxgiCaptureEngine {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String> {
        let mut frame = CapturedFrame {
            width: 0,
            height: 0,
            bgra: Vec::new(),
        };
        self.capture_bgra_inplace(rect, &mut frame)?;
        Ok(frame)
    }

    fn capture_bgra_inplace(
        &mut self,
        rect: WindowRect,
        out_frame: &mut CapturedFrame,
    ) -> Result<(), String> {
        if !rect.is_valid() {
            return Err("Capture rect must have positive dimensions".to_string());
        }

        let _ = self.ensure_output_for_rect(rect);

        unsafe {
            let use_atlas = self.enable_gpu_atlas;

            if use_atlas {
                self.ensure_staging_atlas_textures()?;
                let is_1080p_exact = rect.width == 1920 && rect.height == 1080;
                if !is_1080p_exact {
                    self.ensure_normalizer()?;
                }

                let mut resource = None;
                let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
                let timeout_ms = if out_frame.bgra.is_empty() { 50 } else { 0 };
                let acquire_res =
                    self.duplication
                        .AcquireNextFrame(timeout_ms, &mut frame_info, &mut resource);

                let write_idx = self.atlas_write_idx;
                let prev_idx = 1 - write_idx;

                match acquire_res {
                    Ok(_) => {
                        if let Some(res) = resource {
                            let texture: ID3D11Texture2D = res
                                .cast()
                                .map_err(|e| format!("Query ID3D11Texture2D failed: {e}"))?;

                            if self.atlas_frames_captured == 0 {
                                let mut tex_desc = D3D11_TEXTURE2D_DESC::default();
                                texture.GetDesc(&mut tex_desc);
                                eprintln!(
                                    "[CAPTURE SOURCE] {}x{} format={:?} misc=0x{:X} bind=0x{:X} usage={:?}",
                                    tex_desc.Width,
                                    tex_desc.Height,
                                    tex_desc.Format,
                                    tex_desc.MiscFlags,
                                    tex_desc.BindFlags,
                                    tex_desc.Usage,
                                );
                            }

                            let staging_write = self.staging_atlas_textures[write_idx]
                                .as_ref()
                                .ok_or("Staging atlas texture missing")?;

                            if is_1080p_exact {
                                // Fast path: 1080p 1:1, zero draw call, direct copy from desktop texture
                                let local_left = rect.left - self.output_bounds.left;
                                let local_top = rect.top - self.output_bounds.top;
                                copy_slots_to_atlas(
                                    &self.context,
                                    &texture,
                                    staging_write,
                                    local_left,
                                    local_top,
                                    self.width,
                                    self.height,
                                );
                            } else {
                                // Normalizer path: GPU bilinear blit to 1920x1080 render target
                                let normalizer = self.normalizer.as_mut().unwrap();
                                let norm_tex = match normalizer.normalize(
                                    &self.context,
                                    &texture,
                                    rect,
                                    self.width,
                                    self.height,
                                    self.output_bounds,
                                ) {
                                    Ok(t) => t,
                                    Err(e) => {
                                        let _ = self.duplication.ReleaseFrame();
                                        return Err(e);
                                    }
                                };
                                copy_slots_to_atlas(
                                    &self.context,
                                    norm_tex,
                                    staging_write,
                                    0,
                                    0,
                                    super::normalizer::NORMALIZED_WIDTH,
                                    super::normalizer::NORMALIZED_HEIGHT,
                                );
                            }

                            // GPU 명령 큐를 즉시 비동기 플러시하여 백그라운드 DMA 전송 시작
                            self.context.Flush();
                            let _ = self.duplication.ReleaseFrame();

                            self.atlas_frames_captured += 1;
                            if self.atlas_frames_captured == 1 {
                                // 최초 1프레임: 아직 이전 버퍼가 없으므로 현재 버퍼 직접 맵핑 (1회 웜업)
                                self.atlas_write_idx = prev_idx;
                                return copy_atlas_to_buffer(
                                    &self.context,
                                    staging_write,
                                    out_frame,
                                    self.is_hdr_format,
                                    self.active_sdr_white_level,
                                );
                            } else {
                                // 2번째 프레임부터: 이미 지난 틱에 GPU 복사가 완료된 이전 버퍼를 맵핑 (0ms Stall!)
                                self.atlas_write_idx = prev_idx;
                                let staging_read =
                                    self.staging_atlas_textures[prev_idx]
                                        .as_ref()
                                        .ok_or("Previous staging atlas texture missing")?;
                                return copy_atlas_to_buffer(
                                    &self.context,
                                    staging_read,
                                    out_frame,
                                    self.is_hdr_format,
                                    self.active_sdr_white_level,
                                );
                            }
                        }
                        let _ = self.duplication.ReleaseFrame();
                    }
                    Err(err) => {
                        let code = err.code().0 as u32;
                        if code == 0x887A0027 {
                            if out_frame.bgra.is_empty() {
                                return Err("DXGI initial frame timeout".to_string());
                            }
                        } else {
                            return Err(format!(
                                "DXGI AcquireNextFrame failed with HRESULT 0x{:X}: {}",
                                code, err
                            ));
                        }
                    }
                }

                // 타임아웃(정적 화면) 시 가장 최근 완성된 버퍼를 맵핑
                let read_idx = if self.atlas_frames_captured == 0 {
                    write_idx
                } else {
                    prev_idx
                };
                let staging_read = self.staging_atlas_textures[read_idx]
                    .as_ref()
                    .ok_or("Staging atlas texture missing")?;
                return copy_atlas_to_buffer(
                    &self.context,
                    staging_read,
                    out_frame,
                    self.is_hdr_format,
                    self.active_sdr_white_level,
                );
            }

            self.ensure_staging_texture(self.width, self.height)?;

            let mut resource = None;
            let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();

            // 초기 프레임 미획득 시 50ms 대기하여 안정적으로 첫 프레임을 획득하고, 이후에는 0ms(논블로킹) 폴링
            let timeout_ms = if out_frame.bgra.is_empty() { 50 } else { 0 };
            let acquire_res =
                self.duplication
                    .AcquireNextFrame(timeout_ms, &mut frame_info, &mut resource);

            let staging = self
                .staging_texture
                .as_ref()
                .ok_or("Staging texture missing")?;

            match acquire_res {
                Ok(_) => {
                    if let Some(res) = resource {
                        let texture: ID3D11Texture2D = res
                            .cast()
                            .map_err(|e| format!("Query ID3D11Texture2D failed: {e}"))?;
                        self.context.CopyResource(staging, &texture);
                        let _ = self.duplication.ReleaseFrame();

                        return crop_texture_to_buffer(
                            &self.context,
                            staging,
                            self.width,
                            self.height,
                            rect,
                            self.output_bounds,
                            out_frame,
                            self.is_hdr_format,
                            self.active_sdr_white_level,
                        );
                    }
                    let _ = self.duplication.ReleaseFrame();
                }
                Err(err) => {
                    let code = err.code().0 as u32;
                    if code == 0x887A0027 {
                        if out_frame.bgra.is_empty() {
                            return Err("DXGI initial frame timeout".to_string());
                        }
                    } else {
                        return Err(format!(
                            "DXGI AcquireNextFrame failed with HRESULT 0x{:X}: {}",
                            code, err
                        ));
                    }
                }
            }

            crop_texture_to_buffer(
                &self.context,
                staging,
                self.width,
                self.height,
                rect,
                self.output_bounds,
                out_frame,
                self.is_hdr_format,
                self.active_sdr_white_level,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn crop_texture_to_buffer(
    context: &ID3D11DeviceContext,
    staging: &ID3D11Texture2D,
    desktop_width: u32,
    desktop_height: u32,
    rect: WindowRect,
    output_bounds: RECT,
    out_frame: &mut CapturedFrame,
    is_hdr: bool,
    white_level: f32,
) -> Result<(), String> {
    let mut mapped = Default::default();
    context
        .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        .map_err(|e| format!("Map texture failed: {e}"))?;

    let row_pitch = mapped.RowPitch as usize;
    let data_ptr = mapped.pData as *const u8;

    let local_left = rect.left - output_bounds.left;
    let local_top = rect.top - output_bounds.top;

    let start_x = local_left.clamp(0, desktop_width as i32) as usize;
    let start_y = local_top.clamp(0, desktop_height as i32) as usize;
    let crop_width = (rect.width as usize).min(desktop_width as usize - start_x);
    let crop_height = (rect.height as usize).min(desktop_height as usize - start_y);

    let len = crop_width * crop_height * 4;
    out_frame.width = crop_width as i32;
    out_frame.height = crop_height as i32;
    out_frame.bgra.resize(len, 0);

    let src_bpp = if is_hdr { 8 } else { 4 };
    for y in 0..crop_height {
        let src_offset = (start_y + y) * row_pitch + start_x * src_bpp;
        let dst_offset = y * crop_width * 4;
        let src_row = data_ptr.add(src_offset);
        let dst_row = out_frame.bgra.as_mut_ptr().add(dst_offset);

        if is_hdr {
            super::hdr_pipeline::convert_scrgb_fp16_to_bgra8_p3(
                src_row,
                dst_row,
                crop_width,
                white_level,
            );
        } else {
            // 기존 B8G8R8A8 복사
            std::ptr::copy_nonoverlapping(src_row, dst_row, crop_width * 4);
        }
    }

    context.Unmap(staging, 0);
    Ok(())
}

unsafe fn copy_slots_to_atlas(
    context: &ID3D11DeviceContext,
    src_texture: &ID3D11Texture2D,
    staging_atlas: &ID3D11Texture2D,
    local_left: i32,
    local_top: i32,
    desktop_width: u32,
    desktop_height: u32,
) {
    for slot in ATLAS_SLOTS.iter() {
        let src_x = local_left + slot.src_rect.x;
        let src_y = local_top + slot.src_rect.y;

        if src_x < 0
            || src_y < 0
            || (src_x as u32 + slot.src_rect.width as u32) > desktop_width
            || (src_y as u32 + slot.src_rect.height as u32) > desktop_height
        {
            continue;
        }

        let box_src = D3D11_BOX {
            left: src_x as u32,
            top: src_y as u32,
            front: 0,
            right: (src_x + slot.src_rect.width) as u32,
            bottom: (src_y + slot.src_rect.height) as u32,
            back: 1,
        };

        context.CopySubresourceRegion(
            staging_atlas,
            0,
            slot.atlas_rect.x as u32,
            slot.atlas_rect.y as u32,
            0,
            src_texture,
            0,
            Some(&box_src),
        );
    }
}

unsafe fn copy_atlas_to_buffer(
    context: &ID3D11DeviceContext,
    staging_atlas: &ID3D11Texture2D,
    out_frame: &mut CapturedFrame,
    is_hdr: bool,
    white_level: f32,
) -> Result<(), String> {
    let mut mapped = Default::default();

    context
        .Map(staging_atlas, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        .map_err(|e| format!("Map atlas texture failed: {e}"))?;

    let row_pitch = mapped.RowPitch as usize;
    let data_ptr = mapped.pData as *const u8;

    let w = ATLAS_WIDTH as usize;
    let h = ATLAS_HEIGHT as usize;

    let row_bytes = w * 4;
    let len = row_bytes * h;

    out_frame.width = ATLAS_WIDTH as i32;
    out_frame.height = ATLAS_HEIGHT as i32;
    out_frame.bgra.resize(len, 0);

    let dst_ptr = out_frame.bgra.as_mut_ptr();

    if is_hdr {
        // R16G16B16A16_FLOAT → BGRA8 (DCI-P3 광색역 역변환 적용)
        for y in 0..h {
            let src_row = data_ptr.add(y * row_pitch);
            let dst_row = dst_ptr.add(y * row_bytes);

            super::hdr_pipeline::convert_scrgb_fp16_to_bgra8_p3(src_row, dst_row, w, white_level);
        }
    } else {
        // SDR: B8G8R8A8_UNORM → BGRA8
        if row_pitch == row_bytes {
            std::ptr::copy_nonoverlapping(data_ptr, dst_ptr, len);
        } else {
            for y in 0..h {
                let src_row = data_ptr.add(y * row_pitch);
                let dst_row = dst_ptr.add(y * row_bytes);

                std::ptr::copy_nonoverlapping(src_row, dst_row, row_bytes);
            }
        }
    }

    context.Unmap(staging_atlas, 0);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Dxgi::IDXGIOutput6;

    #[test]
    fn test_dxgi_output6_check() {
        use windows::Win32::Graphics::Dxgi::Common::{
            DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
            DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
        };
        unsafe {
            if let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() {
                let mut adapter_idx = 0;
                while let Ok(adapter) = factory.EnumAdapters1(adapter_idx) {
                    let mut out_idx = 0;
                    while let Ok(output) = adapter.EnumOutputs(out_idx) {
                        if let Ok(desc) = output.GetDesc() {
                            let dev_name = String::from_utf16_lossy(&desc.DeviceName);
                            println!("Adapter {} Output {}: {}", adapter_idx, out_idx, dev_name);
                            if let Ok(output6) = output.cast::<IDXGIOutput6>() {
                                if let Ok(desc1) = output6.GetDesc1() {
                                    let is_hdr = desc1.ColorSpace
                                        == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020
                                        || desc1.ColorSpace
                                            == DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709
                                        || desc1.BitsPerColor > 8;
                                    let is_sdr =
                                        desc1.ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709;
                                    println!("  is_hdr: {}, is_sdr: {}", is_hdr, is_sdr);
                                    assert!(!is_hdr, "Current monitor should be detected as SDR");
                                    assert!(is_sdr, "Current monitor should be detected as SDR");
                                }
                            }
                        }
                        out_idx += 1;
                    }
                    adapter_idx += 1;
                }
            }
        }
    }
}
