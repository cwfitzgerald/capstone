use crate::RendererMode;
pub(crate) use cache::*;
pub use descriptors::*;
pub(crate) use exec::render::*;
pub use passes::*;
use resources::{BufferResource, ImageResource};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};
pub use unsync::*;
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BufferDescriptor, Device, Extent3d, TextureDescriptor, TextureDimension, TextureViewDescriptor,
};

mod cache;
mod descriptors;
mod exec {
    pub mod render;
}
mod passes;
mod resources;
mod unsync;

pub trait RenderList: Send + Sync {
    fn init(&self, recorder: &mut RenderListCreationRecorder<'_>);
    fn render(&self, recorder: &mut RenderListRecorder<'_, '_>);
}

crate::declare_handle!(RenderListBufferHandle, RenderListImageHandle);

pub struct RenderListCreationRecorder<'a> {
    pub(crate) device: &'a Device,
    pub(crate) cache: &'a mut RenderListCache,
    pub(crate) mode: RendererMode,
}

pub struct RenderListRecorder<'a, 'b> {
    pub(crate) creation_rec: &'b mut RenderListCreationRecorder<'a>,
    pub(crate) routines: Vec<RenderRoutine>,
}

impl<'a, 'b> Deref for RenderListRecorder<'a, 'b> {
    type Target = RenderListCreationRecorder<'a>;

    fn deref(&self) -> &Self::Target {
        &self.creation_rec
    }
}

impl<'a, 'b> DerefMut for RenderListRecorder<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.creation_rec
    }
}

pub struct RenderListRoutineRecorder {
    pub(crate) target: RenderRoutineTarget,
    pub(crate) passes: Vec<Pass>,
}

impl<'a> RenderListCreationRecorder<'a> {
    pub fn create_image(
        &mut self,
        desc: ImageResourceDescriptor,
        reuse: Option<RenderListImageHandle>,
    ) -> RenderListImageHandle {
        let handle = if let Some(reuse) = reuse {
            let desc_equal = self.cache.images.try_get(reuse.0).map(|old| old.desc == desc);

            if let Some(true) = desc_equal {
                return reuse;
            }

            reuse.0
        } else {
            self.cache.images.allocate()
        };

        let image = self.device.create_texture(&TextureDescriptor {
            label: desc.label.as_deref(),
            size: Extent3d {
                width: desc.resolution[0],
                height: desc.resolution[1],
                depth: 1,
            },
            mip_level_count: desc.mips,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: desc.format,
            usage: desc.usage,
        });

        let view = image.create_view(&TextureViewDescriptor::default());

        self.cache.images.insert(
            handle,
            ImageResource {
                desc,
                image_view: Arc::new(view),
                image: Arc::new(image),
            },
        );

        RenderListImageHandle(handle)
    }

    pub fn create_buffer(
        &mut self,
        desc: BufferResourceDescriptor,
        reuse: Option<RenderListBufferHandle>,
    ) -> RenderListBufferHandle {
        let handle = if let Some(reuse) = reuse {
            let desc_equal = self.cache.buffers.try_get(reuse.0).map(|old| old.desc == desc);

            if let Some(true) = desc_equal {
                return reuse;
            }

            reuse.0
        } else {
            self.cache.buffers.allocate()
        };

        let buffer = self.device.create_buffer(&BufferDescriptor {
            label: desc.label.as_deref(),
            size: desc.size,
            usage: desc.usage,
            mapped_at_creation: false,
        });

        self.cache.buffers.insert(
            handle,
            BufferResource {
                desc,
                buffer: Arc::new(buffer),
            },
        );

        RenderListBufferHandle(handle)
    }

    pub fn create_buffer_init(&mut self, desc_init: BufferResourceInitDescriptor) -> RenderListBufferHandle {
        let handle = self.cache.buffers.allocate();

        let (desc, data) = desc_init.into_base_descriptor_data();

        let buffer = self.device.create_buffer_init(&BufferInitDescriptor {
            label: desc.label.as_deref(),
            contents: &data,
            usage: desc.usage,
        });

        self.cache.buffers.insert(
            handle,
            BufferResource {
                desc,
                buffer: Arc::new(buffer),
            },
        );

        RenderListBufferHandle(handle)
    }
}

impl<'a, 'b> RenderListRecorder<'a, 'b> {
    pub fn add_render_routine<Func>(
        &mut self,
        processing: ObjectProcessing,
        target: RenderRoutineTarget,
        function: Func,
    ) where
        Func: FnMut(&mut RenderListRoutineRecorder, usize, RendererMode) + 'static,
    {
        self.routines.push(RenderRoutine {
            processing,
            target,
            routine: Box::new(function),
        })
    }
}

impl RenderListRoutineRecorder {
    pub fn add_render_pass(&mut self, desc: RenderPassDescriptor) {
        self.passes.push(Pass::Render(RenderPass { desc, ops: Vec::new() }))
    }

    pub fn add_render_op(&mut self, desc: RenderOpDescriptor) {
        self.passes
            .last_mut()
            .expect("Must be inside a RenderPass to add a RenderOp. Currently no passes have been added.")
            .as_render_mut()
            .expect("Must be inside a RenderPass to add a RenderOp. Last pass was a ComputePass.")
            .ops
            .push(desc);
    }

    pub fn add_compute_pass(&mut self, desc: ComputePassDescriptor) {
        self.passes.push(Pass::Compute(ComputePass { desc, ops: Vec::new() }))
    }

    pub fn add_compute_op(&mut self, desc: ComputeOpDescriptor) {
        self.passes
            .last_mut()
            .expect("Must be inside a ComputePass to add a ComputeOp. Currently no passes have been added.")
            .as_compute_mut()
            .expect("Must be inside a ComputePass to add a ComputeOp. Last pass was a ComputePass.")
            .ops
            .push(desc);
    }
}

pub(crate) struct RenderRoutine {
    pub processing: ObjectProcessing,
    pub target: RenderRoutineTarget,
    pub routine: Box<dyn FnMut(&mut RenderListRoutineRecorder, usize, RendererMode)>,
}

#[derive(Debug)]
pub(crate) enum Pass {
    Compute(ComputePass),
    Render(RenderPass),
}

impl Pass {
    pub fn as_compute_mut(&mut self) -> Option<&mut ComputePass> {
        match *self {
            Pass::Compute(ref mut p) => Some(p),
            Pass::Render(_) => None,
        }
    }

    pub fn as_render_mut(&mut self) -> Option<&mut RenderPass> {
        match *self {
            Pass::Render(ref mut p) => Some(p),
            Pass::Compute(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RenderPass {
    pub desc: RenderPassDescriptor,
    pub ops: Vec<RenderOpDescriptor>,
}

#[derive(Debug, Clone)]
pub(crate) struct ComputePass {
    pub desc: ComputePassDescriptor,
    pub ops: Vec<ComputeOpDescriptor>,
}
pub struct ObjectProcessing {
    filter: ObjectFilter,
    frustum_culling: bool,
    object_sorting: ObjectSortingStyle,
}

pub struct ObjectFilter;

pub enum ObjectSortingStyle {
    None,
    FrontToBack,
    BackToFront,
}
