use crate::graphics::com::ComPtr;
use crate::graphics::command::CommandQueue;

use winapi::ctypes::c_void;
use winapi::shared::{
    dxgi, dxgi1_2, dxgi1_3, dxgi1_4, dxgi1_5, dxgi1_6, dxgiformat, dxgitype, minwindef,
    windef::HWND,
    winerror::{FAILED, SUCCEEDED},
};
use winapi::um::{d3d12, d3dcommon};
use winapi::Interface;

use std::ffi::OsString;
use std::mem;
use std::os::windows::ffi::OsStringExt;
use std::ptr;

#[derive(Debug)]
pub enum Error {
    FactoryCreateFailed,
    FactoryDisableExclusiveFullscreenFailed,
    FactoryCheckFeatureSupportFailed,
    FactoryCastFailed,
    AdapterCreateFailed,
    AdapterCastFailed,
    SwapchainCreateFailed,
    SwapchainCastFailed,
}

pub struct Factory {
    pub(crate) native: ComPtr<dxgi1_4::IDXGIFactory4>,
    window_handle: HWND,
}

impl Factory {
    pub fn new(window_handle: HWND, flags: u32) -> Result<Self, Error> {
        let mut factory = ComPtr::<dxgi1_4::IDXGIFactory4>::empty();
        let hr = unsafe {
            dxgi1_3::CreateDXGIFactory2(
                flags,
                &dxgi1_4::IDXGIFactory4::uuidof(),
                factory.as_mut_void(),
            )
        };
        if SUCCEEDED(hr) {
            Ok(Factory {
                native: factory,
                window_handle,
            })
        } else {
            Err(Error::FactoryCreateFailed)
        }
    }

    pub fn disable_exclusive_fullscreen(&self) -> Result<(), Error> {
        // Does not support exclusive full-screen mode and prevents DXGI from responding to the ALT+ENTER shortcut.
        const DXGI_MWA_NO_ALT_ENTER: u32 = 1 << 1; // DXGI_MWA_NO_ALT_ENTER (can't find it in winit, should be in dxgi.h)
        let hr = unsafe {
            self.native
                .MakeWindowAssociation(self.window_handle, DXGI_MWA_NO_ALT_ENTER)
        };
        if SUCCEEDED(hr) {
            Ok(())
        } else {
            Err(Error::FactoryDisableExclusiveFullscreenFailed)
        }
    }

    pub fn check_feature_support(
        &self,
        feature_type: dxgi1_5::DXGI_FEATURE,
        feature_data: *mut c_void,
        feature_size: usize,
    ) -> Result<(), Error> {
        unsafe {
            match self.native.cast::<dxgi1_5::IDXGIFactory5>() {
                Ok(factory5) => {
                    let hr =
                        factory5.CheckFeatureSupport(feature_type, feature_data, feature_size as _);
                    if SUCCEEDED(hr) {
                        Ok(())
                    } else {
                        Err(Error::FactoryCheckFeatureSupportFailed)
                    }
                }
                Err(_) => Err(Error::FactoryCastFailed),
            }
        }
    }

