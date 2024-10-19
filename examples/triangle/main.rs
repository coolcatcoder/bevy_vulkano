use bevy::prelude::*;
use bevy_vulkano::{VulkanoDefaultPlugins, VulkanoPlugin};

fn main() {
    App::new().add_plugins(VulkanoDefaultPlugins).run();
}
