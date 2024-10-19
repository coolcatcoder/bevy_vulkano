// A heavily tweaked VulkanoWindowRenderer. To find the changes, you can ctrl f tweak.

use std::{sync::Arc, time::Duration};

use bevy::utils::HashMap;
use vulkano::{
    device::{Device, Queue},
    format::Format,
    image::{view::ImageView, Image, ImageCreateInfo, ImageType, ImageUsage},
    memory::allocator::{AllocationCreateInfo, StandardMemoryAllocator},
    swapchain::{self, PresentMode, Surface, Swapchain, SwapchainCreateInfo, SwapchainPresentInfo},
    sync::{self, GpuFuture},
    Validated, VulkanError,
};
use vulkano_util::context::VulkanoContext;
use winit::window::Window;

// There is no way to get an arc, or a window wrapper, or anything from bevy winit. Greedy plugin. At best we can borrow from it.
pub struct VulkanoWindowRendererWithoutWindow {
    graphics_queue: Arc<Queue>,
    compute_queue: Arc<Queue>,
    swapchain: Arc<Swapchain>,
    final_views: Vec<Arc<ImageView>>,
    memory_allocator: Arc<StandardMemoryAllocator>,
    /// Additional image views that you can add which are resized with the window.
    /// Use associated functions to get access to these.
    additional_image_views: HashMap<usize, Arc<ImageView>>,
    recreate_swapchain: bool,
    previous_frame_end: Option<Box<dyn GpuFuture>>,
    image_index: u32,
    present_mode: PresentMode,
}

impl VulkanoWindowRendererWithoutWindow {
    /// Creates a new [`VulkanoWindowRendererWithoutWindow`] which is used to orchestrate your rendering with
    /// Vulkano. Pass [`WindowDescriptor`] and optionally a function modifying the
    /// [`SwapchainCreateInfo`] parameters.
    pub fn new(
        vulkano_context: &VulkanoContext,
        // tweak
        window: &Window,
        // tweak
        present_mode: PresentMode,
        swapchain_create_info_modify: fn(&mut SwapchainCreateInfo),
    ) -> Self {
        // tweak
        //let window = Arc::new(window);

        // Create swap chain & frame(s) to which we'll render
        let (swap_chain, final_views) = VulkanoWindowRenderer::create_swapchain(
            vulkano_context.device().clone(),
            window,
            present_mode,
            swapchain_create_info_modify,
        );

        let previous_frame_end = Some(sync::now(vulkano_context.device().clone()).boxed());

        Self {
            graphics_queue: vulkano_context.graphics_queue().clone(),
            compute_queue: vulkano_context.compute_queue().clone(),
            swapchain: swap_chain,
            final_views,
            memory_allocator: vulkano_context.memory_allocator().clone(),
            additional_image_views: HashMap::default(),
            recreate_swapchain: false,
            previous_frame_end,
            image_index: 0,
            present_mode,
        }
    }
}

/// A window renderer struct holding the winit window surface and functionality for organizing your
/// render between frames.
///
/// Begin rendering with [`VulkanoWindowRenderer::acquire`] and finish with
/// [`VulkanoWindowRenderer::present`]. Between those, you should execute your command buffers.
///
/// The intended usage of this struct is through [`crate::window::VulkanoWindows`].
pub struct VulkanoWindowRenderer<'a, 'b> {
    // tweak
    window: &'a Window,
    // tweak
    without_window: &'b mut VulkanoWindowRendererWithoutWindow,
}

impl<'a, 'b> VulkanoWindowRenderer<'a, 'b> {
    // tweak
    pub fn new(
        window: &'a Window,
        without_window: &'b mut VulkanoWindowRendererWithoutWindow,
    ) -> Self {
        Self {
            window,
            without_window,
        }
    }

    /// Creates the swapchain and its images based on [`WindowDescriptor`]. The swapchain creation
    /// can be modified with the `swapchain_create_info_modify` function passed as an input.
    fn create_swapchain(
        device: Arc<Device>,
        window: &Window,
        // tweak
        present_mode: PresentMode,
        swapchain_create_info_modify: fn(&mut SwapchainCreateInfo),
    ) -> (Arc<Swapchain>, Vec<Arc<ImageView>>) {
        // tweak
        // I think this is safe?
        let surface =
            unsafe { Surface::from_window_ref(device.instance().clone(), window).unwrap() };
        let surface_capabilities = device
            .physical_device()
            .surface_capabilities(&surface, Default::default())
            .unwrap();
        let image_format = device
            .physical_device()
            .surface_formats(&surface, Default::default())
            .unwrap()[0]
            .0;
        let (swapchain, images) = Swapchain::new(device, surface, {
            let mut create_info = SwapchainCreateInfo {
                min_image_count: surface_capabilities.min_image_count.max(2),
                image_format,
                image_extent: window.inner_size().into(),
                image_usage: ImageUsage::COLOR_ATTACHMENT,
                composite_alpha: surface_capabilities
                    .supported_composite_alpha
                    .into_iter()
                    .next()
                    .unwrap(),
                ..Default::default()
            };

            // tweak
            create_info.present_mode = present_mode;
            swapchain_create_info_modify(&mut create_info);
            create_info
        })
        .unwrap();
        let images = images
            .into_iter()
            .map(|image| ImageView::new_default(image).unwrap())
            .collect::<Vec<_>>();

        (swapchain, images)
    }

