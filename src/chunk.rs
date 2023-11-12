pub struct Chunk {
    pub blocks: [[[Block; Chunk::SIZE]; Chunk::SIZE]; Chunk::SIZE],
    pub transparency: u8,
    pub has_valid_mesh: bool,
    pub non_air_block_count: u16,
}

pub enum Transparency {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
    Computed,
}

impl Chunk {
    pub const SIZE: usize = 16;

    pub const MAX_BLOCK_COUNT: u16 = Chunk::SIZE.pow(3) as u16;

    pub fn clear_transparency(&mut self) {
        self.transparency = 0;
    }
    pub fn get_transparency(&self, direction: Transparency) -> bool {
        (self.transparency & (1 << (direction as u8))) != 0
    }

    pub fn compute_transparency(&mut self) {
        let mut transparency = 1 << (Transparency::Computed as u8);

        let s = Chunk::SIZE;
        let mut check = |(ox, oy, oz): (usize, usize, usize), (dx, dy, dz), t: Transparency| {
            for x in (0..s).step_by(dx) {
                for y in (0..s).step_by(dy) {
                    for z in (0..s).step_by(dz) {
                        if self.blocks[ox + x][oy + y][oz + z].transparent() {
                            transparency |= 1 << t as u8;
                            return;
                        }
                    }
                }
            }
        };

        let o = s - 1;
        check((o, 0, 0), (s, 1, 1), Transparency::PosX);
        check((0, o, 0), (1, s, 1), Transparency::PosY);
        check((0, 0, o), (1, 1, s), Transparency::PosZ);

        check((0, 0, 0), (s, 1, 1), Transparency::NegX);
        check((0, 0, 0), (1, s, 1), Transparency::NegY);
        check((0, 0, 0), (1, 1, s), Transparency::NegZ);

        self.transparency = transparency;
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self {
            blocks: Default::default(),
            transparency: 0,
            has_valid_mesh: false,
            non_air_block_count: 0,
        }
    }
}

#[derive(Default)]
pub enum Block {
    #[default]
    Air,
    Dirt,
}

impl Block {
    pub fn transparent(&self) -> bool {
        match self {
            Block::Air => true,
            Block::Dirt => false,
        }
    }
}
