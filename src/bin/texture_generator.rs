use rand::{Rng, RngCore};
use std::ops::{Index, IndexMut};

const TILE_SIZE: usize = 16;

fn tile_offset(x: usize, y: usize) -> (usize, usize) {
    (x * TILE_SIZE, y * TILE_SIZE)
}

fn main() {
    let x_tiles = 4;
    let y_tiles = 4;

    let mut tile = Image::new(TILE_SIZE, TILE_SIZE);
    let mut image = Image::new(x_tiles * TILE_SIZE, y_tiles * TILE_SIZE);

    tile.fill_rgb();
    image.draw_image_at_offset(&mut tile, tile_offset(3, 0));

    tile.fill_random();
    image.draw_image_at_offset(&mut tile, tile_offset(3, 1));

    // grass top
    let grass_a = Pixel::rgb(0x107800);
    let grass_b = Pixel::rgb(0x70c048);
    tile.fill_random_between(grass_a, grass_b);
    image.draw_image_at_offset(&mut tile, tile_offset(0, 3));

    // dirt
    tile.fill_random_between(Pixel::rgb(0x6b5428), Pixel::rgb(0xb69f66));
    image.draw_image_at_offset(&mut tile, tile_offset(0, 2));

    // grass side
    tile.fill_random_between(Pixel::rgb(0x6b5428), Pixel::rgb(0xb69f66));
    for x in 0..TILE_SIZE {
        let height = rand::rng().random_range(2..5);
        for y in 0..height {
            let t = rand::rng().random();
            tile[(x, TILE_SIZE - y - 1)] = Pixel {
                r: lerp(grass_a.r, grass_b.r, t),
                g: lerp(grass_a.g, grass_b.g, t),
                b: lerp(grass_a.b, grass_b.b, t),
            }
        }
    }

    image.draw_image_at_offset(&mut tile, tile_offset(1, 3));

    // stone
    tile.fill_random_between(Pixel::rgb(0x606060), Pixel::rgb(0x808080));
    image.draw_image_at_offset(&mut tile, tile_offset(1, 2));

    // water
    tile.fill_random_between(Pixel::rgb(0x416bdf), Pixel::rgb(0x3ea4f0));
    image.draw_image_at_offset(&mut tile, tile_offset(1, 1));

    // UI
    tile.fill_random_between(Pixel::rgb(0xdddddd), Pixel::rgb(0xdddddd));
    image.draw_image_at_offset(&mut tile, tile_offset(0, 1));

    // sand
    tile.fill_random_between(Pixel::rgb(0xf6d7b0), Pixel::rgb(0xe1bf92));
    image.draw_image_at_offset(&mut tile, tile_offset(2, 3));

    std::fs::write("src/renderer/blocks.bmp", encode_bitmap(&image)).unwrap();
}

#[derive(Copy, Clone, Default)]
struct Pixel {
    r: u8,
    g: u8,
    b: u8,
}

impl Pixel {
    fn rgb(color: u32) -> Pixel {
        let c = color.to_le_bytes();
        Pixel {
            r: c[2],
            g: c[1],
            b: c[0],
        }
    }
}

struct Image {
    width: usize,
    height: usize,
    pixels: Vec<Pixel>,
}

impl Image {
    fn new(width: usize, height: usize) -> Image {
        Image {
            width,
            height,
            pixels: vec![Default::default(); width * height],
        }
    }

    fn draw_image_at_offset(&mut self, other: &Image, (ox, oy): (usize, usize)) {
        for y in 0..other.height {
            for x in 0..other.width {
                self[(ox + x, oy + y)] = other[(x, y)];
            }
        }
    }

    fn fill_rgb(&mut self) {
        self[(0, 0)].r = 255;
        self[(1, 0)].g = 255;
        self[(2, 0)].b = 255;
        self[(15, 15)] = Pixel {
            r: 255,
            g: 255,
            b: 255,
        };
    }

    fn fill_random(&mut self) {
        let mut rng = rand::rng();
        for y in 0..self.height {
            for x in 0..self.width {
                self[(y, x)] = Pixel::rgb(rng.next_u32());
            }
        }
    }

    fn fill_random_between(&mut self, a: Pixel, b: Pixel) {
        let mut rng = rand::rng();

        for y in 0..self.height {
            for x in 0..self.width {
                let t = rng.random();
                self[(y, x)] = Pixel {
                    r: lerp(a.r, b.r, t),
                    g: lerp(a.g, b.g, t),
                    b: lerp(a.b, b.b, t),
                }
            }
        }
    }
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    let from = a.min(b);
    let to = a.max(b);
    from + (t * (to - from) as f32) as u8
}

impl Index<(usize, usize)> for Image {
    type Output = Pixel;

    fn index(&self, (x, y): (usize, usize)) -> &Self::Output {
        &self.pixels[(self.width * y + x) as usize]
    }
}
impl IndexMut<(usize, usize)> for Image {
    fn index_mut(&mut self, (x, y): (usize, usize)) -> &mut Self::Output {
        &mut self.pixels[(self.width * y + x) as usize]
    }
}

fn encode_bitmap(image: &Image) -> Vec<u8> {
    let max = i32::MAX as usize - 1000;
    assert!(
        image.width <= max && image.height <= max,
        "{}{}",
        image.width,
        image.height
    );

    let row_length = ((3 * image.width + 3) / 4) * 4;
    let bf_off_bits: u32 = 138;
    let size: u32 = bf_off_bits + (row_length * image.height) as u32;

    let mut bmp = vec![0; size as usize];
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&size.to_le_bytes());

    bmp[10..14].copy_from_slice(&bf_off_bits.to_le_bytes());

    let info_header_size: u32 = 40;
    bmp[14..18].copy_from_slice(&info_header_size.to_le_bytes());

    bmp[18..22].copy_from_slice(&(image.width as u32).to_le_bytes());
    bmp[22..26].copy_from_slice(&(image.height as u32).to_le_bytes());

    let planes: u16 = 1;
    bmp[26..28].copy_from_slice(&planes.to_le_bytes());

    let bi_bit_count: u16 = 24;
    bmp[28..30].copy_from_slice(&bi_bit_count.to_le_bytes());

    // compression = 0

    for row in 0..image.height {
        let row_start = bf_off_bits as usize + row * row_length;
        for column in 0..image.width {
            let start = (row_start + column * 3) as usize;
            let pixel = &image[(column, row)];

            bmp[start] = pixel.b;
            bmp[start + 1] = pixel.g;
            bmp[start + 2] = pixel.r;
        }
    }
    bmp
}
