use crate::graphics::buffer::BufferCopyRegion;
use crate::graphics::command::CommandAllocator;
use crate::graphics::command::{CommandListType, GraphicsCommandList};
use crate::graphics::device::Device;
use crate::graphics::memory::{AllocationType, MemoryAllocator};
use crate::graphics::resource::GpuResource;

use winapi::um::d3d12;

use std::cell::RefCell;
use std::mem;
use std::ptr;

pub struct CommandContextManager {}

impl CommandContextManager {}

pub struct CommandContext<'a> {
    resource_barriers: RefCell<Vec<d3d12::D3D12_RESOURCE_BARRIER>>,
    command_list: RefCell<GraphicsCommandList>,
    allocator: RefCell<MemoryAllocator<'a>>,
}

impl<'a> CommandContext<'a> {
    pub fn begin(
        device: &'a Device,
        command_allocator: &CommandAllocator,
        command_list_type: CommandListType,
        id: &str,
    ) -> Self {
        CommandContext {
            resource_barriers: RefCell::new(Vec::new()),
            command_list: RefCell::new(
                GraphicsCommandList::new(
                    &device,
                    command_allocator,
                    command_list_type,
                    &format!("Adamant::CommandContext_{}::CommandList", id),
                )
                .unwrap(),
            ),
            allocator: RefCell::new(MemoryAllocator::new(&device, AllocationType::CpuWritable)),
        }
    }

    pub fn end(&self, _wait_for_completion: bool) {
        self.flush_resource_barriers();

        // TODO
        // Queue execute command list
        // Queue discard allocator
        // Cleanup allocated memory, so we need to track it when we do
        // Wait for fence
    }

    pub fn transition_resource(
        &self,
        resource: &mut GpuResource,
        new_state: d3d12::D3D12_RESOURCE_STATES,
        flush: bool,
    ) {
        let old_state = resource.usage_state;
        if old_state != new_state {
            let mut barrier = d3d12::D3D12_RESOURCE_BARRIER {
                Type: d3d12::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: d3d12::D3D12_RESOURCE_BARRIER_FLAG_NONE,
                u: unsafe { mem::zeroed() },
            };
            *unsafe { barrier.u.Transition_mut() } = d3d12::D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: resource.native.as_ptr_mut(),
                Subresource: d3d12::D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: old_state,
                StateAfter: new_state,
            };

            let mut resource_barriers = self.resource_barriers.borrow_mut();
            resource_barriers.push(barrier);

            resource.usage_state = new_state;
        }

        if flush {
            self.flush_resource_barriers();
        }
    }

    pub fn flush_resource_barriers(&self) {
        self.command_list
            .borrow()
            .insert_resource_barriers(&self.resource_barriers.borrow());
    }

    pub fn init_buffer(
        device: &Device,
        command_allocator: &CommandAllocator,
        dest: &mut GpuResource,
        data: ptr::NonNull<u8>,
        size: u64,
        offset: u64,
    ) {
        let init_context = CommandContext::begin(
            device,
            command_allocator,
            CommandListType::Direct,
            "InitBuffer",
        );

        // Upload buffer data into GPU memory
        let mut allocator = init_context.allocator.borrow_mut();
        let memory = allocator.allocate(size);
        unsafe {
            let mapping = memory.resource.map().unwrap();
            ptr::copy_nonoverlapping(data.as_ptr(), mapping, size as _);
            memory.resource.unmap();
        }

        init_context.transition_resource(dest, d3d12::D3D12_RESOURCE_STATE_COPY_DEST, true);
        let command_list = init_context.command_list.borrow();
        command_list.copy_buffer(
            dest,
            &memory.resource,
            &[BufferCopyRegion {
                source_offset: offset,
                dest_offset: 0,
                size,
            }],
        );
        init_context.transition_resource(dest, d3d12::D3D12_RESOURCE_STATE_GENERIC_READ, true);

        init_context.end(true);
    }
}
