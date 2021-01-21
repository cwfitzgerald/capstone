use crate::{
    datatypes::{ClearColor, TextureHandle},
    renderer::list::RenderListImageHandle,
};
pub use wgpu::{Color, LoadOp};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceBinding {
    /// Bindings in All Modes:
    /// 0: Linear Sampler
    /// 1: Shadow Sampler
    GeneralData,
    /// Bindings in All Modes:
    /// 0: Object Data buffer
    ObjectData,
    /// May only be bound in GPU-powered mode:
    /// 0: Material Buffer
    ///
    /// Bindings in CPU-Powered Mode:
    /// 0: Albedo Texture
    /// 1: Normal Texture
    /// 2: Roughness Texture
    /// 3: Metallic Texture
    /// 4: Reflectance Texture
    /// 5: Clear Coat Texture
    /// 6: Clear Coat Roughness Texture
    /// 7: Anisotropy Texture
    /// 8: Ambient Occlusion Texture
    /// 9: Texture Data
    GPUMaterial,
    /// Bindings in All Modes:
    /// 0: Camera Data Uniform Buffer
    CameraData,
    /// May only be bound in GPU-powered mode:
    /// 0: 2D Texture Array
    GPU2DTextures,
    /// May only be bound in GPU-powered mode:
    /// 0: Cubemap Texture Array
    GPUCubeTextures,
    /// Bindings in All Modes:
    /// 0: Shadow `texture2DArray`
    /// 1: Directional light data
    ShadowTexture,
    /// Binding in All Modes:
    /// 0: Current skybox texture
    SkyboxTexture,
    /// Usable in all modes.
    ///
    /// Each given texture will be it's own binding
    Custom2DTexture(Vec<ImageInputReference>),
    /// Usable in all modes.
    ///
    /// Each given texture will be it's own binding
    CustomCubeTexture(Vec<ImageInputReference>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PerObjectResourceBinding {
    /// May only be bound in GPU-powered mode:
    /// 0: Albedo Texture
    /// 1: Normal Texture
    /// 2: Roughness Texture
    /// 3: Metallic Texture
    /// 4: Reflectance Texture
    /// 5: Clear Coat Texture
    /// 6: Clear Coat Roughness Texture
    /// 7: Anisotropy Texture
    /// 8: Ambient Occlusion Texture
    /// 9: Texture Data
    CPUMaterial,
}

pub type ImageFormat = wgpu::TextureFormat;
pub type ImageUsage = wgpu::TextureUsage;
pub type BufferUsage = wgpu::BufferUsage;

pub enum ImageReference {
    OutputImage,
    Handle(TextureHandle),
    Custom(RenderListImageHandle),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImageInputReference {
    Handle(TextureHandle),
    Custom(RenderListImageHandle),
}

#[derive(Debug, Clone)]
pub struct ImageOutput {
    pub output: ImageOutputReference,
    pub resolve_target: Option<ImageOutputReference>,
    pub clear: LoadOp<ClearColor>,
}

#[derive(Debug, Clone)]
pub struct DepthOutput {
    pub output: ImageOutputReference,
    pub clear: LoadOp<f32>,
}

#[derive(Debug, Clone)]
pub enum ImageOutputReference {
    OutputImage,
    Custom(RenderListImageHandle),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResourceDescriptor {
    pub label: Option<String>,
    pub resolution: [u32; 2],
    pub mips: u32,
    pub format: ImageFormat,
    pub samples: u32,
    pub usage: ImageUsage,
}

pub enum BufferReference<'a> {
    Custom(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferResourceDescriptor {
    pub label: Option<String>,
    pub size: u64,
    pub usage: BufferUsage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferResourceInitDescriptor {
    pub label: Option<String>,
    pub data: Vec<u8>,
    pub usage: BufferUsage,
}

impl BufferResourceInitDescriptor {
    pub fn into_base_descriptor_data(self) -> (BufferResourceDescriptor, Vec<u8>) {
        let desc = BufferResourceDescriptor {
            label: self.label,
            size: self.data.len() as u64,
            usage: self.usage,
        };

        let data = self.data;

        (desc, data)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderRoutineTarget {
    Shadow,
    Image(RenderListImageHandle),
    None,
}
