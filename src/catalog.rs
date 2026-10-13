//! What can be bought: locomotives and wagons, their prices, speeds and the years they come
//! and go (GAME-DESIGN.md §7, §10.1). Pure data and rules.

use crate::cargo::Cargo;

/// A locomotive model. Speeds are in tiles per second, prices in pounds, running cost per year.
pub struct Loco {
    pub name: &'static str,
    pub blurb: &'static str,
    pub price: u32,
    pub speed: f32,
    pub max_wagons: u32,
    pub running: u32,
    /// First year it can be bought, and the last.
    pub from: i32,
    pub to: i32,
}

pub const LOCOS: [Loco; 3] = [
    Loco {
        name: "Kestrel (steam)",
        blurb: "Cheap and slow: the default",
        price: 6_000,
        speed: 3.0,
        max_wagons: 5,
        running: 600,
        from: 1900,
        to: 2012,
    },
    Loco {
        name: "Hawk (diesel)",
        blurb: "The best all-rounder",
        price: 11_000,
        speed: 4.5,
        max_wagons: 8,
        running: 1_000,
        from: 1990,
        to: 2100,
    },
    Loco {
        name: "Falcon (electric)",
        blurb: "Fast and dear to run",
        price: 20_000,
        speed: 6.0,
        max_wagons: 10,
        running: 1_700,
        from: 2012,
        to: 2100,
    },
];

impl Loco {
    pub fn available(&self, year: i32) -> bool {
        (self.from..=self.to).contains(&year)
    }

    /// "Coming around 2012" for something not yet on sale.
    pub fn coming(&self, year: i32) -> Option<i32> {
        (year < self.from).then_some(self.from)
    }
}

/// A boat or plane on sale (GAME-DESIGN.md §7.3–7.4).
#[derive(Clone, Copy, Debug)]
pub struct Craft {
    pub name: &'static str,
    pub price: u32,
    /// Tiles a second (cruise, for planes).
    pub speed: f32,
    pub capacity: u32,
    /// A year.
    pub running: u32,
    pub from: i32,
    pub to: i32,
}

impl Craft {
    pub fn available(&self, year: i32) -> bool {
        (self.from..=self.to).contains(&year)
    }
}

pub const BOATS: [Craft; 3] = [
    Craft { name: "Ferry", price: 6_000, speed: 2.5, capacity: 40, running: 500, from: 1950, to: 2040 },
    Craft { name: "Large ferry", price: 16_000, speed: 3.2, capacity: 120, running: 1_300, from: 2008, to: 2200 },
    Craft { name: "Wavepiercer", price: 40_000, speed: 5.0, capacity: 240, running: 2_600, from: 2032, to: 2200 },
];

pub const BUSES: [Craft; 4] = [
    Craft { name: "Minibus", price: 3_000, speed: 1.4, capacity: 20, running: 300, from: 1950, to: 2015 },
    Craft { name: "Bus", price: 5_500, speed: 1.5, capacity: 40, running: 500, from: 1980, to: 2200 },
    Craft { name: "Electric bus", price: 8_000, speed: 1.7, capacity: 50, running: 350, from: 2018, to: 2200 },
    Craft { name: "Autonomous pod bus", price: 12_000, speed: 2.2, capacity: 60, running: 200, from: 2032, to: 2200 },
];

pub const PLANES: [Craft; 4] = [
    Craft { name: "Sparrow", price: 9_000, speed: 7.0, capacity: 16, running: 900, from: 1950, to: 2030 },
    Craft { name: "Kite", price: 24_000, speed: 9.0, capacity: 40, running: 2_200, from: 1990, to: 2200 },
    Craft { name: "Condor", price: 60_000, speed: 14.0, capacity: 100, running: 6_000, from: 2004, to: 2200 },
    Craft { name: "Swift (electric)", price: 90_000, speed: 18.0, capacity: 140, running: 5_000, from: 2034, to: 2200 },
];

/// The cheapest one on sale in `year` (what a buy defaults to).
pub fn default_craft(list: &[Craft], year: i32) -> usize {
    list.iter().position(|c| c.available(year)).unwrap_or(0)
}

/// What one wagon for this cargo costs (a coach, mail van, hopper, tanker, flatbed, goods van, refrigerated van).
pub fn wagon_price(cargo: Cargo) -> u32 {
    match cargo {
        Cargo::Passengers | Cargo::Goods => 600,
        Cargo::Silicon => 500,
        Cargo::Chips | Cargo::NanoGoods => 800,
        Cargo::Regolith => 500,
        Cargo::LunarGlass | Cargo::RareMetals => 650,
        Cargo::Helium3 => 900,
        Cargo::Mail | Cargo::Coal | Cargo::IronOre | Cargo::Grain => 500,
        Cargo::Oil | Cargo::Fuel => 650,
        Cargo::Timber | Cargo::Steel | Cargo::Planks => 550,
        Cargo::Food => 750,
    }
}

