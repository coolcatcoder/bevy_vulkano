#[allow(clippy::needless_question_mark)]
mod game_of_life;
#[allow(clippy::needless_question_mark)]
mod pixels_draw_pipeline;
mod place_over_frame;

use std::time::Duration;

use bevy::{
    app::PluginGroupBuilder, prelude::*, time::common_conditions::on_timer, window::WindowMode,
    winit::WakeUp,
};
use bevy_vulkano::{BevyVulkanoContext, VulkanoPlugin, VulkanoRenderers};

use crate::{game_of_life::GameOfLifeComputePipeline, place_over_frame::RenderPassPlaceOverFrame};

pub struct PluginBundle;

impl PluginGroup for PluginBundle {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<PluginBundle>()
            // Minimum plugins for the demo
            .add(bevy::log::LogPlugin::default())
            .add(bevy::core::TaskPoolPlugin::default())
            .add(bevy::core::TypeRegistrationPlugin)
            .add(bevy::core::FrameCountPlugin)
            .add(bevy::time::TimePlugin)
            .add(bevy::diagnostic::DiagnosticsPlugin)
            .add(bevy::input::InputPlugin)
            // Don't need render, but you will need these plugins:
            .add(bevy::a11y::AccessibilityPlugin)
            .add(bevy::window::WindowPlugin::default())
            .add(bevy::winit::WinitPlugin::<WakeUp>::default())
            .add(VulkanoPlugin)
    }
}

fn main() {
    App::new()
        .add_plugins(PluginBundle.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: (1024.0, 1024.0).into(),
                title: "Bevy Vulkano Game Of Life".to_string(),
                present_mode: bevy::window::PresentMode::Immediate,
                resizable: true,
                mode: WindowMode::Windowed,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, create_pipelines)
        //.add_systems(Update, close_on_esc)
        .add_systems(Update, draw_life_system)
        .add_systems(Update, update_window_title_system)
        .add_systems(
            PostUpdate,
            game_of_life_pipeline_system.run_if(on_timer(Duration::from_secs_f32(1.0 / 60.0))),
        )
        .run();
}

fn update_window_title_system(mut windows: Query<&mut Window>, time: ResMut<Time>) {
    for mut window in windows.iter_mut() {
        let fps = 1.0 / time.delta_seconds();
        window.title = format!("Bevy Vulkano Game Of Life {fps:.2}");
    }
}

/// Creates our simulation pipeline & render pipeline
fn create_pipelines(
    mut commands: Commands,
    window_query: Query<Entity, With<Window>>,
    context: Res<BevyVulkanoContext>,
    mut renderers: VulkanoRenderers,
) {
    let window_entity = window_query.single();
    let primary_window = renderers.get_renderer(window_entity).unwrap();
    // Create compute pipeline to simulate game of life
    let game_of_life_pipeline = GameOfLifeComputePipeline::new(
        context.memory_allocator(),
        primary_window.graphics_queue(),
        [512, 512],
    );
    // Create our render pass
    let place_over_frame = RenderPassPlaceOverFrame::new(
        context.memory_allocator().clone(),
        primary_window.graphics_queue(),
        primary_window.swapchain_format(),
    );
    // Insert resources
    commands.insert_resource(game_of_life_pipeline);
    commands.insert_resource(place_over_frame);
}

/// Draw life at mouse position on the game of life canvas
fn draw_life_system(
    mut game_of_life: ResMut<GameOfLifeComputePipeline>,
    window: Query<&Window>,
    mouse_input: Res<ButtonInput<MouseButton>>,
) {
    if mouse_input.pressed(MouseButton::Left) {
        let primary = window.get_single().unwrap();
        if let Some(pos) = primary.cursor_position() {
            let width = primary.width();
            let height = primary.height();
            let normalized = Vec2::new(
                (pos.x / width).clamp(0.0, 1.0),
                (pos.y / height).clamp(0.0, 1.0),
            );
            let image_size = game_of_life.color_image().image().extent();
            let draw_pos = IVec2::new(
                (image_size[0] as f32 * normalized.x) as i32,
                (image_size[1] as f32 - image_size[1] as f32 * normalized.y) as i32,
            );
            game_of_life.draw_life(draw_pos);
        }
    }
}

/// All render occurs here in one system. If you want to split systems to separate, use
/// `PipelineSyncData` to update futures. You could have `pre_render_system` and `post_render_system` to start and finish frames
fn game_of_life_pipeline_system(
    window_query: Query<Entity, With<Window>>,
    mut renderers: VulkanoRenderers,
    mut game_of_life: ResMut<GameOfLifeComputePipeline>,
    mut place_over_frame: ResMut<RenderPassPlaceOverFrame>,
) {
    if let Ok(window_entity) = window_query.get_single() {
        let mut primary_window = renderers.get_renderer(window_entity).unwrap();

        // Start frame
        let before = match primary_window.acquire(None, |_| {}) {
            Err(e) => {
                bevy::log::error!("Failed to start frame: {}", e);
                return;
            }
            Ok(f) => f,
        };

        let after_compute = game_of_life.compute(before, [1.0, 0.0, 0.0, 1.0], [0.0; 4]);
        let color_image = game_of_life.color_image();
        let final_image = primary_window.swapchain_image_view();
        let after_render = place_over_frame.render(after_compute, color_image, final_image);

        // Finish Frame
        primary_window.present(after_render, true);
    }
}
