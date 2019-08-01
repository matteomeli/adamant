mod com;

pub use com::ComPtr;

use bitflags::bitflags;

use log::{error, info, trace, warn};

use winapi::shared::winerror::{FAILED, SUCCEEDED};
use winapi::shared::{dxgi, dxgi1_3, dxgi1_4, dxgi1_5, dxgi1_6, minwindef};
use winapi::um::{d3d12, d3d12sdklayers, d3dcommon, dxgidebug, synchapi, winnt};
use winapi::Interface;

use std::ffi::OsString;
use std::mem;
use std::os::windows::ffi::OsStringExt;
use std::ptr;

bitflags! {
    pub struct InitFlags: u32 {
        const ALLOW_TEARING = 0b0000_0001;
        const ENABLE_HDR = 0b0000_0010;
    }
}

pub fn init_d3d12(flags: InitFlags) {
    // Enable debug layer
    let factory_flags = enable_debug_layer();

    // Create DXGI factory
    let factory = create_factory(factory_flags);

    // Determine if tearing is supported for fullscreen borderless windows
    let mut _allow_tearing = false;
    if flags.contains(InitFlags::ALLOW_TEARING) {
        trace!("Checking variable refresh rate display support");
        unsafe {
            if let Ok(factory5) = factory.cast::<dxgi1_5::IDXGIFactory5>() {
                let mut allow_tearing_feature = minwindef::FALSE;
                let hr = factory5.CheckFeatureSupport(
                    dxgi1_5::DXGI_FEATURE_PRESENT_ALLOW_TEARING,
                    &mut allow_tearing_feature as *mut _ as *mut _,
                    mem::size_of::<minwindef::BOOL>() as _,
                );
                if SUCCEEDED(hr) && allow_tearing_feature == minwindef::TRUE {
                    _allow_tearing = true;
                }
            }
        }
        if _allow_tearing {
            info!("Variable refresh rate displays supported");
        } else {
            warn!("Variable refresh rate displays not supported");
        }
    }

    let min_feature_level = d3dcommon::D3D_FEATURE_LEVEL_11_0;

    // Get adapter
    let adapter = get_adapter(&factory, min_feature_level);

    // Create D3D12 API device
    let device = create_device(&adapter, min_feature_level);

    // Configure debug device
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
            }
        }
    }

    // Determine maximum feature level supported for the obtained device
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
    let _feature_level = unsafe {
        if SUCCEEDED(device.CheckFeatureSupport(
            d3d12::D3D12_FEATURE_FEATURE_LEVELS,
            &mut feature_levels as *mut _ as *mut _,
            mem::size_of::<d3d12::D3D12_FEATURE_DATA_FEATURE_LEVELS>() as _,
        )) {
            feature_levels.MaxSupportedFeatureLevel
        } else {
            min_feature_level
        }
    };

    // Create command queue
    let _command_queue = create_command_queue(&device);

    // The number of back buffers in the swap chain
    let back_buffer_count = 2;

    // Create descriptor heaps for render target and depth stencil views
    let _rtv_descriptor_heap = create_rtv_descriptor_heap(&device, back_buffer_count);
    let _rtv_descriptor_size =
        unsafe { device.GetDescriptorHandleIncrementSize(d3d12::D3D12_DESCRIPTOR_HEAP_TYPE_RTV) };
    let _dsv_descriptor_heap = create_dsv_descriptor_heap(&device);

    // Create a command allocator for each back buffer that will be rendered to
    let command_allocators = create_command_allocators(&device, back_buffer_count);

    // Create a command list for recording graphics commands
    let _command_list = create_command_list(&device, &command_allocators[0]);

    // Create fence for syncing CPU and GPU execution processes
    const MAX_BACK_BUFFER_COUNT: usize = 3;
    let mut _back_buffer_index = 0;
    let fence_values: [u64; MAX_BACK_BUFFER_COUNT] = [0, 0, 0];
    let (_fence, _fence_event) = create_fence(&device, fence_values[0]);
}