/// What the wagon is called.
pub fn wagon_name(cargo: Cargo) -> &'static str {
    match cargo {
        Cargo::Passengers => "coach",
        Cargo::Mail => "mail van",
        Cargo::Coal | Cargo::IronOre | Cargo::Grain => "hopper",
        Cargo::Oil | Cargo::Fuel => "tanker",
        Cargo::Timber | Cargo::Steel | Cargo::Planks => "flatbed",
        Cargo::Goods | Cargo::Chips => "goods van",
        Cargo::Silicon | Cargo::Regolith => "hopper",
        Cargo::LunarGlass | Cargo::RareMetals => "flatbed",
        Cargo::Helium3 => "cryo tanker",
        Cargo::NanoGoods => "secure van",
        Cargo::Food => "refrigerated van",
    }
}

/// A train as chosen in the buy window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainOrder {
    pub loco: usize,
    pub cargo: Cargo,
    pub wagons: u32,
}

impl Default for TrainOrder {
    fn default() -> TrainOrder {
        // Two coaches behind a Kestrel: what a train has always been.
        TrainOrder {
            loco: 0,
            cargo: Cargo::Passengers,
            wagons: 2,
        }
    }
}

impl TrainOrder {
    pub fn loco(&self) -> &'static Loco {
        &LOCOS[self.loco.min(LOCOS.len() - 1)]
    }

    pub fn price(&self) -> u32 {
        self.loco().price + self.wagons * wagon_price(self.cargo)
    }

    /// Per year: the locomotive and £40 a wagon.
    pub fn running(&self) -> u32 {
        self.loco().running + self.wagons * 40
    }

    pub fn capacity(&self) -> u32 {
        self.wagons * self.cargo.wagon_capacity()
    }

    pub fn speed(&self) -> f32 {
        self.loco().speed
    }

    /// Cars in the train: the locomotive and its wagons.
    pub fn cars(&self) -> u32 {
        self.wagons + 1
    }

    /// Keeps the order sensible for the year: an available locomotive and no more wagons than it pulls.
    pub fn fix(&mut self, year: i32) {
        if !LOCOS[self.loco.min(LOCOS.len() - 1)].available(year) {
            self.loco = LOCOS.iter().position(|l| l.available(year)).unwrap_or(0);
        }
        self.wagons = self.wagons.clamp(1, self.loco().max_wagons);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_year_decides_what_is_for_sale() {
        assert!(LOCOS[0].available(2000) && LOCOS[1].available(2000));
        assert!(!LOCOS[2].available(2000));
        assert_eq!(LOCOS[2].coming(2000), Some(2012));
        assert!(LOCOS[2].available(2030));
        assert!(!LOCOS[0].available(2020), "steam retires");
        // Every year has something to buy.
        for year in 1990..=2100 {
            assert!(LOCOS.iter().any(|l| l.available(year)), "nothing for sale in {year}");
        }
    }

    #[test]
    fn prices_and_capacity_add_up() {
        let o = TrainOrder::default();
        assert_eq!(o.price(), 6_000 + 2 * 600);
        assert_eq!(o.capacity(), 80);
        assert_eq!(o.cars(), 3);
        let coal = TrainOrder {
            loco: 1,
            cargo: Cargo::Coal,
            wagons: 8,
        };
        assert_eq!(coal.price(), 11_000 + 8 * 500);
        assert_eq!(coal.capacity(), 160);
        assert!(coal.running() > o.running());
        // Dearer locomotives are faster and pull more.
        assert!(LOCOS[1].speed > LOCOS[0].speed && LOCOS[2].max_wagons > LOCOS[1].max_wagons);
    }

    #[test]
    fn fixing_an_order_respects_the_year_and_the_locomotive() {
        let mut o = TrainOrder {
            loco: 2,
            cargo: Cargo::Passengers,
            wagons: 10,
        };
        o.fix(2000);
        assert_ne!(o.loco, 2, "the Falcon isn't built yet");
        assert!(o.wagons <= o.loco().max_wagons);
        let mut o = TrainOrder {
            loco: 0,
            cargo: Cargo::Passengers,
            wagons: 9,
        };
        o.fix(2000);
        assert_eq!(o.wagons, 5);
        let mut o = TrainOrder {
            loco: 0,
            cargo: Cargo::Passengers,
            wagons: 2,
        };
        o.fix(2020);
        assert_eq!(o.loco, 1, "steam has retired by 2020");
    }
}
