use crate::com::ComPtr;
use crate::command::{
    CommandAllocator, CommandAllocatorPool, CommandList, CommandListType, GraphicsCommandList,
};
use crate::device::Device;
use crate::sync::{Event, Fence};

use winapi::shared::winerror::{FAILED, SUCCEEDED};
use winapi::um::d3d12;
use winapi::Interface;

use std::cell::RefCell;
use std::ptr;

#[derive(Debug)]
pub enum Error {
    CommandQueueCreateFailed,
    CommandQueueSetNameFailed,
    CommandQueueSignalFailed,
    CommandQueueWaitFailed,
}

pub struct CommandQueue {
    device: Device,
    pub(crate) native: ComPtr<d3d12::ID3D12CommandQueue>,
    command_allocator_pool: RefCell<CommandAllocatorPool>,
    fence: Fence,
    pub(crate) fence_value: u64,
}

impl CommandQueue {
    pub fn new(
        device: &Device,
        type_: CommandListType,
        flags: d3d12::D3D12_COMMAND_QUEUE_FLAGS,
        debug_name: &str,
    ) -> Result<Self, Error> {
        let mut queue: *mut d3d12::ID3D12CommandQueue = ptr::null_mut();
        let desc = d3d12::D3D12_COMMAND_QUEUE_DESC {
            Type: type_ as _,
            Priority: d3d12::D3D12_COMMAND_QUEUE_PRIORITY_NORMAL as _,
            Flags: flags,
            NodeMask: 0,
        };
        let mut hr = unsafe {
            device.native.CreateCommandQueue(
                &desc,
                &d3d12::ID3D12CommandQueue::uuidof(),
                &mut queue as *mut *mut _ as *mut *mut _,
            )
        };
        if FAILED(hr) {
            return Err(Error::CommandQueueCreateFailed);
        }

        /*#[cfg(debug_assertions)]
        {
            hr = unsafe { queue.SetName(debug_name.encode_utf16().collect::<Vec<u16>>().as_ptr()) };
            if FAILED(hr) {
                return Err(Error::CommandQueueSetNameFailed);
            }
        }*/

        Ok(CommandQueue {
            device: device.clone(),
            native: unsafe { ComPtr::from_ptr(queue) },
            command_allocator_pool: RefCell::new(CommandAllocatorPool::new(device.clone(), type_)),
            fence: Fence::new(device).unwrap(),
            fence_value: 0,
        })
    }

    pub fn request_allocator(&self) -> CommandAllocator {
        self.command_allocator_pool
            .borrow_mut()
            .request(self.fence.get_value())
            .clone()
    }

    pub fn free_allocator(&self, command_allocator: CommandAllocator) {
        self.command_allocator_pool
            .borrow_mut()
            .free(self.fence.get_value(), command_allocator);
    }

    pub fn create_command_list(&mut self) -> (GraphicsCommandList, CommandAllocator) {
        let command_allocator = self.request_allocator();
        let device = &self.device;
        (
            GraphicsCommandList::new(
                device,
                &command_allocator,
                CommandListType::Direct,
                "Adamant::CommandList",
            )
            .unwrap(),
            command_allocator,
        )
    }

    pub fn execute_command_list(&self, command_list: CommandList) {
        self.execute_command_lists(&[command_list]);
    }

    pub fn execute_command_lists(&self, command_lists: &[CommandList]) {
        let lists: Vec<*mut d3d12::ID3D12CommandList> = command_lists
            .iter()
            .map(|command_list| command_list.0.as_ptr())
            .collect();
        unsafe {
            self.native
                .ExecuteCommandLists(lists.len() as _, lists.as_ptr())
        };
    }

    pub fn signal_fence(&mut self) -> Result<(), Error> {
        self.fence_value += 1;
        let hr = unsafe { self.native.Signal(self.fence.0.as_ptr(), self.fence_value) };
        if SUCCEEDED(hr) {
            Ok(())
        } else {
            Err(Error::CommandQueueSignalFailed)
        }
    }

    pub fn wait_for_fence(&self) -> Result<(), Error> {
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

    pub fn flush(&mut self) -> Result<(), Error> {
        self.signal_fence().unwrap();
        self.wait_for_fence()
    }

    pub fn is_fence_complete(&self, fence_value: u64) -> bool {
        fence_value <= self.fence.get_value()
    }
}
