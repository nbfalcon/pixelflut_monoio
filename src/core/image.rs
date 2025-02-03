use std::sync::atomic::{AtomicU32, Ordering};

#[repr(align(4))]
#[derive(Default, Clone, Copy)]
pub struct RGBAPixel([u8; 4]);

impl RGBAPixel {
    pub fn new_rgb(r: u8, g: u8, b: u8) -> Self {
        // FIXME: little-endian assumption
        Self([r, g, b, 0])
    }

    // FIXME: proper alpha blending
    pub fn new_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, 0])
    }

    // FIXME: decide on representation
    pub fn into_rgba(&self) -> u32 {
        u32::from_le_bytes(self.0)
    }

    pub fn from_rgba(rgba: u32) -> Self {
        Self(rgba.to_le_bytes())
    }
}

pub type Coord = u32;

pub struct PixelflutImage {
    pub height: Coord,
    pub width: Coord,

    pixel_data: Box<[AtomicU32]>,
}

impl PixelflutImage {
    pub fn new_with(width: Coord, height: Coord) -> Self {
        let total = (height as usize) * (width as usize);
        let mut pixel_data = Vec::new();
        pixel_data.resize_with(total, || AtomicU32::new(0));
        PixelflutImage {
            height,
            width,
            pixel_data: pixel_data.into_boxed_slice(),
        }
    }

    pub fn bounds_check(&self, px: Coord, py: Coord) -> bool {
        px < self.width && py < self.height
    }

    fn index(&self, px: Coord, py: Coord) -> usize {
        assert!(self.bounds_check(px, py));
        (py as usize) * (self.width as usize) + (px as usize)
    }

    // FIMXE: the alpha semantics are completely borked (TBD: use CAS)
    pub fn set_pixel(&self, px: Coord, py: Coord, pixel: RGBAPixel) {
        let i = self.index(px, py);
        self.pixel_data[i].store(pixel.into_rgba(), Ordering::Relaxed);
    }

    pub fn get_pixel(&self, px: Coord, py: Coord) -> RGBAPixel {
        let i = self.index(px, py);
        RGBAPixel::from_rgba(self.pixel_data[i].load(Ordering::Relaxed))
    }
}
