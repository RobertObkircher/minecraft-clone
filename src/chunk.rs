pub struct Chunk {
    pub blocks: [[[Block; Chunk::SIZE]; Chunk::SIZE]; Chunk::SIZE],
    pub transparency: u8,
}

#[allow(unused)]
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

    pub fn clear_transparency(&mut self) {
        self.transparency = 0;
    }
    pub fn get_transparency(&self, direction: Transparency) -> bool {
        (self.transparency & (1 << (direction as u8))) != 0
    }

    pub fn compute_transparency(&mut self) {
        let mut transparency = 1 << (Transparency::Computed as u8);

        'outer: for i in 0..6 {
            let cs = Chunk::SIZE - 1;
            let start = if i % 2 == 0 {
                (0, 0, 0)
            } else {
                (cs, cs, cs)
            };
            let end = match i / 2 {
                0 => (0, cs, cs),
                1 => (cs, 0, 0),
                2 => (cs, 0, cs),
                3 => (0, cs, 0),
                4 => (cs, cs, 0),
                5 => (0, 0, cs),
                _ => unreachable!()
            };
            for x in start.0..end.0 {
                for y in start.0..end.0 {
                    for z in start.0..end.0 {
                        if self.blocks[x][y][z].transparent() {
                            transparency |= 1 << i;
                            continue 'outer;
                        }
                    }
                }
            }
        }
        self.transparency = transparency;
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self {
            blocks: Default::default(),
            transparency: 0,
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