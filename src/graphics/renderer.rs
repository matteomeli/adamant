use crate::graphics::com::ComPtr;
use crate::graphics::command::{
    CommandAllocator, CommandListType, CommandQueue, GraphicsCommandList,
};
use crate::graphics::descriptor::{CPUDescriptor, DescriptorAllocator};
use crate::graphics::device::Device;
use crate::graphics::dxgi::{Adapter, Factory, Swapchain, SwapchainProperties};
use crate::graphics::resource::GpuResource;

use crate::{InitFlags, InitParams};

use log::{info, trace, warn};

use winapi::shared::{
    dxgi, dxgi1_3, dxgi1_5, dxgiformat, dxgitype, minwindef,
    winerror::{self, FAILED, SUCCEEDED},
};
use winapi::um::{d3d12, d3d12sdklayers, d3dcommon, dxgidebug};
use winapi::Interface;

use winit::Window;

#[cfg(target_os = "windows")]
use winit::os::windows::WindowExt;

use std::convert::TryInto;
use std::mem::{self, ManuallyDrop};
use std::ptr;

pub struct Renderer {
    factory: ManuallyDrop<Factory>,
    device: ManuallyDrop<Device>,
    command_queue: ManuallyDrop<CommandQueue>,
    command_allocators: ManuallyDrop<Vec<CommandAllocator>>,
    command_list: ManuallyDrop<GraphicsCommandList>,
    swapchain: ManuallyDrop<Swapchain>,
    descriptor_allocator:
        ManuallyDrop<[DescriptorAllocator; d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_NUM_TYPES as _]>,
    render_targets: ManuallyDrop<Vec<GpuResource>>,
    depth_stencil: ManuallyDrop<GpuResource>,
    rtv_descriptors: Vec<CPUDescriptor>,
    dsv_descriptor: CPUDescriptor,
    screen_viewport: d3d12::D3D12_VIEWPORT,
    scissor_rect: d3d12::D3D12_RECT,
    color_space: dxgitype::DXGI_COLOR_SPACE_TYPE,
    back_buffer_format: dxgiformat::DXGI_FORMAT,
    depth_buffer_format: dxgiformat::DXGI_FORMAT,
    back_buffer_count: u32,
    back_buffer_width: u32,
    back_buffer_height: u32,
    back_buffer_index: u32,
    flags: InitFlags,
}

