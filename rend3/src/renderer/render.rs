use crate::{
    bind_merge::BindGroupBuilder,
    datatypes::{Camera, CameraProjection},
    instruction::Instruction,
    list::{RenderList, RenderPassRunRate},
    renderer::{culling, list, uniforms::WrappedUniform, util::round_to_multiple},
    statistics::RendererStatistics,
    OutputFrame, Renderer, RendererMode, RendererOutput,
};
use futures::{stream::FuturesOrdered, StreamExt};
use std::{borrow::Cow, future::Future, sync::Arc};
use tracing_futures::Instrument;
use wgpu::{
    BindingResource, CommandEncoderDescriptor, ComputePassDescriptor, Extent3d, Maintain, Origin3d, ShaderFlags,
    ShaderModuleDescriptor, ShaderSource, TextureAspect, TextureCopyView, TextureDataLayout, TextureDescriptor,
    TextureDimension, TextureUsage, TextureViewDescriptor, TextureViewDimension,
};

pub fn render_loop<TLD: 'static>(
    renderer: Arc<Renderer<TLD>>,
    render_list: RenderList,
    output: RendererOutput,
) -> impl Future<Output = RendererStatistics> {
    span_transfer!(_ -> render_create_span, INFO, "Render Loop Creation");

    // blocks, do it before we async
    renderer.instructions.swap();

    let render_loop_span = tracing::warn_span!("Render Loop");
    async move {
        let mut instructions = renderer.instructions.consumer.lock();

        span_transfer!(_ -> event_span, INFO, "Process events");

        let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("primary encoder"),
        });

        let mut new_options = None;

        let mut mesh_manager = renderer.mesh_manager.write();
        let mut texture_manager_2d = renderer.texture_manager_2d.write();
        let mut texture_manager_cube = renderer.texture_manager_cube.write();
        let mut material_manager = renderer.material_manager.write();
        let mut object_manager = renderer.object_manager.write();
        let mut directional_light_manager = renderer.directional_light_manager.write();
        let mut global_resources = renderer.global_resources.write();
        let options = renderer.options.read();

        for cmd in instructions.drain(..) {
            match cmd {
                Instruction::AddMesh { handle, mesh } => {
                    mesh_manager.fill(
                        &renderer.device,
                        &renderer.queue,
                        &renderer.gpu_copy,
                        &mut encoder,
                        handle,
                        mesh,
                    );
                }
                Instruction::RemoveMesh { handle } => {
                    mesh_manager.remove(handle);
                }
                Instruction::AddTexture2D { handle, texture } => {
                    let size = Extent3d {
                        width: texture.width,
                        height: texture.height,
                        depth: 1,
                    };

                    assert!(texture.mip_levels > 0, "Mipmap levels must be greater than 0");

                    let block_width = texture.format.pixels_per_block();
                    let block_bytes = texture.format.bytes_per_block();

                    // TODO: This is a workaround, because WGPU 0.6 has an issue when mip map is smaller the format's
                    // block size, eg with BC1 4x4 and mip level being 2x2 pixels
                    // The code below calculates log2 of the smaller dimension to get the maximum mip level before
                    // the texture hits block width
                    let smaller_dim = size.width.min(size.height);
                    let tex_mips = (std::mem::size_of_val(&smaller_dim) * 8) as u32 - smaller_dim.leading_zeros() - 1;
                    let block_mips = (std::mem::size_of_val(&block_width) * 8) as u32 - block_width.leading_zeros() - 1;
                    let max_mip_levels = tex_mips - block_mips + 1;

                    let mip_levels = texture.mip_levels.min(max_mip_levels);

                    let uploaded_tex = renderer.device.create_texture(&TextureDescriptor {
                        label: None,
                        size,
                        mip_level_count: mip_levels,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: texture.format.into(),
                        usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
                    });

                    let mut offset = 0;

                    for mip in 0..mip_levels {
                        // Each mip is 1/4th the size of the previous level, so our divisors are the powers of two
                        let div = 1 << mip;

                        // When using compressed textures, we need to round up the extents to the physical memory used
                        // by the texture, so round to a multiple of the block size (generally 1x1 or 4x4)
                        let mip_size = Extent3d {
                            width: round_to_multiple(size.width / div, block_width),
                            height: round_to_multiple(size.height / div, block_width),
                            depth: 1,
                        };

                        // Size of mip level in pixel blocks
                        let width_blocks = mip_size.width / block_width;
                        let height_blocks = mip_size.height / block_width;

                        let bytes_per_row = width_blocks * block_bytes;
                        let bytes = bytes_per_row * height_blocks;

                        let offset_end = offset + bytes as usize;

                        renderer.queue.write_texture(
                            TextureCopyView {
                                texture: &uploaded_tex,
                                origin: Origin3d::ZERO,
                                mip_level: mip,
                            },
                            &texture.data[offset..offset_end],
                            TextureDataLayout {
                                offset: 0,
                                rows_per_image: 0,
                                bytes_per_row,
                            },
                            mip_size,
                        );

                        offset = offset_end;
                    }

                    texture_manager_2d.fill(
                        handle,
                        uploaded_tex.create_view(&TextureViewDescriptor::default()),
                        Some(texture.format),
                    );
                }
                Instruction::RemoveTexture2D { handle } => {
                    texture_manager_2d.remove(handle);
                }
                Instruction::AddTextureCube { handle, texture } => {
                    let size = Extent3d {
                        width: texture.width,
                        height: texture.height,
                        depth: 6,
                    };

                    assert!(texture.mip_levels > 0, "Mipmap levels must be greater than 0");

                    let block_width = texture.format.pixels_per_block();
                    let block_bytes = texture.format.bytes_per_block();

                    // TODO: This is a workaround, because WGPU 0.6 has an issue when mip map is smaller the format's
                    // block size, eg with BC1 4x4 and mip level being 2x2 pixels
                    // The code below calculates log2 of the smaller dimension to get the maximum mip level before
                    // the texture hits block width
                    let smaller_dim = size.width.min(size.height);
                    let tex_mips = (std::mem::size_of_val(&smaller_dim) * 8) as u32 - smaller_dim.leading_zeros() - 1;
                    let block_mips = (std::mem::size_of_val(&block_width) * 8) as u32 - block_width.leading_zeros() - 1;
                    let max_mip_levels = tex_mips - block_mips + 1;

                    let mip_levels = texture.mip_levels.min(max_mip_levels);

                    let uploaded_tex = renderer.device.create_texture(&TextureDescriptor {
                        label: None,
                        size,
                        mip_level_count: mip_levels,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: texture.format.into(),
                        usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
                    });

                    let mut offset = 0;

                    for layer in 0..6 {
                        // Not 0..mip_levels, because we do want to skip over the unused mip levels
                        for mip in 0..texture.mip_levels {
                            // Each mip is 1/4th the size of the previous level, so our divisors are the powers of two
                            let div = 1 << mip;

                            // When using compressed textures, we need to round up the extents to the physical memory used
                            // by the texture, so round to a multiple of the block size (generally 1x1 or 4x4)
                            let mip_size = Extent3d {
                                width: round_to_multiple(size.width / div, block_width),
                                height: round_to_multiple(size.height / div, block_width),
                                depth: 1,
                            };

                            // Size of mip level in pixel blocks
                            let width_blocks = mip_size.width / block_width;
                            let height_blocks = mip_size.height / block_width;

                            let bytes_per_row = width_blocks * block_bytes;
                            let bytes = bytes_per_row * height_blocks;

                            let offset_end = offset + bytes as usize;

                            // Only write up to the max mip level and skip over unused bytes
                            if mip < max_mip_levels {
                                renderer.queue.write_texture(
                                    TextureCopyView {
                                        texture: &uploaded_tex,
                                        origin: Origin3d { x: 0, y: 0, z: layer },
                                        mip_level: mip,
                                    },
                                    &texture.data[offset..offset_end],
                                    TextureDataLayout {
                                        offset: 0,
                                        bytes_per_row,
                                        rows_per_image: 0,
                                    },
                                    mip_size,
                                );
                            }

                            offset = offset_end;
                        }
                    }

                    texture_manager_cube.fill(
                        handle,
                        uploaded_tex.create_view(&TextureViewDescriptor {
                            label: None,
                            format: Some(texture.format.into()),
                            dimension: Some(TextureViewDimension::Cube),
                            aspect: TextureAspect::All,
                            base_mip_level: 0,
                            level_count: None,
                            base_array_layer: 0,
                            array_layer_count: None,
                        }),
                        Some(texture.format),
                    );
                }
                Instruction::RemoveTextureCube { handle } => {
                    texture_manager_cube.remove(handle);
                }
                Instruction::AddMaterial { handle, material } => {
                    material_manager.fill(
                        &renderer.device,
                        renderer.mode,
                        &mut texture_manager_2d,
                        &global_resources.material_bgl,
                        handle,
                        material,
                    );
                }
                Instruction::ChangeMaterial { handle, change } => {
                    material_manager.update_from_changes(&renderer.queue, handle, change);
                }
                Instruction::RemoveMaterial { handle } => {
                    material_manager.remove(handle);
                }
                Instruction::AddObject { handle, object } => {
                    object_manager.fill(handle, object, &mesh_manager);
                }
                Instruction::SetObjectTransform {
                    handle: object,
                    transform,
                } => {
                    object_manager.set_object_transform(object, transform);
                }
                Instruction::RemoveObject { handle } => {
                    object_manager.remove(handle);
                }
                Instruction::AddDirectionalLight { handle, light } => {
                    directional_light_manager.fill(handle, light);
                }
                Instruction::ChangeDirectionalLight { handle, change } => {
                    // TODO: Move these inside the managers
                    let value = directional_light_manager.get_mut(handle);
                    value.inner.update_from_changes(change);
                    if let Some(direction) = change.direction {
                        value.camera.set_data(
                            Camera {
                                projection: CameraProjection::from_orthographic_direction(direction.into()),
                                ..Camera::default()
                            },
                            None,
                        );
                    }
                }
                Instruction::RemoveDirectionalLight { handle } => directional_light_manager.remove(handle),
                Instruction::AddBinaryShader { handle, shader } => {
                    let module = renderer.device.create_shader_module(&ShaderModuleDescriptor {
                        label: None,
                        source: ShaderSource::SpirV(Cow::Owned(shader)),
                        flags: ShaderFlags::VALIDATION,
                    });
                    renderer.shader_manager.insert(handle, Arc::new(module));
                }
                Instruction::RemoveShader { handle } => {
                    renderer.shader_manager.remove(handle);
                }
                Instruction::RemovePipeline { handle } => {
                    renderer.pipeline_manager.remove(handle);
                }
                Instruction::SetOptions { options } => new_options = Some(options),
                Instruction::SetCameraData { data } => {
                    global_resources.camera.set_data(data, Some(options.aspect_ratio()));
                }
                Instruction::SetBackgroundTexture { handle } => {
                    global_resources.background_texture = Some(handle);
                }
                Instruction::ClearBackgroundTexture => {
                    global_resources.background_texture = None;
                }
            }
        }

        renderer
            .render_list_cache
            .write()
            .add_render_list(&renderer.device, render_list.resources);

        let texture_2d_ready = texture_manager_2d.ready(&renderer.device);
        let texture_cube_ready = texture_manager_cube.ready(&renderer.device);

        let recompile_future = if renderer.mode == RendererMode::GPUPowered {
            Some(renderer.pipeline_manager.recompile_pipelines(
                &renderer,
                texture_2d_ready.dirty.into_gpu(),
                texture_cube_ready.dirty.into_gpu(),
            ))
        } else {
            None
        };

        material_manager.ready(&renderer.device, &mut encoder, &texture_manager_2d);
        let object_count = object_manager.ready(&renderer.device, &mut encoder, &material_manager);
        directional_light_manager.ready(&renderer.device, &mut encoder);

        let object_input_bg = renderer.mode.into_data(
            || (),
            || {
                let mut object_input_bgb = BindGroupBuilder::new(Some(String::from("object input bg")));
                object_manager.gpu_append_to_bgb(&mut object_input_bgb);
                object_input_bgb.build(&renderer.device, &global_resources.object_input_bgl)
            },
        );

        let mut general_bgb = BindGroupBuilder::new(Some(String::from("general bg")));
        global_resources.append_to_bgb(&mut general_bgb);
        let general_bg = general_bgb.build(&renderer.device, &global_resources.general_bgl);

        let material_bg = renderer.mode.into_data(
            || (),
            || {
                let mut material_bgb = BindGroupBuilder::new(Some(String::from("material bg")));
                material_manager.gpu_append_to_bgb(&mut material_bgb);
                material_bgb.build(&renderer.device, &global_resources.material_bgl)
            },
        );

        let mut shadow_bgb = BindGroupBuilder::new(Some(String::from("shadow bg")));
        directional_light_manager.append_to_bgb(&mut shadow_bgb);
        let shadow_bg = shadow_bgb.build(&renderer.device, &global_resources.shadow_texture_bgl);

        let skybox_texture_view = if let Some(ref sky) = global_resources.background_texture {
            texture_manager_cube.get_view(*sky)
        } else {
            texture_manager_cube.ensure_null_view();
            texture_manager_cube.get_null_view()
        };
        let mut skybox_bgb = BindGroupBuilder::new(Some(String::from("skybox bg")));
        skybox_bgb.append(BindingResource::TextureView(skybox_texture_view));
        let skybox_bg = skybox_bgb.build(&renderer.device, &global_resources.skybox_bgl);

        drop((
            options,
            mesh_manager,
            texture_manager_2d,
            texture_manager_cube,
            material_manager,
            object_manager,
            directional_light_manager,
        ));

        span_transfer!(event_span -> resource_update_span, INFO, "Update resources");

        let options = if let Some(new_opt) = new_options {
            let mut option_guard = renderer.options.write();
            global_resources.update(&renderer.device, renderer.surface.as_ref(), &mut *option_guard, new_opt);
            option_guard.clone()
        } else {
            renderer.options.read().clone()
        };

        drop(global_resources);

        if let Some(recomp_future) = recompile_future {
            recomp_future.await;
        }

        let global_resources = renderer.global_resources.read();
        let object_manager = renderer.object_manager.read();
        let directional_light_manager = renderer.directional_light_manager.read();

        let mut command_buffer_futures = FuturesOrdered::new();

        for light in directional_light_manager.values() {
            let mut cull_data = renderer.culling_pass.prepare(culling::CullingPassPrepareArgs {
                device: &renderer.device,
                mode: renderer.mode,
                prefix_sum_bgl: &global_resources.prefix_sum_bgl,
                pre_cull_bgl: &global_resources.pre_cull_bgl,
                output_bgl: &global_resources.object_output_bgl,
                object_count: object_count as _,
                name: String::from("shadow pass"),
            });

            let mut object_bgb = BindGroupBuilder::new(Some(String::from("object bg")));
            object_bgb.append(cull_data.output_buffer.as_entire_binding());
            let object_bg = object_bgb.build(&renderer.device, &global_resources.object_data_bgl);

            let uniform = WrappedUniform::new(&renderer.device, &global_resources.camera_data_bgl);
            uniform.upload(&renderer.queue, &light.camera, options.ambient);

            match renderer.mode {
                RendererMode::CPUPowered => {
                    renderer
                        .culling_pass
                        .cpu_run(
                            &renderer.yard,
                            renderer.yard_priorites,
                            &renderer.queue,
                            &object_manager,
                            &mut cull_data,
                            light.camera,
                        )
                        .await;
                }
                RendererMode::GPUPowered => {
                    let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor::default());

                    renderer.culling_pass.gpu_run(
                        &mut cpass,
                        object_input_bg.as_gpu(),
                        &uniform.uniform_bg,
                        &cull_data,
                    );

                    drop(cpass);
                }
            }

            let binding_data = list::BindingData {
                general_bg: Arc::clone(&general_bg),
                object_bg: Arc::clone(&object_bg),
                material_bg: material_bg.as_ref().map(|_| (), Arc::clone),
                gpu_2d_textures_bg: texture_2d_ready.bg.as_ref().map(|_| (), Arc::clone),
                gpu_cube_textures_bg: texture_cube_ready.bg.as_ref().map(|_| (), Arc::clone),
                shadow_texture_bg: Arc::clone(&shadow_bg),
                skybox_texture_bg: Arc::clone(&skybox_bg),
                wrapped_uniform: Arc::new(uniform),
            };

            let cull_data_arc = Arc::new(cull_data);

            for render_pass in &render_list.passes {
                if render_pass.desc.run_rate != RenderPassRunRate::PerShadow {
                    continue;
                }

                let output = directional_light_manager.get_layer_view_arc(light.shadow_tex);

                command_buffer_futures.push(renderer.yard.spawn(
                    renderer.yard_priorites.compute_pool,
                    renderer.yard_priorites.render_record_priority,
                    list::render_single_render_pass(
                        Arc::clone(&renderer),
                        render_pass.clone(),
                        OutputFrame::View(output),
                        Arc::clone(&cull_data_arc),
                        binding_data.clone(),
                    ),
                ));
            }
        }

        drop(directional_light_manager);

        // In wgpu 0.6, get_current_frame erroneously requires &mut
        drop(global_resources);

        let frame = output.acquire(&mut renderer.global_resources.write().swapchain);

        let global_resources = renderer.global_resources.read();

        {
            let mut cull_data = renderer.culling_pass.prepare(culling::CullingPassPrepareArgs {
                device: &renderer.device,
                mode: renderer.mode,
                prefix_sum_bgl: &global_resources.prefix_sum_bgl,
                pre_cull_bgl: &global_resources.pre_cull_bgl,
                output_bgl: &global_resources.object_output_bgl,
                object_count: object_count as _,
                name: String::from("camera pass"),
            });

            let mut object_bgb = BindGroupBuilder::new(Some(String::from("object bg")));
            object_bgb.append(cull_data.output_buffer.as_entire_binding());
            let object_bg = object_bgb.build(&renderer.device, &global_resources.object_data_bgl);

            let uniform = WrappedUniform::new(&renderer.device, &global_resources.camera_data_bgl);
            uniform.upload(&renderer.queue, &global_resources.camera, options.ambient);

            match renderer.mode {
                RendererMode::CPUPowered => {
                    renderer
                        .culling_pass
                        .cpu_run(
                            &renderer.yard,
                            renderer.yard_priorites,
                            &renderer.queue,
                            &object_manager,
                            &mut cull_data,
                            global_resources.camera,
                        )
                        .await;
                }
                RendererMode::GPUPowered => {
                    let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor::default());

                    renderer.culling_pass.gpu_run(
                        &mut cpass,
                        object_input_bg.as_gpu(),
                        &uniform.uniform_bg,
                        &cull_data,
                    );

                    drop(cpass);
                }
            }

            let binding_data = list::BindingData {
                general_bg: Arc::clone(&general_bg),
                object_bg: Arc::clone(&object_bg),
                material_bg: material_bg.as_ref().map(|_| (), Arc::clone),
                gpu_2d_textures_bg: texture_2d_ready.bg.as_ref().map(|_| (), Arc::clone),
                gpu_cube_textures_bg: texture_cube_ready.bg.as_ref().map(|_| (), Arc::clone),
                shadow_texture_bg: Arc::clone(&shadow_bg),
                skybox_texture_bg: Arc::clone(&skybox_bg),
                wrapped_uniform: Arc::new(uniform),
            };

            let cull_data_arc = Arc::new(cull_data);

            for render_pass in &render_list.passes {
                if render_pass.desc.run_rate != RenderPassRunRate::Once {
                    continue;
                }

                command_buffer_futures.push(renderer.yard.spawn(
                    renderer.yard_priorites.compute_pool,
                    renderer.yard_priorites.render_record_priority,
                    list::render_single_render_pass(
                        Arc::clone(&renderer),
                        render_pass.clone(),
                        frame.clone(),
                        Arc::clone(&cull_data_arc),
                        binding_data.clone(),
                    ),
                ));
            }
        }

        drop((object_manager, global_resources));

        span_transfer!(resource_update_span -> _);

        let mut command_buffers = vec![encoder.finish()];

        while let Some(buffer) = command_buffer_futures.next().await {
            command_buffers.push(buffer);
        }

        span_transfer!(_ -> queue_submit_span, INFO, "Submitting to Queue");

        renderer.device.poll(Maintain::Wait);
        renderer.queue.submit(command_buffers);

        span_transfer!(queue_submit_span -> buffer_pump_span, INFO, "Pumping Buffers");

        let futures = renderer.buffer_manager.lock().pump();
        for future in futures {
            let span = tracing::debug_span!("Buffer recall");
            renderer.yard.spawn(
                renderer.yard_priorites.compute_pool,
                renderer.yard_priorites.buffer_recall_priority,
                future.instrument(span),
            );
        }

        span_transfer!(buffer_pump_span -> present_span, INFO, "Presenting");
        drop(frame); //

        span_transfer!(present_span -> drop_span, INFO, "Dropping loop data");

        RendererStatistics {}
    }
    .instrument(render_loop_span)
}
