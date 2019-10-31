use crate::com::ComPtr;
use crate::device::Device;
use crate::resource::GpuResource;

use winapi::shared::{dxgiformat, dxgitype, winerror::SUCCEEDED};
use winapi::um::d3d12;
use winapi::Interface;

use std::ptr;

#[derive(Debug)]
pub enum Error {
    MemoryCreateFailed,
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
    pub fn new(device: &Device, type_: AllocationType, size: u64) -> Result<Self, Error> {
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
                        match type_ {
                            AllocationType::GpuOnly => d3d12::D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                            AllocationType::CpuWritable => d3d12::D3D12_RESOURCE_STATE_GENERIC_READ,
                        },
                    ),
                })
            } else {
                Err(Error::MemoryCreateFailed)
            }
        }
    }
}

pub struct MemoryAllocator {
    device: Device,
    allocations: Vec<Memory>,
    type_: AllocationType,
}

impl MemoryAllocator {
    pub fn new(device: Device, type_: AllocationType) -> Self {
        MemoryAllocator {
            device,
            allocations: Vec::new(),
            type_,
        }
    }

    // Just 1-2-1 allocation with required memory for a resource now, no fancy memory management
    pub fn allocate(&mut self, size: u64) -> &Memory {
        let allocation = Memory::new(&self.device, self.type_, size).unwrap();
        self.allocations.push(allocation);
        self.allocations.last().unwrap()
    }
}
