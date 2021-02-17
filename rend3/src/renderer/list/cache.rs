use crate::{
    registry::ResourceRegistry,
    renderer::list::{
        resources::{BufferResource, ImageResource},
        RenderListBufferHandle, RenderListImageHandle,
    },
};
use wgpu::{Buffer, TextureView};

#[derive(Debug)]
pub(crate) struct RenderListCache {
    pub(crate) images: ResourceRegistry<ImageResource>,
    pub(crate) buffers: ResourceRegistry<BufferResource>,
}

impl RenderListCache {
    pub fn new() -> Self {
        Self {
            images: ResourceRegistry::new(),
            buffers: ResourceRegistry::new(),
        }
    }

    pub fn get_buffer(&self, handle: &RenderListBufferHandle) -> &Buffer {
        &*self.buffers.get(handle.0).buffer
    }

    pub fn get_image(&self, handle: &RenderListImageHandle) -> &TextureView {
        &*self.images.get(handle.0).image_view
    }
}
