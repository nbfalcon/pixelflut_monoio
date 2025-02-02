    use crate::core::{config::Config, game::PixelflutGame};
use std::{cmp::min, num::NonZeroU32};
use winit::{dpi::PhysicalSize, event::Event, event_loop::EventLoopBuilder};

pub fn winit_window_loop(config: &Config, game: &mut PixelflutGame) {
    let event_loop = EventLoopBuilder::new()
        .build()
        .expect("Failed to init event loop");
    let window = winit::window::WindowBuilder::new()
        .with_inner_size(PhysicalSize::new(config.image_width, config.image_height))
        .with_title("Pixelflut (Monoio)")
        .build(&event_loop)
        .expect("Failed to create window");
    let window_ctx = softbuffer::Context::new(&window).expect("Failed to initialize softbuffer");
    let mut window_surface =
        softbuffer::Surface::new(&window_ctx, &window).expect("Failed to create surface");

    event_loop
        .run(|event, _window| {
            if let Event::WindowEvent {
                event: winit::event::WindowEvent::RedrawRequested,
                window_id,
            } = event
                && window_id == window.id()
            {
                // 1. Acquire buffer we can draw to
                let PhysicalSize {
                    width: window_width,
                    height: window_height,
                } = window.inner_size();
                window_surface
                    .resize(
                        NonZeroU32::new(window_width).unwrap(),
                        NonZeroU32::new(window_height).unwrap(),
                    )
                    .expect("Resize Surface??");
                let mut buffer = window_surface.buffer_mut().unwrap();

                // TODO: maybe handle the fast path where width == width && height == height? Then we denegerate to memcpy. Maybe we can also just use gstreamer
                // and be done with it.
                // 2. Acquire an image from pixelflut
                let current_image = {
                    game.combine_all();
                    game.image()
                };
                for y in 0..min(current_image.height, window_height) {
                    for x in 0..min(current_image.width, window_width) {
                        // FIXME: is this cast sound?
                        let i_buffer = window_width * y + x;
                        buffer[i_buffer as usize] = current_image.get_pixel(x, y).into_rgba();
                    }
                }

                // 3. Display
                window.pre_present_notify();
                window.request_redraw();
                buffer.present().expect("Present buffer");
            }
        })
        .expect("Event loop");
}
