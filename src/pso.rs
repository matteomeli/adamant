use crate::com::ComPtr;
use crate::root_signature::RootSignature;
use crate::{Blob, Device};

use winapi::shared::{
    dxgiformat, dxgitype,
    winerror::{FAILED, SUCCEEDED},
};
use winapi::um::{d3d12, d3dcompiler};
use winapi::Interface;

use log::info;

use std::fs::File;
use std::io::{BufReader, Read};
use std::{mem, ptr, slice};

use bitflags::bitflags;

bitflags! {
    pub struct PipelineStateFlags: u32 {
        const TOOL_DEBUG = d3d12::D3D12_PIPELINE_STATE_FLAG_TOOL_DEBUG;
    }
}

bitflags! {
    pub struct ShaderCompilerFlags: u32 {
        const DEBUG = d3dcompiler::D3DCOMPILE_DEBUG;
        const SKIP_OPTIMIZATION = d3dcompiler::D3DCOMPILE_SKIP_OPTIMIZATION;
    }
}

#[derive(Copy, Clone, Debug)]
pub enum PipelineStage {
    Vertex,
    Hull,
    Domain,
    Geometry,
    Pixel,
    Compute,
}

#[derive(Copy, Clone, Debug)]
pub enum ShaderModel {
    V5_0,
    V5_1,
    V6_0,
}

#[repr(transparent)]
pub struct Shader {
    bytecode: d3d12::D3D12_SHADER_BYTECODE,
}

impl Shader {
    pub fn from_blob(blob: Blob) -> Self {
        Shader {
            bytecode: unsafe {
                d3d12::D3D12_SHADER_BYTECODE {
                    BytecodeLength: blob.GetBufferSize(),
                    pShaderBytecode: blob.GetBufferPointer(),
                }
            },
        }
    }

    pub fn from_code(
        code: &[u8],
        entry: &str,
        stage: PipelineStage,
        model: ShaderModel,
        flags: ShaderCompilerFlags,
    ) -> Shader {
        let target = {
            let stage = match stage {
                PipelineStage::Vertex => "vs",
                PipelineStage::Pixel => "ps",
                PipelineStage::Compute => "cs",
                _ => unimplemented!(),
            };

            let model = match model {
                ShaderModel::V5_0 => "5_0",
                ShaderModel::V5_1 => "5_1",
                ShaderModel::V6_0 => "6_0",
            };

            format!("{}_{}\0", stage, model)
        };

        let mut shader = Blob::null();
        let mut error = Blob::null();

        let hr = unsafe {
            d3dcompiler::D3DCompile(
                code.as_ptr() as *const _,
                code.len() as _,
                ptr::null(),     // Source Name: NOT USED
                ptr::null(),     // Defines: NOT USED
                ptr::null_mut(), // Includes: NOT USED
                entry.as_ptr() as *const _,
                target.as_ptr() as *const _,
                flags.bits(),
                0, // NOT USED
                shader.as_mut_void() as *mut *mut _,
                error.as_mut_void() as *mut *mut _,
            )
        };

        if FAILED(hr) {
            let message = unsafe {
                let pointer = error.GetBufferPointer();
                let size = error.GetBufferSize();
                let slice = slice::from_raw_parts(pointer as *const u8, size as usize);
                String::from_utf8_lossy(slice).into_owned()
            };
            unsafe {
                error.destroy();
            }
            panic!("Failed to compile shader: {}", message);
        }

        Self::from_blob(shader)
    }

    pub fn from_file(
        entry: &str,
        stage: PipelineStage,
        model: ShaderModel,
        flags: ShaderCompilerFlags,
    ) -> Shader {
        let file = File::open("foo.txt").unwrap();
        let mut buf_reader = BufReader::new(file);
        let mut contents = Vec::new();
        buf_reader.read_to_end(&mut contents).unwrap();
        Self::from_code(&contents, entry, stage, model, flags)
    }
}

pub enum PSOKind {
    Graphics,
    Compute,
}

#[repr(transparent)]
pub struct PipelineStateBuilder {
    desc: d3d12::D3D12_GRAPHICS_PIPELINE_STATE_DESC,
}

impl PipelineStateBuilder {
    pub fn with_blend_state(mut self, desc: d3d12::D3D12_BLEND_DESC) -> PipelineStateBuilder {
        self.desc.BlendState = desc;
        self
    }

    pub fn with_rasterizer_state(
        mut self,
        desc: d3d12::D3D12_RASTERIZER_DESC,
    ) -> PipelineStateBuilder {
        self.desc.RasterizerState = desc;
        self
    }

