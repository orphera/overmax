use crate::capture_engine::CaptureEngine;
use crate::window_tracker::WindowRect;
use crate::screen_capture::CapturedFrame;

use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    D3D11_CPU_ACCESS_READ, D3D11_MAP_READ,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11_CREATE_DEVICE_FLAG
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIDevice, IDXGIOutput1, IDXGIOutputDuplication, DXGI_OUTDUPL_FRAME_INFO
};

pub struct DxgiCaptureEngine {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging_texture: Option<ID3D11Texture2D>,
    width: u32,
    height: u32,
}

unsafe impl Send for DxgiCaptureEngine {}
unsafe impl Sync for DxgiCaptureEngine {}

impl DxgiCaptureEngine {
    pub fn new() -> Result<Self, String> {
        unsafe {
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

            let dxgi_device: IDXGIDevice = device.cast().map_err(|e| format!("Query IDXGIDevice failed: {e}"))?;
            let adapter = dxgi_device.GetAdapter().map_err(|e| format!("GetAdapter failed: {e}"))?;
            let output = adapter.EnumOutputs(0).map_err(|e| format!("EnumOutputs failed: {e}"))?;
            let output1: IDXGIOutput1 = output.cast().map_err(|e| format!("Query IDXGIOutput1 failed: {e}"))?;

            let duplication = output1.DuplicateOutput(&device).map_err(|e| format!("DuplicateOutput failed: {e}"))?;
            let desc = duplication.GetDesc();

            Ok(Self {
                device,
                context,
                duplication,
                staging_texture: None,
                width: desc.ModeDesc.Width,
                height: desc.ModeDesc.Height,
            })
        }
    }

    fn ensure_staging_texture(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.staging_texture.is_none() {
            unsafe {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
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
}

impl CaptureEngine for DxgiCaptureEngine {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String> {
        if !rect.is_valid() {
            return Err("Capture rect must have positive dimensions".to_string());
        }

        unsafe {
            self.ensure_staging_texture(self.width, self.height)?;

            let mut resource = None;
            let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();

            // 50ms 대기하여 프레임 획득 시도
            let _ = self.duplication.AcquireNextFrame(50, &mut frame_info, &mut resource);

            let staging = self.staging_texture.as_ref().ok_or("Staging texture missing")?;

            if let Some(res) = resource {
                let texture: ID3D11Texture2D = res.cast().map_err(|e| format!("Query ID3D11Texture2D failed: {e}"))?;
                self.context.CopyResource(staging, &texture);
                let _ = self.duplication.ReleaseFrame();
            }

            crop_texture_to_frame(&self.context, staging, self.width, self.height, rect)
        }
    }
}

unsafe fn crop_texture_to_frame(
    context: &ID3D11DeviceContext,
    staging: &ID3D11Texture2D,
    desktop_width: u32,
    desktop_height: u32,
    rect: WindowRect,
) -> Result<CapturedFrame, String> {
    let mut mapped = Default::default();
    context
        .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        .map_err(|e| format!("Map texture failed: {e}"))?;

    let row_pitch = mapped.RowPitch as usize;
    let data_ptr = mapped.pData as *const u8;

    let start_x = rect.left.clamp(0, desktop_width as i32) as usize;
    let start_y = rect.top.clamp(0, desktop_height as i32) as usize;
    let crop_width = (rect.width as usize).min(desktop_width as usize - start_x);
    let crop_height = (rect.height as usize).min(desktop_height as usize - start_y);

    let mut bgra = vec![0u8; crop_width * crop_height * 4];

    for y in 0..crop_height {
        let src_offset = (start_y + y) * row_pitch + start_x * 4;
        let dst_offset = y * crop_width * 4;
        let src_row = data_ptr.add(src_offset);
        let dst_row = bgra.as_mut_ptr().add(dst_offset);
        std::ptr::copy_nonoverlapping(src_row, dst_row, crop_width * 4);
    }

    context.Unmap(staging, 0);

    Ok(CapturedFrame {
        width: crop_width as i32,
        height: crop_height as i32,
        bgra,
    })
}
