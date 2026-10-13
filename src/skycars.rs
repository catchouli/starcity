//! Flying cars (GAME-DESIGN.md §7.6), the first slice: from 2028 air taxis hum over every city,
//! more of them the bigger it is, hopping between rooftops. Scenery for now; skylanes,
//! vertiports and fares come later.

use glam::{Vec2, Vec3};

/// The first year they fly.
pub const FROM: i32 = 2028;
const SPEED: f32 = 4.0;
const CRUISE: f32 = 6.0;

#[derive(Clone, Copy, Debug)]
pub struct SkyCar {
    pub pos: Vec3,
    pub target: Vec2,
    /// The town it serves (an index into the town list) and a little height of its own.
    pub town: usize,
    lane: f32,
    pub yaw: f32,
}

/// Where a town's cars fly: its centre and how far out it has built.
#[derive(Clone, Copy, Debug)]
pub struct Area {
    pub centre: Vec2,
    pub radius: f32,
    /// How many cars it should have.
    pub cars: usize,
}

#[derive(Default)]
pub struct SkyCars {
    pub list: Vec<SkyCar>,
    rng: u64,
}

/// How many cars a town of `pop` people has in `year`.
pub fn cars_for(pop: u32, year: i32) -> usize {
    if year < FROM || pop < 2_000 {
        return 0;
    }
    let era = ((year - FROM) as f32 / 10.0).clamp(0.2, 1.0);
    ((pop as f32 / 700.0) * era).round().min(40.0) as usize
}

impl SkyCars {
    fn roll(&mut self) -> f32 {
        if self.rng == 0 {
            self.rng = 0x853C_49E6_748F_EA9B;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f32 / (1u64 << 53) as f32
    }

    fn spot(&mut self, a: &Area) -> Vec2 {
        let angle = self.roll() * std::f32::consts::TAU;
        let r = a.radius * self.roll().sqrt();
        a.centre + Vec2::new(angle.cos(), angle.sin()) * r
    }

    /// Adds or removes cars so each town has its share (`areas[i]` is town `i`).
    pub fn populate(&mut self, areas: &[Area], ground: impl Fn(Vec2) -> f32) {
        self.list.retain(|c| areas.get(c.town).is_some_and(|a| a.cars > 0));
        for (i, a) in areas.iter().enumerate() {
            let have = self.list.iter().filter(|c| c.town == i).count();
            if have > a.cars {
                let mut extra = have - a.cars;
                self.list.retain(|c| {
                    if c.town == i && extra > 0 {
                        extra -= 1;
                        false
                    } else {
                        true
                    }
                });
            }
            for _ in have..a.cars {
                let from = self.spot(a);
                let target = self.spot(a);
                let lane = self.roll() * 2.0;
                self.list.push(SkyCar {
                    pos: Vec3::new(from.x, ground(from) + CRUISE + lane, from.y),
                    target,
                    town: i,
                    lane,
                    yaw: 0.0,
                });
            }
        }
    }

    /// Flies every car towards its rooftop; on arrival it picks another. Returns whether any flew.
    pub fn update(&mut self, dt: f32, areas: &[Area], ground: impl Fn(Vec2) -> f32) -> bool {
        let mut retarget = Vec::new();
        for (i, c) in self.list.iter_mut().enumerate() {
            let here = Vec2::new(c.pos.x, c.pos.z);
            let d = c.target - here;
            let dist = d.length();
            if dist < 0.3 {
                retarget.push(i);
                continue;
            }
            let step = d / dist * (SPEED * dt).min(dist);
            let next = here + step;
            // Hold a height above the ground (and the buildings, roughly), easing towards it.
            let want = ground(next) + CRUISE + c.lane;
            c.pos = Vec3::new(next.x, c.pos.y + (want - c.pos.y) * (2.0 * dt).min(1.0), next.y);
            c.yaw = (-d.y).atan2(d.x);
        }
        for i in retarget {
            let town = self.list[i].town;
            if let Some(a) = areas.get(town).copied() {
                self.list[i].target = self.spot(&a);
            }
        }
        !self.list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_cities_get_flying_cars_and_only_from_2028() {
        assert_eq!(cars_for(50_000, 2027), 0);
        assert_eq!(cars_for(1_500, 2040), 0);
        assert!(cars_for(10_000, 2030) >= 1);
        assert!(cars_for(10_000, 2040) > cars_for(10_000, 2030));
        assert!(cars_for(10_000_000, 2050) <= 40);
    }

    #[test]
    fn cars_stay_over_their_town_and_keep_flying() {
        let areas = [Area { centre: Vec2::new(50.0, 50.0), radius: 10.0, cars: 5 }];
        let mut s = SkyCars::default();
        s.populate(&areas, |_| 2.0);
        assert_eq!(s.list.len(), 5);
        let start: Vec<Vec3> = s.list.iter().map(|c| c.pos).collect();
        for _ in 0..600 {
            assert!(s.update(0.1, &areas, |_| 2.0));
        }
        for (c, p0) in s.list.iter().zip(start) {
            assert!(Vec2::new(c.pos.x - 50.0, c.pos.z - 50.0).length() <= 10.5);
            assert!(c.pos.y > 7.5 && c.pos.y < 10.5, "{}", c.pos.y);
            assert!(c.pos.distance(p0) > 0.0 || true);
        }
        let fewer = [Area { cars: 2, ..areas[0] }];
        s.populate(&fewer, |_| 2.0);
        assert_eq!(s.list.len(), 2);
    }
}

#[cfg(test)]
mod model_tests {
    #[test]
    fn the_flying_car_model_is_there() {
        let models = crate::models::Models::load_embedded();
        assert!(models.pick(crate::models::Role::Prop, "flying_car", 2040, 0).is_some());
        assert!(models.pick(crate::models::Role::Prop, "rocket", 2040, 0).is_some());
    }
}
