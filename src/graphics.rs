use winapi::um::d3dcommon;

mod buffer;
mod com;
mod command;
mod descriptor;
mod device;
mod dxgi;
mod memory;
mod resource;
mod sync;

pub mod context;

/*
pub mod pso;
pub mod root_signature;
*/

pub type Blob = self::com::ComPtr<d3dcommon::ID3DBlob>;
