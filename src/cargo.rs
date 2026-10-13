//! Cargo: what there is to carry, what it pays, and how stations hold it
//! and are rated for it (GAME-DESIGN.md §3, §4, §5.2, §5.6). Pure data and
//! rules, tested without a window.

// Most of the table is for the industries of the next phase.
#![allow(dead_code)]

/// The twelve cargoes the game starts with (GAME-DESIGN.md §4). Era cargoes
/// (chips, containers, lunar glass...) join this list with their industries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cargo {
    Passengers,
    Mail,
    Coal,
    IronOre,
    Grain,
    Oil,
    Fuel,
    Timber,
    Steel,
    Planks,
    Food,
    Goods,
    Silicon,
    Chips,
    Regolith,
    LunarGlass,
    Helium3,
    RareMetals,
    NanoGoods,
}

pub const COUNT: usize = 19;

/// What kind of vehicle body can carry it (GAME-DESIGN.md §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    People,
    Bulk,
    General,
    Fresh,
}

impl Cargo {
    pub const ALL: [Cargo; COUNT] = [
        Cargo::Passengers,
        Cargo::Mail,
        Cargo::Coal,
        Cargo::IronOre,
        Cargo::Grain,
        Cargo::Oil,
        Cargo::Fuel,
        Cargo::Timber,
        Cargo::Steel,
        Cargo::Planks,
        Cargo::Food,
        Cargo::Goods,
        Cargo::Silicon,
        Cargo::Chips,
        Cargo::Regolith,
        Cargo::LunarGlass,
        Cargo::Helium3,
        Cargo::RareMetals,
        Cargo::NanoGoods,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Cargo::Passengers => "Passengers",
            Cargo::Mail => "Mail",
            Cargo::Coal => "Coal",
            Cargo::IronOre => "Iron ore",
            Cargo::Grain => "Grain",
            Cargo::Oil => "Oil",
            Cargo::Fuel => "Fuel",
            Cargo::Timber => "Timber",
            Cargo::Steel => "Steel",
            Cargo::Planks => "Planks",
            Cargo::Food => "Food",
            Cargo::Goods => "Goods",
            Cargo::Silicon => "Silicon",
            Cargo::Chips => "Chips",
            Cargo::Regolith => "Regolith",
            Cargo::LunarGlass => "Lunar glass",
            Cargo::Helium3 => "Helium-3",
            Cargo::RareMetals => "Rare metals",
            Cargo::NanoGoods => "Nano-goods",
        }
    }

    /// What one unit is called, for windows ("30 bags").
    pub fn unit(self) -> &'static str {
        match self {
            Cargo::Passengers => "passengers",
            Cargo::Mail => "bags",
            Cargo::Oil | Cargo::Fuel => "kl",
            Cargo::Goods | Cargo::Chips | Cargo::NanoGoods => "crates",
            Cargo::Helium3 => "canisters",
            _ => "t",
        }
    }

    pub fn class(self) -> Class {
        match self {
            Cargo::Passengers | Cargo::Mail => Class::People,
            Cargo::Coal | Cargo::IronOre | Cargo::Grain | Cargo::Oil | Cargo::Fuel | Cargo::Silicon | Cargo::Regolith => {
                Class::Bulk
            }
            Cargo::Food => Class::Fresh,
            _ => Class::General,
        }
    }

    /// Pay per unit per 10 tiles.
    pub fn rate(self) -> f32 {
        match self {
            Cargo::Passengers => 2.4,
            Cargo::Mail => 3.0,
            Cargo::Coal => 3.4,
            Cargo::IronOre => 3.2,
            Cargo::Grain => 3.0,
            Cargo::Oil => 3.2,
            Cargo::Fuel => 4.4,
            Cargo::Timber => 3.4,
            Cargo::Steel => 4.8,
            Cargo::Planks => 4.4,
            Cargo::Food => 4.8,
            Cargo::Goods => 6.4,
            Cargo::Silicon => 3.4,
            Cargo::Chips => 9.0,
            Cargo::Regolith => 4.0,
            Cargo::LunarGlass => 8.0,
            Cargo::Helium3 => 12.0,
            Cargo::RareMetals => 9.0,
            Cargo::NanoGoods => 16.0,
        }
    }

    /// Seconds in transit with full pay, and seconds more over which pay falls to 30%.
    pub fn freshness(self) -> (f32, f32) {
        match self {
            Cargo::Passengers => (20.0, 60.0),
            Cargo::Mail => (20.0, 50.0),
            Cargo::Coal | Cargo::IronOre => (100.0, 150.0),
            Cargo::Grain => (40.0, 80.0),
            Cargo::Oil => (60.0, 120.0),
            Cargo::Fuel => (60.0, 100.0),
            Cargo::Timber => (80.0, 120.0),
            Cargo::Steel => (80.0, 120.0),
            Cargo::Planks => (70.0, 110.0),
            Cargo::Food => (20.0, 40.0),
            Cargo::Goods => (30.0, 70.0),
            Cargo::Silicon => (100.0, 150.0),
            Cargo::Chips => (40.0, 80.0),
            Cargo::Regolith => (100.0, 150.0),
            Cargo::LunarGlass | Cargo::RareMetals => (60.0, 120.0),
            Cargo::Helium3 => (40.0, 80.0),
            Cargo::NanoGoods => (30.0, 60.0),
        }
    }

    /// Units one wagon holds (a coach seats 40; a mail van holds 30 bags; the rest 20).
    pub fn wagon_capacity(self) -> u32 {
        match self {
            Cargo::Passengers => 40,
            Cargo::Mail => 30,
            _ => 20,
        }
    }
}

