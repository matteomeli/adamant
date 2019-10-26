use crate::graphics::com::ComPtr;
use crate::graphics::command::{CommandList, CommandListType};
use crate::graphics::device::Device;
use crate::graphics::sync::{Event, Fence};

use winapi::shared::winerror::{FAILED, SUCCEEDED};
use winapi::um::d3d12;
use winapi::Interface;

#[derive(Debug)]
pub enum Error {
    CommandQueueCreateFailed,
    CommandQueueSetNameFailed,
    CommandQueueSignalFailed,
    CommandQueueWaitFailed,
}

pub struct CommandQueue {
    pub(crate) native: ComPtr<d3d12::ID3D12CommandQueue>,
    fence: Fence,
    fence_value: u64,
}

impl CommandQueue {
    pub fn new(
        device: &Device,
        command_list_type: CommandListType,
        flags: d3d12::D3D12_COMMAND_QUEUE_FLAGS,
        debug_name: &str,
    ) -> Result<Self, Error> {
        let mut queue = ComPtr::<d3d12::ID3D12CommandQueue>::empty();
        let desc = d3d12::D3D12_COMMAND_QUEUE_DESC {
            Type: command_list_type as _,
            Priority: d3d12::D3D12_COMMAND_QUEUE_PRIORITY_NORMAL as _,
            Flags: flags,
            NodeMask: 0,
        };
        let mut hr = unsafe {
            device.native.CreateCommandQueue(
                &desc,
                &d3d12::ID3D12CommandQueue::uuidof(),
                queue.as_mut_void(),
            )
        };
        if FAILED(hr) {
            return Err(Error::CommandQueueCreateFailed);
        }

        #[cfg(debug_assertions)]
        {
            hr = unsafe { queue.SetName(debug_name.encode_utf16().collect::<Vec<u16>>().as_ptr()) };
            if FAILED(hr) {
                return Err(Error::CommandQueueSetNameFailed);
            }
        }

        Ok(CommandQueue {
            native: queue,
            fence: Fence::new(device).unwrap(),
            fence_value: 0,
        })
    }

    pub fn execute_command_list(&self, command_list: CommandList) {
        self.execute_command_lists(&[command_list]);
    }

    pub fn execute_command_lists(&self, command_lists: &[CommandList]) {
        let lists: Vec<*mut d3d12::ID3D12CommandList> = command_lists
            .iter()
            .map(|command_list| command_list.0.as_ptr_mut())
            .collect();
        unsafe {
            self.native
                .ExecuteCommandLists(lists.len() as _, lists.as_ptr())
        };
    }

    pub fn signal_fence(&mut self) -> Result<(), Error> {
        self.fence_value += 1;
        let hr = unsafe {
            self.native
                .Signal(self.fence.0.as_ptr_mut(), self.fence_value)
        };
        if SUCCEEDED(hr) {
            Ok(())
        } else {
            Err(Error::CommandQueueSignalFailed)
        }
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        self.fence_value += 1;
        let hr = unsafe {
            self.native
                .Signal(self.fence.0.as_ptr_mut(), self.fence_value)
        };
        if FAILED(hr) {
            return Err(Error::CommandQueueSignalFailed);
        }

        if self.fence.get_value() < self.fence_value {
            let event = Event::new();
            self.fence
                .wait(event, self.fence_value)
                .and_then(|_| {
                    event.close();
                    Ok(())
                })
                .map_err(|_| Error::CommandQueueWaitFailed)
        } else {
            Ok(())
        }
    }
}
