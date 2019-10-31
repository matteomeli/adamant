use crate::com::ComPtr;
use crate::command::CommandListType;
use crate::device::Device;

use winapi::shared::winerror::{FAILED, SUCCEEDED};
use winapi::um::d3d12;
use winapi::Interface;

#[derive(Debug)]
pub enum Error {
    CommandAllocatorCreateFailed,
    CommandAllocatorSetNameFailed,
    CommandAllocatorResetFailed,
}

pub struct CommandAllocatorPool {
    device: Device,
    type_: CommandListType,
    pool: Vec<CommandAllocator>,
    free_list: Vec<(u64, usize)>,
}

impl CommandAllocatorPool {
    pub fn new(device: Device, type_: CommandListType) -> Self {
        CommandAllocatorPool {
            device,
            type_,
            pool: Vec::new(),
            free_list: Vec::new(),
        }
    }

    pub fn request(&mut self, completed_fence_value: u64) -> &CommandAllocator {
        match self.free_list.last() {
            Some((fence_value, command_allocator_id)) if *fence_value <= completed_fence_value => {
                &self.pool[*command_allocator_id]
            }
            _ => {
                let id = self.pool.len();
                let command_allocator =
                    CommandAllocator::new(&self.device, self.type_, id).unwrap();
                self.pool.push(command_allocator);
                self.pool.last().unwrap()
            }
        }
    }

    pub fn free(&mut self, fence_value: u64, command_allocator: CommandAllocator) {
        self.free_list.push((fence_value, command_allocator.id))
    }
}

#[derive(Clone)]
pub struct CommandAllocator {
    pub(crate) native: ComPtr<d3d12::ID3D12CommandAllocator>,
    pub(crate) id: usize,
}

impl CommandAllocator {
    pub fn new(device: &Device, type_: CommandListType, id: usize) -> Result<Self, Error> {
        let mut command_allocator = ComPtr::<d3d12::ID3D12CommandAllocator>::empty();
        let mut hr = unsafe {
            device.native.CreateCommandAllocator(
                type_ as _,
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
                command_allocator.SetName(
                    format!("Adamant::CommandAllocator_{}", id)
                        .encode_utf16()
                        .collect::<Vec<u16>>()
                        .as_ptr(),
                )
            };
            if FAILED(hr) {
                return Err(Error::CommandAllocatorSetNameFailed);
            }
        }

        Ok(CommandAllocator {
            native: command_allocator,
            id,
        })
    }

    pub fn reset(&self) -> Result<(), Error> {
        let hr = unsafe { self.native.Reset() };
        if SUCCEEDED(hr) {
            Ok(())
        } else {
            Err(Error::CommandAllocatorResetFailed)
        }
    }
}
