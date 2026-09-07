use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader, ID3D11RenderTargetView,
    ID3D11SamplerState, ID3D11Texture2D, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER,
    D3D11_BIND_RENDER_TARGET, D3D11_BUFFER_DESC, D3D11_COMPARISON_NEVER,
    D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_FLOAT32_MAX, D3D11_SAMPLER_DESC, D3D11_TEXTURE2D_DESC,
    D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

use super::shader_bytes::{PS_BYTECODE, VS_BYTECODE};
use crate::capture::window_tracker::WindowRect;

pub const NORMALIZED_WIDTH: u32 = 1920;
pub const NORMALIZED_HEIGHT: u32 = 1080;

pub struct D3d11Normalizer {
    device: ID3D11Device,
    render_target: ID3D11Texture2D,
    rtv: ID3D11RenderTargetView,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    cb_uv: ID3D11Buffer,
    viewport: D3D11_VIEWPORT,
}

unsafe impl Send for D3d11Normalizer {}
unsafe impl Sync for D3d11Normalizer {}

/// Calculates normalized UV coordinates [u_min, v_min, u_scale, v_scale]
/// for sampling the active 16:9 game area from the captured desktop texture.
pub fn calculate_uv_rect(
    rect: WindowRect,
    desktop_width: u32,
    desktop_height: u32,
    output_bounds: RECT,
) -> [f32; 4] {
    const REF_WIDTH: f32 = 1920.0;
    const REF_HEIGHT: f32 = 1080.0;
    const REF_ASPECT: f32 = REF_WIDTH / REF_HEIGHT;
    const ASPECT_EPSILON: f32 = 0.005;

    if desktop_width == 0 || desktop_height == 0 || !rect.is_valid() {
        return [0.0, 0.0, 1.0, 1.0];
    }

    let (offset_x, offset_y, scale) = {
        let current_aspect = rect.width as f32 / rect.height as f32;
        if (current_aspect - REF_ASPECT).abs() < ASPECT_EPSILON {
            (0.0, 0.0, rect.width as f32 / REF_WIDTH)
        } else if current_aspect > REF_ASPECT {
            // Ultrawide (e.g. 21:9): pillarboxed left and right
            let s = rect.height as f32 / REF_HEIGHT;
            let ox = (rect.width as f32 - REF_WIDTH * s) / 2.0;
            (ox, 0.0, s)
        } else {
            // Letterboxed (e.g. 16:10): letterboxed top and bottom
            let s = rect.width as f32 / REF_WIDTH;
            let oy = (rect.height as f32 - REF_HEIGHT * s) / 2.0;
            (0.0, oy, s)
        }
    };

    let game_left = (rect.left - output_bounds.left) as f32 + offset_x;
    let game_top = (rect.top - output_bounds.top) as f32 + offset_y;
    let game_width = REF_WIDTH * scale;
    let game_height = REF_HEIGHT * scale;

    let u_min = game_left / desktop_width as f32;
    let v_min = game_top / desktop_height as f32;
    let u_scale = game_width / desktop_width as f32;
    let v_scale = game_height / desktop_height as f32;

    [u_min, v_min, u_scale, v_scale]
}

impl D3d11Normalizer {
    pub fn new(device: &ID3D11Device) -> Result<Self, String> {
        Self::new_with_format(device, DXGI_FORMAT_B8G8R8A8_UNORM)
    }

