use std::num::Wrapping;

use rand::prelude::SliceRandom;
use rand::rngs::StdRng;

/// Based on the Improved Noise reference implementation by Ken Perlin: https://mrl.cs.nyu.edu/~perlin/noise/
/// For the 2d version I also looked at https://rtouti.github.io/graphics/perlin-noise-algorithm
pub struct ImprovedNoise {
    permutation: [u8; 256],
}

impl ImprovedNoise {
    pub fn new(random: &mut StdRng) -> Self {
        let mut permutation = [0u8; 256];
        permutation.iter_mut().enumerate().for_each(|(i, v)| *v = i as u8);
        permutation.shuffle(random);
        Self {
            permutation
        }
    }

    #[allow(non_snake_case)]
    pub fn noise(&self, mut x: f64, mut y: f64, mut z: f64) -> f64 {
        // FIND UNIT CUBE THAT CONTAINS POINT.
        let X = Wrapping(x.floor() as i32 as u8);
        let Y = Wrapping(y.floor() as i32 as u8);
        let Z = Wrapping(z.floor() as i32 as u8);

        // FIND RELATIVE X,Y,Z OF POINT IN CUBE.
        x -= x.floor();
        y -= y.floor();
        z -= z.floor();

        // COMPUTE FADE CURVES FOR EACH OF X,Y,Z.
        let u = fade(x);
        let v = fade(y);
        let w = fade(z);

        // HASH COORDINATES OF THE 8 CUBE CORNERS,
        let p = |i: Wrapping<u8>| Wrapping(self.permutation[i.0 as usize]);
        let A = p(X) + Y;
        let AA = p(A) + Z;
        let AB = p(A + Wrapping(1)) + Z;
        let B = p(X + Wrapping(1)) + Y;
        let BA = p(B) + Z;
        let BB = p(B + Wrapping(1)) + Z;

        // AND ADD BLENDED RESULTS FROM 8 CORNERS OF CUBE
        lerp(w,
             lerp(v,
                  lerp(u, grad(p(AA), x, y, z),
                       grad(p(BA), x - 1.0, y, z),
                  ),
                  lerp(u, grad(p(AB), x, y - 1.0, z),
                       grad(p(BB), x - 1.0, y - 1.0, z),
                  ),
             ),
             lerp(v,
                  lerp(u, grad(p(AA + Wrapping(1)), x, y, z - 1.0),
                       grad(p(BA + Wrapping(1)), x - 1.0, y, z - 1.0),
                  ),
                  lerp(u, grad(p(AB + Wrapping(1)), x, y - 1.0, z - 1.0),
                       grad(p(BB + Wrapping(1)), x - 1.0, y - 1.0, z - 1.0),
                  ),
             ),
        )
    }

    #[allow(non_snake_case)]
    pub fn noise_2d(&self, mut x: f64, mut y: f64) -> f64 {
        let X = Wrapping(x.floor() as i32 as u8);
        let Y = Wrapping(y.floor() as i32 as u8);

        x -= x.floor();
        y -= y.floor();

        let u = fade(x);
        let v = fade(y);

        let p = |i: Wrapping<u8>| Wrapping(self.permutation[i.0 as usize]);

        let A = p(X) + Y;
        let B = p(X + Wrapping(1)) + Y;

        lerp(v,
             lerp(u,
                  grad_2(p(A), x, y),
                  grad_2(p(B), x - 1.0, y),
             ),
             lerp(u,
                  grad_2(p(A + Wrapping(1)), x, y - 1.0),
                  grad_2(p(B + Wrapping(1)), x - 1.0, y - 1.0),
             ),
        )
    }
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

/// CONVERT LO 4 BITS OF HASH CODE INTO 12 GRADIENT DIRECTIONS.
fn grad(hash: Wrapping<u8>, x: f64, y: f64, z: f64) -> f64 {
    let h = hash.0 & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 { y } else if h == 12 || h == 14 { x } else { z };
    return if (h & 1) == 0 { u } else { -u } + if (h & 2) == 0 { v } else { -v };
}

fn grad_2(hash: Wrapping<u8>, x: f64, y: f64) -> f64 {
    match hash.0 & 3 {
        0b00 => 1.0 * x + 1.0 * y,
        0b01 => -1.0 * x + 1.0 * y,
        0b10 => 1.0 * x + -1.0 * y,
        0b11 => -1.0 * x + -1.0 * y,
        _ => unreachable!()
    }
}