impl Renderer {
    pub fn new(window: &Window, params: &InitParams) -> Self {
        let window_handle = window.get_hwnd() as *mut _;

        // Enable debug layer.
        let factory_flags = Self::enable_debug_layer();

        // Create DXGI factory.
        let factory = Factory::new(window_handle, factory_flags).unwrap();
        factory.disable_exclusive_fullscreen().unwrap();

        // Determine if tearing is supported for fullscreen borderless windows.
        let mut flags = params.flags;
        if params.flags.contains(InitFlags::ALLOW_TEARING) {
            let mut allow_tearing_feature = minwindef::FALSE;
            let check = factory.check_feature_support(
                dxgi1_5::DXGI_FEATURE_PRESENT_ALLOW_TEARING,
                &mut allow_tearing_feature as *mut _ as *mut _,
                mem::size_of::<minwindef::BOOL>() as _,
            );
            if check.is_err() || allow_tearing_feature == minwindef::FALSE {
                flags.remove(InitFlags::ALLOW_TEARING);
            }
        }

        // Get adapter.
        let adapter = Adapter::new(&factory, d3dcommon::D3D_FEATURE_LEVEL_11_0, false).unwrap();

        // Create D3D12 API device.
        let device = Device::new(&adapter, d3dcommon::D3D_FEATURE_LEVEL_11_0).unwrap();

        // Create command queue.
        let command_queue = CommandQueue::new(
            &device,
            CommandListType::Direct,
            d3d12::D3D12_COMMAND_QUEUE_FLAG_NONE,
            "Adamant::CommandQueue",
        )
        .unwrap();

        // Create a command allocator for each render target that will be rendered to.
        let mut command_allocators = Vec::with_capacity(params.back_buffer_count as usize);
        for n in 0..params.back_buffer_count {
            command_allocators.push(
                CommandAllocator::new(
                    &device,
                    CommandListType::Direct,
                    &format!("Adamant::CommandAllocator{}", n),
                )
                .unwrap(),
            );
        }

        // Create a command list for recording graphics commands.
        let command_list = GraphicsCommandList::new(
            &device,
            &command_allocators[0],
            CommandListType::Direct,
            "Adamant::CommandList",
        )
        .unwrap();

        // Start off in a closed state. This is because the first time we refer
        // to the command list we will Reset it, and it needs to be closed before
        // calling Reset.
        command_list.close().unwrap();

        // Compute appropriate back buffer format.
        let back_buffer_format = Self::no_srgb(params.back_buffer_format);

        // Create swapchain.
        let swapchain = Swapchain::new(
            &factory,
            &command_queue,
            SwapchainProperties {
                window_handle,
                back_buffer_count: params.back_buffer_count,
                back_buffer_width: params.window_width,
                back_buffer_height: params.window_height,
                back_buffer_format,
                is_tearing_supported: flags.contains(InitFlags::ALLOW_TEARING),
            },
        )
        .unwrap();

        // Cache back buffer index.
        let back_buffer_index = swapchain.get_current_back_buffer_index();

        // Handle HDR output.
        let color_space = swapchain
            .compute_color_space(back_buffer_format, flags.contains(InitFlags::ENABLE_HDR));

        // Create cpu descriptor allocator.
        let mut descriptor_allocator = [
            DescriptorAllocator::new(
                device.clone(),
                d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            ),
            DescriptorAllocator::new(device.clone(), d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER),
            DescriptorAllocator::new(device.clone(), d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_RTV),
            DescriptorAllocator::new(device.clone(), d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_DSV),
        ];

        // Create render targets for each bak buffer.
        let (render_targets, rtv_descriptors) = Self::create_render_targets(
            &device,
            &swapchain,
            &mut descriptor_allocator[d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_RTV as usize],
            back_buffer_format,
            params.back_buffer_count,
        );

        // Allocate a 2-D surface as the depth/stencil buffer and create a depth/stencil view on this surface.
        let (depth_stencil, dsv_descriptor) = Self::create_depth_stencil(
            &device,
            &mut descriptor_allocator[d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_DSV as usize],
            params.depth_buffer_format,
            params.window_width,
            params.window_height,
        );

        // Set rendering viewport and scissor rectangle to fit client window.
        let screen_viewport = d3d12::D3D12_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: params.window_width as _,
            Height: params.window_height as _,
            MinDepth: d3d12::D3D12_MIN_DEPTH,
            MaxDepth: d3d12::D3D12_MAX_DEPTH,
        };
        let scissor_rect = d3d12::D3D12_RECT {
            left: 0,
            top: 0,
            right: params.window_width as _,
            bottom: params.window_height as _,
        };

