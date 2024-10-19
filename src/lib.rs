use std::sync::Arc;
use bevy::prelude::*;
use vulkano::{device::{physical::{PhysicalDevice, PhysicalDeviceType}, DeviceExtensions, DeviceFeatures}, instance::{debug::DebugUtilsMessengerCreateInfo, InstanceCreateInfo}, Version};
use vulkano_util::context::{VulkanoConfig, VulkanoContext};
use vulkano_windows::{create_renderer, destroy_renderer, update_present_mode};

pub mod vulkano_windows;
pub mod renderer;

pub struct VulkanoPlugin;

impl Plugin for VulkanoPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send_resource::<VulkanoContext>()
        .add_systems(PostUpdate, (create_renderer, update_present_mode.after(create_renderer), destroy_renderer));
    }
}