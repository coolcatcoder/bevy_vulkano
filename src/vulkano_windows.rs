// Mostly same as in `bevy::winit`, but modified to hold `VulkanoWindow` instead of just winit Window
use bevy::a11y::AccessibilityRequested;
use bevy::ecs::entity::Entity;

use bevy::ecs::entity::EntityHashMap;
use bevy::utils::{tracing::warn, HashMap};
use bevy::window::PresentMode;
use bevy::window::{
    CursorGrabMode, Window, WindowMode, WindowPosition, WindowResolution, WindowWrapper,
};

use vulkano_util::context::VulkanoContext;
use vulkano_util::renderer::VulkanoWindowRenderer;
use vulkano_util::window::{
    WindowDescriptor as VulkanoWindowDescriptor,
    WindowResizeConstraints as VulkanoWindowResizeConstraints,
};
use winit::{
    dpi::{LogicalSize, PhysicalPosition},
    event_loop::ActiveEventLoop,
    monitor::{MonitorHandle, VideoModeHandle},
    window::{CursorGrabMode as WinitCursorGrabMode, Fullscreen, Window as WinitWindow, WindowId},
};

use crate::accessibility::prepare_accessibility_for_window;
use crate::accessibility::AccessKitAdapters;
use crate::accessibility::WinitActionRequestHandlers;
use crate::
    converters::{convert_enabled_buttons, convert_window_level, convert_window_theme}
;

pub struct VulkanoWindow {
    pub renderer: VulkanoWindowRenderer,
    #[cfg(feature = "gui")]
    pub gui: Gui,
}

impl VulkanoWindow {
    pub fn window(&self) -> &winit::window::Window {
        self.renderer.window()
    }
}

/// A resource mapping window entities to their `winit`-backend [`Window`](winit::window::Window)
/// states.
#[derive(Default)]
pub struct BevyVulkanoWindows {
    /// Stores windows by window identifier.
    pub windows: HashMap<WindowId, VulkanoWindow>,
    /// Maps entities to `winit` window identifiers.
    pub entity_to_winit: EntityHashMap<WindowId>,
    /// Maps `winit` window identifiers to entities.
    pub winit_to_entity: HashMap<WindowId, Entity>,
    // Many `winit` window functions (e.g. `set_window_icon`) can only be called on the main thread.
    // If they're called on other threads, the program might hang. This marker indicates that this
    // type is not thread-safe and will be `!Send` and `!Sync`.
    _not_send_sync: core::marker::PhantomData<*const ()>,
}

