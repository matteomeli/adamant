use crate::{ComPtr, InitFlags, InitParams};

use log::{info, trace, warn};

use winapi::shared::{
    dxgi, dxgi1_2, dxgi1_3, dxgi1_4, dxgi1_5, dxgi1_6, dxgiformat, dxgitype, minwindef,
    windef::HWND,
    winerror::{self, FAILED, SUCCEEDED},
};
use winapi::um::{d3d12, d3d12sdklayers, d3dcommon, dxgidebug, synchapi, winbase, winnt};
use winapi::Interface;

use std::convert::TryInto;
use std::ffi::OsString;
use std::mem;
use std::os::windows::ffi::OsStringExt;
use std::ptr;

pub struct D3D12Layer {
    // Direct3D objects
    device: ComPtr<d3d12::ID3D12Device>,
    command_queue: ComPtr<d3d12::ID3D12CommandQueue>,
    command_list: ComPtr<d3d12::ID3D12GraphicsCommandList>,
    command_allocators: Vec<ComPtr<d3d12::ID3D12CommandAllocator>>,
    // Swap chain objects
    factory: ComPtr<dxgi1_4::IDXGIFactory4>,
    swap_chain: ComPtr<dxgi1_4::IDXGISwapChain3>,
    render_targets: Vec<ComPtr<d3d12::ID3D12Resource>>,
    depth_stencil: ComPtr<d3d12::ID3D12Resource>,
    // Presentation/synchronization fence objects
    fence: ComPtr<d3d12::ID3D12Fence>,
    fence_values: Vec<u64>,
    fence_event: winnt::HANDLE,
    // Direct3D rendering objects
    rtv_descriptor_heap: ComPtr<d3d12::ID3D12DescriptorHeap>,
    dsv_descriptor_heap: ComPtr<d3d12::ID3D12DescriptorHeap>,
    rtv_descriptor_size: u32,
    screen_viewport: d3d12::D3D12_VIEWPORT,
    scissor_rect: d3d12::D3D12_RECT,
    // Direct3D properties
    back_buffer_format: dxgiformat::DXGI_FORMAT,
    depth_buffer_format: dxgiformat::DXGI_FORMAT,
    back_buffer_count: u32,
    min_feature_level: d3dcommon::D3D_FEATURE_LEVEL,
    feature_level: d3dcommon::D3D_FEATURE_LEVEL,
    // Cached device properties
    window_handle: HWND,
    back_buffer_width: i32,
    back_buffer_height: i32,
    factory_flags: u32,
    // HDR support
    color_space: dxgitype::DXGI_COLOR_SPACE_TYPE,
    // Other options
    flags: InitFlags,
    // Frame properties
    back_buffer_index: u32,
}

