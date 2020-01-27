use winapi::shared::dxgiformat;
use winapi::um::d3dcommon;

mod buffer;
mod com;
mod command;
mod context;
mod descriptor;
mod device;
mod dxgi;
mod memory;
mod pso;
mod resource;
mod root_signature;
mod sync;
mod timer;

pub use self::context::Context;
pub use self::timer::GameTimer;

use bitflags::bitflags;

bitflags! {
    pub struct ContextFlags: u32 {
        const ALLOW_TEARING = 0b0000_0001;
        const ENABLE_HDR = 0b0000_0010;
    }
}

#[derive(Clone, Debug)]
pub struct ContextParams {
    pub window_title: String,
    pub window_width: u32,
    pub window_height: u32,
    pub back_buffer_format: dxgiformat::DXGI_FORMAT,
    pub depth_buffer_format: dxgiformat::DXGI_FORMAT,
    pub back_buffer_count: u32,
    pub min_feature_level: d3dcommon::D3D_FEATURE_LEVEL,
    pub flags: ContextFlags,
}

impl ContextParams {
    pub fn new(
        window_title: String,
        window_width: u32,
        window_height: u32,
        flags: ContextFlags,
    ) -> Self {
        Self {
            window_title,
            window_width,
            window_height,
            back_buffer_format: dxgiformat::DXGI_FORMAT_R8G8B8A8_UNORM,
            depth_buffer_format: dxgiformat::DXGI_FORMAT_D32_FLOAT,
            back_buffer_count: 3,
            min_feature_level: d3dcommon::D3D_FEATURE_LEVEL_11_0,
            flags,
        }
    }
}

pub struct Blob(com::ComPtr<d3dcommon::ID3DBlob>);
