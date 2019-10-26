use crate::graphics::com::ComPtr;
use crate::graphics::dxgi::Adapter;

use winapi::shared::{
    minwindef,
    winerror::{FAILED, SUCCEEDED},
};
use winapi::um::{d3d12, d3d12sdklayers, d3dcommon};
use winapi::Interface;

use std::mem;

#[derive(Debug)]
pub enum Error {
    DeviceCreateFailed,
    DeviceSetNameFailed,
}

#[derive(Clone)]
pub struct Device {
    pub(crate) native: ComPtr<d3d12::ID3D12Device>,
    feature_level: d3dcommon::D3D_FEATURE_LEVEL,
}

impl Device {
    pub fn new(
        adapter: &Adapter,
        min_feature_level: d3dcommon::D3D_FEATURE_LEVEL,
    ) -> Result<Self, Error> {
        let mut device = ComPtr::<d3d12::ID3D12Device>::empty();
        let mut hr = unsafe {
            d3d12::D3D12CreateDevice(
                adapter.0.as_ptr_mut() as _,
                min_feature_level,
                &d3d12::ID3D12Device::uuidof(),
                device.as_mut_void(),
            )
        };

        if FAILED(hr) {
            return Err(Error::DeviceCreateFailed);
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
                min_feature_level
            }
        };

        // Configure device for debugging.
        #[cfg(debug_assertions)]
        {
            Self::configure_debug_device(&device);

            hr = unsafe {
                device.SetName(
                    "Adamant::Device"
                        .encode_utf16()
                        .collect::<Vec<u16>>()
                        .as_ptr(),
                )
            };
            if FAILED(hr) {
                return Err(Error::DeviceSetNameFailed);
            }
        }

        Ok(Device {
            native: device,
            feature_level,
        })
    }

    fn configure_debug_device(device: &ComPtr<d3d12::ID3D12Device>) {
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

                let mut severities: Vec<d3d12sdklayers::D3D12_MESSAGE_SEVERITY> =
                    vec![d3d12sdklayers::D3D12_MESSAGE_SEVERITY_INFO];

                let mut deny_ids: Vec<d3d12sdklayers::D3D12_MESSAGE_ID> = vec![
                    d3d12sdklayers::D3D12_MESSAGE_ID_EXECUTECOMMANDLISTS_WRONGSWAPCHAINBUFFERREFERENCE,
                    d3d12sdklayers::D3D12_MESSAGE_ID_CLEARRENDERTARGETVIEW_MISMATCHINGCLEARVALUE,
                    d3d12sdklayers::D3D12_MESSAGE_ID_MAP_INVALID_NULLRANGE,
                    d3d12sdklayers::D3D12_MESSAGE_ID_UNMAP_INVALID_NULLRANGE,
                    d3d12sdklayers::D3D12_MESSAGE_ID_INVALID_DESCRIPTOR_HANDLE,
                    d3d12sdklayers::D3D12_MESSAGE_ID_CREATEGRAPHICSPIPELINESTATE_PS_OUTPUT_RT_OUTPUT_MISMATCH,
                    d3d12sdklayers::D3D12_MESSAGE_ID_COMMAND_LIST_DESCRIPTOR_TABLE_NOT_SET,
                    1008,
                ];
                let mut filter = d3d12sdklayers::D3D12_INFO_QUEUE_FILTER {
                    DenyList: d3d12sdklayers::D3D12_INFO_QUEUE_FILTER_DESC {
                        pSeverityList: severities.as_mut_ptr(),
                        NumSeverities: severities.len() as _,
                        NumIDs: deny_ids.len() as _,
                        pIDList: deny_ids.as_mut_ptr(),
                        ..mem::zeroed()
                    },
                    ..mem::zeroed()
                };
                info_queue.AddStorageFilterEntries(&mut filter);
            }
        }
    }
}
