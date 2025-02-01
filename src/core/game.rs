use monoio::time::Instant;

use super::{
    config::Config,
    image::{PixelflutImage, PixelflutTripleBuffer},
    state::{PixelflutGlobalConfig, PixelflutIOWorkerState},
};

pub struct PixelflutGameCommon {
    workers: Vec<PixelflutIOWorkerState>,
}

impl PixelflutGameCommon {
    pub fn for_worker(&'static self, id: usize) -> &'static PixelflutIOWorkerState {
        &self.workers[id]
    }
}

pub struct PixelflutGame {
    complete_image: PixelflutImage,
    common: &'static PixelflutGameCommon,
}

impl PixelflutGame {
    pub fn new(config: &Config) -> &'static mut PixelflutGame {
        let global_config = PixelflutGlobalConfig {
            start_time: Instant::now(),
            width: config.image_width,
            height: config.image_height,
        };

        let mut workers = Vec::new();
        workers.resize_with(config.num_io_threads, || PixelflutIOWorkerState {
            global_config,
            my_present_queue: PixelflutTripleBuffer::new_with(
                config.image_width,
                config.image_height,
            ),
        });

        let game_common = Box::new(PixelflutGameCommon { workers });
        let game_box = Box::new(PixelflutGame {
            complete_image: PixelflutImage::new_with(config.image_width, config.image_height),
            common: Box::leak(game_common),
        });
        // FIXME: implement proper shutdown logic in a way that would make valgrind happy
        Box::leak(game_box)
    }

    pub fn combine_all(&mut self) {
        for worker in self.common.workers.iter() {
            worker.my_present_queue.swap_consumer_side();
            let buffer = unsafe { worker.my_present_queue.consumer_buffer() };
            self.complete_image.combine_with(buffer);
        }
    }

    pub fn image(&self) -> &PixelflutImage {
        &self.complete_image
    }

    pub fn common(&self) -> &'static PixelflutGameCommon {
        self.common
    }
}
