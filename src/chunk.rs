

pub struct Chunk {
    pub blocks: [[[Block; Chunk::SIZE]; Chunk::SIZE]; Chunk::SIZE],
}

impl Chunk {
    pub const SIZE: usize = 16;
}

impl Default for Chunk {
    fn default() -> Self {
        Self {
            blocks: Default::default(),
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