impl BevyVulkanoWindows {
    /// Creates a `winit` window and associates it with our entity.
    pub fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        entity: Entity,
        window: &Window,
        adapters: &mut AccessKitAdapters,
        handlers: &mut WinitActionRequestHandlers,
        accessibility_requested: &AccessibilityRequested,
        vulkano_context: &VulkanoContext,
    ) -> &VulkanoWindow {
        let mut winit_window_attributes = WinitWindow::default_attributes();

        // Due to a UIA limitation, winit windows need to be invisible for the
        // AccessKit adapter is initialized.
        winit_window_attributes = winit_window_attributes.with_visible(false);

        winit_window_attributes = match window.mode {
            WindowMode::BorderlessFullscreen => winit_window_attributes
                .with_fullscreen(Some(Fullscreen::Borderless(event_loop.primary_monitor()))),
            mode @ (WindowMode::Fullscreen | WindowMode::SizedFullscreen) => {
                if let Some(primary_monitor) = event_loop.primary_monitor() {
                    let videomode = match mode {
                        WindowMode::Fullscreen => get_best_videomode(&primary_monitor),
                        WindowMode::SizedFullscreen => get_fitting_videomode(
                            &primary_monitor,
                            window.width() as u32,
                            window.height() as u32,
                        ),
                        _ => unreachable!(),
                    };

                    winit_window_attributes.with_fullscreen(Some(Fullscreen::Exclusive(videomode)))
                } else {
                    warn!("Could not determine primary monitor, ignoring exclusive fullscreen request for window {:?}", window.title);
                    winit_window_attributes
                }
            }
            WindowMode::Windowed => {
                if let Some(position) = winit_window_position(
                    &window.position,
                    &window.resolution,
                    event_loop.available_monitors(),
                    event_loop.primary_monitor(),
                    None,
                ) {
                    winit_window_attributes = winit_window_attributes.with_position(position);
                }

                let logical_size = LogicalSize::new(window.width(), window.height());
                if let Some(sf) = window.resolution.scale_factor_override() {
                    winit_window_attributes
                        .with_inner_size(logical_size.to_physical::<f64>(sf.into()))
                } else {
                    winit_window_attributes.with_inner_size(logical_size)
                }
            }
        };

        winit_window_attributes = winit_window_attributes
            .with_window_level(convert_window_level(window.window_level))
            .with_theme(window.window_theme.map(convert_window_theme))
            .with_resizable(window.resizable)
            .with_enabled_buttons(convert_enabled_buttons(window.enabled_buttons))
            .with_decorations(window.decorations)
            .with_transparent(window.transparent)
            .with_visible(window.visible);

        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            winit_window_attributes =
                winit_window_attributes.with_skip_taskbar(window.skip_taskbar);
        }

        #[cfg(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "windows"
        ))]
        if let Some(name) = &window.name {
            #[cfg(all(
                feature = "wayland",
                any(
                    target_os = "linux",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                )
            ))]
            {
                winit_window_attributes =
                    winit::platform::wayland::WindowAttributesExtWayland::with_name(
                        winit_window_attributes,
                        name.clone(),
                        "",
                    );
            }

            #[cfg(all(
                feature = "x11",
                any(
                    target_os = "linux",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                )
            ))]
            {
                winit_window_attributes = winit::platform::x11::WindowAttributesExtX11::with_name(
                    winit_window_attributes,
                    name.clone(),
                    "",
                );
            }
            #[cfg(target_os = "windows")]
            {
                winit_window_attributes =
                    winit::platform::windows::WindowAttributesExtWindows::with_class_name(
                        winit_window_attributes,
                        name.clone(),
                    );
            }
        }

        let constraints = window.resize_constraints.check_constraints();
        let min_inner_size = LogicalSize {
            width: constraints.min_width,
            height: constraints.min_height,
        };
        let max_inner_size = LogicalSize {
            width: constraints.max_width,
            height: constraints.max_height,
        };

        let winit_window_attributes =
            if constraints.max_width.is_finite() && constraints.max_height.is_finite() {
                winit_window_attributes
                    .with_min_inner_size(min_inner_size)
                    .with_max_inner_size(max_inner_size)
            } else {
                winit_window_attributes.with_min_inner_size(min_inner_size)
            };

        #[allow(unused_mut)]
        let mut winit_window_attributes = winit_window_attributes.with_title(window.title.as_str());

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;

            if let Some(selector) = &window.canvas {
                let window = web_sys::window().unwrap();
                let document = window.document().unwrap();
                let canvas = document
                    .query_selector(selector)
                    .expect("Cannot query for canvas element.");
                if let Some(canvas) = canvas {
                    let canvas = canvas.dyn_into::<web_sys::HtmlCanvasElement>().ok();
                    winit_window_attributes = winit_window_attributes.with_canvas(canvas);
                } else {
                    panic!("Cannot find element: {}.", selector);
                }
            }

            winit_window_attributes =
                winit_window_attributes.with_prevent_default(window.prevent_default_event_handling);
            winit_window_attributes = winit_window_attributes.with_append(true);
        }

        let winit_window = event_loop.create_window(winit_window_attributes).unwrap();
        let name = window.title.clone();
        prepare_accessibility_for_window(
            &winit_window,
            entity,
            name,
            accessibility_requested.clone(),
            adapters,
            handlers,
        );

        // Do not set the grab mode on window creation if it's none. It can fail on mobile.
        if window.cursor.grab_mode != CursorGrabMode::None {
            attempt_grab(&winit_window, window.cursor.grab_mode);
        }

        winit_window.set_cursor_visible(window.cursor.visible);

        // Do not set the cursor hittest on window creation if it's false, as it will always fail on
        // some platforms and log an unfixable warning.
        if !window.cursor.hit_test {
            if let Err(err) = winit_window.set_cursor_hittest(window.cursor.hit_test) {
                warn!(
                    "Could not set cursor hit test for window {:?}: {:?}",
                    window.title, err
                );
            }
        }

        let vulkano_window = {
            let pos = winit_window
                .inner_position()
                .ok()
                .map(|p| [p.x as f32, p.y as f32]);
            let window_renderer = VulkanoWindowRenderer::new(
                vulkano_context,
                winit_window,
                &window_descriptor_to_vulkano_window_descriptor(window, pos),
                move |ci| {
                    ci.image_format = vulkano::format::Format::B8G8R8A8_SRGB;
                    ci.min_image_count = ci.min_image_count.max(2);
                },
            );

            #[cfg(feature = "gui")]
            {
                let gui = Gui::new(
                    event_loop,
                    window_renderer.surface(),
                    window_renderer.graphics_queue(),
                    vulkano::format::Format::B8G8R8A8_SRGB,
                    GuiConfig {
                        is_overlay: _settings.is_gui_overlay,
                        allow_srgb_render_target: true,
                        ..Default::default()
                    },
                );
                VulkanoWindow {
                    renderer: window_renderer,
                    gui,
                }
            }
            #[cfg(not(feature = "gui"))]
            {
                VulkanoWindow {
                    renderer: window_renderer,
                }
            }
        };

        self.entity_to_winit.insert(entity, vulkano_window.window().id());
        self.winit_to_entity.insert(vulkano_window.window().id(), entity);

        self.windows
            .entry(vulkano_window.window().id())
            .insert(vulkano_window)
            .into_mut()
    }

    /// Get the winit window that is associated with our entity.
    pub fn get_window(&self, entity: Entity) -> Option<&VulkanoWindow> {
        self.entity_to_winit
            .get(&entity)
            .and_then(|winit_id| self.windows.get(winit_id))
    }

    /// Get the entity associated with the winit window id.
    ///
    /// This is mostly just an intermediary step between us and winit.
    pub fn get_window_entity(&self, winit_id: WindowId) -> Option<Entity> {
        self.winit_to_entity.get(&winit_id).cloned()
    }

    /// Remove a window from winit.
    ///
    /// This should mostly just be called when the window is closing.
    pub fn remove_window(&mut self, entity: Entity) -> Option<VulkanoWindow> {
        let winit_id = self.entity_to_winit.remove(&entity)?;
        self.winit_to_entity.remove(&winit_id);
        self.windows.remove(&winit_id)
    }
}

