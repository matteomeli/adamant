#![feature(ptr_internals)]

pub mod game_core;
pub mod game_timer;
pub mod graphics;

use bitflags::bitflags;

use winapi::shared::dxgiformat;
use winapi::um::d3dcommon;

bitflags! {
    pub struct InitFlags: u32 {
        const ALLOW_TEARING = 0b0000_0001;
        const ENABLE_HDR = 0b0000_0010;
    }
}

#[derive(Clone, Debug)]
pub struct InitParams {
    pub window_title: String,
    pub window_width: u32,
    pub window_height: u32,
    pub back_buffer_format: dxgiformat::DXGI_FORMAT,
    pub depth_buffer_format: dxgiformat::DXGI_FORMAT,
    pub back_buffer_count: u32,
    pub min_feature_level: d3dcommon::D3D_FEATURE_LEVEL,
    pub flags: InitFlags,
}

impl InitParams {
    pub fn new(window_title: String, window_width: u32, window_height: u32) -> Self {
        Self {
            window_title,
            window_width,
            window_height,
            back_buffer_format: dxgiformat::DXGI_FORMAT_R8G8B8A8_UNORM,
            depth_buffer_format: dxgiformat::DXGI_FORMAT_D32_FLOAT,
            back_buffer_count: 3,
            min_feature_level: d3dcommon::D3D_FEATURE_LEVEL_11_0,
            flags: InitFlags::empty(),
        }
    }
}
