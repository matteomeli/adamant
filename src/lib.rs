mod com;

pub use com::ComPtr;

use bitflags::bitflags;

use log::{error, info, trace, warn};

use winapi::shared::winerror::{FAILED, SUCCEEDED};
use winapi::shared::{dxgi, dxgi1_3, dxgi1_4, dxgi1_5, dxgi1_6, minwindef};
use winapi::um::{d3d12, d3d12sdklayers, d3dcommon, dxgidebug};
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
    let mut dxgi_factory_flags = 0;

    // Enable debug layer
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

    // Create DXGI factory
    trace!("Creating DXGI factory");
    let mut factory = ComPtr::<dxgi1_4::IDXGIFactory4>::null();
    unsafe {
        if SUCCEEDED(dxgi1_3::CreateDXGIFactory2(
            dxgi_factory_flags,
            &dxgi1_4::IDXGIFactory4::uuidof(),
            factory.as_mut_void(),
        )) {
            info!("DXGI factory created");
        } else {
            error!("Failed to create DXGI factory");
            panic!();
        }
    }

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

    // Get adapter
    trace!("Searching for D3D12 adapter");
    let adapter = get_adapter(&factory);
    unsafe {
        let mut desc = dxgi::DXGI_ADAPTER_DESC1 { ..mem::zeroed() };
        let hr = adapter.GetDesc1(&mut desc);
        if FAILED(hr) {
            error!("Failed to get adapter description");
            panic!();
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

    // Create D3D12 API device
    trace!("Creating D3D12 device");
    let device = create_device(&adapter);
    unsafe {
        device.SetName(
            "AdamantDevice"
                .encode_utf16()
                .collect::<Vec<u16>>()
                .as_ptr(),
        );
    }
}

fn get_adapter(factory: &ComPtr<dxgi1_4::IDXGIFactory4>) -> ComPtr<dxgi::IDXGIAdapter1> {
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
                        d3dcommon::D3D_FEATURE_LEVEL_11_0,
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
                    d3dcommon::D3D_FEATURE_LEVEL_11_0,
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
                error!("Failed to create the WARP adapter. ");
                panic!();
            }
        }
    }

    if adapter.is_null() {
        error!("No D3D12 adapter found");
        panic!();
    }
    adapter
}

fn create_device(adapter: &ComPtr<dxgi::IDXGIAdapter1>) -> ComPtr<d3d12::ID3D12Device> {
    let mut device = ComPtr::<d3d12::ID3D12Device>::null();
    unsafe {
        if SUCCEEDED(d3d12::D3D12CreateDevice(
            adapter.as_raw() as _,
            d3dcommon::D3D_FEATURE_LEVEL_11_0,
            &d3d12::ID3D12Device::uuidof(),
            device.as_mut_void(),
        )) {
            info!("D3D12 device created");
        } else {
            error!("Failed to create D3D12 device");
            panic!();
        }
    }
    device
}