/// Gets the "best" video mode which fits the given dimensions.
///
/// The heuristic for "best" prioritizes width, height, and refresh rate in that order.
pub fn get_fitting_videomode(monitor: &MonitorHandle, width: u32, height: u32) -> VideoModeHandle {
    let mut modes = monitor.video_modes().collect::<Vec<_>>();

    fn abs_diff(a: u32, b: u32) -> u32 {
        if a > b {
            return a - b;
        }
        b - a
    }

    modes.sort_by(|a, b| {
        use std::cmp::Ordering::*;
        match abs_diff(a.size().width, width).cmp(&abs_diff(b.size().width, width)) {
            Equal => {
                match abs_diff(a.size().height, height).cmp(&abs_diff(b.size().height, height)) {
                    Equal => b
                        .refresh_rate_millihertz()
                        .cmp(&a.refresh_rate_millihertz()),
                    default => default,
                }
            }
            default => default,
        }
    });

    modes.first().unwrap().clone()
}

/// Gets the "best" video-mode handle from a monitor.
///
/// The heuristic for "best" prioritizes width, height, and refresh rate in that order.
pub fn get_best_videomode(monitor: &MonitorHandle) -> VideoModeHandle {
    let mut modes = monitor.video_modes().collect::<Vec<_>>();
    modes.sort_by(|a, b| {
        use std::cmp::Ordering::*;
        match b.size().width.cmp(&a.size().width) {
            Equal => match b.size().height.cmp(&a.size().height) {
                Equal => b
                    .refresh_rate_millihertz()
                    .cmp(&a.refresh_rate_millihertz()),
                default => default,
            },
            default => default,
        }
    });

    modes.first().unwrap().clone()
}

pub(crate) fn attempt_grab(winit_window: &WinitWindow, grab_mode: CursorGrabMode) {
    let grab_result = match grab_mode {
        CursorGrabMode::None => winit_window.set_cursor_grab(WinitCursorGrabMode::None),
        CursorGrabMode::Confined => winit_window
            .set_cursor_grab(WinitCursorGrabMode::Confined)
            .or_else(|_e| winit_window.set_cursor_grab(WinitCursorGrabMode::Locked)),
        CursorGrabMode::Locked => winit_window
            .set_cursor_grab(WinitCursorGrabMode::Locked)
            .or_else(|_e| winit_window.set_cursor_grab(WinitCursorGrabMode::Confined)),
    };

    if let Err(err) = grab_result {
        let err_desc = match grab_mode {
            CursorGrabMode::Confined | CursorGrabMode::Locked => "grab",
            CursorGrabMode::None => "ungrab",
        };

        bevy::utils::tracing::error!("Unable to {} cursor: {}", err_desc, err);
    }
}

