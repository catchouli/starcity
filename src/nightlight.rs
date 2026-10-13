//! The glow of towns at night: a coarse light map (one value per `CELL`×`CELL` tiles) made from
//! where people live and where the streets run, blurred so it spills softly over the land. The
//! terrain, water and models add it, warm, as the daylight fades. Built on the CPU now and then
//! (it's small), read with one texture lookup per pixel, so it costs almost nothing on the GPU.

use crate::network::Layer;

/// Tiles per light-map cell.
pub const CELL: usize = 4;

/// Size of the light map for a map of this many tiles.
pub fn dims(size_x: usize, size_z: usize) -> (usize, usize) {
    (size_x.div_ceil(CELL), size_z.div_ceil(CELL))
}

/// The light map: `people` gives (tile, residents) for each building, `roads` the street lamps.
/// Values are 0–255, brightest in city centres.
pub fn build(size_x: usize, size_z: usize, people: impl Iterator<Item = ((i32, i32), u32)>, roads: &Layer) -> Vec<u8> {
    let (w, h) = dims(size_x, size_z);
    let mut light = vec![0.0f32; w * h];
    let mut add = |x: i32, z: i32, v: f32| {
        if x >= 0 && z >= 0 {
            let (cx, cz) = (x as usize / CELL, z as usize / CELL);
            if cx < w && cz < h {
                light[cz * w + cx] += v;
            }
        }
    };
    for ((x, z), residents) in people {
        // Light grows with the people, but more slowly: a tower isn't a hundred times brighter than a house.
        add(x, z, (residents as f32).sqrt() * 0.08);
    }
    for z in 0..roads.size_z as i32 {
        for x in 0..roads.size_x as i32 {
            if roads.get(x, z) != 0 {
                add(x, z, 0.06);
            }
        }
    }
    // Spill: two passes of a box blur, so the glow fades out over a few cells.
    for _ in 0..2 {
        light = blur(&light, w, h, 2);
    }
    // Soft limit, so city centres saturate instead of burning white.
    light
        .iter()
        .map(|&v| ((1.0 - (-v * 1.6).exp()) * 255.0) as u8)
        .collect()
}

fn blur(src: &[f32], w: usize, h: usize, r: i32) -> Vec<f32> {
    let n = (2 * r + 1) as f32;
    let mut tmp = vec![0.0; src.len()];
    for z in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for d in -r..=r {
                let xx = (x as i32 + d).clamp(0, w as i32 - 1) as usize;
                s += src[z * w + xx];
            }
            tmp[z * w + x] = s / n;
        }
    }
    let mut out = vec![0.0; src.len()];
    for z in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for d in -r..=r {
                let zz = (z as i32 + d).clamp(0, h as i32 - 1) as usize;
                s += tmp[zz * w + x];
            }
            out[z * w + x] = s / n;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_city_glows_and_the_glow_spills_but_fades() {
        let roads = Layer::new(128, 128);
        let city: Vec<((i32, i32), u32)> = (0..400).map(|i| ((60 + i % 20, 60 + i / 20), 200)).collect();
        let map = build(128, 128, city.into_iter(), &roads);
        let (w, _) = dims(128, 128);
        let at = |x: usize, z: usize| map[(z / CELL) * w + x / CELL];
        assert!(at(70, 70) > 200, "the centre is bright: {}", at(70, 70));
        assert!(at(90, 70) > 0 && at(90, 70) < at(70, 70), "spills out a little");
        assert_eq!(at(5, 5), 0, "the countryside stays dark");
    }

    #[test]
    fn building_it_is_cheap() {
        let mut roads = Layer::new(512, 512);
        for x in 0..512 {
            roads.set(x, 256, 1);
        }
        let people: Vec<((i32, i32), u32)> = (0..20_000).map(|i| ((i % 512, (i / 512) * 4), 50)).collect();
        let t = std::time::Instant::now();
        let map = build(512, 512, people.into_iter(), &roads);
        let e = t.elapsed();
        assert_eq!(map.len(), 128 * 128);
        assert!(e.as_millis() < 20, "{e:?}");
    }
}
