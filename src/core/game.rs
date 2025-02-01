use monoio::time::Instant;

use super::{
    config::Config,
    image::{PixelflutImage, PixelflutTripleBuffer},
    state::{PixelflutGlobalConfig, PixelflutIOWorkerState},
};

pub struct PixelflutGame {
    complete_image: PixelflutImage,
    workers: Vec<PixelflutIOWorkerState>,
}

impl PixelflutGame {
    pub fn new(config: &Config) -> &'static mut PixelflutGame {
        let global_config = PixelflutGlobalConfig {
            start_time: Instant::now(),
            width: config.image_width,
            height: config.image_height,
        };
        
        let mut workers = Vec::with_capacity(config.num_io_threads);
        workers.fill_with(|| {
            PixelflutIOWorkerState {
                global_config: global_config,
                my_present_queue: PixelflutTripleBuffer::new_with(
                    config.image_width,
                    config.image_height,
                ),
            }
        });
        
        let game_box = Box::new(PixelflutGame {
            complete_image: PixelflutImage::new_with(config.image_width, config.image_height),
            workers,
        });
        // FIXME: implement proper shutdown logic in a way that would make valgrind happy
        Box::leak(game_box)
    }

    pub fn combine_all(&mut self) {
        for worker in self.workers.iter() {
            worker.my_present_queue.swap_consumer_side();
            let buffer = unsafe { worker.my_present_queue.consumer_buffer() };
            self.complete_image.combine_with(buffer);
        }
    }

    pub fn image(&self) -> &PixelflutImage {
        &self.complete_image
    }

    pub fn for_worker(&'static self, worker: usize) -> &'static PixelflutIOWorkerState {
        &self.workers[worker]
    }
}
