use crate::core::game::PixelflutGame;
use gstreamer::{
    glib::object::Cast,
    prelude::{ElementExt, GstBinExt},
};
use gstreamer_app::AppSrcCallbacks;

pub fn gstreamer_pipeline(game: &'static PixelflutGame) {
    gstreamer::init().unwrap();

    let main = glib::MainLoop::new(None, true);

    let width = game.image().width;
    let height = game.image().height;

    let pipeline = gstreamer::parse::launch("appsrc name=input ! videoconvert ! autovideosink")
        .expect("Failed to create pipeline");
    let pipeline: gstreamer::Bin = pipeline.downcast().unwrap();

    let appsrc: gstreamer_app::AppSrc = pipeline.by_name("input").unwrap().downcast().unwrap();
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
    appsrc.set_stream_type(gstreamer_app::AppStreamType::Stream);
    appsrc.set_callbacks(
        AppSrcCallbacks::builder()
            .need_data(|appsrc, _count| {
                let mut buffer = gstreamer::Buffer::new();
                let mut memory = gstreamer::Memory::with_size(game.image().scanout_size());
                {
                    let mut memory_mapw = memory.get_mut().unwrap().map_writable().unwrap();
                    let memory_slice = memory_mapw.as_mut_slice();
                    game.image().scanout(memory_slice);
                }
                buffer.get_mut().unwrap().append_memory(memory);

                appsrc.push_buffer(buffer).unwrap();
            })
            .build(),
    );

    pipeline.set_state(gstreamer::State::Playing).unwrap();
    main.run();
}
