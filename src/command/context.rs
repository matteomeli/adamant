use crate::buffer::BufferCopyRegion;
use crate::command::{CommandAllocator, CommandListType, CommandQueue, GraphicsCommandList};
use crate::descriptor::CpuDescriptor;
use crate::device::Device;
use crate::memory::{AllocationType, MemoryAllocator};
use crate::resource::GpuResource;

use winapi::shared::minwindef;
use winapi::um::d3d12;

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;
use std::ptr;

pub struct CommandContextPool {
    device: Device,
    pool: HashMap<CommandListType, Vec<CommandContext>>,
    free_list: HashMap<CommandListType, Vec<usize>>,
}

impl CommandContextPool {
    pub fn new(device: Device) -> Self {
        CommandContextPool {
            device,
            pool: HashMap::new(),
            free_list: HashMap::new(),
        }
    }

    pub fn request(
        &mut self,
        type_: CommandListType,
        command_allocator: &CommandAllocator,
    ) -> &CommandContext {
        if !self.free_list.contains_key(&type_) {
            let command_contexts = self.pool.entry(type_).or_insert_with(Vec::new);
            let id = command_contexts.len();
            let command_context = CommandContext::new(&self.device, command_allocator, type_, id);
            command_contexts.push(command_context);
            command_contexts.last().unwrap()
        } else {
            let free_list = self.free_list.get_mut(&type_).unwrap();
            let context_id = free_list.pop().unwrap();
            let command_contexts = self.pool.get(&type_).unwrap();
            let command_context = &command_contexts[context_id];
            command_context.reset();
            command_context
        }
    }

    pub fn free(&mut self, command_context: &CommandContext) {
        let free_list = self
            .free_list
            .entry(command_context.type_)
            .or_insert_with(Vec::new);
        free_list.push(command_context.id);
    }
}

pub struct CommandContext {
    resource_barriers: RefCell<Vec<d3d12::D3D12_RESOURCE_BARRIER>>,
    command_list: RefCell<GraphicsCommandList>,
    cpu_memory_allocator: RefCell<MemoryAllocator>,
    pub(crate) type_: CommandListType,
    pub(crate) id: usize,
}

impl CommandContext {
    pub fn new(
        device: &Device,
        allocator: &CommandAllocator,
        type_: CommandListType,
        id: usize,
    ) -> Self {
        CommandContext {
            resource_barriers: RefCell::new(Vec::new()),
            command_list: RefCell::new(
                GraphicsCommandList::new(
                    &device,
                    allocator,
                    type_,
                    &format!("Adamant::CommandContext_{}::CommandList", id),
                )
                .unwrap(),
            ),
            cpu_memory_allocator: RefCell::new(MemoryAllocator::new(
                device.clone(),
                AllocationType::CpuWritable,
            )),
            type_,
            id,
        }
    }

    pub fn begin(&self) {}

    pub fn flush(
        &self,
        command_queue: &mut CommandQueue,
        command_allocator: CommandAllocator,
        wait_for_completion: bool,
    ) {
        self.flush_resource_barriers();

        command_queue.execute_command_list(self.command_list.borrow().as_command_list());
        command_queue.signal_fence().unwrap();

        if wait_for_completion {
            command_queue.wait_for_fence().unwrap();
        }

        self.command_list
            .borrow()
            .reset(&command_allocator)
            .unwrap();
    }

    pub fn end(
        &self,
        command_queue: &mut CommandQueue,
        command_allocator: CommandAllocator,
        command_context_pool: &mut CommandContextPool,
        wait_for_completion: bool,
    ) {
        self.flush_resource_barriers();

        command_queue.execute_command_list(self.command_list.borrow().as_command_list());
        command_queue.signal_fence().unwrap();
        command_queue.free_allocator(command_allocator);

        if wait_for_completion {
            command_queue.wait_for_fence().unwrap();
        }

        command_context_pool.free(&self);
    }

    pub fn reset(&self) {}

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

    pub fn set_render_targets(
        &self,
        rtv_descriptors: &[CpuDescriptor],
        dsv_descriptor: CpuDescriptor,
    ) {
        unsafe {
            self.command_list.borrow().0.OMSetRenderTargets(
                rtv_descriptors.len() as _,
                rtv_descriptors.as_ptr(),
                minwindef::FALSE,
                &dsv_descriptor,
            );
        }
    }

    pub fn init_buffer(
        command_queue: &mut CommandQueue,
        command_allocator: CommandAllocator,
        command_context_pool: &mut CommandContextPool,
        pool: &mut CommandContextPool,
        dest: &mut GpuResource,
        data: ptr::NonNull<u8>,
        size: u64,
        offset: u64,
    ) {
        let init_context = pool.request(CommandListType::Direct, &command_allocator);
        init_context.begin();

        // Upload buffer data into GPU memory
        let mut allocator = init_context.cpu_memory_allocator.borrow_mut();
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

        init_context.end(command_queue, command_allocator, command_context_pool, true);
    }
}