impl D3D12Layer {
    pub fn new(params: InitParams) -> Self {
        trace!("Initializing D3D12 layer.");

        // Enable debug layer.
        let factory_flags = Self::enable_debug_layer();

        // Create DXGI factory.
        let factory = Self::create_factory(factory_flags);

        // Determine if tearing is supported for fullscreen borderless windows.
        let mut flags = params.flags;
        if params.flags.contains(InitFlags::ALLOW_TEARING) {
            trace!("Checking variable refresh rate display support.");
            unsafe {
                if let Ok(factory5) = factory.cast::<dxgi1_5::IDXGIFactory5>() {
                    let mut allow_tearing_feature = minwindef::FALSE;
                    let hr = factory5.CheckFeatureSupport(
                        dxgi1_5::DXGI_FEATURE_PRESENT_ALLOW_TEARING,
                        &mut allow_tearing_feature as *mut _ as *mut _,
                        mem::size_of::<minwindef::BOOL>() as _,
                    );
                    factory5.destroy();
                    if FAILED(hr) || allow_tearing_feature == minwindef::FALSE {
                        flags.remove(InitFlags::ALLOW_TEARING);
                    }
                }
            }
            if params.flags.contains(InitFlags::ALLOW_TEARING) {
                info!("Variable refresh rate displays supported.");
            } else {
                warn!("Variable refresh rate displays not supported.");
            }
        }

        // Get adapter.
        let adapter = Self::get_adapter(factory, params.min_feature_level);

        // Create D3D12 API device.
        let device = Self::create_device(adapter, params.min_feature_level);

        // Destroy adapter as it's not needed anymore
        unsafe {
            adapter.destroy();
        }

        // Configure debug device.
        #[cfg(debug_assertions)]
        {
            unsafe {
                if let Ok(info_queue) = device.cast::<d3d12sdklayers::ID3D12InfoQueue>() {
                    info_queue.SetBreakOnSeverity(
                        d3d12sdklayers::D3D12_MESSAGE_SEVERITY_CORRUPTION,
                        minwindef::TRUE,
                    );
                    info_queue.SetBreakOnSeverity(
                        d3d12sdklayers::D3D12_MESSAGE_SEVERITY_ERROR,
                        minwindef::TRUE,
                    );

                    let mut hide: Vec<d3d12sdklayers::D3D12_MESSAGE_ID> = vec![
                        d3d12sdklayers::D3D12_MESSAGE_ID_EXECUTECOMMANDLISTS_WRONGSWAPCHAINBUFFERREFERENCE,
                        d3d12sdklayers::D3D12_MESSAGE_ID_CLEARRENDERTARGETVIEW_MISMATCHINGCLEARVALUE,
                        d3d12sdklayers::D3D12_MESSAGE_ID_MAP_INVALID_NULLRANGE,
                        d3d12sdklayers::D3D12_MESSAGE_ID_UNMAP_INVALID_NULLRANGE,
                    ];
                    let mut filter = d3d12sdklayers::D3D12_INFO_QUEUE_FILTER {
                        DenyList: d3d12sdklayers::D3D12_INFO_QUEUE_FILTER_DESC {
                            NumIDs: hide.len() as _,
                            pIDList: hide.as_mut_ptr(),
                            ..mem::zeroed()
                        },
                        ..mem::zeroed()
                    };
                    info_queue.AddStorageFilterEntries(&mut filter);
                    info_queue.destroy();
                }
            }
        }

        // Determine maximum feature level supported for the obtained device.
        let levels: [d3dcommon::D3D_FEATURE_LEVEL; 4] = [
            d3dcommon::D3D_FEATURE_LEVEL_12_1,
            d3dcommon::D3D_FEATURE_LEVEL_12_0,
            d3dcommon::D3D_FEATURE_LEVEL_11_1,
            d3dcommon::D3D_FEATURE_LEVEL_11_0,
        ];
        let mut feature_levels = d3d12::D3D12_FEATURE_DATA_FEATURE_LEVELS {
            NumFeatureLevels: levels.len() as _,
            pFeatureLevelsRequested: levels.as_ptr(),
            MaxSupportedFeatureLevel: d3dcommon::D3D_FEATURE_LEVEL_11_0,
        };
        let feature_level = unsafe {
            if SUCCEEDED(device.CheckFeatureSupport(
                d3d12::D3D12_FEATURE_FEATURE_LEVELS,
                &mut feature_levels as *mut _ as *mut _,
                mem::size_of::<d3d12::D3D12_FEATURE_DATA_FEATURE_LEVELS>() as _,
            )) {
                feature_levels.MaxSupportedFeatureLevel
            } else {
                params.min_feature_level
            }
        };

        // Create command queue.
        let command_queue = Self::create_command_queue(device);

        // Create descriptor heaps for render target and depth stencil views.
        let rtv_descriptor_heap =
            Self::create_rtv_descriptor_heap(device, params.back_buffer_count);
        let rtv_descriptor_size = unsafe {
            device.GetDescriptorHandleIncrementSize(d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
        };
        let dsv_descriptor_heap = Self::create_dsv_descriptor_heap(device);

        // Create a command allocator for each back buffer that will be rendered to.
        let command_allocators = Self::create_command_allocators(device, params.back_buffer_count);

        // Create a command list for recording graphics commands.
        let command_list = Self::create_command_list(device, command_allocators[0]);

        // Create fence for syncing CPU and GPU execution processes.
        let fence_values = vec![0; params.back_buffer_count as usize];
        let fence = Self::create_fence(device, fence_values[0]);
        let fence_event = Self::create_fence_event();

        // Compute appropriate back buffer format.
        let back_buffer_format = Self::no_srgb(params.back_buffer_format);

        // Create swapchain.
        let swap_chain = Self::create_swap_chain(
            factory,
            command_queue,
            params.window_handle,
            params.window_width,
            params.window_height,
            back_buffer_format,
            params.back_buffer_count,
            flags.contains(InitFlags::ALLOW_TEARING),
        );

        // Handle HDR output.
        let color_space = Self::compute_color_space(swap_chain, back_buffer_format, flags);

        // Create render targets for each bak buffer.
        let render_targets = Self::create_render_targets(
            device,
            swap_chain,
            rtv_descriptor_heap,
            back_buffer_format,
            params.back_buffer_count,
            rtv_descriptor_size,
        );

        let back_buffer_index = unsafe { swap_chain.GetCurrentBackBufferIndex() };

        // Allocate a 2-D surface as the depth/stencil buffer and create a depth/stencil view on this surface.
        let depth_stencil = Self::create_depth_stencil(
            device,
            dsv_descriptor_heap,
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

        info!("D3D12 layer initialized successfully.");

        D3D12Layer {
            device,
            command_queue,
            command_list,
            command_allocators,
            factory,
            swap_chain,
            render_targets,
            depth_stencil,
            fence,
            fence_values,
            fence_event,
            rtv_descriptor_heap,
            dsv_descriptor_heap,
            rtv_descriptor_size,
            screen_viewport,
            scissor_rect,
            back_buffer_format: params.back_buffer_format,
            depth_buffer_format: params.depth_buffer_format,
            back_buffer_count: params.back_buffer_count,
            min_feature_level: d3dcommon::D3D_FEATURE_LEVEL_11_0,
            feature_level,
            window_handle: params.window_handle,
            back_buffer_width: params.window_width as _,
            back_buffer_height: params.window_height as _,
            factory_flags,
            color_space,
            flags,
            back_buffer_index,
        }
    }

    pub fn prepare(&self) {
        let current_index = self.back_buffer_index as usize;
        unsafe {
            if FAILED(self.command_allocators[current_index].Reset()) {
                panic!(
                    "Failed to reset command allocator for back buffer {}",
                    current_index
                );
            }
            if FAILED(self.command_list.Reset(
                self.command_allocators[current_index].as_raw(),
                ptr::null_mut(),
            )) {
                panic!("Failed to reset command list");
            }

            // Transition the render target into the correct state to allow for drawing into it.
            let mut barrier = d3d12::D3D12_RESOURCE_BARRIER {
                Type: d3d12::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: d3d12::D3D12_RESOURCE_BARRIER_FLAG_NONE,
                u: mem::zeroed(),
            };
            *barrier.u.Transition_mut() = d3d12::D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: self.render_targets[current_index].as_raw(),
                Subresource: d3d12::D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: d3d12::D3D12_RESOURCE_STATE_PRESENT,
                StateAfter: d3d12::D3D12_RESOURCE_STATE_RENDER_TARGET,
            };
            self.command_list.ResourceBarrier(1, &barrier);
        }
    }

    pub fn clear(&self) {
        unsafe {
            let rtv_descriptor = self.get_rtv_descriptor();
            let dsv_descriptor = self
                .dsv_descriptor_heap
                .GetCPUDescriptorHandleForHeapStart();
            self.command_list.OMSetRenderTargets(
                1,
                &rtv_descriptor,
                minwindef::FALSE,
                &dsv_descriptor,
            );
            let clear_color = [0.392, 0.584, 0.929, 1.0];
            self.command_list
                .ClearRenderTargetView(rtv_descriptor, &clear_color, 0, ptr::null());
            self.command_list.ClearDepthStencilView(
                dsv_descriptor,
                d3d12::D3D12_CLEAR_FLAG_DEPTH,
                1.0,
                0,
                0,
                ptr::null(),
            );
            self.command_list.RSSetViewports(1, &self.screen_viewport);
            self.command_list.RSSetScissorRects(1, &self.scissor_rect);
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
                pResource: self.render_targets[current_index].as_raw(),
                Subresource: d3d12::D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: d3d12::D3D12_RESOURCE_STATE_RENDER_TARGET,
                StateAfter: d3d12::D3D12_RESOURCE_STATE_PRESENT,
            };
            self.command_list.ResourceBarrier(1, &barrier);

            // Send the command list off to the GPU for processing.
            if FAILED(self.command_list.Close()) {
                panic!("Failed to close command list");
            }
            let command_list = self.command_list.as_raw() as *mut _;
            let command_lists = vec![command_list];
            self.command_queue
                .ExecuteCommandLists(1, command_lists.as_ptr());

            let hr = if self.flags.contains(InitFlags::ALLOW_TEARING) {
                // Recommended to always use tearing if supported when using a sync interval of 0.
                // Note this will fail if in true 'fullscreen' mode.
                self.swap_chain.Present(0, dxgi::DXGI_PRESENT_ALLOW_TEARING)
            } else {
                // The first argument instructs DXGI to block until VSync, putting the application
                // to sleep until the next VSync. This ensures we don't waste any cycles rendering
                // frames that will never be displayed to the screen.
                self.swap_chain.Present(1, 0)
            };
            // If the device was reset we must completely reinitialize the renderer.
            if hr == winerror::DXGI_ERROR_DEVICE_REMOVED || hr == winerror::DXGI_ERROR_DEVICE_RESET
            {
                unimplemented!();
            } else if FAILED(hr) {
                panic!("Failed to present");
            } else {
                self.move_to_next_frame();

                if self.factory.IsCurrent() == minwindef::FALSE {
                    // Output information is cached on the DXGI Factory. If it is stale we need to create a new factory.
                    self.factory.destroy();
                    if FAILED(dxgi1_3::CreateDXGIFactory2(
                        self.factory_flags,
                        &dxgi1_4::IDXGIFactory4::uuidof(),
                        &mut self.factory.as_raw() as *mut *mut _ as *mut *mut _,
                    )) {
                        panic!("Failed to create DXGI factory");
                    }
                }
            }
        }
    }

