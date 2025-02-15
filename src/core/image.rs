use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU32, Ordering},
};

use bit_set::BitSet;

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
}

pub type Coord = u32;

pub struct PixelflutImage {
    pub height: Coord,
    pub width: Coord,

    // Two arrays of [height][width] (i.e. row-major)
    // TODO: Maybe we could cook something up with MaybeUninit
    pixel_data: Box<[RGBAPixel]>,
    pixels_dirty: BitSet, // TODO: Maybe replace with fixed bit_set
}

impl PixelflutImage {
    pub fn new_with(width: Coord, height: Coord) -> Self {
        let total = (height as usize) * (width as usize);
        PixelflutImage {
            height,
            width,
            pixel_data: vec![RGBAPixel::default(); total].into_boxed_slice(),
            pixels_dirty: BitSet::with_capacity(total),
        }
    }

    pub fn bounds_check(&self, px: Coord, py: Coord) -> bool {
        px < self.width && py < self.height
    }

    fn index(&self, px: Coord, py: Coord) -> usize {
        assert!(self.bounds_check(px, py));
        (py as usize) * (self.width as usize) + (px as usize)
    }

    // FIMXE: the alpha semantics are completely borked
    pub fn set_pixel(&mut self, px: Coord, py: Coord, pixel: RGBAPixel) {
        let i = self.index(px, py);
        self.pixel_data[i] = pixel;
        self.pixels_dirty.insert(i);
    }

    pub fn get_pixel(&self, px: Coord, py: Coord) -> RGBAPixel {
        let i = self.index(px, py);
        self.pixel_data[i]
    }

    pub fn combine_with(&mut self, other: &mut PixelflutImage) {
        assert!(other.width == self.width && other.height == self.height);
        assert!(other.pixel_data.len() == self.pixel_data.len());

        for i in 0..self.pixel_data.len() {
            if other.pixels_dirty.contains(i) {
                // FIXME: correctly handle alpha-transparency
                self.pixel_data[i] = other.pixel_data[i];
            }
        }

        other.pixels_dirty.clear();
    }
}

// TODO: Generalize
pub struct PixelflutTripleBuffer {
    images: [UnsafeCell<PixelflutImage>; 3],
    // This is an atomic integer whose first three bytes (<< 0, << 8, << 16) index into images
    // The idea is that we can swap buffers and then atomically swap the buffer indices
    buffer_indices: AtomicU32,
}

unsafe impl Send for PixelflutTripleBuffer {}
unsafe impl Sync for PixelflutTripleBuffer {}

impl PixelflutTripleBuffer {
    pub fn new_with(width: Coord, height: Coord) -> Self {
        Self {
            // TODO: This is hacky https://stackoverflow.com/questions/31360993/what-is-the-proper-way-to-initialize-a-fixed-length-array
            images: [(); 3].map(|_| UnsafeCell::new(PixelflutImage::new_with(width, height))),
            buffer_indices: AtomicU32::new(u32::from_ne_bytes([0, 1, 2, 0xFF])),
        }
    }

    pub fn swap_present_side(&self) {
        loop {
            // We don't care about visibility yet, only at the CAS side
            let old_conf = self.buffer_indices.load(Ordering::Relaxed);
            let mut cur_conf: [u8; 4] = old_conf.to_ne_bytes();
            (cur_conf[0], cur_conf[1]) = (cur_conf[1], cur_conf[0]);
            let new_conf = u32::from_ne_bytes(cur_conf);

            if self
                .buffer_indices
                .compare_exchange_weak(old_conf, new_conf, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
    }

    pub fn swap_consumer_side(&self) {
        loop {
            // We don't care about visibility yet, only at the CAS side
            let old_conf = self.buffer_indices.load(Ordering::Relaxed);
            let mut cur_conf: [u8; 4] = old_conf.to_ne_bytes();
            (cur_conf[1], cur_conf[2]) = (cur_conf[2], cur_conf[1]);
            let new_conf = u32::from_ne_bytes(cur_conf);

            if self
                .buffer_indices
                .compare_exchange_weak(old_conf, new_conf, Ordering::Release, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
        }
    }

    // TODO: We can split this into two safe data structures that are not sync to be not-unsafe+sound
    // I.e. there is a Producer (&mut self) + Consumer(&self) -> can then be shared using e.g. a Cell
    pub unsafe fn producer_buffer(&self) -> &mut PixelflutImage {
        // We "acquired" changes from the other thread at the swap
        // FIXME: this might make the data-structure unsound when moving readers/writers
        let i = self.buffer_indices.load(Ordering::Relaxed).to_ne_bytes()[0];
        &mut *self.images[i as usize].get()
    }

    pub unsafe fn consumer_buffer(&self) -> &mut PixelflutImage {
        // We "acquired" changes from the other thread at the swap
        // FIXME: this might make the data-structure unsound when moving readers/writers
        let i = self.buffer_indices.load(Ordering::Relaxed).to_ne_bytes()[2];
        &mut *self.images[i as usize].get()
    }
}