    pub fn with_depth_stencil_state(
        mut self,
        desc: d3d12::D3D12_DEPTH_STENCIL_DESC,
    ) -> PipelineStateBuilder {
        self.desc.DepthStencilState = desc;
        self
    }

    pub fn with_sample_mask(mut self, mask: u32) -> PipelineStateBuilder {
        self.desc.SampleMask = mask;
        self
    }

    pub fn with_primitive_topology_type(
        mut self,
        topology: d3d12::D3D12_PRIMITIVE_TOPOLOGY_TYPE,
    ) -> PipelineStateBuilder {
        self.desc.PrimitiveTopologyType = topology;
        self
    }

    pub fn with_render_target_formats(
        self,
        rtv_formats: &[dxgiformat::DXGI_FORMAT],
        dsv_format: dxgiformat::DXGI_FORMAT,
    ) -> PipelineStateBuilder {
        self.with_render_target_formats_msaa(rtv_formats, dsv_format, 1, 0)
    }

    pub fn with_render_target_format(
        self,
        rtv_format: dxgiformat::DXGI_FORMAT,
        dsv_format: dxgiformat::DXGI_FORMAT,
    ) -> PipelineStateBuilder {
        self.with_render_target_formats_msaa(&[rtv_format], dsv_format, 1, 0)
    }

    pub fn with_render_target_formats_msaa(
        mut self,
        rtv_formats: &[dxgiformat::DXGI_FORMAT],
        dsv_format: dxgiformat::DXGI_FORMAT,
        msaa_count: u32,
        msaa_quality: u32,
    ) -> PipelineStateBuilder {
        const MAX_RTV_COUNT: usize = 8;
        self.desc
            .RTVFormats
            .copy_from_slice(&rtv_formats[0..MAX_RTV_COUNT]);
        if rtv_formats.len() < MAX_RTV_COUNT {
            for i in rtv_formats.len()..MAX_RTV_COUNT {
                self.desc.RTVFormats[i] = dxgiformat::DXGI_FORMAT_UNKNOWN;
            }
        }
        self.desc.NumRenderTargets = rtv_formats.len() as _;
        self.desc.DSVFormat = dsv_format;
        self.desc.SampleDesc = dxgitype::DXGI_SAMPLE_DESC {
            Count: msaa_count,
            Quality: msaa_quality,
        };
        self
    }

    pub fn with_render_target_format_msaa(
        self,
        rtv_format: dxgiformat::DXGI_FORMAT,
        dsv_format: dxgiformat::DXGI_FORMAT,
        msaa_count: u32,
        msaa_quality: u32,
    ) -> PipelineStateBuilder {
        self.with_render_target_formats_msaa(&[rtv_format], dsv_format, msaa_count, msaa_quality)
    }

    pub fn with_input_layout(
        mut self,
        input_layouts: &[d3d12::D3D12_INPUT_ELEMENT_DESC],
    ) -> PipelineStateBuilder {
        self.desc.InputLayout.NumElements = input_layouts.len() as _;
        self.desc.InputLayout.pInputElementDescs = input_layouts.as_ptr();
        self
    }

    pub fn with_vertex_shader(mut self, shader: &Shader) -> PipelineStateBuilder {
        self.desc.VS = shader.bytecode;
        self
    }

    pub fn with_pixel_shader(mut self, shader: &Shader) -> PipelineStateBuilder {
        self.desc.PS = shader.bytecode;
        self
    }

    pub fn with_root_signature(mut self, root_signature: RootSignature) -> PipelineStateBuilder {
        self.desc.pRootSignature = root_signature.as_raw();
        self
    }

    pub fn build(self, device: Device, kind: PSOKind) -> PipelineState {
        let mut pso = ComPtr::<d3d12::ID3D12PipelineState>::null();
        match kind {
            PSOKind::Graphics => unsafe {
                if SUCCEEDED(device.CreateGraphicsPipelineState(
                    &self.desc,
                    &d3d12::ID3D12RootSignature::uuidof(),
                    pso.as_mut_void(),
                )) {
                    info!("Graphics pipeline state object created.")
                } else {
                    panic!("Failed to create D3D12 graphics pipeline state object.");
                }
            },
            PSOKind::Compute => unimplemented!(),
        }
        // TODO: Cache compiled psos
        pso
    }
}

impl Default for PipelineStateBuilder {
    fn default() -> Self {
        let desc = unsafe {
            d3d12::D3D12_GRAPHICS_PIPELINE_STATE_DESC {
                Flags: PipelineStateFlags::TOOL_DEBUG.bits(),
                ..mem::zeroed()
            }
        };
        PipelineStateBuilder { desc }
    }
}

pub type PipelineState = ComPtr<d3d12::ID3D12PipelineState>;
