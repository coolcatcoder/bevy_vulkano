use std::sync::Arc;
use bevy::{ecs::entity::EntityHashMap, prelude::*};
use renderer::VulkanoWindowRendererWithoutWindow;
use vulkano::{device::{physical::{PhysicalDevice, PhysicalDeviceType}, DeviceExtensions, DeviceFeatures}, instance::{debug::DebugUtilsMessengerCreateInfo, InstanceCreateInfo}, Version};
use vulkano_util::context::{VulkanoConfig, VulkanoContext};
use vulkano_renderers::{create_renderer, destroy_renderer, update_present_mode};

pub mod vulkano_renderers;
pub mod renderer;

pub use vulkano_renderers::VulkanoRenderers;

#[derive(Resource, Deref, DerefMut, Default)]
pub struct BevyVulkanoContext(VulkanoContext);

pub struct VulkanoPlugin;

impl Plugin for VulkanoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BevyVulkanoContext>()
        .init_non_send_resource::<EntityHashMap<VulkanoWindowRendererWithoutWindow>>()
        .add_systems(PostUpdate, (create_renderer, update_present_mode.after(create_renderer), destroy_renderer));
    }
}