    /// Set window renderer present mode. This triggers a swapchain recreation.
    #[inline]
    pub fn set_present_mode(&mut self, present_mode: PresentMode) {
        if self.without_window.present_mode != present_mode {
            self.without_window.present_mode = present_mode;
            self.without_window.recreate_swapchain = true;
        }
    }

    /// Return swapchain image format.
    #[inline]
    pub fn swapchain_format(&self) -> Format {
        self.without_window.final_views[self.without_window.image_index as usize].format()
    }

    /// Returns the index of last swapchain image that is the next render target.
    #[inline]
    pub fn image_index(&self) -> u32 {
        self.without_window.image_index
    }

    /// Graphics queue of this window. You also can access this through [`VulkanoContext`].
    #[inline]
    pub fn graphics_queue(&self) -> Arc<Queue> {
        self.without_window.graphics_queue.clone()
    }

    /// Compute queue of this window. You can also access this through [`VulkanoContext`].
    #[inline]
    pub fn compute_queue(&self) -> Arc<Queue> {
        self.without_window.compute_queue.clone()
    }

    /// Render target surface.
    #[inline]
    pub fn surface(&self) -> Arc<Surface> {
        self.without_window.swapchain.surface().clone()
    }

    /// Winit window (you can manipulate the window through this).
    #[inline]
    pub fn window(&self) -> &Window {
        self.window
    }

    /// Size of the physical window.
    #[inline]
    pub fn window_size(&self) -> [f32; 2] {
        let size = self.window().inner_size();
        [size.width as f32, size.height as f32]
    }

    /// Size of the final swapchain image (surface).
    #[inline]
    pub fn swapchain_image_size(&self) -> [u32; 2] {
        self.without_window.final_views[0].image().extent()[0..2]
            .try_into()
            .unwrap()
    }

    /// Return the current swapchain image view.
    #[inline]
    pub fn swapchain_image_view(&self) -> Arc<ImageView> {
        self.without_window.final_views[self.without_window.image_index as usize].clone()
    }

    /// Return scale factor accounted window size.
    #[inline]
    pub fn resolution(&self) -> [f32; 2] {
        let size = self.window().inner_size();
        let scale_factor = self.window().scale_factor();
        [
            (size.width as f64 / scale_factor) as f32,
            (size.height as f64 / scale_factor) as f32,
        ]
    }

    #[inline]
    pub fn aspect_ratio(&self) -> f32 {
        let dims = self.window_size();
        dims[0] / dims[1]
    }

    /// Returns a reference to the swapchain image views.
    #[inline]
    #[must_use]
    // swapchain_image_views or swapchain_images_views, neither sounds good.
    pub fn swapchain_image_views(&self) -> &[Arc<ImageView>] {
        // Why do we use "final views" as the field name,
        // yet always externally refer to them as "swapchain image views"?
        &self.without_window.final_views
    }

    /// Resize swapchain and camera view images at the beginning of next frame based on window
    /// size.
    #[inline]
    pub fn resize(&mut self) {
        self.without_window.recreate_swapchain = true;
    }

    /// Add interim image view that resizes with window.
    #[inline]
    pub fn add_additional_image_view(&mut self, key: usize, format: Format, usage: ImageUsage) {
        let final_view_image = self.without_window.final_views[0].image();
        let image = ImageView::new_default(
            Image::new(
                self.without_window.memory_allocator.clone(),
                ImageCreateInfo {
                    image_type: ImageType::Dim2d,
                    format,
                    extent: final_view_image.extent(),
                    usage,
                    ..Default::default()
                },
                AllocationCreateInfo::default(),
            )
            .unwrap(),
        )
        .unwrap();
        self.without_window
            .additional_image_views
            .insert(key, image);
    }

    /// Get additional image view by key.
    #[inline]
    pub fn get_additional_image_view(&mut self, key: usize) -> Arc<ImageView> {
        self.without_window
            .additional_image_views
            .get(&key)
            .unwrap()
            .clone()
    }

