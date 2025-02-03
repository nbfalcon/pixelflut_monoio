use super::image::{Coord, PixelflutImage};

/// State of each IO-Thread, shared between multiple clients
pub struct PixelflutThreadState {
    pub global_config: PixelflutGlobalConfig,
    pub global_state: &'static PixelflutGlobalState,
}

/// Configuration shared by all threads
#[derive(Clone, Copy)]
pub struct PixelflutGlobalConfig {
    pub width: Coord,
    pub height: Coord,
}

/// State of the entire pixelflut core (shared between all threads)
pub struct PixelflutGlobalState {
    pub image: PixelflutImage,
}