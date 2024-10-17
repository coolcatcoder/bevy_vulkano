#![allow(
    clippy::needless_question_mark,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::module_inception,
    clippy::single_match,
    clippy::match_like_matches_macro
)]

mod config;
mod converters;
mod system;
mod vulkano_windows;
pub mod winit_event;
pub mod state;
pub mod accessibility;

use accessibility::{AccessKitAdapters, WinitActionRequestHandlers};
use bevy::{
    a11y::AccessibilityRequested, app::{App, AppExit, Plugin}, ecs::{
        event::{Events, ManualEventReader},
        system::{SystemParam, SystemState},
    }, input::{
        keyboard::KeyboardInput,
        mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel},
        touch::TouchInput,
    }, math::{ivec2, DVec2, Vec2}, prelude::*, utils::Instant, window::{
        exit_on_all_closed, CursorEntered, CursorLeft, CursorMoved, FileDragAndDrop, RawHandleWrapperHolder, ReceivedCharacter, RequestRedraw, WindowBackendScaleFactorChanged, WindowCloseRequested, WindowCreated, WindowFocused, WindowMoved, WindowResized, WindowScaleFactorChanged
    }
};
pub use config::*;
#[cfg(feature = "gui")]
pub use egui_winit_vulkano;
use state::winit_runner;
use vulkano_util::context::{VulkanoConfig, VulkanoContext};
pub use vulkano_windows::*;

/// Wrapper around [`VulkanoContext`] to allow using them as resources
#[derive(Resource, Deref, DerefMut)]
pub struct BevyVulkanoContext {
    pub context: VulkanoContext,
}

#[cfg(target_os = "android")]
pub use winit::platform::android::activity::AndroidApp;
use winit::{
    event::{self, DeviceEvent, Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, ActiveEventLoop},
};
use winit_event::WinitEvent;

use crate::system::{changed_windows, create_windows, despawn_windows, CachedWindow};

#[cfg(target_os = "android")]
pub static ANDROID_APP: once_cell::sync::OnceCell<AndroidApp> = once_cell::sync::OnceCell::new();

/// A [`Plugin`] that utilizes [`winit`] for window creation and event loop management.
/// In addition, windows include custom render functionality with Vulkano.
/// This is intended to replace `bevy_winit`.
#[derive(Default)]
pub struct VulkanoWinitPlugin {
    pub run_on_any_thread: bool,
}

impl Plugin for VulkanoWinitPlugin {
    fn name(&self) -> &str {
        "bevy_vulkano::VulkanoWinitPlugin"
    }

    fn build(&self, app: &mut App) {
        let mut event_loop_builder = EventLoop::<()>::with_user_event();

        // linux check is needed because x11 might be enabled on other platforms.
        #[cfg(all(target_os = "linux", feature = "x11"))]
        {
            use winit::platform::x11::EventLoopBuilderExtX11;

            // This allows a Bevy app to be started and ran outside the main thread.
            // A use case for this is to allow external applications to spawn a thread
            // which runs a Bevy app without requiring the Bevy app to need to reside on
            // the main thread, which can be problematic.
            event_loop_builder.with_any_thread(self.run_on_any_thread);
        }

        // linux check is needed because wayland might be enabled on other platforms.
        #[cfg(all(target_os = "linux", feature = "wayland"))]
        {
            use winit::platform::wayland::EventLoopBuilderExtWayland;
            event_loop_builder.with_any_thread(self.run_on_any_thread);
        }

        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            event_loop_builder.with_any_thread(self.run_on_any_thread);
        }

        #[cfg(target_os = "android")]
        {
            use winit::platform::android::EventLoopBuilderExtAndroid;
            let msg = "Bevy must be setup with the #[bevy_main] macro on Android";
            event_loop_builder.with_android_app(ANDROID_APP.get().expect(msg).clone());
        }

        // Retrieve config, or use default.
        let config = app.world_mut().remove_non_send_resource::<VulkanoConfig>().unwrap_or_default();

