use wgpu::{Device, Extent3d, ImageCopyTexture, ImageDataLayout, Origin3d, Queue, Sampler, SamplerDescriptor, Texture, TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView};

pub struct BlockTexture {
    pub texture: Texture,
    pub view: TextureView,
    pub sampler: Sampler,
}

impl BlockTexture {
    pub fn from_bitmap_bytes(device: &Device, queue: &Queue, bytes: &[u8], label: &str) -> Self {
        let decoded = decode_bitmap(bytes);

        let size = Extent3d {
            width: decoded.width,
            height: decoded.height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&TextureDescriptor {
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            label: Some(label),
            view_formats: &[],
        });

        queue.write_texture(
            ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &decoded.rgba,
            ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * size.width),
                rows_per_image: Some(size.height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some(label),
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
        }
    }
}

struct DecodedImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn decode_bitmap(raw: &[u8]) -> DecodedImage {
    assert_eq!(&raw[0..2], "BM".as_bytes());

    let bf_off_bits = u32::from_le_bytes(raw[10..14].try_into().unwrap());
    assert!(bf_off_bits >= 54, "must be after headers");

    let bi_width = i32::from_le_bytes(raw[18..22].try_into().unwrap());
    assert!(bi_width > 0);
    let bi_height = i32::from_le_bytes(raw[22..26].try_into().unwrap());
    assert!(bi_height > 0, "assume bottom up, starts with lowest line");

    let bi_bit_count = u16::from_le_bytes(raw[28..30].try_into().unwrap());
    assert_eq!(bi_bit_count, 24);

    let bi_compression = u32::from_le_bytes(raw[30..34].try_into().unwrap());
    assert_eq!(bi_compression, 0, "assume uncompressed rgb");

    let width = bi_width as u32;
    let height = bi_height as u32;
    let mut data = Vec::with_capacity((4 * width * height) as usize);

    let row_length = ((3 * width as usize + 3) / 4) * 4;
    for row in 0..height as usize {
        let row_start = bf_off_bits as usize + row * row_length;
        for column in 0..width as usize {
            let start = row_start + column * 3;

            let blue = raw[start];
            let green = raw[start + 1];
            let red = raw[start + 2];

            data.extend_from_slice(&[red, green, blue, 255]);
        }
    }

    return DecodedImage {
        width,
        height,
        rgba: data,
    };
}