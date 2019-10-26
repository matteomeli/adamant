use crate::graphics::com::ComPtr;
use crate::graphics::command::CommandListType;
use crate::graphics::device::Device;

use winapi::shared::winerror::{FAILED, SUCCEEDED};
use winapi::um::d3d12;
use winapi::Interface;

#[derive(Debug)]
pub enum Error {
    CommandAllocatorCreateFailed,
    CommandAllocatorSetNameFailed,
    CommandAllocatorResetFailed,
}

pub struct CommandAllocator(pub(crate) ComPtr<d3d12::ID3D12CommandAllocator>);

impl CommandAllocator {
    pub fn new(
        device: &Device,
        command_list_type: CommandListType,
        debug_name: &str,
    ) -> Result<Self, Error> {
        let mut command_allocator = ComPtr::<d3d12::ID3D12CommandAllocator>::empty();
        let mut hr = unsafe {
            device.native.CreateCommandAllocator(
                command_list_type as _,
                &d3d12::ID3D12CommandAllocator::uuidof(),
                command_allocator.as_mut_void(),
            )
        };
        if FAILED(hr) {
            return Err(Error::CommandAllocatorCreateFailed);
        }

        #[cfg(debug_assertions)]
        {
            hr = unsafe {
                command_allocator.SetName(debug_name.encode_utf16().collect::<Vec<u16>>().as_ptr())
            };
            if FAILED(hr) {
                return Err(Error::CommandAllocatorSetNameFailed);
            }
        }

        Ok(CommandAllocator(command_allocator))
    }

    pub fn reset(&self) -> Result<(), Error> {
        let hr = unsafe { self.0.Reset() };
        if SUCCEEDED(hr) {
            Ok(())
        } else {
            Err(Error::CommandAllocatorResetFailed)
        }
    }
}
