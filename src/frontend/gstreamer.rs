use crate::core::{config::Config, game::PixelflutGame, image::PixelflutImage};
use glib::{object::ObjectExt, SourceId};
use gstreamer::{
    glib::object::Cast,
    prelude::{ElementExt, ElementExtManual, GstBinExt},
};
use gstreamer_app::{AppSrc, AppSrcCallbacks, AppStreamType};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

pub fn gstreamer_pipeline(config: &Config, game: &'static PixelflutGame) {
    gstreamer::init().unwrap();

    let mainloop = glib::MainLoop::new(None, true);

    let pipeline = gstreamer::parse::launch("appsrc block=true do-timestamp=true is-live=true name=input ! videoconvert ! tee name=branch")
        .expect("Failed to create pipeline");
    let pipeline: gstreamer::Bin = pipeline.downcast().unwrap();
    bus_dispatcher(&pipeline);
    let appsrc: gstreamer_app::AppSrc = pipeline.by_name("input").unwrap().downcast().unwrap();
    let tee = pipeline.by_name("branch").unwrap();

    appsrc.set_caps(Some(
        &gstreamer::Caps::builder_full()
            .structure(
                gstreamer::Structure::builder("video/x-raw")
                    .field("format", gstreamer_video::VideoFormat::Rgba.to_str())
                    .field("width", game.image().width as i32)
                    .field("height", game.image().height as i32)
                    .build(),
            )
            .build(),
    ));
    appsrc.set_stream_type(AppStreamType::Stream); // push-mode
    appsrc_handler(&appsrc, |appsrc| {
        println!("Meow");
        let buffer = scanout_image(game.image());
        appsrc.push_buffer(buffer).unwrap();
    });

    if config.gst_window {
        let videobranch =
            gstreamer::parse::bin_from_description("queue ! videoconvert ! autovideosink sync=false", true)
                .expect("Display branch");

        pipeline.add(&videobranch).unwrap();
        tee.link(&videobranch).unwrap();
    }

    if let Some(ref recordingfile) = config.record_to_file {
        // We need matroska because there is no graceful shutdown
        let recordingbranch = gstreamer::parse::bin_from_description(
            "queue ! videoconvert ! vah264enc ! h264parse ! matroskamux ! filesink name=file",
            true,
        )
        .expect("Recording branch");
        let filesink = recordingbranch.by_name("file").unwrap();

        filesink.set_property("location", recordingfile);

        pipeline.add(&recordingbranch).unwrap();
        tee.link(&recordingbranch).unwrap();
    }

    pipeline.set_state(gstreamer::State::Playing).unwrap();
    mainloop.run();
}

fn scanout_image(image: &PixelflutImage) -> gstreamer::Buffer {
    let mut buffer = gstreamer::Buffer::new();
    let mut memory = gstreamer::Memory::with_size(image.scanout_size());
    {
        let mut memory_mapw = memory.get_mut().unwrap().map_writable().unwrap();
        let memory_slice = memory_mapw.as_mut_slice();
        image.scanout(memory_slice);
    }
    buffer.get_mut().unwrap().append_memory(memory);
    buffer
}

fn appsrc_handler<F: FnMut(&AppSrc) + Send + 'static>(appsrc: &AppSrc, handler: F) {
    struct RegStateHandler<F> {
        handler: F,
        timer_source: Option<SourceId>,
    }
    struct RegState<F> {
        appsrc: AppSrc,
        // NOTE: This could be replaced with UnsafeCell due to gstreamers thread semantics, but I won't do this here.
        handler: Mutex<RegStateHandler<F>>,
    }
    let state = Arc::new(RegState {
        appsrc: appsrc.clone(),
        handler: Mutex::new(RegStateHandler {
            handler,
            timer_source: None,
        }),
    });

    appsrc.set_callbacks(
        AppSrcCallbacks::builder()
            .need_data({
                let state = state.clone();
                move |_appsrc, _count| {
                    let mut state_borrow = state.handler.lock().unwrap();

                    if state_borrow.timer_source.is_none() {
                        let state = state.clone();
                        state_borrow.timer_source =
                            Some(glib::timeout_add(Duration::from_millis(15), move || {
                                let mut state_borrow = state.handler.lock().unwrap();
                                (state_borrow.handler)(&state.appsrc);

                                glib::ControlFlow::Continue
                            }));
                    }
                }
            })
            .enough_data({
                let state = state.clone();
                move |_appsrc| {
                    let mut state_borrow = state.handler.lock().unwrap();
                    if let Some(source) = state_borrow.timer_source.take() {
                        source.remove();
                    }
                }
            })
            .build(),
    );
}

fn bus_dispatcher(pipeline: &gstreamer::Bin) {
    let bus = pipeline.bus().unwrap();
    bus.add_signal_watch();
    bus.connect_message(None, |_bus, message| match message.view() {
        gstreamer::MessageView::Error(error) => {
            let e = error.debug().unwrap();
            println!("Error: {e}");
        }
        gstreamer::MessageView::Warning(warning) => {
            let e = warning.debug().unwrap();
            println!("Warn: {e}");
        }
        gstreamer::MessageView::Info(info) => {
            let e = info.debug().unwrap();
            println!("Info: {e}");
        }
        _ => {}
    });
}