    pub fn enum_adapter_by_gpu_preference(
        &self,
        min_feature_level: d3dcommon::D3D_FEATURE_LEVEL,
    ) -> Result<ComPtr<dxgi::IDXGIAdapter1>, Error> {
        let mut adapter = ComPtr::<dxgi::IDXGIAdapter1>::empty();
        // Pretty much all unsafe here.
        unsafe {
            match self.native.cast::<dxgi1_6::IDXGIFactory6>() {
                Ok(factory6) => {
                    let mut index = 0;
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
                                continue;
                            }

                            // Skip the Basic Render Driver adapter.
                            if (desc.Flags & dxgi::DXGI_ADAPTER_FLAG_SOFTWARE) != 0 {
                                continue;
                            }

                            if SUCCEEDED(d3d12::D3D12CreateDevice(
                                adapter.as_ptr_mut() as _,
                                min_feature_level,
                                &d3d12::ID3D12Device::uuidof(),
                                ptr::null_mut(),
                            )) {
                                break;
                            }
                        }
                    }
                    if !adapter.is_null() {
                        Ok(adapter)
                    } else {
                        Err(Error::AdapterCreateFailed)
                    }
                }
                Err(_) => Err(Error::FactoryCastFailed),
            }
        }
    }

    pub fn enum_adapter(
        &self,
        min_feature_level: d3dcommon::D3D_FEATURE_LEVEL,
    ) -> Result<ComPtr<dxgi::IDXGIAdapter1>, Error> {
        // Find the adapter with the largest dedicated video memory.
        let mut adapter = ComPtr::<dxgi::IDXGIAdapter1>::empty();
        let mut max_dedicated_video_memeory: usize = 0;
        let mut adapter_index = 0;
        let mut index = 0;
        unsafe {
            while SUCCEEDED(
                self.native
                    .EnumAdapters1(index, adapter.as_mut_void() as *mut *mut _),
            ) {
                index += 1;

                let mut desc = dxgi::DXGI_ADAPTER_DESC1 { ..mem::zeroed() };
                let hr = adapter.GetDesc1(&mut desc);
                if FAILED(hr) {
                    continue;
                }

                // Skip the Basic Render Driver adapter.
                if (desc.Flags & dxgi::DXGI_ADAPTER_FLAG_SOFTWARE) != 0 {
                    continue;
                }

                if SUCCEEDED(d3d12::D3D12CreateDevice(
                    adapter.as_ptr_mut() as _,
                    min_feature_level,
                    &d3d12::ID3D12Device::uuidof(),
                    ptr::null_mut(),
                )) && desc.DedicatedVideoMemory > max_dedicated_video_memeory
                {
                    max_dedicated_video_memeory = desc.DedicatedVideoMemory;
                    adapter_index = index - 1;
                }
            }

            // Need to retrieve the adapter again as it would have been reset with the last EnumAdapter1 call
            if max_dedicated_video_memeory > 0
                && SUCCEEDED(
                    self.native
                        .EnumAdapters1(adapter_index, adapter.as_mut_void() as *mut *mut _),
                )
            {
                Ok(adapter)
            } else {
                Err(Error::AdapterCreateFailed)
            }
        }
    }

    pub fn enum_adapter_warp(&self) -> Result<ComPtr<dxgi::IDXGIAdapter1>, Error> {
        let mut adapter = ComPtr::<dxgi::IDXGIAdapter1>::empty();
        let hr = unsafe {
            self.native
                .EnumWarpAdapter(&dxgi::IDXGIAdapter1::uuidof(), adapter.as_mut_void())
        };
        if SUCCEEDED(hr) {
            Ok(adapter)
        } else {
            Err(Error::AdapterCreateFailed)
        }
    }
}

pub struct Adapter(pub(crate) ComPtr<dxgi1_6::IDXGIAdapter4>);

impl Adapter {
    pub fn new(
        factory: &Factory,
        min_feature_level: d3dcommon::D3D_FEATURE_LEVEL,
        use_warp_adapter: bool,
    ) -> Result<Self, Error> {
        let adapter = if use_warp_adapter {
            factory.enum_adapter_warp()
        } else {
            factory
                .enum_adapter_by_gpu_preference(min_feature_level)
                .or_else(|_| factory.enum_adapter(min_feature_level))
                .or_else(|_| factory.enum_adapter_warp())
        };

        unsafe {
            adapter.and_then(|adapter| match adapter.cast::<dxgi1_6::IDXGIAdapter4>() {
                Ok(adapter4) => {
                    let mut desc = dxgi::DXGI_ADAPTER_DESC1 { ..mem::zeroed() };
                    let hr = adapter4.GetDesc1(&mut desc);
                    if FAILED(hr) {
                        // This should never happen
                        panic!("Failed to get adapter description.");
                    }
                    let device_name = {
                        let len = desc.Description.iter().take_while(|&&c| c != 0).count();
                        let name = <OsString as OsStringExt>::from_wide(&desc.Description[..len]);
                        name.to_string_lossy().into_owned()
                    };
                    println!(
                        "Found D3D12 adapter '{}' with {}MB of dedicated video memory.",
                        device_name,
                        desc.DedicatedVideoMemory / 1000 / 1000
                    );

                    Ok(Adapter(adapter4))
                }
                Err(_) => Err(Error::AdapterCastFailed),
            })
        }
    }
}

