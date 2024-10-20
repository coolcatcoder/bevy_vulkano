use std::{sync::Arc, time::Duration};

use bevy::{
    a11y::AccessibilityPlugin,
    prelude::*,
    winit::{WakeUp, WinitPlugin},
};
use bevy_vulkano::{BevyVulkanoContext, VulkanoPlugin, VulkanoRenderers};
use vulkano::{
    buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer},
    command_buffer::{
        allocator::StandardCommandBufferAllocator, AutoCommandBufferBuilder, CommandBufferUsage,
        RenderPassBeginInfo, SubpassBeginInfo, SubpassContents,
    },
    image::view::ImageView,
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter},
    pipeline::{
        graphics::{
            color_blend::{ColorBlendAttachmentState, ColorBlendState},
            input_assembly::InputAssemblyState,
            multisample::MultisampleState,
            rasterization::RasterizationState,
            vertex_input::{Vertex, VertexDefinition},
            viewport::{Viewport, ViewportState},
            GraphicsPipelineCreateInfo,
        },
        layout::PipelineDescriptorSetLayoutCreateInfo,
        DynamicState, GraphicsPipeline, PipelineLayout, PipelineShaderStageCreateInfo,
    },
    render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass},
    sync::GpuFuture,
};

fn main() {
    App::new()
        .add_plugins((
            AccessibilityPlugin,
            WindowPlugin::default(),
            WinitPlugin::<WakeUp>::default(),
            VulkanoPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, render)
        .run();
}

#[derive(BufferContents, Vertex)]
#[repr(C)]
struct MyVertex {
    #[format(R32G32_SFLOAT)]
    position: [f32; 2],
}

#[derive(Resource)]
struct Stuff {
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    vertices: Subbuffer<[MyVertex]>,
    render_pass: Arc<RenderPass>,
    framebuffers: Vec<Arc<Framebuffer>>,
    pipeline: Arc<GraphicsPipeline>,
}

fn setup(
    mut renderers: VulkanoRenderers,
    context: Res<BevyVulkanoContext>,
    mut commands: Commands,
) {
    let renderer = renderers.get_renderer_single().unwrap();

    mod vs {
        vulkano_shaders::shader! {
            ty: "vertex",
            src: r"
                #version 450

                layout(location = 0) in vec2 position;

                void main() {
                    gl_Position = vec4(position, 0.0, 1.0);
                }
            ",
        }
    }

    mod fs {
        vulkano_shaders::shader! {
            ty: "fragment",
            src: r"
                #version 450

                layout(location = 0) out vec4 f_color;

                void main() {
                    f_color = vec4(1.0, 0.0, 0.0, 1.0);
                }
            ",
        }
    }

    let vertices = [
        MyVertex {
            position: [-0.5, -0.25],
        },
        MyVertex {
            position: [0.0, 0.5],
        },
        MyVertex {
            position: [0.25, -0.1],
        },
    ];
    let vertex_buffer = Buffer::from_iter(
        context.memory_allocator().clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        vertices,
    )
    .unwrap();

    let render_pass = vulkano::single_pass_renderpass!(
        context.device().clone(),
        attachments: {
            color: {
                format: renderer.swapchain_format(),
                samples: 1,
                load_op: Clear,
                store_op: Store,
            },
        },
        pass: {
            color: [color],
            depth_stencil: {},
        },
    )
    .unwrap();

    let framebuffers = on_swapchain_recreation(renderer.swapchain_image_views(), &render_pass);

    let pipeline = {
        // First, we load the shaders that the pipeline will use: the vertex shader and the
        // fragment shader.
        //
        // A Vulkan shader can in theory contain multiple entry points, so we have to specify
        // which one.
        let vs = vs::load(context.device().clone())
            .unwrap()
            .entry_point("main")
            .unwrap();
        let fs = fs::load(context.device().clone())
            .unwrap()
            .entry_point("main")
            .unwrap();

        // Automatically generate a vertex input state from the vertex shader's input
        // interface, that takes a single vertex buffer containing `Vertex` structs.
        let vertex_input_state = MyVertex::per_vertex().definition(&vs).unwrap();

        // Make a list of the shader stages that the pipeline will have.
        let stages = [
            PipelineShaderStageCreateInfo::new(vs),
            PipelineShaderStageCreateInfo::new(fs),
        ];

        // We must now create a **pipeline layout** object, which describes the locations and
        // types of descriptor sets and push constants used by the shaders in the pipeline.
        //
        // Multiple pipelines can share a common layout object, which is more efficient. The
        // shaders in a pipeline must use a subset of the resources described in its pipeline
        // layout, but the pipeline layout is allowed to contain resources that are not present
        // in the shaders; they can be used by shaders in other pipelines that share the same
        // layout. Thus, it is a good idea to design shaders so that many pipelines have common
        // resource locations, which allows them to share pipeline layouts.
        let layout = PipelineLayout::new(
            context.device().clone(),
            // Since we only have one pipeline in this example, and thus one pipeline layout,
            // we automatically generate the creation info for it from the resources used in
            // the shaders. In a real application, you would specify this information manually
            // so that you can re-use one layout in multiple pipelines.
            PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
                .into_pipeline_layout_create_info(context.device().clone())
                .unwrap(),
        )
        .unwrap();

        // We have to indicate which subpass of which render pass this pipeline is going to be
        // used in. The pipeline will only be usable from this particular subpass.
        let subpass = Subpass::from(render_pass.clone(), 0).unwrap();

        // Finally, create the pipeline.
        GraphicsPipeline::new(context.device().clone(), None, GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            // How vertex data is read from the vertex buffers into the vertex shader.
            vertex_input_state: Some(vertex_input_state),
            // How vertices are arranged into primitive shapes. The default primitive shape
            // is a triangle.
            input_assembly_state: Some(InputAssemblyState::default()),
            // How primitives are transformed and clipped to fit the framebuffer. We use a
            // resizable viewport, set to draw over the entire window.
            viewport_state: Some(ViewportState::default()),
            // How polygons are culled and converted into a raster of pixels. The default
            // value does not perform any culling.
            rasterization_state: Some(RasterizationState::default()),
            // How multiple fragment shader samples are converted to a single pixel value.
            // The default value does not perform any multisampling.
            multisample_state: Some(MultisampleState::default()),
            // How pixel values are combined with the values already present in the
            // framebuffer. The default value overwrites the old value with the new one,
            // without any blending.
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState::default(),
            )),
            // Dynamic states allows us to specify parts of the pipeline settings when
            // recording the command buffer, before we perform drawing. Here, we specify
            // that the viewport should be dynamic.
            dynamic_state: [DynamicState::Viewport].into_iter().collect(),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        })
        .unwrap()
    };

    commands.insert_resource(Stuff {
        command_buffer_allocator: Arc::new(StandardCommandBufferAllocator::new(
            context.device().clone(),
            default(),
        )),
        vertices: vertex_buffer,
        render_pass,
        framebuffers,
        pipeline,
    });
}