/// Compute the physical window position for a given [`WindowPosition`].
// Ideally we could generify this across window backends, but we only really have winit atm
// so whatever.
pub fn winit_window_position(
    position: &WindowPosition,
    resolution: &WindowResolution,
    mut available_monitors: impl Iterator<Item = MonitorHandle>,
    primary_monitor: Option<MonitorHandle>,
    current_monitor: Option<MonitorHandle>,
) -> Option<PhysicalPosition<i32>> {
    match position {
        WindowPosition::Automatic => {
            /* Window manager will handle position */
            None
        }
        WindowPosition::Centered(monitor_selection) => {
            use bevy::window::MonitorSelection::*;
            let maybe_monitor = match monitor_selection {
                Current => {
                    if current_monitor.is_none() {
                        warn!("Can't select current monitor on window creation or cannot find current monitor!");
                    }
                    current_monitor
                }
                Primary => primary_monitor,
                Index(n) => available_monitors.nth(*n),
            };

            if let Some(monitor) = maybe_monitor {
                let screen_size = monitor.size();

                let scale_factor = match resolution.scale_factor_override() {
                    Some(scale_factor_override) => scale_factor_override as f64,
                    // We use the monitors scale factor here since `WindowResolution.scale_factor` is
                    // not yet populated when windows are created during plugin setup.
                    None => monitor.scale_factor(),
                };

                // Logical to physical window size
                let (width, height): (u32, u32) =
                    LogicalSize::new(resolution.width(), resolution.height())
                        .to_physical::<u32>(scale_factor)
                        .into();

                let position = PhysicalPosition {
                    x: screen_size.width.saturating_sub(width) as f64 / 2.
                        + monitor.position().x as f64,
                    y: screen_size.height.saturating_sub(height) as f64 / 2.
                        + monitor.position().y as f64,
                };

                Some(position.cast::<i32>())
            } else {
                warn!("Couldn't get monitor selected with: {monitor_selection:?}");
                None
            }
        }
        WindowPosition::At(position) => {
            Some(PhysicalPosition::new(position[0] as f64, position[1] as f64).cast::<i32>())
        }
    }
}

fn window_descriptor_to_vulkano_window_descriptor(
    wd: &Window,
    position: Option<[f32; 2]>,
) -> VulkanoWindowDescriptor {
    let mut window_descriptor = VulkanoWindowDescriptor::default();
    window_descriptor.width = wd.width();
    window_descriptor.height = wd.height();
    window_descriptor.position = position;
    window_descriptor.resize_constraints = VulkanoWindowResizeConstraints {
        min_width: wd.resize_constraints.min_width,
        min_height: wd.resize_constraints.min_height,
        max_width: wd.resize_constraints.max_width,
        max_height: wd.resize_constraints.max_height,
    };
    window_descriptor.scale_factor_override = wd.resolution.scale_factor_override().map(|sfo|sfo.into());
    window_descriptor.title = wd.title.clone();
    window_descriptor.present_mode = match wd.present_mode {
        PresentMode::Fifo => vulkano::swapchain::PresentMode::Fifo,
        PresentMode::Immediate => vulkano::swapchain::PresentMode::Immediate,
        PresentMode::Mailbox => vulkano::swapchain::PresentMode::Mailbox,
        PresentMode::AutoNoVsync => vulkano::swapchain::PresentMode::Immediate,
        PresentMode::AutoVsync => vulkano::swapchain::PresentMode::FifoRelaxed,
        PresentMode::FifoRelaxed => vulkano::swapchain::PresentMode::FifoRelaxed,
    };
    window_descriptor.resizable = wd.resizable;
    window_descriptor.decorations = wd.decorations;
    window_descriptor.cursor_visible = wd.cursor.visible;
    window_descriptor.cursor_locked = match wd.cursor.grab_mode {
        bevy::window::CursorGrabMode::Locked => true,
        _ => false,
    };
    window_descriptor.mode = match wd.mode {
        WindowMode::Windowed => vulkano_util::window::WindowMode::Windowed,
        WindowMode::Fullscreen => vulkano_util::window::WindowMode::Fullscreen,
        WindowMode::BorderlessFullscreen => vulkano_util::window::WindowMode::BorderlessFullscreen,
        WindowMode::SizedFullscreen => vulkano_util::window::WindowMode::SizedFullscreen,
    };
    window_descriptor.transparent = wd.transparent;
    window_descriptor
}