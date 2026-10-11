//! Mouse picking: which terrain tile is under the cursor.

use crate::terrain::{BLOCK_HEIGHT, Shape, World};
use glam::{Mat4, Vec2, Vec3};

pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

/// The ray through a point in normalised device coordinates (-1..1, y up).
pub fn screen_ray(view_proj: Mat4, ndc: Vec2) -> Ray {
    let inv = view_proj.inverse();
    let near = inv.project_point3(ndc.extend(0.0));
    let far = inv.project_point3(ndc.extend(1.0));
    Ray {
        origin: near,
        dir: (far - near).normalize(),
    }
}

/// The first tile whose top surface the ray hits. Walks the tiles under the ray
/// front to back (2D DDA over x/z) and tests each tile's two surface triangles.
pub fn pick_tile(world: &World, ray: &Ray) -> Option<(i32, i32)> {
    let bounds = Vec3::new(
        world.size_x as f32,
        world.height as f32 * BLOCK_HEIGHT,
        world.size_z as f32,
    );
    let (t_enter, t_exit) = clip_to_box(ray, Vec3::ZERO, bounds)?;

    let start = ray.origin + ray.dir * t_enter;
    let mut x = (start.x.floor() as i32).clamp(0, world.size_x as i32 - 1);
    let mut z = (start.z.floor() as i32).clamp(0, world.size_z as i32 - 1);
    let step_x = if ray.dir.x >= 0.0 { 1 } else { -1 };
    let step_z = if ray.dir.z >= 0.0 { 1 } else { -1 };
    let delta_x = (1.0 / ray.dir.x).abs();
    let delta_z = (1.0 / ray.dir.z).abs();
    let next_x = if step_x > 0 { x as f32 + 1.0 } else { x as f32 };
    let next_z = if step_z > 0 { z as f32 + 1.0 } else { z as f32 };
    let mut t_x = if ray.dir.x != 0.0 {
        (next_x - ray.origin.x) / ray.dir.x
    } else {
        f32::INFINITY
    };
    let mut t_z = if ray.dir.z != 0.0 {
        (next_z - ray.origin.z) / ray.dir.z
    } else {
        f32::INFINITY
    };

    loop {
        if hit_tile(world, ray, x, z).is_some() {
            return Some((x, z));
        }
        if t_x < t_z {
            if t_x > t_exit {
                return None;
            }
            x += step_x;
            t_x += delta_x;
        } else {
            if t_z > t_exit {
                return None;
            }
            z += step_z;
            t_z += delta_z;
        }
        if !world.in_bounds(x, z) {
            return None;
        }
    }
}

fn clip_to_box(ray: &Ray, min: Vec3, max: Vec3) -> Option<(f32, f32)> {
    let inv = ray.dir.recip();
    let t0 = (min - ray.origin) * inv;
    let t1 = (max - ray.origin) * inv;
    let t_enter = t0.min(t1).max_element().max(0.0);
    let t_exit = t0.max(t1).min_element();
    (t_enter <= t_exit).then_some((t_enter, t_exit))
}

fn hit_tile(world: &World, ray: &Ray, x: i32, z: i32) -> Option<f32> {
    let (base, shape) = world.surface(x, z)?;
    let p = Shape::CORNERS.map(|(cx, cz)| {
        Vec3::new(
            (x + cx as i32) as f32,
            (base + shape.corner(cx, cz)) as f32 * BLOCK_HEIGHT,
            (z + cz as i32) as f32,
        )
    });
    let tris = if shape.splits_13() {
        [[p[0], p[1], p[3]], [p[1], p[2], p[3]]]
    } else {
        [[p[0], p[1], p[2]], [p[0], p[2], p[3]]]
    };
    tris.iter().filter_map(|t| ray_triangle(ray, t)).reduce(f32::min)
}

/// Möller–Trumbore, two-sided.
fn ray_triangle(ray: &Ray, [a, b, c]: &[Vec3; 3]) -> Option<f32> {
    let e1 = *b - *a;
    let e2 = *c - *a;
    let h = ray.dir.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-8 {
        return None;
    }
    let f = 1.0 / det;
    let s = ray.origin - *a;
    let u = f * s.dot(h);
    if !(-1e-4..=1.0 + 1e-4).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = f * ray.dir.dot(q);
    if v < -1e-4 || u + v > 1.0 + 1e-4 {
        return None;
    }
    let t = f * e2.dot(q);
    (t > 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::{Block, Material};

    #[test]
    fn picks_tile_straight_down() {
        let mut w = World::new(8, 8, 4, 0);
        for x in 0..8 {
            for z in 0..8 {
                w.set(x, 0, z, Block::full(Material::Grass));
            }
        }
        let ray = Ray {
            origin: Vec3::new(3.5, 10.0, 5.5),
            dir: Vec3::new(0.0, -1.0, 0.0),
        };
        assert_eq!(pick_tile(&w, &ray), Some((3, 5)));
    }

    #[test]
    fn raised_tile_intercepts_slanted_ray() {
        let mut w = World::new(8, 8, 8, 0);
        for x in 0..8 {
            for z in 0..8 {
                w.set(x, 0, z, Block::full(Material::Grass));
            }
        }
        // Ground surface is at y = 0.5; this ray reaches it at x = 5.2.
        let ray = Ray {
            origin: Vec3::new(2.7, 3.0, 2.5),
            dir: Vec3::new(1.0, -1.0, 0.0).normalize(),
        };
        assert_eq!(pick_tile(&w, &ray), Some((5, 2)));
        // Raising tile (4, 2) by one block (surface y = 1.0) catches it at x = 4.7 instead.
        w.set(4, 1, 2, Block::full(Material::Grass));
        assert_eq!(pick_tile(&w, &ray), Some((4, 2)));
    }
}
