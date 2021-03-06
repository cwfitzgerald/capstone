fn load_gltf(
    renderer: &rend3::Renderer,
    path: &'static str,
) -> (rend3::datatypes::MeshHandle, rend3::datatypes::MaterialHandle) {
    let (doc, datas, _) = gltf::import(path).unwrap();
    let mesh_data = doc.meshes().next().expect("no meshes in data.glb");

    let primitive = mesh_data.primitives().next().expect("no primitives in data.glb");
    let reader = primitive.reader(|b| Some(&datas.get(b.index())?.0[..b.length()]));

    let vertex_positions: Vec<_> = reader.read_positions().unwrap().map(glam::Vec3::from).collect();
    let vertex_normals: Vec<_> = reader.read_normals().unwrap().map(glam::Vec3::from).collect();
    let vertex_tangents: Vec<_> = reader
        .read_tangents()
        .unwrap()
        .map(glam::Vec4::from)
        .map(From::from)
        .collect();
    let vertex_uvs: Vec<_> = reader
        .read_tex_coords(0)
        .unwrap()
        .into_f32()
        .map(glam::Vec2::from)
        .collect();
    let indices = reader.read_indices().unwrap().into_u32().collect();

    let mesh = rend3::datatypes::MeshBuilder::new(vertex_positions.to_vec())
        .with_vertex_normals(vertex_normals)
        .with_vertex_tangents(vertex_tangents)
        .with_vertex_uvs(vertex_uvs)
        .with_indices(indices)
        .with_right_handed()
        .build();

    // Add mesh to renderer's world
    let mesh_handle = renderer.add_mesh(mesh);

    // Add basic material with all defaults except a single color.
    let material = primitive.material();
    let metallic_roughness = material.pbr_metallic_roughness();
    let material_handle = renderer.add_material(rend3::datatypes::Material {
        albedo: rend3::datatypes::AlbedoComponent::Value(metallic_roughness.base_color_factor().into()),
        ..Default::default()
    });

    (mesh_handle, material_handle)
}

fn main() {
    // Setup logging
    wgpu_subscriber::initialize_default_subscriber(None);

    // Create event loop and window
    let event_loop = winit::event_loop::EventLoop::new();
    let window = {
        let mut builder = winit::window::WindowBuilder::new();
        builder = builder.with_title("rend3 gltf");
        builder.build(&event_loop).expect("Could not build window")
    };

    let window_size = window.inner_size();

    let mut options = rend3::RendererOptions {
        vsync: rend3::VSyncMode::On,
        size: [window_size.width, window_size.height],
        ambient: glam::Vec4::default(),
    };

    let renderer = pollster::block_on(rend3::RendererBuilder::new(options.clone()).window(&window).build()).unwrap();

    // Create the default set of shaders and pipelines
    let pipelines = pollster::block_on(async {
        let shaders = rend3_list::DefaultShaders::new(&renderer).await;
        rend3_list::DefaultPipelines::new(&renderer, &shaders).await
    });

    // Create mesh and calculate smooth normals based on vertices
    let (mesh, material) = load_gltf(&renderer, concat!(env!("CARGO_MANIFEST_DIR"), "/data.glb"));

    // Combine the mesh and the material with a location to give an object.
    let object = rend3::datatypes::Object {
        mesh,
        material,
        transform: rend3::datatypes::AffineTransform {
            // Need to flip gltf's coords and winding order
            transform: glam::Mat4::from_scale(glam::Vec3::new(1.0, 1.0, -1.0)),
        },
    };
    let _object_handle = renderer.add_object(object);

    // Set camera's location
    renderer.set_camera_data(rend3::datatypes::Camera {
        projection: rend3::datatypes::CameraProjection::Projection {
            vfov: 60.0,
            near: 0.1,
            pitch: 0.43,
            yaw: -0.49,
        },
        location: glam::Vec3A::new(3.0, 3.0, -5.0),
    });

    // Create a single directional light
    renderer.add_directional_light(rend3::datatypes::DirectionalLight {
        color: glam::Vec3::ONE,
        intensity: 10.0,
        // Direction will be normalized
        direction: glam::Vec3::new(-1.0, -4.0, 2.0),
    });

    event_loop.run(move |event, _, control| match event {
        // Close button was clicked, we should close.
        winit::event::Event::WindowEvent {
            event: winit::event::WindowEvent::CloseRequested,
            ..
        } => {
            *control = winit::event_loop::ControlFlow::Exit;
        }
        // Window was resized, need to resize renderer.
        winit::event::Event::WindowEvent {
            event: winit::event::WindowEvent::Resized(size),
            ..
        } => {
            options.size = [size.width, size.height];
            renderer.set_options(options.clone());
        }
        // Render!
        winit::event::Event::MainEventsCleared => {
            // Size of the internal buffers used for rendering.
            //
            // This can be different from the size of the swapchain,
            // it will be scaled to the swapchain size when being
            // rendered onto the swapchain.
            let internal_renderbuffer_size = options.size;

            // Default set of rendering commands using the default shaders.
            let render_list = rend3_list::default_render_list(renderer.mode(), internal_renderbuffer_size, &pipelines);

            // Dispatch a render!
            let handle = renderer.render(render_list, rend3::RendererOutput::InternalSwapchain);

            // Wait until it's done
            pollster::block_on(handle);
        }
        // Other events we don't care about
        _ => {}
    });
}
