use serde::Deserialize;

use super::image::Coord;

#[derive(Deserialize)]
#[derive(Clone)]
pub struct Config {
    pub num_io_threads: usize,
    pub image_width: Coord,
    pub image_height: Coord,

    pub listen_addr: String,
}