/// A dial on everything cargo pays, kept apart from the rate table so the table's relative
/// values (value rising along a chain) stay as designed. Tuned by the headless economy test.
pub const PAY_SCALE: f32 = 1.3;

/// What a delivered load pays (GAME-DESIGN.md §3): units × (tiles / 10) × rate × time factor.
/// Full pay while the cargo is fresher than its `fresh` seconds, then falling
/// in a straight line to 30% by `fresh + stale`.
pub fn fare(cargo: Cargo, units: u32, tiles: f32, seconds: f32) -> u32 {
    let (fresh, stale) = cargo.freshness();
    let time = if seconds <= fresh {
        1.0
    } else {
        (1.0 - 0.7 * ((seconds - fresh) / stale)).max(0.3)
    };
    (units as f32 * (tiles / 10.0) * cargo.rate() * PAY_SCALE * time).round() as u32
}

/// Cargo waiting at a station: whole units come out, fractions pile up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stock {
    units: [f32; COUNT],
}

/// The most of one cargo a station keeps; the rest is thrown away (GAME-DESIGN.md §5.6).
pub const STATION_CAP: f32 = 1_000.0;

impl Stock {
    pub fn get(&self, cargo: Cargo) -> u32 {
        self.units[cargo.index()] as u32
    }

    pub fn add(&mut self, cargo: Cargo, amount: f32) {
        let u = &mut self.units[cargo.index()];
        *u = (*u + amount).min(STATION_CAP);
    }

    /// Takes up to `want` whole units. Returns how many.
    pub fn take(&mut self, cargo: Cargo, want: u32) -> u32 {
        let u = &mut self.units[cargo.index()];
        let got = (*u as u32).min(want);
        *u -= got as f32;
        got
    }
}

/// What decides a station's rating for a cargo (GAME-DESIGN.md §5.2).
#[derive(Clone, Copy, Debug)]
pub struct RatingInputs {
    /// Seconds since a vehicle last loaded this cargo here.
    pub since_pickup: f32,
    /// How fast the last vehicle that loaded was able to go (tiles per second).
    pub speed: f32,
    /// Units waiting now.
    pub waiting: u32,
    /// Years old the last vehicle was.
    pub age: i32,
}

/// The rating (0 to 1) a station is heading for.
pub fn target_rating(i: &RatingInputs) -> f32 {
    // A base that a bare station with no service sits at, then the factors.
    let base = 0.25;
    // Speed of the last vehicle: up to +17%.
    let speed = (i.speed / 6.0).clamp(0.0, 1.0) * 0.17;
    // Time since a vehicle last loaded: +25% when just served, falling to -50% after two minutes.
    let recency = if i.since_pickup <= 15.0 {
        0.25
    } else {
        0.25 - 0.75 * ((i.since_pickup - 15.0) / 105.0).min(1.0)
    };
    // Cargo waiting: +10% when little, -20% when the platform is piling up.
    let waiting = match i.waiting {
        0..=100 => 0.10,
        101..=500 => 0.10 - 0.10 * ((i.waiting - 100) as f32 / 400.0),
        _ => -0.20 * ((i.waiting - 500) as f32 / 500.0).min(1.0),
    };
    // New vehicles are better: up to +10%, nothing after ten years.
    let age = ((10 - i.age.max(0)) as f32 / 10.0).clamp(0.0, 1.0) * 0.10;
    (base + speed + recency + waiting + age).clamp(0.0, 1.0)
}

/// How much of what the buildings nearby would send actually shows up at a
/// station with this rating: 66% is "normal" (1.0), 100% brings half as much again.
pub fn share(rating: f32) -> f32 {
    rating * 1.5
}

/// Per-station, per-cargo service record and rating.
#[derive(Clone, Copy, Debug)]
pub struct Service {
    pub rating: f32,
    /// Game seconds when something last loaded; `None` if nothing ever has.
    pub last_pickup: Option<f64>,
    pub last_speed: f32,
    pub last_age: i32,
}

