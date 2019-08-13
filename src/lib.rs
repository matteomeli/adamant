pub mod com;
pub mod d3d12;
pub mod game_loop;

use com::ComPtr;

use bitflags::bitflags;

use winapi::shared::{dxgiformat, windef::HWND};
use winapi::um::d3dcommon;

bitflags! {
    pub struct InitFlags: u32 {
        const ALLOW_TEARING = 0b0000_0001;
        const ENABLE_HDR = 0b0000_0010;
    }
}

#[derive(Copy, Clone, Debug)]
pub struct InitParams {
    pub window_handle: HWND,
    pub window_width: u32,
    pub window_height: u32,
    pub back_buffer_format: dxgiformat::DXGI_FORMAT,
    pub depth_buffer_format: dxgiformat::DXGI_FORMAT,
    pub back_buffer_count: u32,
    pub min_feature_level: d3dcommon::D3D_FEATURE_LEVEL,
    pub flags: InitFlags,
}

impl InitParams {
    pub fn new(window_handle: HWND, window_width: u32, window_height: u32) -> Self {
        Self {
            window_handle,
            window_width,
            window_height,
            back_buffer_format: dxgiformat::DXGI_FORMAT_B8G8R8A8_UNORM,
            depth_buffer_format: dxgiformat::DXGI_FORMAT_D32_FLOAT,
            back_buffer_count: 2,
            min_feature_level: d3dcommon::D3D_FEATURE_LEVEL_11_0,
            flags: InitFlags::empty(),
        }
    }
}
