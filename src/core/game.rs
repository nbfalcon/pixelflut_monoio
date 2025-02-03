use super::{
    config::Config,
    image::PixelflutImage,
    state::{PixelflutGlobalConfig, PixelflutGlobalState, PixelflutThreadState},
};

pub struct PixelflutGame {
    state: PixelflutGlobalState,
    workers: Vec<PixelflutThreadState>,
}

impl PixelflutGame {
    pub fn new(config: &Config) -> &'static PixelflutGame {
        let global_config = PixelflutGlobalConfig {
            width: config.image_width,
            height: config.image_height,
        };

        let game = Box::leak(Box::new(PixelflutGame {
            state: PixelflutGlobalState{
                image: PixelflutImage::new_with(config.image_width, config.image_height)
            },
            workers: Vec::new(),
        }));
        game.workers.resize_with(config.num_io_threads, || PixelflutThreadState {
            global_config,
            global_state: &game.state,
        });

        game
    }

    pub fn image(&self) -> &PixelflutImage {
        &self.state.image
    }

    pub fn for_worker(&'static self, id: usize) -> &'static PixelflutThreadState {
        &self.workers[id]
    }
}