        Renderer {
            factory: ManuallyDrop::new(factory),
            device: ManuallyDrop::new(device),
            command_queue: ManuallyDrop::new(command_queue),
            command_allocators: ManuallyDrop::new(command_allocators),
            command_list: ManuallyDrop::new(command_list),
            swapchain: ManuallyDrop::new(swapchain),
            descriptor_allocator: ManuallyDrop::new(descriptor_allocator),
            render_targets: ManuallyDrop::new(render_targets),
            depth_stencil: ManuallyDrop::new(depth_stencil),
            rtv_descriptors,
            dsv_descriptor,
            screen_viewport,
            scissor_rect,
            color_space,
            back_buffer_format: params.back_buffer_format,
            depth_buffer_format: params.depth_buffer_format,
            back_buffer_count: params.back_buffer_count,
            back_buffer_width: params.window_width,
            back_buffer_height: params.window_height,
            back_buffer_index,
            flags,
        }
    }

    pub fn prepare(&self) {
        let current_index = self.back_buffer_index as usize;
        unsafe {
            self.command_allocators[current_index].reset().unwrap();
            self.command_list
                .reset(&self.command_allocators[current_index])
                .unwrap();

            // Transition the render target into the correct state to allow for drawing into it.
            let mut barrier = d3d12::D3D12_RESOURCE_BARRIER {
                Type: d3d12::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: d3d12::D3D12_RESOURCE_BARRIER_FLAG_NONE,
                u: mem::zeroed(),
            };
            *barrier.u.Transition_mut() = d3d12::D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: self.render_targets[current_index].native.as_ptr_mut(),
                Subresource: d3d12::D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: d3d12::D3D12_RESOURCE_STATE_PRESENT,
                StateAfter: d3d12::D3D12_RESOURCE_STATE_RENDER_TARGET,
            };
            self.command_list.0.ResourceBarrier(1, &barrier);
        }
    }

    pub fn clear(&self) {
        unsafe {
            let rtv_descriptor = self.rtv_descriptors[self.back_buffer_index as usize];
            self.command_list.0.OMSetRenderTargets(
                1,
                &rtv_descriptor,
                minwindef::FALSE,
                &self.dsv_descriptor,
            );
            let clear_color = [0.392, 0.584, 0.929, 1.0];
            self.command_list
                .0
                .ClearRenderTargetView(rtv_descriptor, &clear_color, 0, ptr::null());
            self.command_list.0.ClearDepthStencilView(
                self.dsv_descriptor,
                d3d12::D3D12_CLEAR_FLAG_DEPTH,
                1.0,
                0,
                0,
                ptr::null(),
            );
            self.command_list.0.RSSetViewports(1, &self.screen_viewport);
            self.command_list.0.RSSetScissorRects(1, &self.scissor_rect);
        }
    }

    pub fn present(&mut self) {
        let current_index = self.back_buffer_index as usize;
        unsafe {
            // Transition the render target to the state that allows it to be presented to the display.
            let mut barrier = d3d12::D3D12_RESOURCE_BARRIER {
                Type: d3d12::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: d3d12::D3D12_RESOURCE_BARRIER_FLAG_NONE,
                u: mem::zeroed(),
            };
            *barrier.u.Transition_mut() = d3d12::D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: self.render_targets[current_index].native.as_ptr_mut(),
                Subresource: d3d12::D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: d3d12::D3D12_RESOURCE_STATE_RENDER_TARGET,
                StateAfter: d3d12::D3D12_RESOURCE_STATE_PRESENT,
            };
            self.command_list.0.ResourceBarrier(1, &barrier);

            // Send the command list off to the GPU for processing.
            self.command_list.close().unwrap();
            let command_list = self.command_list.0.as_ptr_mut() as *mut _;
            let command_lists = vec![command_list];
            self.command_queue
                .native
                .ExecuteCommandLists(command_lists.len() as _, command_lists.as_ptr());

            let hr = if self.flags.contains(InitFlags::ALLOW_TEARING) {
                // Recommended to always use tearing if supported when using a sync interval of 0.
                // Note this will fail if in true 'fullscreen' mode.
                self.swapchain
                    .0
                    .Present(0, dxgi::DXGI_PRESENT_ALLOW_TEARING)
            } else {
                // The first argument instructs DXGI to block until VSync, putting the application
                // to sleep until the next VSync. This ensures we don't waste any cycles rendering
                // frames that will never be displayed to the screen.
                self.swapchain.0.Present(1, 0)
            };

            // If the device was reset we must completely reinitialize the renderer.
            if SUCCEEDED(hr) {
                // Cache next back buffer index from swapchain.
                self.back_buffer_index = self.swapchain.get_current_back_buffer_index();
                // Wait until frame commands are complete. This waiting is inefficient and is
                // done for simplicity for now. Organize rendering code so it does not have to wait per frame.
                self.command_queue.flush().unwrap();
            } else if hr == winerror::DXGI_ERROR_DEVICE_REMOVED
                || hr == winerror::DXGI_ERROR_DEVICE_RESET
            {
                panic!(
                    "Device lost on Present() function call. Reason code: {}",
                    if hr == winerror::DXGI_ERROR_DEVICE_REMOVED {
                        self.device.native.GetDeviceRemovedReason()
                    } else {
                        hr
                    }
                );
            } else if FAILED(hr) {
                panic!("Failed to present");
            }
        }
    }

    pub fn on_window_resized(&mut self, width: u32, height: u32) {
        if self.back_buffer_width != width && self.back_buffer_height != height {
            self.back_buffer_width = u32::max(width, 1);
            self.back_buffer_height = u32::max(height, 1);

            // Wait until all previous GPU work is complete.
            self.command_queue.flush().unwrap();

            // Release resources that are tied to the swap chain and update fence values.
            unsafe {
                ManuallyDrop::drop(&mut self.render_targets);
                ManuallyDrop::drop(&mut self.depth_stencil);
            }

            // Resize swap chain.
            unsafe {
                let hr = self.swapchain.0.ResizeBuffers(
                    self.back_buffer_count,
                    self.back_buffer_width.try_into().unwrap(),
                    self.back_buffer_height.try_into().unwrap(),
                    self.back_buffer_format,
                    if self.flags.contains(InitFlags::ALLOW_TEARING) {
                        dxgi::DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING
                    } else {
                        0
                    },
                );
                if hr == winerror::DXGI_ERROR_DEVICE_REMOVED
                    || hr == winerror::DXGI_ERROR_DEVICE_RESET
                {
                    panic!(
                        "Device lost on ResizeBuffers() function call. Reason code: {}",
                        if hr == winerror::DXGI_ERROR_DEVICE_REMOVED {
                            self.device.native.GetDeviceRemovedReason()
                        } else {
                            hr
                        }
                    );
                } else if FAILED(hr) {
                    panic!("Failed to resize resources on window size changed.");
                }
            }

            self.back_buffer_index = self.swapchain.get_current_back_buffer_index();

            // Handle HDR output
            self.color_space = self.swapchain.compute_color_space(
                self.back_buffer_format,
                self.flags.contains(InitFlags::ENABLE_HDR),
            );

            // Create render targets for each back buffer.
            let (render_targets, rtv_descriptors) = Self::create_render_targets(
                &self.device,
                &self.swapchain,
                &mut self.descriptor_allocator[d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_RTV as usize],
                self.back_buffer_format,
                self.back_buffer_count,
            );
            self.render_targets = ManuallyDrop::new(render_targets);
            self.rtv_descriptors = rtv_descriptors;

            let (depth_stencil, dsv_descriptor) = Self::create_depth_stencil(
                &self.device,
                &mut self.descriptor_allocator[d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_DSV as usize],
                self.depth_buffer_format,
                self.back_buffer_width,
                self.back_buffer_height,
            );
            self.depth_stencil = ManuallyDrop::new(depth_stencil);
            self.dsv_descriptor = dsv_descriptor;

            // Set rendering viewport and scissor rectangle to fit client window.
            self.screen_viewport = d3d12::D3D12_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.back_buffer_width as _,
                Height: self.back_buffer_height as _,
                MinDepth: d3d12::D3D12_MIN_DEPTH,
                MaxDepth: d3d12::D3D12_MAX_DEPTH,
            };
            self.scissor_rect = d3d12::D3D12_RECT {
                left: 0,
                top: 0,
                right: self.back_buffer_width as _,
                bottom: self.back_buffer_height as _,
            };
        }
    }

    fn enable_debug_layer() -> u32 {
        let mut dxgi_factory_flags = 0;
        #[cfg(debug_assertions)]
        {
            trace!("Enabling D3D12 debug device.");
            let mut debug_controller = ComPtr::<d3d12sdklayers::ID3D12Debug>::empty();
            unsafe {
                if SUCCEEDED(d3d12::D3D12GetDebugInterface(
                    &d3d12sdklayers::ID3D12Debug::uuidof(),
                    debug_controller.as_mut_void(),
                )) {
                    info!("D3D12 debug device enabled.");
                    debug_controller.EnableDebugLayer();
                } else {
                    warn!("D3D12 debug device is not available.");
                }
            }

            trace!("Enabling DXGI info queue.");
            let mut info_queue = ComPtr::<dxgidebug::IDXGIInfoQueue>::empty();
            unsafe {
                if SUCCEEDED(dxgi1_3::DXGIGetDebugInterface1(
                    0,
                    &dxgidebug::IDXGIInfoQueue::uuidof(),
                    info_queue.as_mut_void(),
                )) {
                    info!("DXGI info queue enabled.");
                    dxgi_factory_flags = dxgi1_3::DXGI_CREATE_FACTORY_DEBUG;
                    info_queue.SetBreakOnSeverity(
                        dxgidebug::DXGI_DEBUG_ALL,
                        dxgidebug::DXGI_INFO_QUEUE_MESSAGE_SEVERITY_CORRUPTION,
                        minwindef::TRUE,
                    );
                    info_queue.SetBreakOnSeverity(
                        dxgidebug::DXGI_DEBUG_ALL,
                        dxgidebug::DXGI_INFO_QUEUE_MESSAGE_SEVERITY_ERROR,
                        minwindef::TRUE,
                    );

                    let mut hide: Vec<dxgidebug::DXGI_INFO_QUEUE_MESSAGE_ID> = vec![
                        80, // IDXGISwapChain::GetContainingOutput: The swapchain's adapter does not control the output on which the swapchain's window resides.
                    ];
                    let filter = dxgidebug::DXGI_INFO_QUEUE_FILTER {
                        DenyList: dxgidebug::DXGI_INFO_QUEUE_FILTER_DESC {
                            NumIDs: hide.len() as _,
                            pIDList: hide.as_mut_ptr(),
                            ..mem::zeroed()
                        },
                        ..mem::zeroed()
                    };
                    info_queue.AddStorageFilterEntries(dxgidebug::DXGI_DEBUG_DXGI, &filter);
                } else {
                    warn!("DXGI info queue is not available.");
                }
            }
        }
        dxgi_factory_flags
    }

    fn no_srgb(format: dxgiformat::DXGI_FORMAT) -> dxgiformat::DXGI_FORMAT {
        match format {
            dxgiformat::DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => dxgiformat::DXGI_FORMAT_R8G8B8A8_UNORM,
            dxgiformat::DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => dxgiformat::DXGI_FORMAT_B8G8R8A8_UNORM,
            dxgiformat::DXGI_FORMAT_B8G8R8X8_UNORM_SRGB => dxgiformat::DXGI_FORMAT_B8G8R8X8_UNORM,
            _ => format,
        }
    }

    fn create_render_targets(
        device: &Device,
        swapchain: &Swapchain,
        descriptor_allocator: &mut DescriptorAllocator,
        back_buffer_format: dxgiformat::DXGI_FORMAT,
        back_buffer_count: u32,
    ) -> (Vec<GpuResource>, Vec<CPUDescriptor>) {
        let mut render_targets = Vec::with_capacity(back_buffer_count as _);
        let mut rtv_descriptors = Vec::with_capacity(back_buffer_count as _);
        unsafe {
            for n in 0..back_buffer_count {
                let mut render_target = ComPtr::<d3d12::ID3D12Resource>::empty();
                if SUCCEEDED(swapchain.0.GetBuffer(
                    n,
                    &d3d12::ID3D12Resource::uuidof(),
                    render_target.as_mut_void(),
                )) {
                    info!("D3D12 render target view created for back buffer {}.", n);
                    #[cfg(debug_assertions)]
                    {
                        render_target.SetName(
                            format!("AdamantRenderTarget{}", n)
                                .encode_utf16()
                                .collect::<Vec<u16>>()
                                .as_ptr(),
                        );
                    }
                } else {
                    panic!(
                        "Failed to create D3D12 render target view for back buffer {}.",
                        n
                    );
                }

                let rtv_desc = d3d12::D3D12_RENDER_TARGET_VIEW_DESC {
                    Format: back_buffer_format,
                    ViewDimension: d3d12::D3D12_RTV_DIMENSION_TEXTURE2D,
                    ..mem::zeroed()
                };
                let rtv_descriptor = descriptor_allocator.allocate();
                device.native.CreateRenderTargetView(
                    render_target.as_ptr_mut(),
                    &rtv_desc,
                    rtv_descriptor,
                );
                rtv_descriptors.push(rtv_descriptor);
                render_targets.push(GpuResource::create(
                    render_target,
                    d3d12::D3D12_RESOURCE_STATE_PRESENT,
                ));
            }
        }
        (render_targets, rtv_descriptors)
    }

    fn create_depth_stencil(
        device: &Device,
        descriptor_allocator: &mut DescriptorAllocator,
        depth_buffer_format: dxgiformat::DXGI_FORMAT,
        back_buffer_width: u32,
        back_buffer_height: u32,
    ) -> (GpuResource, CPUDescriptor) {
        trace!("Creating D3D12 depth stencil buffer.");
        let dsv_descriptor = descriptor_allocator.allocate();
        let depth_heap_properties = d3d12::D3D12_HEAP_PROPERTIES {
            Type: d3d12::D3D12_HEAP_TYPE_DEFAULT,
            CPUPageProperty: d3d12::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: d3d12::D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };
        let depth_stencil_desc = d3d12::D3D12_RESOURCE_DESC {
            Dimension: d3d12::D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: u64::from(back_buffer_width),
            Height: back_buffer_height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: depth_buffer_format,
            SampleDesc: dxgitype::DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: d3d12::D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: d3d12::D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
        };

        let mut depth_stencil = ComPtr::<d3d12::ID3D12Resource>::empty();
        unsafe {
            let mut depth_optimized_clear_value = d3d12::D3D12_CLEAR_VALUE {
                Format: depth_buffer_format,
                ..mem::zeroed()
            };
            *depth_optimized_clear_value.u.DepthStencil_mut() = d3d12::D3D12_DEPTH_STENCIL_VALUE {
                Depth: 1.0,
                Stencil: 0,
            };
            if SUCCEEDED(device.native.CreateCommittedResource(
                &depth_heap_properties,
                d3d12::D3D12_HEAP_FLAG_NONE,
                &depth_stencil_desc,
                d3d12::D3D12_RESOURCE_STATE_DEPTH_WRITE,
                &depth_optimized_clear_value,
                &d3d12::ID3D12Resource::uuidof(),
                depth_stencil.as_mut_void(),
            )) {
                info!("D3D12 depth/stencil buffer created.");
            } else {
                panic!("Failed to create D3D12 depth/stencil buffer.");
            }

            #[cfg(debug_assertions)]
            {
                depth_stencil.SetName(
                    "AdamantDepthStencil"
                        .encode_utf16()
                        .collect::<Vec<u16>>()
                        .as_ptr(),
                );
            }

            let dsv_desc = d3d12::D3D12_DEPTH_STENCIL_VIEW_DESC {
                Format: depth_buffer_format,
                ViewDimension: d3d12::D3D12_DSV_DIMENSION_TEXTURE2D,
                ..mem::zeroed()
            };
            device.native.CreateDepthStencilView(
                depth_stencil.as_ptr_mut(),
                &dsv_desc,
                dsv_descriptor,
            );
        }
        (
            GpuResource::create(depth_stencil, d3d12::D3D12_RESOURCE_STATE_DEPTH_WRITE),
            dsv_descriptor,
        )
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // Wait for GPU to finish all work.
        self.command_queue.flush().unwrap();

        unsafe {
            ManuallyDrop::drop(&mut self.depth_stencil);
            ManuallyDrop::drop(&mut self.render_targets);
            ManuallyDrop::drop(&mut self.descriptor_allocator);
            ManuallyDrop::drop(&mut self.swapchain);
            ManuallyDrop::drop(&mut self.command_list);
            ManuallyDrop::drop(&mut self.command_allocators);
            ManuallyDrop::drop(&mut self.command_queue);

            #[cfg(debug_assertions)]
            {
                // Debug tracking alive device objects
                if let Ok(debug_device) = self
                    .device
                    .native
                    .cast::<d3d12sdklayers::ID3D12DebugDevice>()
                {
                    debug_device.ReportLiveDeviceObjects(
                        d3d12sdklayers::D3D12_RLDO_DETAIL
                            | d3d12sdklayers::D3D12_RLDO_IGNORE_INTERNAL,
                    );
                }
            }

            ManuallyDrop::drop(&mut self.device);
            ManuallyDrop::drop(&mut self.factory);

            #[cfg(debug_assertions)]
            {
                // Debug tracking alive dxgi objects
                let mut dxgi_debug = ComPtr::<dxgidebug::IDXGIDebug1>::empty();
                if winerror::SUCCEEDED(dxgi1_3::DXGIGetDebugInterface1(
                    0,
                    &dxgidebug::IDXGIDebug1::uuidof(),
                    dxgi_debug.as_mut_void(),
                )) {
                    dxgi_debug.ReportLiveObjects(
                        dxgidebug::DXGI_DEBUG_ALL,
                        dxgidebug::DXGI_DEBUG_RLO_SUMMARY
                            | dxgidebug::DXGI_DEBUG_RLO_IGNORE_INTERNAL,
                    );
                }
            }
        }
    }
}
