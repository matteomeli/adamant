use crate::graphics::buffer::BufferCopyRegion;
use crate::graphics::com::ComPtr;
use crate::graphics::command::CommandAllocator;
use crate::graphics::device::Device;
use crate::graphics::resource::GpuResource;

use winapi::shared::winerror::{FAILED, SUCCEEDED};
use winapi::um::d3d12;
use winapi::Interface;

use std::ptr;

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum CommandListType {
    Direct = d3d12::D3D12_COMMAND_LIST_TYPE_DIRECT,
    Compute = d3d12::D3D12_COMMAND_LIST_TYPE_COMPUTE,
    Copy = d3d12::D3D12_COMMAND_LIST_TYPE_COPY,
}

#[derive(Debug)]
pub enum Error {
    CommandListCreateFailed,
    CommandListSetNameFailed,
    CommandListResetFailed,
    CommandListCloseFailed,
}

pub struct CommandList(pub(crate) ComPtr<d3d12::ID3D12CommandList>);

pub struct GraphicsCommandList(pub(crate) ComPtr<d3d12::ID3D12GraphicsCommandList>);

impl GraphicsCommandList {
    pub fn new(
        device: &Device,
        command_allocator: &CommandAllocator,
        command_list_type: CommandListType,
        debug_name: &str,
    ) -> Result<Self, Error> {
        let mut command_list = ComPtr::<d3d12::ID3D12GraphicsCommandList>::empty();
        let mut hr = unsafe {
            device.native.CreateCommandList(
                0,
                command_list_type as _,
                command_allocator.0.as_ptr_mut(),
                ptr::null_mut(),
                &d3d12::ID3D12GraphicsCommandList::uuidof(),
                command_list.as_mut_void(),
            )
        };
        if FAILED(hr) {
            return Err(Error::CommandListCreateFailed);
        }

        #[cfg(debug_assertions)]
        {
            hr = unsafe {
                command_list.SetName(debug_name.encode_utf16().collect::<Vec<u16>>().as_ptr())
            };
            if FAILED(hr) {
                return Err(Error::CommandListSetNameFailed);
            }
        }

        Ok(GraphicsCommandList(command_list))
    }

    pub fn copy_buffer(
        &self,
        dest: &GpuResource,
        source: &GpuResource,
        regions: &[BufferCopyRegion],
    ) {
        unsafe {
            for region in regions {
                self.0.CopyBufferRegion(
                    dest.native.as_ptr_mut(),
                    region.dest_offset,
                    source.native.as_ptr_mut(),
                    region.source_offset,
                    region.size,
                );
            }
        }
    }

    pub fn insert_resource_barriers(&self, barriers: &[d3d12::D3D12_RESOURCE_BARRIER]) {
        unsafe {
            self.0
                .ResourceBarrier(barriers.len() as _, barriers.as_ptr());
        }
    }

    pub fn reset(&self, command_allocator: &CommandAllocator) -> Result<(), Error> {
        let hr = unsafe {
            self.0
                .Reset(command_allocator.0.as_ptr_mut(), ptr::null_mut())
        };
        if SUCCEEDED(hr) {
            Ok(())
        } else {
            Err(Error::CommandListResetFailed)
        }
    }

    pub fn close(&self) -> Result<(), Error> {
        let hr = unsafe { self.0.Close() };
        if SUCCEEDED(hr) {
            Ok(())
        } else {
            Err(Error::CommandListCloseFailed)
        }
    }

    pub fn as_command_list(&self) -> CommandList {
        CommandList(self.0.clone().up::<d3d12::ID3D12CommandList>())
    }
}