fn enable_debug_layer() -> u32 {
    let mut dxgi_factory_flags = 0;
    #[cfg(debug_assertions)]
    {
        trace!("Enabling D3D12 debug device");
        let mut debug_controller = ComPtr::<d3d12sdklayers::ID3D12Debug>::null();
        unsafe {
            if SUCCEEDED(d3d12::D3D12GetDebugInterface(
                &d3d12sdklayers::ID3D12Debug::uuidof(),
                debug_controller.as_mut_void(),
            )) {
                info!("D3D12 debug device enabled");
                debug_controller.EnableDebugLayer();
            } else {
                warn!("D3D12 debug device is not available");
            }
        }

        trace!("Enabling DXGI info queue");
        let mut info_queue = ComPtr::<dxgidebug::IDXGIInfoQueue>::null();
        unsafe {
            if SUCCEEDED(dxgi1_3::DXGIGetDebugInterface1(
                0,
                &dxgidebug::IDXGIInfoQueue::uuidof(),
                info_queue.as_mut_void(),
            )) {
                info!("DXGI info queue enabled");
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
                warn!("DXGI info queue is not available");
            }
        }
    }
    dxgi_factory_flags
}

fn create_factory(flags: u32) -> ComPtr<dxgi1_4::IDXGIFactory4> {
    trace!("Creating DXGI factory");
    let mut factory = ComPtr::<dxgi1_4::IDXGIFactory4>::null();
    unsafe {
        if SUCCEEDED(dxgi1_3::CreateDXGIFactory2(
            flags,
            &dxgi1_4::IDXGIFactory4::uuidof(),
            factory.as_mut_void(),
        )) {
            info!("DXGI factory created");
        } else {
            panic!("Failed to create DXGI factory");
        }
    }
    factory
}

fn get_adapter(
    factory: &ComPtr<dxgi1_4::IDXGIFactory4>,
    min_feature_level: u32,
) -> ComPtr<dxgi::IDXGIAdapter1> {
    trace!("Searching for D3D12 adapter");
    let mut adapter = ComPtr::<dxgi::IDXGIAdapter1>::null();
    unsafe {
        // Pretty much all unsafe here
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
                        error!("Failed to get adapter description");
                        panic!();
                    }

                    // Skip the Basic Render Driver adapter.
                    if (desc.Flags & dxgi::DXGI_ADAPTER_FLAG_SOFTWARE) != 0 {
                        continue;
                    }

                    if SUCCEEDED(d3d12::D3D12CreateDevice(
                        adapter.as_raw() as _,
                        min_feature_level,
                        &d3d12::ID3D12Device::uuidof(),
                        ptr::null_mut(),
                    )) {
                        break;
                    }
                }
            }
        } else {
            // Find the adapter with the largest dedicated video memory
            let mut current_adapter = ComPtr::<dxgi::IDXGIAdapter1>::null();
            let mut index = 0;
            let mut max_dedicated_video_memeory_found: usize = 0;
            while SUCCEEDED(
                factory.EnumAdapters1(index, current_adapter.as_mut_void() as *mut *mut _),
            ) {
                index += 1;

                let mut desc = dxgi::DXGI_ADAPTER_DESC1 { ..mem::zeroed() };
                let hr = current_adapter.GetDesc1(&mut desc);
                if FAILED(hr) {
                    error!("Failed to get adapter description");
                    panic!();
                }

                // Skip the Basic Render Driver adapter.
                if (desc.Flags & dxgi::DXGI_ADAPTER_FLAG_SOFTWARE) != 0 {
                    continue;
                }

                if SUCCEEDED(d3d12::D3D12CreateDevice(
                    current_adapter.as_raw() as _,
                    min_feature_level,
                    &d3d12::ID3D12Device::uuidof(),
                    ptr::null_mut(),
                )) && desc.DedicatedVideoMemory > max_dedicated_video_memeory_found
                {
                    max_dedicated_video_memeory_found = desc.DedicatedVideoMemory;
                    adapter = current_adapter.clone();
                }
            }
        }

        #[cfg(debug_assertions)]
        {
            if adapter.is_null()
                && FAILED(
                    factory.EnumWarpAdapter(&dxgi::IDXGIAdapter1::uuidof(), adapter.as_mut_void()),
                )
            {
                panic!("Failed to create the WARP adapter");
            }
        }
    }

    if adapter.is_null() {
        panic!("No D3D12 adapter found");
    }

    unsafe {
        let mut desc = dxgi::DXGI_ADAPTER_DESC1 { ..mem::zeroed() };
        let hr = adapter.GetDesc1(&mut desc);
        if FAILED(hr) {
            panic!("Failed to get adapter description");
        }
        let device_name = {
            let len = desc.Description.iter().take_while(|&&c| c != 0).count();
            let name = <OsString as OsStringExt>::from_wide(&desc.Description[..len]);
            name.to_string_lossy().into_owned()
        };
        info!(
            "Found D3D12 adapter '{}' with {}MB of dedicated video memory",
            device_name,
            desc.DedicatedVideoMemory / 1000 / 1000
        );
    }
    adapter
}

