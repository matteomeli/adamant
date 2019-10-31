use crate::com::ComPtr;
use crate::device::Device;
use crate::resource::GpuResource;

use winapi::shared::{dxgiformat, dxgitype, winerror::SUCCEEDED};
use winapi::um::d3d12;
use winapi::Interface;

use std::ptr;

pub struct BufferCopyRegion {
    pub source_offset: u64,
    pub dest_offset: u64,
    pub size: u64,
}

#[derive(Debug)]
pub enum Error {
    GpuBufferCreateFailed,
}

pub struct GpuBuffer {
    resource: GpuResource,
    size: u64,
}

impl GpuBuffer {
    pub fn create(device: Device, size: u64) -> Result<Self, Error> {
        let mut resource: *mut d3d12::ID3D12Resource = ptr::null_mut();
        let resource_desc = d3d12::D3D12_RESOURCE_DESC {
            Alignment: 0,
            DepthOrArraySize: 1,
            Dimension: d3d12::D3D12_RESOURCE_DIMENSION_BUFFER,
            Flags: d3d12::D3D12_RESOURCE_FLAG_NONE,
            Format: dxgiformat::DXGI_FORMAT_UNKNOWN,
            Height: 1,
            Layout: d3d12::D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            MipLevels: 1,
            SampleDesc: dxgitype::DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Width: size,
        };
        let heap_properties = d3d12::D3D12_HEAP_PROPERTIES {
            Type: d3d12::D3D12_HEAP_TYPE_DEFAULT,
            CPUPageProperty: d3d12::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: d3d12::D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };
        unsafe {
            let hr = device.native.CreateCommittedResource(
                &heap_properties,
                d3d12::D3D12_HEAP_FLAG_NONE,
                &resource_desc,
                d3d12::D3D12_RESOURCE_STATE_COMMON,
                ptr::null(),
                &d3d12::ID3D12Resource::uuidof(),
                &mut resource as *mut *mut _ as *mut *mut _,
            );
            if SUCCEEDED(hr) {
                Ok(GpuBuffer {
                    resource: GpuResource::create(
                        unsafe { ComPtr::from_ptr(resource) },
                        d3d12::D3D12_RESOURCE_STATE_COMMON,
                    ),
                    size,
                })
            } else {
                Err(Error::GpuBufferCreateFailed)
            }
        }
    }
}
