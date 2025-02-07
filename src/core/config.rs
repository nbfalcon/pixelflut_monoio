use serde::Deserialize;

use super::image::Coord;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub num_io_threads: usize,
    pub image_width: Coord,
    pub image_height: Coord,

    pub listen_addr: String,

    pub gst_window: bool,
    pub record_to_file: Option<String>,
}