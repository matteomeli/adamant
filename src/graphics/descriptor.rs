use crate::graphics::com::ComPtr;
use crate::graphics::device::Device;

use winapi::shared::winerror::FAILED;
use winapi::um::d3d12;
use winapi::Interface;

use std::mem;

#[derive(Debug)]
pub enum Error {
    DescriptorHeapCreateFailed,
    DescriptorHeapSetNameFailed,
}

pub type CPUDescriptor = d3d12::D3D12_CPU_DESCRIPTOR_HANDLE;

pub struct DescriptorHeap {
    _native: ComPtr<d3d12::ID3D12DescriptorHeap>,
    pub(crate) descriptor_size: u32,
    next_cpu_handle: CPUDescriptor,
}

impl DescriptorHeap {
    pub fn new(
        device: &Device,
        descriptor_heap_type: d3d12::D3D12_DESCRIPTOR_HEAP_TYPE,
        flags: d3d12::D3D12_DESCRIPTOR_HEAP_FLAGS,
        descriptors_count: u32,
        debug_name: &str,
    ) -> Result<Self, Error> {
        let mut descriptor_heap = ComPtr::<d3d12::ID3D12DescriptorHeap>::empty();
        let desc = d3d12::D3D12_DESCRIPTOR_HEAP_DESC {
            NumDescriptors: descriptors_count,
            Type: descriptor_heap_type,
            Flags: flags,
            ..unsafe { mem::zeroed() }
        };
        let mut hr = unsafe {
            device.native.CreateDescriptorHeap(
                &desc,
                &d3d12::ID3D12DescriptorHeap::uuidof(),
                descriptor_heap.as_mut_void(),
            )
        };
        if FAILED(hr) {
            return Err(Error::DescriptorHeapCreateFailed);
        }

        #[cfg(debug_assertions)]
        {
            hr = unsafe {
                descriptor_heap.SetName(debug_name.encode_utf16().collect::<Vec<u16>>().as_ptr())
            };
            if FAILED(hr) {
                return Err(Error::DescriptorHeapSetNameFailed);
            }
        }

        let next_cpu_handle = unsafe { descriptor_heap.GetCPUDescriptorHandleForHeapStart() };
        let descriptor_size = unsafe {
            device
                .native
                .GetDescriptorHandleIncrementSize(descriptor_heap_type)
        };

        Ok(DescriptorHeap {
            _native: descriptor_heap,
            descriptor_size,
            next_cpu_handle,
        })
    }

    pub fn allocate_cpu(&mut self, count: u32) -> CPUDescriptor {
        let handle = self.next_cpu_handle;
        self.next_cpu_handle = CPUDescriptor {
            ptr: self.next_cpu_handle.ptr + (count * self.descriptor_size) as usize,
        };
        handle
    }
}

const DESCRIPTOR_HEAP_SIZE: u32 = 256;

pub struct DescriptorAllocator {
    device: Device,
    heap_type: d3d12::D3D12_DESCRIPTOR_HEAP_TYPE,
    heaps: Vec<DescriptorHeap>,
    current_heap_id: Option<usize>,
    free_descriptors_count: u32,
}

impl DescriptorAllocator {
    pub fn new(device: Device, heap_type: d3d12::D3D12_DESCRIPTOR_HEAP_TYPE) -> Self {
        DescriptorAllocator {
            device,
            heap_type,
            heaps: Vec::new(),
            current_heap_id: None,
            free_descriptors_count: DESCRIPTOR_HEAP_SIZE,
        }
    }

    pub fn allocate_many(&mut self, count: u32) -> CPUDescriptor {
        let heap_id = if self.current_heap_id.is_none() || count > self.free_descriptors_count {
            // Allocate a new heap here
            let id = self.heaps.len();
            self.heaps.push(
                DescriptorHeap::new(
                    &self.device,
                    self.heap_type,
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

    pub fn allocate(&mut self) -> CPUDescriptor {
        self.allocate_many(1)
    }
}
