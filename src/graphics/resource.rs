use crate::graphics::com::ComPtr;

use winapi::shared::winerror::SUCCEEDED;
use winapi::um::d3d12;

use std::ptr;

#[derive(Debug)]
pub enum GpuResourceError {
    GpuResourceMapFailed,
}

pub struct GpuResource {
    pub(crate) native: ComPtr<d3d12::ID3D12Resource>,
    pub(crate) usage_state: d3d12::D3D12_RESOURCE_STATES,
}

impl GpuResource {
    pub fn create(
        resource: ComPtr<d3d12::ID3D12Resource>,
        state: d3d12::D3D12_RESOURCE_STATES,
    ) -> Self {
        GpuResource {
            native: resource,
            usage_state: state,
        }
    }

    pub fn map(&self) -> Result<*mut u8, GpuResourceError> {
        unsafe {
            let mut ptr = ptr::null_mut();
            let hr = self.native.Map(0, ptr::null(), &mut ptr);
            if SUCCEEDED(hr) {
                Ok(ptr as *mut _)
            } else {
                Err(GpuResourceError::GpuResourceMapFailed)
            }
        }
    }

    pub fn unmap(&self) {
        unsafe { self.native.Unmap(0, ptr::null()) }
    }
}
