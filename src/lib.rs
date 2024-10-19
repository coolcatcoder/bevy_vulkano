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

pub struct VulkanoDefaultPlugins;

impl PluginGroup for VulkanoDefaultPlugins {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();
        group = group
            .add(bevy::app::PanicHandlerPlugin)
            .add(bevy::log::LogPlugin::default())
            .add(bevy::core::TaskPoolPlugin::default())
            .add(bevy::core::TypeRegistrationPlugin)
            .add(bevy::core::FrameCountPlugin)
            .add(bevy::time::TimePlugin)
            .add(bevy::transform::TransformPlugin)
            .add(bevy::hierarchy::HierarchyPlugin)
            .add(bevy::diagnostic::DiagnosticsPlugin)
            .add(bevy::input::InputPlugin)
            .add(bevy::window::WindowPlugin::default())
            .add(bevy::a11y::AccessibilityPlugin)
            .add(VulkanoPlugin)
            .add::<bevy::winit::WinitPlugin>(bevy::winit::WinitPlugin::default());

        #[cfg(feature = "bevy::asset")]
        {
            group = group.add(bevy::asset::AssetPlugin::default());
        }

        #[cfg(feature = "bevy::scene")]
        {
            group = group.add(bevy::scene::ScenePlugin);
        }

        #[cfg(feature = "bevy::core_pipeline")]
        {
            group = group.add(bevy::core_pipeline::CorePipelinePlugin);
        }

        #[cfg(feature = "bevy::sprite")]
        {
            group = group.add(bevy::sprite::SpritePlugin);
        }

        #[cfg(feature = "bevy::text")]
        {
            group = group.add(bevy::text::TextPlugin);
        }

        #[cfg(feature = "bevy::ui")]
        {
            group = group.add(bevy::ui::UiPlugin);
        }

        #[cfg(feature = "bevy::pbr")]
        {
            group = group.add(bevy::pbr::PbrPlugin::default());
        }

        // NOTE: Load this after renderer initialization so that it knows about the supported
        // compressed texture formats
        #[cfg(feature = "bevy::gltf")]
        {
            group = group.add(bevy::gltf::GltfPlugin::default());
        }

        #[cfg(feature = "bevy::audio")]
        {
            group = group.add(bevy::audio::AudioPlugin::default());
        }

        #[cfg(feature = "bevy::gilrs")]
        {
            group = group.add(bevy::gilrs::GilrsPlugin);
        }

        #[cfg(feature = "bevy::animation")]
        {
            group = group.add(bevy::animation::AnimationPlugin);
        }

        #[cfg(feature = "bevy::gizmos")]
        {
            group = group.add(bevy::gizmos::GizmoPlugin);
        }

        #[cfg(feature = "bevy::state")]
        {
            group = group.add(bevy::state::app::StatesPlugin);
        }

        #[cfg(feature = "bevy::dev_tools")]
        {
            group = group.add(bevy::dev_tools::DevToolsPlugin);
        }

        #[cfg(feature = "bevy::ci_testing")]
        {
            group = group.add(bevy::dev_tools::ci_testing::CiTestingPlugin);
        }

        // TODO: ????
        //group = group.add(IgnoreAmbiguitiesPlugin);

        group
    }
}