fn render(
    mut renderers: VulkanoRenderers,
    stuff: Option<ResMut<Stuff>>,
    context: Res<BevyVulkanoContext>,
) {
    let Some(mut stuff) = stuff else {
        return;
    };

    let mut renderer = renderers.get_renderer_single().unwrap();

    let previous_frame_end = renderer
        .acquire(Some(Duration::from_millis(1000)), |swapchain_images| {
            stuff.framebuffers = on_swapchain_recreation(swapchain_images, &stuff.render_pass);
        })
        .unwrap();

    let mut builder = AutoCommandBufferBuilder::primary(
        stuff.command_buffer_allocator.clone(),
        context.graphics_queue().queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    builder
        // Before we can draw, we have to *enter a render pass*.
        .begin_render_pass(
            RenderPassBeginInfo {
                // A list of values to clear the attachments with. This list contains
                // one item for each attachment in the render pass. In this case, there
                // is only one attachment, and we clear it with a blue color.
                //
                // Only attachments that have `AttachmentLoadOp::Clear` are provided
                // with clear values, any others should use `None` as the clear value.
                clear_values: vec![Some([0.0, 0.0, 1.0, 1.0].into())],

                ..RenderPassBeginInfo::framebuffer(
                    stuff.framebuffers[renderer.image_index() as usize].clone(),
                )
            },
            SubpassBeginInfo {
                // The contents of the first (and only) subpass. This can be either
                // `Inline` or `SecondaryCommandBuffers`. The latter is a bit more
                // advanced and is not covered here.
                contents: SubpassContents::Inline,
                ..Default::default()
            },
        )
        .unwrap()
        // We are now inside the first subpass of the render pass.
        //
        // TODO: Document state setting and how it affects subsequent draw commands.
        .set_viewport(
            0,
            [Viewport {
                offset: [0.; 2],
                extent: renderer.window_size(),
                depth_range: 0.0..=1.,
            }]
            .into_iter()
            .collect(),
        )
        .unwrap()
        .bind_pipeline_graphics(stuff.pipeline.clone())
        .unwrap()
        .bind_vertex_buffers(0, stuff.vertices.clone())
        .unwrap();

    unsafe {
        builder
            // We add a draw command.
            .draw(stuff.vertices.len() as u32, 1, 0, 0)
            .unwrap();
    }

    builder
        // We leave the render pass. Note that if we had multiple subpasses we could
        // have called `next_subpass` to jump to the next subpass.
        .end_render_pass(Default::default())
        .unwrap();

    // Finish recording the command buffer by calling `end`.
    let command_buffer = builder.build().unwrap();

    let future = previous_frame_end
        .then_execute(context.graphics_queue().clone(), command_buffer)
        .unwrap()
        .boxed();

    // The color output is now expected to contain our triangle. But in order to show
    // it on the screen, we have to *present* the image by calling `present` on the
    // window renderer.
    //
    // This function does not actually present the image immediately. Instead it
    // submits a present command at the end of the queue. This means that it will only
    // be presented once the GPU has finished executing the command buffer that draws
    // the triangle.
    renderer.present(future, false);
}

/// This function is called whenever a swapchain is created.
fn on_swapchain_recreation(
    swapchain_images: &[Arc<ImageView>],
    render_pass: &Arc<RenderPass>,
) -> Vec<Arc<Framebuffer>> {
    swapchain_images
        .iter()
        .map(|swapchain_image| {
            Framebuffer::new(render_pass.clone(), FramebufferCreateInfo {
                attachments: vec![swapchain_image.clone()],
                ..Default::default()
            })
            .unwrap()
        })
        .collect::<Vec<_>>()
}
