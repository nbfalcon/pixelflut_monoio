use super::image::{Coord, PixelflutTripleBuffer};

/// State of each IO-Thread, shared between multiple clients
pub struct PixelflutIOWorkerState {
    pub global_config: PixelflutGlobalConfig,
    pub my_present_queue: PixelflutTripleBuffer,
}

#[derive(Clone, Copy)]
pub struct PixelflutGlobalConfig {
    pub width: Coord,
    pub height: Coord,
}
