use bevy::{app::PluginGroupBuilder, ecs::entity::EntityHashMap, prelude::*};
use renderer::VulkanoWindowRendererWithoutWindow;
use vulkano_renderers::{create_renderer, destroy_renderer, resize, update_present_mode};
use vulkano_util::context::VulkanoContext;

pub mod renderer;
pub mod vulkano_renderers;

pub use vulkano_renderers::VulkanoRenderers;

#[derive(Resource, Deref, DerefMut, Default)]
pub struct BevyVulkanoContext(VulkanoContext);

pub struct VulkanoPlugin;

impl Plugin for VulkanoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BevyVulkanoContext>()
            .init_non_send_resource::<EntityHashMap<VulkanoWindowRendererWithoutWindow>>()
            // Systems in startup can access a renderer immediately with this, I hope.
            .add_systems(PreStartup, create_renderer)
            .add_systems(
                PostUpdate,
                (
                    create_renderer,
                    (update_present_mode, resize).after(create_renderer),
                    destroy_renderer,
                ),
            );
    }
}