        // Create vulkano context using the vulkano config from settings
        let vulkano_context = BevyVulkanoContext {
            context: VulkanoContext::new(config),
        };

        app.init_non_send_resource::<BevyVulkanoWindows>()
            .insert_resource(vulkano_context)
            .init_resource::<WinitSettings>()
            .add_event::<WinitEvent>()
            .set_runner(winit_runner)
            // exit_on_all_closed only uses the query to determine if the query is empty,
            // and so doesn't care about ordering relative to changed_window
            .add_systems(
                Last,
                (
                    changed_windows.ambiguous_with(exit_on_all_closed),
                    // Update the state of the window before attempting to despawn to ensure consistent event ordering
                    despawn_windows.after(changed_windows),
                ),
            );

        let event_loop = event_loop_builder
        .build()
        .expect("Failed to build event loop");

        // `winit`'s windows are bound to the event loop that created them, so the event loop must
        // be inserted as a resource here to pass it onto the runner.
        app.insert_non_send_resource(event_loop);

        // TODO: Old stuff I think. Keeping it commented out, cause I don't really know what I'm doing.
        // #[cfg(feature = "gui")]
        // {
        //     app.add_systems(PreUpdate, begin_egui_frame_system);
        // }

        // let mut create_window_system_state: SystemState<(
        //     Commands,
        //     NonSendMut<EventLoop<()>>,
        //     Query<(Entity, &mut Window)>,
        //     EventWriter<WindowCreated>,
        //     NonSendMut<BevyVulkanoWindows>,
        //     Res<BevyVulkanoContext>,
        //     NonSend<BevyVulkanoSettings>,
        // )> = SystemState::from_world(&mut app.world);

        // // And for ios and macos, we should not create window early, all ui related code should be executed inside
        // // UIApplicationMain/NSApplicationMain.
        // #[cfg(not(any(target_os = "android", target_os = "ios", target_os = "macos")))]
        // {
        //     let (
        //         commands,
        //         event_loop,
        //         mut new_windows,
        //         event_writer,
        //         vulkano_windows,
        //         context,
        //         settings,
        //     ) = create_window_system_state.get_mut(&mut app.world);

        //     // Here we need to create a winit-window and give it a WindowHandle which the renderer can use.
        //     // It needs to be spawned before the start of the startup schedule, so we cannot use a regular system.
        //     // Instead we need to create the window and spawn it using direct world access
        //     create_window(
        //         commands,
        //         &event_loop,
        //         new_windows.iter_mut(),
        //         event_writer,
        //         vulkano_windows,
        //         context,
        //         settings,
        //     );
        // }

        // create_window_system_state.apply(&mut app.world);
    }
}

/// The default event that can be used to wake the window loop
/// Wakes up the loop if in wait state
#[derive(Debug, Default, Clone, Copy, Event)]
pub struct WakeUp;

/// The [`winit::event_loop::EventLoopProxy`].
///
/// The `EventLoopProxy` can be used to request a redraw from outside bevy.
///
/// Use `NonSend<EventLoopProxy>` to receive this resource.
pub type EventLoopProxy = winit::event_loop::EventLoopProxy<()>;

trait AppSendEvent {
    fn send(&mut self, event: impl Into<WinitEvent>);
}

impl AppSendEvent for Vec<WinitEvent> {
    fn send(&mut self, event: impl Into<WinitEvent>) {
        self.push(Into::<WinitEvent>::into(event));
    }
}

/// The parameters of the [`create_windows`] system.
pub type CreateWindowParams<'w, 's, F = ()> = (
    Commands<'w, 's>,
    Query<
        'w,
        's,
        (
            Entity,
            &'static mut Window,
            Option<&'static RawHandleWrapperHolder>,
        ),
        F,
    >,
    EventWriter<'w, WindowCreated>,
    NonSendMut<'w, BevyVulkanoWindows>,
    Res<'w, BevyVulkanoContext>,
    NonSendMut<'w, AccessKitAdapters>,
    ResMut<'w, WinitActionRequestHandlers>,
    Res<'w, AccessibilityRequested>,
);