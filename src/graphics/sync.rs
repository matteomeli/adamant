use crate::graphics::com::ComPtr;
use crate::graphics::device::Device;

use winapi::shared::winerror::{self, SUCCEEDED};
use winapi::um::{d3d12, handleapi, synchapi, winbase, winnt};
use winapi::Interface;

use std::ptr;

#[repr(transparent)]
#[derive(Debug, Copy, Clone)]
pub struct Event {
    pub(crate) handle: winnt::HANDLE,
}

impl Event {
    pub fn new() -> Self {
        Event {
            handle: unsafe {
                synchapi::CreateEventExW(
                    ptr::null_mut(),
                    ptr::null(),
                    0,
                    winnt::EVENT_MODIFY_STATE | winnt::SYNCHRONIZE,
                )
            },
        }
    }

    pub fn wait(self, timeout_ms: u32) -> bool {
        let hr = unsafe { synchapi::WaitForSingleObject(self.handle, timeout_ms) };
        match hr {
            winbase::WAIT_OBJECT_0 => true,
            winbase::WAIT_ABANDONED => true,
            winerror::WAIT_TIMEOUT => false,
            _ => panic!("Unexpected event wait result"),
        }
    }

    pub fn close(self) {
        unsafe { handleapi::CloseHandle(self.handle) };
    }
}

#[derive(Debug)]
pub enum Error {
    FenceCreateFailed,
    FenceSignalFailed,
    FenceSetCompletionEventFailed,
}

pub struct Fence(pub(crate) ComPtr<d3d12::ID3D12Fence>);

impl Fence {
    pub fn new(device: &Device) -> Result<Self, Error> {
        Fence::new_with_value(device, 0)
    }

    pub fn new_with_value(device: &Device, fence_value: u64) -> Result<Self, Error> {
        let mut fence = ComPtr::<d3d12::ID3D12Fence>::empty();
        let hr = unsafe {
            device.native.CreateFence(
                fence_value,
                d3d12::D3D12_FENCE_FLAG_NONE,
                &d3d12::ID3D12Fence::uuidof(),
                fence.as_mut_void(),
            )
        };
        if SUCCEEDED(hr) {
            Ok(Fence(fence))
        } else {
            Err(Error::FenceCreateFailed)
        }
    }

    pub fn signal(&self, value: u64) -> Result<(), Error> {
        let hr = unsafe { self.0.Signal(value) };
        if SUCCEEDED(hr) {
            Ok(())
        } else {
            Err(Error::FenceSignalFailed)
        }
    }

    pub fn reset(&self) -> Result<(), Error> {
        self.signal(0)
    }

    pub fn get_value(&self) -> u64 {
        unsafe { self.0.GetCompletedValue() }
    }

    pub fn set_event_on_completion(&self, event: Event, value: u64) -> Result<(), Error> {
        let hr = unsafe { self.0.SetEventOnCompletion(value, event.handle) };
        if winerror::SUCCEEDED(hr) {
            Ok(())
        } else {
            Err(Error::FenceSetCompletionEventFailed)
        }
    }

    pub fn wait(&self, event: Event, value: u64) -> Result<bool, Error> {
        self.wait_with_timeout(event, value, u64::max_value())
    }

    pub fn wait_with_timeout(
        &self,
        event: Event,
        value: u64,
        timeout_ns: u64,
    ) -> Result<bool, Error> {
        if self.get_value() >= value {
            return Ok(true);
        }

        self.set_event_on_completion(event, value)
            .map(|_| event.wait(timeout_ns as _))
    }
}
