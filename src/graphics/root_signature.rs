use crate::graphics::com::ComPtr;
use crate::graphics::{Blob, Device};

use winapi::shared::winerror::FAILED;
use winapi::um::d3d12;
use winapi::Interface;

use std::mem;

#[repr(transparent)]
pub struct DescriptorRange(d3d12::D3D12_DESCRIPTOR_RANGE);

#[repr(transparent)]
pub struct RootParameter(d3d12::D3D12_ROOT_PARAMETER);
impl RootParameter {
    pub fn new_descriptor_table(
        visibility: d3d12::D3D12_SHADER_VISIBILITY,
        ranges: &[DescriptorRange],
    ) -> Self {
        let mut parameter = unsafe {
            d3d12::D3D12_ROOT_PARAMETER {
                ParameterType: d3d12::D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                ShaderVisibility: visibility,
                ..mem::zeroed()
            }
        };

        *unsafe { parameter.u.DescriptorTable_mut() } = d3d12::D3D12_ROOT_DESCRIPTOR_TABLE {
            NumDescriptorRanges: ranges.len() as _,
            pDescriptorRanges: ranges.as_ptr() as *const _,
        };

        RootParameter(parameter)
    }
}

// Maximum 64 DWORDS divied up amongst all root parameters.
// Root constants = 1 DWORD * NumConstants
// Root descriptor (CBV, SRV, or UAV) = 2 DWORDs each
// Descriptor table pointer = 1 DWORD
// Static samplers = 0 DWORDS (compiled into shader)
pub struct RootSignatureBuilder {
    parameters: Vec<RootParameter>,
}

impl RootSignatureBuilder {
    pub fn add_parameter(mut self, parameter: RootParameter) -> RootSignatureBuilder {
        self.parameters.push(parameter);
        self
    }

    pub fn build(self, device: Device) -> RootSignature {
        self.build_with_flags(device, d3d12::D3D12_ROOT_SIGNATURE_FLAG_NONE)
    }

    pub fn build_with_flags(
        self,
        device: Device,
        flags: d3d12::D3D12_ROOT_SIGNATURE_FLAGS,
    ) -> RootSignature {
        let mut desc = unsafe { d3d12::D3D12_ROOT_SIGNATURE_DESC { ..mem::zeroed() } };
        desc.NumParameters = self.parameters.len() as _;
        desc.pParameters = self.parameters.as_ptr() as *const _;
        desc.Flags = flags;

        let mut out_blob = Blob::null();
        let mut error_blob = Blob::null();
        let mut signature = ComPtr::<d3d12::ID3D12RootSignature>::null();
        unsafe {
            if FAILED(d3d12::D3D12SerializeRootSignature(
                &desc,
                d3d12::D3D_ROOT_SIGNATURE_VERSION_1,
                out_blob.as_mut_void() as *mut *mut _,
                error_blob.as_mut_void() as *mut *mut _,
            )) {
                panic!("Failed to serialize root signature.");
            }

            if FAILED(device.CreateRootSignature(
                0,
                out_blob.GetBufferPointer(),
                out_blob.GetBufferSize(),
                &d3d12::ID3D12RootSignature::uuidof(),
                signature.as_mut_void(),
            )) {
                panic!("Failed to create root signature");
            }
        }
        // TODO: Cache compiled root signatures
        signature
    }
}

impl Default for RootSignatureBuilder {
    fn default() -> Self {
        RootSignatureBuilder {
            parameters: Vec::new(),
        }
    }
}

pub type RootSignature = ComPtr<d3d12::ID3D12RootSignature>;
