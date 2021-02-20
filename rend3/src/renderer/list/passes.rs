use glam::UVec3;

use crate::{
    datatypes::{ComputePipelineHandle, RenderPipelineHandle},
    renderer::list::{DepthOutput, ImageOutput, PerObjectResourceBinding, ResourceBinding, RenderListBufferHandle},
};

#[derive(Debug, Clone)]
pub struct BufferReference {
    pub buffer: RenderListBufferHandle,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct ComputePassDescriptor {
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ComputeOpDescriptor {
    pub pipeline: ComputePipelineHandle,
    pub per_op_bindings: Vec<ResourceBinding>,
    pub size: UVec3,
}

#[derive(Debug, Clone)]
pub enum ComputeOpDispatchType {
    Cpu {
        size: UVec3,
    },
    Gpu {
        indirect_buffer: BufferReference,
    }
}

#[derive(Debug, Clone)]
pub struct RenderPassDescriptor {
    pub name: Option<String>,
    pub outputs: Vec<ImageOutput>,
    pub depth: Option<DepthOutput>,
}

#[derive(Debug, Clone)]
pub struct RenderOpDescriptor {
    pub pipeline: RenderPipelineHandle,
    pub per_op_bindings: Vec<ResourceBinding>,
    pub per_object_bindings: Vec<PerObjectResourceBinding>,
    pub draw_type: RenderOpDrawType,
}

#[derive(Debug, Clone)]
pub enum RenderOpDrawType {
    Cpu {
        input: RenderOpInputType,
    },
    Gpu {
        indirect_buffer: BufferReference,
        count_buffer: BufferReference,
        max_count: usize,
    }
}

#[derive(Debug, Clone)]
pub enum RenderOpInputType {
    /// No bound vertex inputs, just a simple `draw(0..3)`
    FullscreenTriangle,
    /// Render all 3D models.
    Models3D,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShaderSource {
    SpirV(Vec<u32>),
    Glsl(SourceShaderDescriptor),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShaderSourceType {
    /// Load shader from given file
    File(String),
    /// Load builtin shader
    Builtin(String),
    /// Use given shader source
    Value(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceShaderDescriptor {
    pub source: ShaderSourceType,
    pub stage: ShaderSourceStage,
    pub includes: Vec<String>,
    pub defines: Vec<(String, Option<String>)>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ShaderSourceStage {
    Vertex,
    Fragment,
    Compute,
}

impl From<ShaderSourceStage> for shaderc::ShaderKind {
    fn from(stage: ShaderSourceStage) -> Self {
        match stage {
            ShaderSourceStage::Vertex => shaderc::ShaderKind::Vertex,
            ShaderSourceStage::Fragment => shaderc::ShaderKind::Fragment,
            ShaderSourceStage::Compute => shaderc::ShaderKind::Compute,
        }
    }
}