    /// Remove additional image by key.
    #[inline]
    pub fn remove_additional_image_view(&mut self, key: usize) {
        self.without_window.additional_image_views.remove(&key);
    }

    /// Begin your rendering by calling `acquire`.
    /// 'on_recreate_swapchain' is called when the swapchain gets recreated, due to being resized,
    /// suboptimal, or changing the present mode. Returns a [`GpuFuture`] representing the time
    /// after which the swapchain image has been acquired and previous frame ended.
    /// Execute your command buffers after calling this function and
    /// finish rendering by calling [`VulkanoWindowRenderer::present`].
    #[inline]
    pub fn acquire(
        &mut self,
        timeout: Option<Duration>,
        on_recreate_swapchain: impl FnOnce(&[Arc<ImageView>]),
    ) -> Result<Box<dyn GpuFuture>, VulkanError> {
        // Recreate swap chain if needed (when resizing of window occurs or swapchain is outdated)
        // Also resize render views if needed
        if self.without_window.recreate_swapchain {
            self.recreate_swapchain_and_views();
            on_recreate_swapchain(&self.without_window.final_views);
        }

        // Acquire next image in the swapchain
        let (image_index, suboptimal, acquire_future) =
            match swapchain::acquire_next_image(self.without_window.swapchain.clone(), timeout)
                .map_err(Validated::unwrap)
            {
                Ok(r) => r,
                Err(VulkanError::OutOfDate) => {
                    self.without_window.recreate_swapchain = true;
                    return Err(VulkanError::OutOfDate);
                }
                Err(e) => panic!("failed to acquire next image: {e}"),
            };
        if suboptimal {
            self.without_window.recreate_swapchain = true;
        }
        // Update our image index
        self.without_window.image_index = image_index;

        let future = self
            .without_window
            .previous_frame_end
            .take()
            .unwrap()
            .join(acquire_future);

        Ok(future.boxed())
    }

    /// Finishes rendering by presenting the swapchain. Pass your last future as an input to this
    /// function.
    ///
    /// Depending on your implementation, you may want to wait on your future. For example, a
    /// compute shader dispatch using an image that's being later drawn should probably be waited
    /// on.
    #[inline]
    pub fn present(&mut self, after_future: Box<dyn GpuFuture>, wait_future: bool) {
        let future = after_future
            .then_swapchain_present(
                self.without_window.graphics_queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(
                    self.without_window.swapchain.clone(),
                    self.without_window.image_index,
                ),
            )
            .then_signal_fence_and_flush();
        match future.map_err(Validated::unwrap) {
            Ok(mut future) => {
                if wait_future {
                    future.wait(None).unwrap_or_else(|e| println!("{e}"))
                    // wait allows you to organize resource waiting yourself.
                } else {
                    future.cleanup_finished();
                }

                self.without_window.previous_frame_end = Some(future.boxed());
            }
            Err(VulkanError::OutOfDate) => {
                self.without_window.recreate_swapchain = true;
                self.without_window.previous_frame_end =
                    Some(sync::now(self.without_window.graphics_queue.device().clone()).boxed());
            }
            Err(e) => {
                println!("failed to flush future: {e}");
                self.without_window.previous_frame_end =
                    Some(sync::now(self.without_window.graphics_queue.device().clone()).boxed());
            }
        }
    }

    /// Recreates swapchain images and image views which follow the window size.
    fn recreate_swapchain_and_views(&mut self) {
        let image_extent: [u32; 2] = self.window().inner_size().into();

        if image_extent.contains(&0) {
            return;
        }

        let (new_swapchain, new_images) = self
            .without_window
            .swapchain
            .recreate(SwapchainCreateInfo {
                image_extent,
                // Use present mode from current state
                present_mode: self.without_window.present_mode,
                ..self.without_window.swapchain.create_info()
            })
            .expect("failed to recreate swapchain");

        self.without_window.swapchain = new_swapchain;
        let new_images = new_images
            .into_iter()
            .map(|image| ImageView::new_default(image).unwrap())
            .collect::<Vec<_>>();
        self.without_window.final_views = new_images;
        // Resize images that follow swapchain size
        let resizable_views = self
            .without_window
            .additional_image_views
            .iter()
            .map(|c| *c.0)
            .collect::<Vec<usize>>();
        for i in resizable_views {
            let format = self.get_additional_image_view(i).format();
            let usage = self.get_additional_image_view(i).usage();
            self.remove_additional_image_view(i);
            self.add_additional_image_view(i, format, usage);
        }
        self.without_window.recreate_swapchain = false;
    }
}