fn create_device(
    adapter: &ComPtr<dxgi::IDXGIAdapter1>,
    min_feature_level: u32,
) -> ComPtr<d3d12::ID3D12Device> {
    trace!("Creating D3D12 device");
    let mut device = ComPtr::<d3d12::ID3D12Device>::null();
    unsafe {
        if SUCCEEDED(d3d12::D3D12CreateDevice(
            adapter.as_raw() as _,
            min_feature_level,
            &d3d12::ID3D12Device::uuidof(),
            device.as_mut_void(),
        )) {
            info!("D3D12 device created");
        } else {
            panic!("Failed to create D3D12 device");
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

fn create_command_queue(device: &ComPtr<d3d12::ID3D12Device>) -> ComPtr<d3d12::ID3D12CommandQueue> {
    trace!("Creating D3D12 command queue");
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
            info!("D3D12 command queue created");
        } else {
            panic!("Failed to create D3D12 command queue");
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
    device: &ComPtr<d3d12::ID3D12Device>,
    back_buffer_count: u32,
) -> ComPtr<d3d12::ID3D12DescriptorHeap> {
    trace!("Creating D3D12 render target view descriptor heap");
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
            info!("D3D12 render target view descriptor heap created");
        } else {
            panic!("Failed to create D3D12 render target view descriptor heap")
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
    device: &ComPtr<d3d12::ID3D12Device>,
) -> ComPtr<d3d12::ID3D12DescriptorHeap> {
    trace!("Creating D3D12 depth stencil view descriptor heap");
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
            info!("D3D12 depth stencil view descriptor heap created");
        } else {
            panic!("Failed to create D3D12 depth stencil view descriptor heap")
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
    device: &ComPtr<d3d12::ID3D12Device>,
    back_buffer_count: u32,
) -> Vec<ComPtr<d3d12::ID3D12CommandAllocator>> {
    trace!(
        "Creating D3D12 command allocators for {} back buffers",
        back_buffer_count
    );
    let mut command_allocators: Vec<ComPtr<d3d12::ID3D12CommandAllocator>> = Vec::with_capacity(2);
    unsafe {
        for n in 0..back_buffer_count {
            let mut command_allocator = ComPtr::<d3d12::ID3D12CommandAllocator>::null();
            if SUCCEEDED(device.CreateCommandAllocator(
                d3d12::D3D12_COMMAND_LIST_TYPE_DIRECT,
                &d3d12::ID3D12CommandAllocator::uuidof(),
                command_allocator.as_mut_void(),
            )) {
                info!("D3D12 command allocator created for back buffer {}", n);
                command_allocator.SetName(
                    format!("AdamantRenderTarget{}", n)
                        .encode_utf16()
                        .collect::<Vec<u16>>()
                        .as_ptr(),
                );
                command_allocators.push(command_allocator);
            } else {
                panic!(
                    "Failed to create D3D12 command allocator for back buffer {}",
                    n
                );
            }
        }
    }
    command_allocators
}

fn create_command_list(
    device: &ComPtr<d3d12::ID3D12Device>,
    command_allocator: &ComPtr<d3d12::ID3D12CommandAllocator>,
) -> ComPtr<d3d12::ID3D12GraphicsCommandList> {
    trace!("Creating D3D12 command list");
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
            info!("D3D12 command list created");
        } else {
            panic!("Failed to create D3D12 command list")
        }

        if FAILED(command_list.Close()) {
            panic!("Failed to close D3D12 command list")
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

fn create_fence(
    device: &ComPtr<d3d12::ID3D12Device>,
    value: u64,
) -> (ComPtr<d3d12::ID3D12Fence>, winnt::HANDLE) {
    trace!("Creating D3D12 fence");
    let mut fence = ComPtr::<d3d12::ID3D12Fence>::null();
    unsafe {
        if SUCCEEDED(device.CreateFence(
            value,
            d3d12::D3D12_FENCE_FLAG_NONE,
            &d3d12::ID3D12Fence::uuidof(),
            fence.as_mut_void(),
        )) {
            info!("D3D12 fence created")
        } else {
            panic!("Failed to create D3D12 fence")
        }

        fence.SetName("AdamantFence".encode_utf16().collect::<Vec<u16>>().as_ptr());
    }
    let fence_event: winnt::HANDLE = unsafe {
        synchapi::CreateEventExW(
            ptr::null_mut(),
            ptr::null(),
            0,
            winnt::EVENT_MODIFY_STATE | winnt::SYNCHRONIZE,
        )
    };
    (fence, fence_event)
}
