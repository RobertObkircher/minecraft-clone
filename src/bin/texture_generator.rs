use std::ops::{Index, IndexMut};

fn main() {
    let mut image = Image {
        width: 16,
        height: 16,
        pixels: vec![Default::default(); 16 * 16],
    };
    image[(0, 0)].r = 255;
    image[(1, 0)].g = 255;
    image[(2, 0)].b = 255;
    image[(15, 15)] = Pixel {
        r: 255,
        g: 255,
        b: 255,
    };
    let bits = encode_bitmap(&image);
    std::fs::write("test.bmp", bits).unwrap();
}

#[derive(Copy, Clone, Default)]
struct Pixel {
    r: u8,
    g: u8,
    b: u8,
}

struct Image {
    width: u32,
    height: u32,
    pixels: Vec<Pixel>,
}
impl Index<(u32, u32)> for Image {
    type Output = Pixel;

    fn index(&self, index: (u32, u32)) -> &Self::Output {
        &self.pixels[(self.width * index.1 + index.0) as usize]
    }
}
impl IndexMut<(u32, u32)> for Image {
    fn index_mut(&mut self, index: (u32, u32)) -> &mut Self::Output {
        &mut self.pixels[(self.width * index.1 + index.0) as usize]
    }
}

fn encode_bitmap(image: &Image) -> Vec<u8> {
    let row_length = ((3 * image.width + 3) / 4) * 4;
    let bf_off_bits: u32 = 138;
    let size: u32 = bf_off_bits + row_length * image.height;

    let mut bmp = vec![0; size as usize];
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&size.to_le_bytes());

    bmp[10..14].copy_from_slice(&bf_off_bits.to_le_bytes());

    let info_header_size: u32 = 40;
    bmp[14..18].copy_from_slice(&info_header_size.to_le_bytes());

    bmp[18..22].copy_from_slice(&image.width.to_le_bytes());
    bmp[22..26].copy_from_slice(&image.height.to_le_bytes());

    let planes: u16 = 1;
    bmp[26..28].copy_from_slice(&planes.to_le_bytes());

    let bi_bit_count: u16 = 24;
    bmp[28..30].copy_from_slice(&bi_bit_count.to_le_bytes());

    // compression = 0

    for row in 0..image.height {
        let row_start = bf_off_bits + row * row_length;
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
