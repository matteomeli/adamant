use crate::graphics::com::ComPtr;
use crate::graphics::device::Device;
use crate::graphics::resource::GpuResource;

use winapi::shared::{
    dxgiformat, dxgitype,
    winerror::{HRESULT, SUCCEEDED},
};
use winapi::um::d3d12;
use winapi::Interface;

use std::ptr;

pub enum Error {
    CreateFailed(HRESULT),
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to allocate memory.")
    }
}

#[derive(Copy, Clone)]
pub enum AllocationType {
    GpuOnly,
    CpuWritable,
}

pub struct Memory {
    pub(crate) resource: GpuResource,
}

impl Memory {
    pub fn new(device: &Device, alloc_type: AllocationType, size: u64) -> Result<Self, Error> {
        let mut resource = ComPtr::<d3d12::ID3D12Resource>::empty();
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
            Type: d3d12::D3D12_HEAP_TYPE_UPLOAD,
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
                d3d12::D3D12_RESOURCE_STATE_GENERIC_READ,
                ptr::null(),
                &d3d12::ID3D12Resource::uuidof(),
                resource.as_mut_void(),
            );
            if SUCCEEDED(hr) {
                Ok(Memory {
                    resource: GpuResource::create(
                        resource,
                        match alloc_type {
                            AllocationType::GpuOnly => d3d12::D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                            AllocationType::CpuWritable => d3d12::D3D12_RESOURCE_STATE_GENERIC_READ,
                        },
                    ),
                })
            } else {
                Err(Error::CreateFailed(hr))
            }
        }
    }
}

pub struct MemoryAllocator<'a> {
    device: &'a Device,
    allocations: Vec<Memory>,
    allocation_type: AllocationType,
}

impl<'a> MemoryAllocator<'a> {
    pub fn new(device: &'a Device, allocation_type: AllocationType) -> Self {
        MemoryAllocator {
            device,
            allocations: Vec::new(),
            allocation_type,
        }
    }

    // Just 1-2-1 allocation with required memory for a resource now, no fancy memory management
    pub fn allocate(&mut self, size: u64) -> &Memory {
        let allocation = Memory::new(self.device, self.allocation_type, size).unwrap();
        self.allocations.push(allocation);
        self.allocations.last().unwrap()
    }
}