pub struct SwapchainProperties {
    pub window_handle: HWND,
    pub back_buffer_count: u32,
    pub back_buffer_width: u32,
    pub back_buffer_height: u32,
    pub back_buffer_format: dxgiformat::DXGI_FORMAT,
    pub is_tearing_supported: bool,
}

pub struct Swapchain(pub(crate) ComPtr<dxgi1_5::IDXGISwapChain4>);

impl Swapchain {
    pub fn new(
        factory: &Factory,
        command_queue: &CommandQueue,
        properties: SwapchainProperties,
    ) -> Result<Self, Error> {
        unsafe {
            let desc = dxgi1_2::DXGI_SWAP_CHAIN_DESC1 {
                Width: properties.back_buffer_width,
                Height: properties.back_buffer_height,
                Format: properties.back_buffer_format,
                Stereo: minwindef::FALSE,
                SampleDesc: dxgitype::DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: dxgitype::DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: properties.back_buffer_count,
                Scaling: dxgi1_2::DXGI_SCALING_STRETCH,
                SwapEffect: dxgi::DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: dxgi1_2::DXGI_ALPHA_MODE_UNSPECIFIED,
                Flags: if properties.is_tearing_supported {
                    dxgi::DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING
                } else {
                    0
                },
            };
            let fullscreen_desc = dxgi1_2::DXGI_SWAP_CHAIN_FULLSCREEN_DESC {
                Windowed: minwindef::TRUE,
                ..mem::zeroed()
            };
            let mut swapchain = ComPtr::<dxgi1_2::IDXGISwapChain1>::empty();
            if FAILED(factory.native.CreateSwapChainForHwnd(
                command_queue.native.as_ptr_mut() as *mut _,
                properties.window_handle,
                &desc,
                &fullscreen_desc,
                ptr::null_mut(),
                swapchain.as_mut_void() as *mut *mut _ as *mut *mut _,
            )) {
                return Err(Error::SwapchainCreateFailed);
            }
            if let Ok(swapchain4) = swapchain.cast::<dxgi1_5::IDXGISwapChain4>() {
                Ok(Swapchain(swapchain4))
            } else {
                Err(Error::SwapchainCastFailed)
            }
        }
    }

    pub fn compute_color_space(
        &self,
        back_buffer_format: dxgiformat::DXGI_FORMAT,
        is_hdr_enabled: bool,
    ) -> dxgitype::DXGI_COLOR_SPACE_TYPE {
        let mut is_hdr10_supported = false;
        let output = ComPtr::<dxgi::IDXGIOutput>::empty();
        unsafe {
            if SUCCEEDED(self.0.GetContainingOutput(&mut output.as_ptr_mut())) {
                if let Ok(output6) = output.cast::<dxgi1_6::IDXGIOutput6>() {
                    let mut desc = dxgi1_6::DXGI_OUTPUT_DESC1 { ..mem::zeroed() };
                    if FAILED(output6.GetDesc1(&mut desc)) {
                        panic!("Failed to retrieve DXGI output description.");
                    }
                    if desc.ColorSpace == dxgitype::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 {
                        is_hdr10_supported = true;
                    }
                }
            }
        }

        let color_space = if is_hdr_enabled && is_hdr10_supported {
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
            if SUCCEEDED(
                self.0
                    .CheckColorSpaceSupport(color_space, &mut color_space_support),
            ) && (color_space_support
                & dxgi1_4::DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT)
                != 0
                && FAILED(self.0.SetColorSpace1(color_space))
            {
                panic!("Failed to set swapchain's color space to support HDR.");
            }
        }
        color_space
    }

    pub fn get_current_back_buffer_index(&self) -> u32 {
        unsafe { self.0.GetCurrentBackBufferIndex() }
    }
}
