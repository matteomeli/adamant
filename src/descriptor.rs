use crate::com::ComPtr;
use crate::device::Device;

use winapi::shared::winerror::FAILED;
use winapi::um::d3d12;
use winapi::Interface;

use std::{mem, ptr};

#[derive(Debug)]
pub enum Error {
    DescriptorHeapCreateFailed,
    DescriptorHeapSetNameFailed,
}

pub type CpuDescriptor = d3d12::D3D12_CPU_DESCRIPTOR_HANDLE;

pub struct DescriptorHeap {
    _native: ComPtr<d3d12::ID3D12DescriptorHeap>,
    pub(crate) descriptor_size: u32,
    next_descriptor: CpuDescriptor,
}

impl DescriptorHeap {
    pub fn new(
        device: &Device,
        type_: d3d12::D3D12_DESCRIPTOR_HEAP_TYPE,
        flags: d3d12::D3D12_DESCRIPTOR_HEAP_FLAGS,
        descriptors_count: u32,
        debug_name: &str,
    ) -> Result<Self, Error> {
        let mut descriptor_heap: *mut d3d12::ID3D12DescriptorHeap = ptr::null_mut();
        let desc = d3d12::D3D12_DESCRIPTOR_HEAP_DESC {
            NumDescriptors: descriptors_count,
            Type: type_,
            Flags: flags,
            ..unsafe { mem::zeroed() }
        };
        let mut hr = unsafe {
            device.native.CreateDescriptorHeap(
                &desc,
                &d3d12::ID3D12DescriptorHeap::uuidof(),
                &mut descriptor_heap as *mut *mut _ as *mut *mut _,
            )
        };
        if FAILED(hr) {
            return Err(Error::DescriptorHeapCreateFailed);
        }

        /*#[cfg(debug_assertions)]
        {
            hr = unsafe {
                descriptor_heap.SetName(debug_name.encode_utf16().collect::<Vec<u16>>().as_ptr())
            };
            if FAILED(hr) {
                return Err(Error::DescriptorHeapSetNameFailed);
            }
        }*/

        let next_descriptor = unsafe { (*descriptor_heap).GetCPUDescriptorHandleForHeapStart() };
        let descriptor_size = unsafe { device.native.GetDescriptorHandleIncrementSize(type_) };

        Ok(DescriptorHeap {
            _native: unsafe { ComPtr::from_ptr(descriptor_heap) },
            descriptor_size,
            next_descriptor,
        })
    }

    pub fn allocate_cpu(&mut self, count: u32) -> CpuDescriptor {
        let handle = self.next_descriptor;
        self.next_descriptor = CpuDescriptor {
            ptr: self.next_descriptor.ptr + (count * self.descriptor_size) as usize,
        };
        handle
    }
}

const DESCRIPTOR_HEAP_SIZE: u32 = 256;

pub struct CpuDescriptorPool {
    device: Device,
    type_: d3d12::D3D12_DESCRIPTOR_HEAP_TYPE,
    heaps: Vec<DescriptorHeap>,
    current_heap_id: Option<usize>,
    free_descriptors_count: u32,
}

impl CpuDescriptorPool {
    pub fn new(device: &Device, type_: d3d12::D3D12_DESCRIPTOR_HEAP_TYPE) -> Self {
        CpuDescriptorPool {
            device: device.clone(),
            type_,
            heaps: Vec::new(),
            current_heap_id: None,
            free_descriptors_count: DESCRIPTOR_HEAP_SIZE,
        }
    }

    pub fn allocate_many(&mut self, count: u32) -> CpuDescriptor {
        let heap_id = if self.current_heap_id.is_none() || count > self.free_descriptors_count {
            // Allocate a new heap here
            let id = self.heaps.len();
            self.heaps.push(
                DescriptorHeap::new(
                    &self.device,
                    self.type_,
                    d3d12::D3D12_DESCRIPTOR_HEAP_FLAG_NONE, /* no need to be shader visible */
                    DESCRIPTOR_HEAP_SIZE,
                    &format!("Adamant::DescriptorHeap{}", id),
                )
                .unwrap(),
            );
            self.current_heap_id = Some(id);
            self.free_descriptors_count = DESCRIPTOR_HEAP_SIZE;
            id
        } else {
            self.current_heap_id.unwrap()
        };

        let heap = &mut self.heaps[heap_id];
        let descriptor = heap.allocate_cpu(count);
        self.free_descriptors_count -= count;

        descriptor
    }

    pub fn allocate(&mut self) -> CpuDescriptor {
        self.allocate_many(1)
    }
}
