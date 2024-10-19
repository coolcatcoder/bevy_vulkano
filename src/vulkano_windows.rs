use bevy::{ecs::{entity::EntityHashMap, system::SystemParam}, prelude::*, window::{PresentMode, WindowClosing, WindowCreated, WindowMode}, winit::WinitWindows};
use vulkano_util::context::VulkanoContext;

use crate::renderer::{VulkanoWindowRenderer, VulkanoWindowRendererWithoutWindow};

#[derive(SystemParam)]
pub struct VulkanoRenderers<'w> {
    pub renderers: NonSendMut<'w, EntityHashMap<VulkanoWindowRendererWithoutWindow>>,
    pub windows: NonSend<'w, WinitWindows>,
}

impl<'w> VulkanoRenderers<'w> {
    pub fn get_renderer(&mut self, entity: Entity) -> Option<VulkanoWindowRenderer> {
        let window = self.windows.get_window(entity)?;
        let renderer = self.renderers.get_mut(&entity)?;

        Some(VulkanoWindowRenderer::new(window, renderer))
    }
}

/// When a window is created, we hook vulkano into it.
pub fn create_renderer(context: NonSend<VulkanoContext>, mut renderers: VulkanoRenderers, mut windows_created: EventReader<WindowCreated>, windows: Query<&Window>) {
    for window_created in windows_created.read() {
        let window_entity = window_created.window;

        if renderers.renderers.contains_key(&window_entity) {
            error!("We were told that a window was created, but that window already exists according to vulkano... What have you done?");
        } else {
            let Some(window) = renderers.windows.get_window(window_created.window) else {
                error!("This shouldn't happen! Somehow a window both exists and doesn't exist!");
                continue;
            };

            let present_mode = {
                let Ok(window) = windows.get(window_entity) else {
                    error!("This shouldn't happen! Somehow a window both exists and doesn't exist!");
                    continue;
                };

                bevy_to_vulkano_present_mode(window.present_mode)
            };

            let renderer = VulkanoWindowRendererWithoutWindow::new(&context, window, present_mode, |_|{});

            // Safe, as the if statement already checked if it contained a key.
            renderers.renderers.insert_unique_unchecked(window_created.window, renderer);
        };
    }
}

pub fn update_present_mode(mut renderers: VulkanoRenderers, windows: Query<(Entity, &Window), Changed<Window>>) {
    for (entity, window) in &windows {
        let Some(mut renderer) = renderers.get_renderer(entity) else {
            error!("A window was found without a renderer!");
            continue;
        };

        // Only triggers a swapchain recreation if it was actually changed, don't worry!
        renderer.set_present_mode(bevy_to_vulkano_present_mode(window.present_mode));
    }
}

pub fn destroy_renderer(mut renderers: VulkanoRenderers, mut windows_closing: EventReader<WindowClosing>) {
    for window_closing in windows_closing.read() {
        renderers.renderers.remove(&window_closing.window);
    }
}

pub fn bevy_to_vulkano_present_mode(present_mode: PresentMode) -> vulkano::swapchain::PresentMode {
    match present_mode {
        PresentMode::Fifo => vulkano::swapchain::PresentMode::Fifo,
        PresentMode::Immediate => vulkano::swapchain::PresentMode::Immediate,
        PresentMode::Mailbox => vulkano::swapchain::PresentMode::Mailbox,
        PresentMode::AutoNoVsync => vulkano::swapchain::PresentMode::Immediate,
        PresentMode::AutoVsync => vulkano::swapchain::PresentMode::FifoRelaxed,
        PresentMode::FifoRelaxed => vulkano::swapchain::PresentMode::FifoRelaxed,
    }
}