    pub fn on_window_size_changed(&mut self, width: i32, height: i32) -> bool {
        if self.back_buffer_width == width && self.back_buffer_height == height {
            self.color_space =
                Self::compute_color_space(self.swap_chain, self.back_buffer_format, self.flags);
            false
        } else {
            trace!("Window size has changed, updating resources.");

            self.back_buffer_width = width;
            self.back_buffer_height = height;
            self.update_window_size_dependent_resources();

            info!("Swap chain resized to {}x{}.", width, height);

            true
        }
    }

    fn enable_debug_layer() -> u32 {
        let mut dxgi_factory_flags = 0;
        #[cfg(debug_assertions)]
        {
            trace!("Enabling D3D12 debug device.");
            let mut debug_controller = ComPtr::<d3d12sdklayers::ID3D12Debug>::null();
            unsafe {
                if SUCCEEDED(d3d12::D3D12GetDebugInterface(
                    &d3d12sdklayers::ID3D12Debug::uuidof(),
                    debug_controller.as_mut_void(),
                )) {
                    info!("D3D12 debug device enabled.");
                    debug_controller.EnableDebugLayer();
                    debug_controller.destroy();
                } else {
                    warn!("D3D12 debug device is not available.");
                }
            }

            trace!("Enabling DXGI info queue.");
            let mut info_queue = ComPtr::<dxgidebug::IDXGIInfoQueue>::null();
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
                    info_queue.destroy();
                } else {
                    warn!("DXGI info queue is not available.");
                }
            }
        }
        dxgi_factory_flags
    }

    fn create_factory(flags: u32) -> ComPtr<dxgi1_4::IDXGIFactory4> {
        trace!("Creating DXGI factory.");
        let mut factory = ComPtr::<dxgi1_4::IDXGIFactory4>::null();
        unsafe {
            if SUCCEEDED(dxgi1_3::CreateDXGIFactory2(
                flags,
                &dxgi1_4::IDXGIFactory4::uuidof(),
                factory.as_mut_void(),
            )) {
                info!("DXGI factory created.");
            } else {
                panic!("Failed to create DXGI factory.");
            }
        }
        factory
    }

    fn get_adapter(
        factory: ComPtr<dxgi1_4::IDXGIFactory4>,
        min_feature_level: u32,
    ) -> ComPtr<dxgi::IDXGIAdapter1> {
        trace!("Searching for D3D12 adapter.");
        let mut adapter = ComPtr::<dxgi::IDXGIAdapter1>::null();
        unsafe {
            // Pretty much all unsafe here.
            let mut index = 0;
            if let Ok(factory6) = factory.cast::<dxgi1_6::IDXGIFactory6>() {
                loop {
                    if SUCCEEDED(factory6.EnumAdapterByGpuPreference(
                        index,
                        dxgi1_6::DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                        &dxgi::IDXGIAdapter1::uuidof(),
                        adapter.as_mut_void(),
                    )) {
                        index += 1;
                        let mut desc = dxgi::DXGI_ADAPTER_DESC1 { ..mem::zeroed() };
                        let hr = adapter.GetDesc1(&mut desc);
                        if FAILED(hr) {
                            panic!("Failed to get adapter description.");
                        }

                        // Skip the Basic Render Driver adapter.
                        if (desc.Flags & dxgi::DXGI_ADAPTER_FLAG_SOFTWARE) != 0 {
                            continue;
                        }

                        if SUCCEEDED(d3d12::D3D12CreateDevice(
                            adapter.as_unknown() as *const _ as *mut _,
                            min_feature_level,
                            &d3d12::ID3D12Device::uuidof(),
                            ptr::null_mut(),
                        )) {
                            break;
                        }
                    }
                }
                factory6.destroy();
            } else {
                // Find the adapter with the largest dedicated video memory.
                let mut current_adapter = ComPtr::<dxgi::IDXGIAdapter1>::null();
                let mut found_adapter_index = 0;
                let mut max_dedicated_video_memeory_found: usize = 0;
                while SUCCEEDED(factory.EnumAdapters1(
                    index,
                    current_adapter.as_mut_void() as *mut *mut _ as *mut *mut _,
                )) {
                    index += 1;

                    let mut desc = dxgi::DXGI_ADAPTER_DESC1 { ..mem::zeroed() };
                    let hr = current_adapter.GetDesc1(&mut desc);
                    if FAILED(hr) {
                        panic!("Failed to get adapter description.");
                    }

                    // Skip the Basic Render Driver adapter.
                    if (desc.Flags & dxgi::DXGI_ADAPTER_FLAG_SOFTWARE) != 0 {
                        continue;
                    }

                    if SUCCEEDED(d3d12::D3D12CreateDevice(
                        current_adapter.as_unknown() as *const _ as *mut _,
                        min_feature_level,
                        &d3d12::ID3D12Device::uuidof(),
                        ptr::null_mut(),
                    )) && desc.DedicatedVideoMemory > max_dedicated_video_memeory_found
                    {
                        max_dedicated_video_memeory_found = desc.DedicatedVideoMemory;
                        found_adapter_index = index - 1;
                        current_adapter.destroy();
                    }
                }

                if FAILED(factory.EnumAdapters1(
                    found_adapter_index,
                    adapter.as_mut_void() as *mut *mut _ as *mut *mut _,
                )) {
                    panic!("Failed to get adapter.");
                }
            }

            #[cfg(debug_assertions)]
            {
                if adapter.is_null()
                    && FAILED(
                        factory
                            .EnumWarpAdapter(&dxgi::IDXGIAdapter1::uuidof(), adapter.as_mut_void()),
                    )
                {
                    panic!("Failed to create the WARP adapter.");
                }
            }
        }

        if adapter.is_null() {
            panic!("No D3D12 adapter found.");
        }

        unsafe {
            let mut desc = dxgi::DXGI_ADAPTER_DESC1 { ..mem::zeroed() };
            let hr = adapter.GetDesc1(&mut desc);
            if FAILED(hr) {
                panic!("Failed to get adapter description.");
            }
            let device_name = {
                let len = desc.Description.iter().take_while(|&&c| c != 0).count();
                let name = <OsString as OsStringExt>::from_wide(&desc.Description[..len]);
                name.to_string_lossy().into_owned()
            };
            info!(
                "Found D3D12 adapter '{}' with {}MB of dedicated video memory.",
                device_name,
                desc.DedicatedVideoMemory / 1000 / 1000
            );
        }
        adapter
    }

    fn create_device(
        adapter: ComPtr<dxgi::IDXGIAdapter1>,
        min_feature_level: u32,
    ) -> ComPtr<d3d12::ID3D12Device> {
        trace!("Creating D3D12 device.");
        let mut device = ComPtr::<d3d12::ID3D12Device>::null();
        unsafe {
            if SUCCEEDED(d3d12::D3D12CreateDevice(
                adapter.as_raw() as _,
                min_feature_level,
                &d3d12::ID3D12Device::uuidof(),
                device.as_mut_void(),
            )) {
                info!("D3D12 device created.");
            } else {
                panic!("Failed to create D3D12 device.");
            }

            device.SetName(
                "AdamantDevice"
                    .encode_utf16()
                    .collect::<Vec<u16>>()
                    .as_ptr(),
            );
        }
        device
    }

    fn create_command_queue(
        device: ComPtr<d3d12::ID3D12Device>,
    ) -> ComPtr<d3d12::ID3D12CommandQueue> {
        trace!("Creating D3D12 command queue.");
        let mut command_queue = ComPtr::<d3d12::ID3D12CommandQueue>::null();
        unsafe {
            let desc = d3d12::D3D12_COMMAND_QUEUE_DESC {
                Flags: d3d12::D3D12_COMMAND_QUEUE_FLAG_NONE,
                Type: d3d12::D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..mem::zeroed()
            };

            if SUCCEEDED(device.CreateCommandQueue(
                &desc,
                &d3d12::ID3D12CommandQueue::uuidof(),
                command_queue.as_mut_void(),
            )) {
                info!("D3D12 command queue created.");
            } else {
                panic!("Failed to create D3D12 command queue.");
            }

            command_queue.SetName(
                "AdamantCommandQueue"
                    .encode_utf16()
                    .collect::<Vec<u16>>()
                    .as_ptr(),
            );
        }
        command_queue
    }

    fn create_rtv_descriptor_heap(
        device: ComPtr<d3d12::ID3D12Device>,
        back_buffer_count: u32,
    ) -> ComPtr<d3d12::ID3D12DescriptorHeap> {
        trace!("Creating D3D12 render target view descriptor heap.");
        let mut rtv_descriptor_heap = ComPtr::<d3d12::ID3D12DescriptorHeap>::null();
        unsafe {
            let desc = d3d12::D3D12_DESCRIPTOR_HEAP_DESC {
                NumDescriptors: back_buffer_count,
                Type: d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                ..mem::zeroed()
            };

            if SUCCEEDED(device.CreateDescriptorHeap(
                &desc,
                &d3d12::ID3D12DescriptorHeap::uuidof(),
                rtv_descriptor_heap.as_mut_void(),
            )) {
                info!("D3D12 render target view descriptor heap created.");
            } else {
                panic!("Failed to create D3D12 render target view descriptor heap.")
            }

            rtv_descriptor_heap.SetName(
                "AdamantRTVDescriptorHeap"
                    .encode_utf16()
                    .collect::<Vec<u16>>()
                    .as_ptr(),
            );
        }
        rtv_descriptor_heap
    }

    fn create_dsv_descriptor_heap(
        device: ComPtr<d3d12::ID3D12Device>,
    ) -> ComPtr<d3d12::ID3D12DescriptorHeap> {
        trace!("Creating D3D12 depth stencil view descriptor heap.");
        let mut dsv_descriptor_heap = ComPtr::<d3d12::ID3D12DescriptorHeap>::null();
        unsafe {
            let desc = d3d12::D3D12_DESCRIPTOR_HEAP_DESC {
                NumDescriptors: 1,
                Type: d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
                ..mem::zeroed()
            };

            if SUCCEEDED(device.CreateDescriptorHeap(
                &desc,
                &d3d12::ID3D12DescriptorHeap::uuidof(),
                dsv_descriptor_heap.as_mut_void(),
            )) {
                info!("D3D12 depth stencil view descriptor heap created.");
            } else {
                panic!("Failed to create D3D12 depth stencil view descriptor heap.")
            }

            dsv_descriptor_heap.SetName(
                "AdamantDSVDescriptorHeap"
                    .encode_utf16()
                    .collect::<Vec<u16>>()
                    .as_ptr(),
            );
        }
        dsv_descriptor_heap
    }

    fn create_command_allocators(
        device: ComPtr<d3d12::ID3D12Device>,
        back_buffer_count: u32,
    ) -> Vec<ComPtr<d3d12::ID3D12CommandAllocator>> {
        trace!(
            "Creating D3D12 command allocators for {} back buffers.",
            back_buffer_count
        );
        let mut command_allocators: Vec<ComPtr<d3d12::ID3D12CommandAllocator>> =
            Vec::with_capacity(2);
        unsafe {
            for n in 0..back_buffer_count {
                let mut command_allocator = ComPtr::<d3d12::ID3D12CommandAllocator>::null();
                if SUCCEEDED(device.CreateCommandAllocator(
                    d3d12::D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &d3d12::ID3D12CommandAllocator::uuidof(),
                    command_allocator.as_mut_void(),
                )) {
                    info!("D3D12 command allocator created for back buffer {}.", n);
                    command_allocator.SetName(
                        format!("AdamantCommandAllocator{}", n)
                            .encode_utf16()
                            .collect::<Vec<u16>>()
                            .as_ptr(),
                    );
                    command_allocators.push(command_allocator);
                } else {
                    panic!(
                        "Failed to create D3D12 command allocator for back buffer {}.",
                        n
                    );
                }
            }
        }
        command_allocators
    }

    fn create_command_list(
        device: ComPtr<d3d12::ID3D12Device>,
        command_allocator: ComPtr<d3d12::ID3D12CommandAllocator>,
    ) -> ComPtr<d3d12::ID3D12GraphicsCommandList> {
        trace!("Creating D3D12 command list.");
        let mut command_list = ComPtr::<d3d12::ID3D12GraphicsCommandList>::null();
        unsafe {
            if SUCCEEDED(device.CreateCommandList(
                0,
                d3d12::D3D12_COMMAND_LIST_TYPE_DIRECT,
                command_allocator.as_raw(),
                ptr::null_mut(),
                &d3d12::ID3D12GraphicsCommandList::uuidof(),
                command_list.as_mut_void(),
            )) {
                info!("D3D12 command list created.");
            } else {
                panic!("Failed to create D3D12 command list.")
            }

            if FAILED(command_list.Close()) {
                panic!("Failed to close D3D12 command list.")
            }

            command_list.SetName(
                "AdamantCommandList"
                    .encode_utf16()
                    .collect::<Vec<u16>>()
                    .as_ptr(),
            );
        }
        command_list
    }

    fn create_fence(device: ComPtr<d3d12::ID3D12Device>, value: u64) -> ComPtr<d3d12::ID3D12Fence> {
        trace!("Creating D3D12 fence.");
        let mut fence = ComPtr::<d3d12::ID3D12Fence>::null();
        unsafe {
            if SUCCEEDED(device.CreateFence(
                value,
                d3d12::D3D12_FENCE_FLAG_NONE,
                &d3d12::ID3D12Fence::uuidof(),
                fence.as_mut_void(),
            )) {
                info!("D3D12 fence created.")
            } else {
                panic!("Failed to create D3D12 fence.")
            }

            fence.SetName("AdamantFence".encode_utf16().collect::<Vec<u16>>().as_ptr());
        }
        fence
    }

    fn create_fence_event() -> winnt::HANDLE {
        unsafe {
            synchapi::CreateEventExW(
                ptr::null_mut(),
                ptr::null(),
                0,
                winnt::EVENT_MODIFY_STATE | winnt::SYNCHRONIZE,
            )
        }
    }

    fn no_srgb(format: dxgiformat::DXGI_FORMAT) -> dxgiformat::DXGI_FORMAT {
        match format {
            dxgiformat::DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => dxgiformat::DXGI_FORMAT_R8G8B8A8_UNORM,
            dxgiformat::DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => dxgiformat::DXGI_FORMAT_B8G8R8A8_UNORM,
            dxgiformat::DXGI_FORMAT_B8G8R8X8_UNORM_SRGB => dxgiformat::DXGI_FORMAT_B8G8R8X8_UNORM,
            _ => format,
        }
    }

    fn create_swap_chain(
        factory: ComPtr<dxgi1_4::IDXGIFactory4>,
        command_queue: ComPtr<d3d12::ID3D12CommandQueue>,
        window_handle: HWND,
        back_buffer_width: u32,
        back_buffer_height: u32,
        back_buffer_format: dxgiformat::DXGI_FORMAT,
        back_buffer_count: u32,
        is_tearing_allowed: bool,
    ) -> ComPtr<dxgi1_4::IDXGISwapChain3> {
        trace!("Creating D3D12 swap chain.");
        unsafe {
            let desc = dxgi1_2::DXGI_SWAP_CHAIN_DESC1 {
                Width: back_buffer_width,
                Height: back_buffer_height,
                Format: back_buffer_format,
                SampleDesc: dxgitype::DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: dxgitype::DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: back_buffer_count,
                Scaling: dxgi1_2::DXGI_SCALING_STRETCH,
                SwapEffect: dxgi::DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: dxgi1_2::DXGI_ALPHA_MODE_IGNORE,
                Flags: if is_tearing_allowed {
                    dxgi::DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING
                } else {
                    0
                },
                ..mem::zeroed()
            };
            let fullscreen_desc = dxgi1_2::DXGI_SWAP_CHAIN_FULLSCREEN_DESC {
                Windowed: minwindef::TRUE,
                ..mem::zeroed()
            };
            let mut swap_chain = ComPtr::<dxgi1_2::IDXGISwapChain1>::null();
            if SUCCEEDED(factory.CreateSwapChainForHwnd(
                command_queue.as_raw() as *mut _,
                window_handle,
                &desc,
                &fullscreen_desc,
                ptr::null_mut(),
                swap_chain.as_mut_void() as *mut *mut _ as *mut *mut _,
            )) {
                info!("D3D12 swapchain created.");
            } else {
                panic!("Failed to create D3D12 swapchain.");
            }
            if let Ok(swap_chain3) = swap_chain.cast::<dxgi1_4::IDXGISwapChain3>() {
                // Does not support exclusive full-screen mode and prevents DXGI from responding to the ALT+ENTER shortcut.
                let hr = factory.MakeWindowAssociation(window_handle, 1 << 1); // DXGI_MWA_NO_ALT_ENTER (can't find it in winit)
                if FAILED(hr) {
                    panic!("Failed to disable ALT+ENTER shortcut to go fullscreen.");
                }
                swap_chain.destroy();
                swap_chain3
            } else {
                panic!("Failed to create D3D12 swapchain.")
            }
        }
    }

    fn compute_color_space(
        swap_chain: ComPtr<dxgi1_4::IDXGISwapChain3>,
        back_buffer_format: dxgiformat::DXGI_FORMAT,
        flags: InitFlags,
    ) -> dxgitype::DXGI_COLOR_SPACE_TYPE {
        let mut is_hdr10_supported = false;
        let output = ComPtr::<dxgi::IDXGIOutput>::null();
        unsafe {
            if SUCCEEDED(swap_chain.GetContainingOutput(&mut output.as_raw())) {
                if let Ok(output6) = output.cast::<dxgi1_6::IDXGIOutput6>() {
                    let mut desc = dxgi1_6::DXGI_OUTPUT_DESC1 { ..mem::zeroed() };
                    if FAILED(output6.GetDesc1(&mut desc)) {
                        panic!("Failed to retrieve DXGI output description.");
                    }
                    output6.destroy();
                    if desc.ColorSpace == dxgitype::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 {
                        is_hdr10_supported = true;
                    }
                }
                output.destroy();
            }
        }

        let color_space = if flags.contains(InitFlags::ENABLE_HDR) && is_hdr10_supported {
            match back_buffer_format {
                // The application creates the HDR10 signal.
                dxgiformat::DXGI_FORMAT_R10G10B10A2_UNORM => {
                    dxgitype::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020
                }
                // The system creates the HDR10 signal; application uses linear values.
                dxgiformat::DXGI_FORMAT_R16G16B16A16_FLOAT => {
                    dxgitype::DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709
                }
                _ => dxgitype::DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
            }
        } else {
            dxgitype::DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709
        };

        let mut color_space_support = 0;
        unsafe {
            if SUCCEEDED(swap_chain.CheckColorSpaceSupport(color_space, &mut color_space_support))
                && (color_space_support & dxgi1_4::DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT)
                    != 0
                && FAILED(swap_chain.SetColorSpace1(color_space))
            {
                panic!("Failed to set swapchain's color space to support HDR.");
            }
        }
        color_space
    }

    fn create_render_targets(
        device: ComPtr<d3d12::ID3D12Device>,
        swap_chain: ComPtr<dxgi1_4::IDXGISwapChain3>,
        rtv_descriptor_heap: ComPtr<d3d12::ID3D12DescriptorHeap>,
        back_buffer_format: dxgiformat::DXGI_FORMAT,
        back_buffer_count: u32,
        rtv_descriptor_size: u32,
    ) -> Vec<ComPtr<d3d12::ID3D12Resource>> {
        trace!(
            "Creating D3D12 render target views for {} back buffers.",
            back_buffer_count
        );
        let mut render_targets = Vec::with_capacity(back_buffer_count as _);
        unsafe {
            for n in 0..back_buffer_count {
                let mut render_target = ComPtr::<d3d12::ID3D12Resource>::null();
                if SUCCEEDED(swap_chain.GetBuffer(
                    n,
                    &d3d12::ID3D12Resource::uuidof(),
                    render_target.as_mut_void(),
                )) {
                    info!("D3D12 render target view created for back buffer {}.", n);
                    render_target.SetName(
                        format!("AdamantRenderTarget{}", n)
                            .encode_utf16()
                            .collect::<Vec<u16>>()
                            .as_ptr(),
                    );
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
                let rtv_descriptor = d3d12::D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: rtv_descriptor_heap.GetCPUDescriptorHandleForHeapStart().ptr
                        + (rtv_descriptor_size * n) as usize,
                };
                device.CreateRenderTargetView(render_target.as_raw(), &rtv_desc, rtv_descriptor);

                render_targets.push(render_target);
            }
        }
        render_targets
    }

    fn create_depth_stencil(
        device: ComPtr<d3d12::ID3D12Device>,
        dsv_descriptor_heap: ComPtr<d3d12::ID3D12DescriptorHeap>,
        depth_buffer_format: dxgiformat::DXGI_FORMAT,
        back_buffer_width: u32,
        back_buffer_height: u32,
    ) -> ComPtr<d3d12::ID3D12Resource> {
        trace!("Creating D3D12 depth stencil buffer.");
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

        let mut depth_stencil = ComPtr::<d3d12::ID3D12Resource>::null();
        unsafe {
            let mut depth_optimized_clear_value = d3d12::D3D12_CLEAR_VALUE {
                Format: depth_buffer_format,
                ..mem::zeroed()
            };
            *depth_optimized_clear_value.u.DepthStencil_mut() = d3d12::D3D12_DEPTH_STENCIL_VALUE {
                Depth: 1.0,
                Stencil: 0,
            };
            if SUCCEEDED(device.CreateCommittedResource(
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

            (*depth_stencil).SetName(
                "AdamantDepthStencil"
                    .encode_utf16()
                    .collect::<Vec<u16>>()
                    .as_ptr(),
            );

            let dsv_desc = d3d12::D3D12_DEPTH_STENCIL_VIEW_DESC {
                Format: depth_buffer_format,
                ViewDimension: d3d12::D3D12_DSV_DIMENSION_TEXTURE2D,
                ..mem::zeroed()
            };
            device.CreateDepthStencilView(
                depth_stencil.as_raw(),
                &dsv_desc,
                dsv_descriptor_heap.GetCPUDescriptorHandleForHeapStart(),
            );
        }
        depth_stencil
    }

    fn get_rtv_descriptor(&self) -> d3d12::D3D12_CPU_DESCRIPTOR_HANDLE {
        unsafe {
            d3d12::D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: self
                    .rtv_descriptor_heap
                    .GetCPUDescriptorHandleForHeapStart()
                    .ptr
                    + (self.rtv_descriptor_size * self.back_buffer_index) as usize,
            }
        }
    }

    fn move_to_next_frame(&mut self) {
        let current_fence_value = self.fence_values[self.back_buffer_index as usize];
        unsafe {
            // Schedule a Signal command in the queue.
            if FAILED(
                self.command_queue
                    .Signal(self.fence.as_raw(), current_fence_value),
            ) {
                panic!("Failed to signal fence value.");
            }

            // Update the back buffer index.
            self.back_buffer_index = self.swap_chain.GetCurrentBackBufferIndex();

            // If the next frame is not ready to be rendered yet, wait until it is ready.
            if self.fence.GetCompletedValue() < self.fence_values[self.back_buffer_index as usize] {
                if FAILED(self.fence.SetEventOnCompletion(
                    self.fence_values[self.back_buffer_index as usize],
                    self.fence_event,
                )) {
                    panic!("Failed to set fence event on completion.");
                }
                synchapi::WaitForSingleObjectEx(
                    self.fence_event,
                    winbase::INFINITE,
                    minwindef::FALSE,
                );
            }
        }

        // Set the fence value for the next frame.
        self.fence_values[self.back_buffer_index as usize] = current_fence_value + 1;
    }

    fn update_window_size_dependent_resources(&mut self) {
        // Wait until all previous GPU work is complete.
        self.wait_for_gpu();

        // Release resources that are tied to the swap chain and update fence values.
        for n in 0..self.back_buffer_count {
            unsafe {
                self.render_targets[n as usize].destroy();
            }
            self.fence_values[n as usize] = self.fence_values[self.back_buffer_index as usize];
        }
        self.render_targets.clear();

        // Resize swap chain.
        unsafe {
            let hr = self.swap_chain.ResizeBuffers(
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
            if FAILED(hr) {
                panic!("Failed to resize resources on window size changed.");
            }
        }

        // Handle HDR output
        self.color_space =
            Self::compute_color_space(self.swap_chain, self.back_buffer_format, self.flags);

        // Create render targets for each back buffer.
        self.render_targets = Self::create_render_targets(
            self.device,
            self.swap_chain,
            self.rtv_descriptor_heap,
            self.back_buffer_format,
            self.back_buffer_count,
            self.rtv_descriptor_size,
        );

        self.back_buffer_index = unsafe { self.swap_chain.GetCurrentBackBufferIndex() };

        // Allocate a 2-D surface as the depth/stencil buffer and create a depth/stencil view on this surface.
        unsafe {
            self.depth_stencil.destroy();
        }
        self.depth_stencil = Self::create_depth_stencil(
            self.device,
            self.dsv_descriptor_heap,
            self.depth_buffer_format,
            self.back_buffer_width.try_into().unwrap(),
            self.back_buffer_height.try_into().unwrap(),
        );

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

    fn wait_for_gpu(&mut self) {
        let fence_value = self.fence_values[self.back_buffer_index as usize];
        unsafe {
            // Schedule a Signal command in the GPU queue.
            if SUCCEEDED(self.command_queue.Signal(self.fence.as_raw(), fence_value)) {
                // Wait until the Signal has been processed.
                if SUCCEEDED(
                    self.fence
                        .SetEventOnCompletion(fence_value, self.fence_event),
                ) {
                    synchapi::WaitForSingleObjectEx(
                        self.fence_event,
                        winbase::INFINITE,
                        minwindef::FALSE,
                    );

                    // Increment the fence value for the current frame.
                    self.fence_values[self.back_buffer_index as usize] += 1;
                }
            }
        }
    }
}

impl Drop for D3D12Layer {
    fn drop(&mut self) {
        unsafe {
            // Ensure that the GPU is no longer referencing resources that are about to be destroyed.
            self.wait_for_gpu();

            // Destroy resources in reverse order
            self.depth_stencil.destroy();
            for render_target in self.render_targets.iter() {
                render_target.destroy();
            }
            self.render_targets.clear();

            self.swap_chain.destroy();
            self.fence.destroy();
            self.command_list.destroy();

            for command_allocator in self.command_allocators.iter() {
                command_allocator.destroy();
            }
            self.command_allocators.clear();

            self.dsv_descriptor_heap.destroy();
            self.rtv_descriptor_heap.destroy();

            self.command_queue.destroy();

            #[cfg(debug_assertions)]
            {
                // Debug tracking alive device objects
                if let Ok(debug_device) = self.device.cast::<d3d12sdklayers::ID3D12DebugDevice>() {
                    debug_device.ReportLiveDeviceObjects(
                        d3d12sdklayers::D3D12_RLDO_DETAIL
                            | d3d12sdklayers::D3D12_RLDO_IGNORE_INTERNAL,
                    );
                    debug_device.destroy();
                }
            }

            self.device.destroy();
            self.factory.destroy();

            #[cfg(debug_assertions)]
            {
                // Debug tracking alive dxgi objects
                let mut dxgi_debug = ComPtr::<dxgidebug::IDXGIDebug1>::null();
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
                    dxgi_debug.destroy();
                }
            }
        }
    }
}