impl Default for Service {
    fn default() -> Service {
        Service {
            rating: 0.25,
            last_pickup: None,
            last_speed: 0.0,
            last_age: 10,
        }
    }
}

impl Service {
    /// Moves the rating a little towards where it should be. `dt` in seconds.
    pub fn update(&mut self, now: f64, waiting: u32, dt: f32) {
        let since = self.last_pickup.map_or(1_000.0, |t| (now - t).max(0.0) as f32);
        let target = target_rating(&RatingInputs {
            since_pickup: since,
            speed: self.last_speed,
            waiting,
            age: self.last_age,
        });
        // About ten seconds to settle: a service visibly earns its rating.
        let step = (dt / 10.0).min(1.0);
        self.rating += (target - self.rating) * step;
    }

    pub fn loaded(&mut self, now: f64, speed: f32, age: i32) {
        self.last_pickup = Some(now);
        self.last_speed = speed;
        self.last_age = age;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_covers_every_cargo() {
        for (i, c) in Cargo::ALL.iter().enumerate() {
            assert_eq!(c.index(), i);
            assert!(c.rate() > 0.0 && !c.name().is_empty() && c.wagon_capacity() > 0);
            let (fresh, stale) = c.freshness();
            assert!(fresh > 0.0 && stale > 0.0);
        }
        assert_eq!(Cargo::ALL.len(), COUNT);
        // Value rises along the chains (GAME-DESIGN.md §4).
        assert!(Cargo::Goods.rate() > Cargo::Steel.rate() && Cargo::Steel.rate() > Cargo::Coal.rate());
        assert_eq!(Cargo::Mail.class(), Class::People);
        assert_eq!(Cargo::Food.class(), Class::Fresh);
    }

    #[test]
    fn fares_follow_the_worked_examples() {
        // GAME-DESIGN.md §3: 120 passengers 100 tiles quickly is about £2,880 at full pay.
        let full = fare(Cargo::Passengers, 120, 100.0, 15.0);
        assert_eq!(full, (2_880.0 * PAY_SCALE).round() as u32);
        assert!(fare(Cargo::Passengers, 120, 100.0, 50.0) < full);
        // Coal barely cares about time; food does.
        let slow = |c| fare(c, 100, 100.0, 120.0) as f32 / fare(c, 100, 100.0, 1.0) as f32;
        assert!(slow(Cargo::Coal) > 0.9, "coal {}", slow(Cargo::Coal));
        assert!(slow(Cargo::Food) < 0.35, "food {}", slow(Cargo::Food));
        assert_eq!(fare(Cargo::Mail, 25, 0.0, 0.0), 0);
    }

    #[test]
    fn stock_hands_out_whole_units_and_has_a_cap() {
        let mut s = Stock::default();
        s.add(Cargo::Passengers, 12.6);
        s.add(Cargo::Mail, 3.0);
        assert_eq!(s.get(Cargo::Passengers), 12);
        assert_eq!(s.take(Cargo::Passengers, 30), 12);
        assert_eq!(s.get(Cargo::Passengers), 0);
        assert_eq!(s.get(Cargo::Mail), 3, "other cargo is untouched");
        s.add(Cargo::Coal, 5_000.0);
        assert_eq!(s.get(Cargo::Coal), 1_000);
    }

    #[test]
    fn frequent_fast_new_service_earns_a_better_rating_than_neglect() {
        let served = target_rating(&RatingInputs {
            since_pickup: 5.0,
            speed: 6.0,
            waiting: 30,
            age: 1,
        });
        let ok = target_rating(&RatingInputs {
            since_pickup: 20.0,
            speed: 3.0,
            waiting: 100,
            age: 5,
        });
        let neglected = target_rating(&RatingInputs {
            since_pickup: 600.0,
            speed: 0.0,
            waiting: 900,
            age: 20,
        });
        assert!(served > 0.8, "{served}");
        assert!(ok > 0.4 && ok < served, "{ok}");
        assert!(neglected < 0.05, "{neglected}");
        // Piling up cargo costs rating.
        let busy = |w| {
            target_rating(&RatingInputs {
                since_pickup: 10.0,
                speed: 3.0,
                waiting: w,
                age: 3,
            })
        };
        assert!(busy(50) > busy(400) && busy(400) > busy(1000));
    }

    #[test]
    fn a_rating_builds_with_service_and_fades_without() {
        let mut s = Service::default();
        let mut now = 0.0;
        for _ in 0..60 {
            now += 1.0;
            s.loaded(now, 3.0, 1);
            s.update(now, 20, 1.0);
        }
        let good = s.rating;
        assert!(good > 0.6, "{good}");
        for _ in 0..300 {
            now += 1.0;
            s.update(now, 20, 1.0);
        }
        assert!(s.rating < good - 0.3, "{} after neglect", s.rating);
        // A normal rating brings the normal flow.
        assert!((share(0.66) - 1.0).abs() < 0.02);
    }
}