    pub fn new_with_format(
        device: &ID3D11Device,
        format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    ) -> Result<Self, String> {
        unsafe {
            // 1. Create Normalized 1920x1080 Render Target Texture
            let rt_desc = D3D11_TEXTURE2D_DESC {
                Width: NORMALIZED_WIDTH,
                Height: NORMALIZED_HEIGHT,
                MipLevels: 1,
                ArraySize: 1,
                Format: format,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut render_target = None;
            device
                .CreateTexture2D(&rt_desc, None, Some(&mut render_target))
                .map_err(|e| format!("Create normalizer render target failed: {e}"))?;
            let render_target = render_target.ok_or("Normalizer render target is None")?;

            // 2. Create Render Target View
            let mut rtv = None;
            device
                .CreateRenderTargetView(&render_target, None, Some(&mut rtv))
                .map_err(|e| format!("Create normalizer RTV failed: {e}"))?;
            let rtv = rtv.ok_or("Normalizer RTV is None")?;

            // 3. Create Shaders
            let mut vs = None;
            device
                .CreateVertexShader(VS_BYTECODE, None, Some(&mut vs))
                .map_err(|e| format!("Create normalizer VS failed: {e}"))?;
            let vertex_shader = vs.ok_or("Normalizer VS is None")?;

            let mut ps = None;
            device
                .CreatePixelShader(PS_BYTECODE, None, Some(&mut ps))
                .map_err(|e| format!("Create normalizer PS failed: {e}"))?;
            let pixel_shader = ps.ok_or("Normalizer PS is None")?;

            // 4. Create Bilinear Clamp Sampler
            let sampler_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                MipLODBias: 0.0,
                MaxAnisotropy: 1,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                BorderColor: [0.0; 4],
                MinLOD: 0.0,
                MaxLOD: D3D11_FLOAT32_MAX,
            };
            let mut sampler = None;
            device
                .CreateSamplerState(&sampler_desc, Some(&mut sampler))
                .map_err(|e| format!("Create normalizer sampler failed: {e}"))?;
            let sampler = sampler.ok_or("Normalizer sampler is None")?;

            // 5. Create UV Offset Constant Buffer (16 bytes = float4)
            let cb_desc = D3D11_BUFFER_DESC {
                ByteWidth: std::mem::size_of::<[f32; 4]>() as u32,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
                StructureByteStride: 0,
            };
            let mut cb_uv = None;
            device
                .CreateBuffer(&cb_desc, None, Some(&mut cb_uv))
                .map_err(|e| format!("Create normalizer CB failed: {e}"))?;
            let cb_uv = cb_uv.ok_or("Normalizer CB is None")?;

            let viewport = D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: NORMALIZED_WIDTH as f32,
                Height: NORMALIZED_HEIGHT as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };

            Ok(Self {
                device: device.clone(),
                render_target,
                rtv,
                vertex_shader,
                pixel_shader,
                sampler,
                cb_uv,
                viewport,
            })
        }
    }

    /// Normalizes any resolution/aspect game window to 1920x1080 Bilinear intermediate texture.
    pub fn normalize(
        &mut self,
        context: &ID3D11DeviceContext,
        src_texture: &ID3D11Texture2D,
        rect: WindowRect,
        desktop_width: u32,
        desktop_height: u32,
        output_bounds: RECT,
    ) -> Result<&ID3D11Texture2D, String> {
        unsafe {
            let uv_rect = calculate_uv_rect(rect, desktop_width, desktop_height, output_bounds);

            context.UpdateSubresource(&self.cb_uv, 0, None, uv_rect.as_ptr() as *const _, 0, 0);

            let mut srv = None;
            self.device
                .CreateShaderResourceView(src_texture, None, Some(&mut srv))
                .map_err(|e| format!("Create SRV for src_texture failed: {e}"))?;
            let srv = srv.ok_or("SRV is None")?;

            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.IASetInputLayout(None);
            context.VSSetShader(&self.vertex_shader, None);
            context.VSSetConstantBuffers(0, Some(&[Some(self.cb_uv.clone())]));
            context.PSSetShader(&self.pixel_shader, None);
            context.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            context.PSSetShaderResources(0, Some(&[Some(srv)]));
            context.RSSetViewports(Some(&[self.viewport]));
            context.OMSetRenderTargets(Some(&[Some(self.rtv.clone())]), None);

            context.Draw(3, 0);

            // Unbind to prevent hazard / resource lock
            context.OMSetRenderTargets(None, None);
            context.PSSetShaderResources(0, Some(&[None]));

            Ok(&self.render_target)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uv_rect_1080p_exact() {
        let rect = WindowRect {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        };
        let bounds = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let uv = calculate_uv_rect(rect, 1920, 1080, bounds);
        assert!((uv[0] - 0.0).abs() < 1e-5);
        assert!((uv[1] - 0.0).abs() < 1e-5);
        assert!((uv[2] - 1.0).abs() < 1e-5);
        assert!((uv[3] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_uv_rect_1440p_fullscreen() {
        let rect = WindowRect {
            left: 0,
            top: 0,
            width: 2560,
            height: 1440,
        };
        let bounds = RECT {
            left: 0,
            top: 0,
            right: 2560,
            bottom: 1440,
        };
        let uv = calculate_uv_rect(rect, 2560, 1440, bounds);
        assert!((uv[0] - 0.0).abs() < 1e-5);
        assert!((uv[1] - 0.0).abs() < 1e-5);
        assert!((uv[2] - 1.0).abs() < 1e-5);
        assert!((uv[3] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_uv_rect_21_9_ultrawide_pillarbox() {
        let rect = WindowRect {
            left: 0,
            top: 0,
            width: 2560,
            height: 1080,
        };
        let bounds = RECT {
            left: 0,
            top: 0,
            right: 2560,
            bottom: 1080,
        };
        let uv = calculate_uv_rect(rect, 2560, 1080, bounds);
        // Pillarbox width: (2560 - 1920) / 2 = 320 px.
        // u_min = 320 / 2560 = 0.125
        // u_scale = 1920 / 2560 = 0.75
        assert!((uv[0] - 0.125).abs() < 1e-4);
        assert!((uv[1] - 0.0).abs() < 1e-4);
        assert!((uv[2] - 0.75).abs() < 1e-4);
        assert!((uv[3] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_uv_rect_16_10_letterbox() {
        let rect = WindowRect {
            left: 0,
            top: 0,
            width: 1920,
            height: 1200,
        };
        let bounds = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1200,
        };
        let uv = calculate_uv_rect(rect, 1920, 1200, bounds);
        // Letterbox height: (1200 - 1080) / 2 = 60 px.
        // v_min = 60 / 1200 = 0.05
        // v_scale = 1080 / 1200 = 0.9
        assert!((uv[0] - 0.0).abs() < 1e-4);
        assert!((uv[1] - 0.05).abs() < 1e-4);
        assert!((uv[2] - 1.0).abs() < 1e-4);
        assert!((uv[3] - 0.9).abs() < 1e-4);
    }

    #[test]
    fn test_uv_rect_multi_monitor_offset() {
        // Monitor 2 placed at x=1920, game running on monitor 2
        let rect = WindowRect {
            left: 1920,
            top: 0,
            width: 1920,
            height: 1080,
        };
        let bounds = RECT {
            left: 1920,
            top: 0,
            right: 3840,
            bottom: 1080,
        };
        let uv = calculate_uv_rect(rect, 1920, 1080, bounds);
        assert!((uv[0] - 0.0).abs() < 1e-5);
        assert!((uv[1] - 0.0).abs() < 1e-5);
        assert!((uv[2] - 1.0).abs() < 1e-5);
        assert!((uv[3] - 1.0).abs() < 1e-5);
    }
}
