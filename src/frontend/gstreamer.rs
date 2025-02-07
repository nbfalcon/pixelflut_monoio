use crate::core::{config::Config, game::PixelflutGame, image::{self, PixelflutImage}};
use glib::object::ObjectExt;
use gstreamer::{
    glib::object::Cast,
    prelude::{ElementExt, ElementExtManual, GstBinExt},
};
use gstreamer_app::AppSrcCallbacks;

pub fn gstreamer_pipeline(config: &Config, game: &'static PixelflutGame) {
    gstreamer::init().unwrap();

    let width = game.image().width;
    let height = game.image().height;

    let pipeline = gstreamer::parse::launch(
        "appsrc name=input ! videoconvert ! tee name=branch",
    )
    .expect("Failed to create pipeline");
    let pipeline: gstreamer::Bin = pipeline.downcast().unwrap();
    let appsrc: gstreamer_app::AppSrc = pipeline.by_name("input").unwrap().downcast().unwrap();
    let tee = pipeline.by_name("branch").unwrap();

    appsrc.set_caps(Some(
        &gstreamer::Caps::builder_full()
            .structure(
                gstreamer::Structure::builder("video/x-raw")
                    .field("format", gstreamer_video::VideoFormat::Rgba.to_str())
                    .field("width", width as i32)
                    .field("height", height as i32)
                    .build(),
            )
            .build(),
    ));
    // FIXME: appsrc does not work in "pull-mode", because if we have another stream (like vah264enc), it will be forced into "push-mode".
    // What we actually want is some kind of appsrc that has a "periodic callback", and will be called every
    // 16ms or something like that. It'd automatically handle need_data/stop_data
    appsrc.set_stream_type(gstreamer_app::AppStreamType::Stream);
    appsrc.set_callbacks(
        AppSrcCallbacks::builder()
            .need_data(|appsrc, _count| {
                println!("Pushed buffer");
                let buffer = scanout_image(game);

                appsrc.push_buffer(buffer).unwrap();
            })
            .build(),
    );

    if config.gst_window {
        let videosink = gstreamer::ElementFactory::make("autovideosink")
            .build()
            .expect("autovideosink");
        pipeline.add(&videosink).unwrap();
        tee.link(&videosink).unwrap();
    }

    if let Some(ref recordingfile) = config.record_to_file {
        let recordingbranch = gstreamer::parse::bin_from_description(
            "queue ! videoconvert ! vah264enc ! h264parse ! matroskamux ! filesink name=file",
            true,
        )
        .expect("encoding branch");
        let filesink = recordingbranch.by_name("file").unwrap();
        filesink.set_property("location", recordingfile);

        pipeline.add(&recordingbranch).unwrap();
        tee.link(&recordingbranch).unwrap();
    }

    bus_dispatcher(&pipeline);

    pipeline.set_state(gstreamer::State::Playing).unwrap();
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

fn bus_dispatcher(pipeline: &gstreamer::Bin) {
    let bus = pipeline.bus().unwrap();
    bus.add_signal_watch();
    bus.connect_message(None, |bus, message| match message.view() {
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
