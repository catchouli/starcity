//! Compute, the game's real score (GAME-DESIGN.md §5.8–5.9). Server farms turn Goods into
//! compute-units (CU); the company's running total is the number on the toolbar, and its
//! units change name as it grows. Pure arithmetic, tested without a window.

/// CU one batch of work makes at the year-2000 chip efficiency.
pub const BATCH_CU: f64 = 25.0;

/// Chips get twice as good about every 2.5 years, so an old farm quietly improves.
pub fn efficiency(year: f64) -> f64 {
    2f64.powf((year - 2000.0) / 2.5)
}

/// What selling one batch of spare compute earns (£): the AI boom triples it from 2024, and the
/// 2029 correction takes some of that back.
pub fn hosting_per_batch(year: i32) -> u32 {
    let base = 40.0 * (1.0 + 0.02 * (year - 2000).max(0) as f32);
    let boom = match year {
        ..=2023 => 1.0,
        2024..=2028 => 3.0,
        _ => 1.8,
    };
    (base * boom).round() as u32
}

/// Unit names, three orders of magnitude apart. The last three are the game's own.
const UNITS: [&str; 12] = [
    "CU", "kCU", "MCU", "GCU", "TCU", "PCU", "ECU", "ZCU", "YCU", "Brim", "Halo", "Sol",
];

/// A value as a number and the unit it reads best in.
pub fn scale(value: f64) -> (f64, &'static str) {
    if value < 1000.0 {
        return (value.max(0.0), UNITS[0]);
    }
    let step = ((value.log10() / 3.0).floor() as usize).min(UNITS.len() - 1);
    (value / 1000f64.powi(step as i32), UNITS[step])
}

/// "12.3 kCU": three significant figures.
pub fn show(value: f64) -> String {
    let (n, unit) = scale(value);
    let digits = if n < 10.0 && unit != "CU" {
        2
    } else if n < 100.0 && unit != "CU" {
        1
    } else {
        0
    };
    format!("{n:.digits$} {unit}")
}

/// Capability tiers on the running total (names are placeholders, thresholds as the design's table).
pub const TIERS: [(&str, f64); 7] = [
    ("Index", 1e3),
    ("Forecast", 1e6),
    ("Assist", 1e9),
    ("Agent", 1e12),
    ("Manager", 1e15),
    ("Architect", 1e18),
    ("Convergence", 1e21),
];

/// Tiers reached by `total`: 0 means none yet.
pub fn tier(total: f64) -> usize {
    TIERS.iter().take_while(|&&(_, t)| total >= t).count()
}

/// What the company's compute is spent on (GAME-DESIGN.md §5.8). Whatever isn't given to a program
/// is sold as hosting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    Analytics,
    Optimiser,
    Design,
    /// Pulls every model and industry's introduction year forward (the Tech Pace).
    Research,
}

pub const PROGRAMS: [Program; 4] = [Program::Analytics, Program::Optimiser, Program::Design, Program::Research];

impl Program {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Program::Analytics => "Route analytics",
            Program::Optimiser => "Fleet optimiser",
            Program::Design => "Generative design",
            Program::Research => "Research",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Program::Analytics => "Better timetables and loading: every fare pays more.",
            Program::Optimiser => "Smoother running and fewer queues: lower running costs.",
            Program::Design => "Plans cheaper cuts and routes: lower construction costs.",
            Program::Research => "Brings new machines and industries forward: the Tech Pace, in years.",
        }
    }

    pub fn from_year(self) -> i32 {
        match self {
            Program::Analytics => 2003,
            Program::Optimiser => 2015,
            Program::Design => 2026,
            Program::Research => 2030,
        }
    }

    /// The most it can do (a fraction) at full strength in `year`: it starts as a curiosity and
    /// grows with the era.
    pub fn strongest(self, year: i32) -> f32 {
        let (lo, hi, span) = match self {
            Program::Analytics => (0.03, 0.12, 22.0),
            Program::Optimiser => (0.05, 0.25, 20.0),
            Program::Design => (0.10, 0.50, 14.0),
            Program::Research => (0.5, 4.0, 20.0),
        };
        let t = ((year - self.from_year()) as f32 / span).clamp(0.0, 1.0);
        lo + (hi - lo) * t
    }

    /// CU a month that it takes to run it properly on a company of this size.
    pub fn need(self, year: i32, fleet: u32) -> f64 {
        let base = match self {
            Program::Analytics => 300.0,
            Program::Optimiser => 500.0,
            Program::Design => 800.0,
            Program::Research => 1500.0,
        };
        base * (1.0 + fleet as f64) * efficiency(year as f64 + 0.5).powf(0.8)
    }

    /// How much of its strongest effect `assigned` CU a month buys: diminishing returns, so a
    /// whole month's compute on one program gives about 60% more than a third of it.
    pub fn effect(self, year: i32, assigned: f64, fleet: u32) -> f32 {
        if year < self.from_year() {
            return 0.0;
        }
        let used = (assigned / self.need(year, fleet)).max(0.0);
        self.strongest(year) * (1.0 - (-1.2 * used).exp()) as f32
    }
}

/// How the monthly compute is shared: a fraction for each program, the rest sold as hosting.
#[derive(Clone, Copy, Debug, Default)]
pub struct Allocation {
    pub share: [f32; 4],
}

impl Allocation {
    pub fn hosting(&self) -> f32 {
        (1.0 - self.share.iter().sum::<f32>()).max(0.0)
    }

    /// Sets one program's share, never taking more than is left after the others.
    pub fn set(&mut self, p: Program, value: f32) {
        let others: f32 = self.share.iter().sum::<f32>() - self.share[p.index()];
        self.share[p.index()] = value.clamp(0.0, (1.0 - others).max(0.0));
    }
}

/// What the running programs do to the company this month.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Effects {
    pub fare_bonus: f32,
    pub running_discount: f32,
    pub build_discount: f32,
    /// Years the world's technology is pulled forward for this company.
    pub tech_pace: f32,
}

pub fn effects(alloc: &Allocation, year: i32, monthly_cu: f64, fleet: u32) -> Effects {
    let e = |p: Program| p.effect(year, monthly_cu * alloc.share[p.index()] as f64, fleet);
    Effects {
        fare_bonus: e(Program::Analytics),
        running_discount: e(Program::Optimiser),
        build_discount: e(Program::Design),
        tech_pace: e(Program::Research),
    }
}

const HISTORY: usize = 120;

#[derive(Clone, Debug, Default)]
pub struct Compute {
    /// Everything made since the company began: the score.
    pub total: f64,
    /// CU a second over the last month.
    pub rate: f64,
    /// The last months' rates on a log axis (log10 of CU/s, at least 0), oldest first.
    pub history: Vec<f32>,
    pub tier: usize,
}

impl Compute {
    /// A month's work: `cu` made over `seconds`. Returns the new tier when one was just reached.
    pub fn month(&mut self, cu: f64, seconds: f64) -> Option<usize> {
        self.total += cu;
        self.rate = cu / seconds.max(1.0);
        self.history.push(self.rate.max(1.0).log10() as f32);
        if self.history.len() > HISTORY {
            self.history.remove(0);
        }
        let now = tier(self.total);
        let reached = (now > self.tier).then_some(now);
        self.tier = now;
        reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_change_every_three_orders_of_magnitude() {
        assert_eq!(show(950.0), "950 CU");
        assert_eq!(show(1_000.0), "1.00 kCU");
        assert_eq!(show(12_345.0), "12.3 kCU");
        assert_eq!(show(4.5e9), "4.50 GCU");
        assert_eq!(show(2.0e27), "2.00 Brim");
        // Past the last name it keeps counting in it rather than panicking.
        assert!(show(1e60).ends_with("Sol"));
    }

    #[test]
    fn efficiency_doubles_every_two_and_a_half_years() {
        assert!((efficiency(2000.0) - 1.0).abs() < 1e-9);
        assert!((efficiency(2002.5) - 2.0).abs() < 1e-9);
        assert!(efficiency(2026.0) > 1000.0);
    }

    #[test]
    fn hosting_booms_in_the_ai_years_and_corrects_after() {
        assert!(hosting_per_batch(2024) > 2 * hosting_per_batch(2023));
        assert!(hosting_per_batch(2030) < hosting_per_batch(2028));
        assert!(hosting_per_batch(2030) > hosting_per_batch(2010));
    }

    #[test]
    fn tiers_fire_once_as_the_total_climbs() {
        let mut c = Compute::default();
        assert_eq!(c.month(500.0, 30.0), None);
        assert_eq!(c.month(600.0, 30.0), Some(1), "past a thousand: Index");
        assert_eq!(c.month(100.0, 30.0), None);
        assert_eq!(c.month(5e8, 30.0), Some(2), "straight past Forecast");
        assert_eq!(TIERS[c.tier - 1].0, "Forecast");
    }

    #[test]
    fn the_history_keeps_a_log_axis_and_is_bounded() {
        let mut c = Compute::default();
        for _ in 0..300 {
            c.month(30_000.0, 30.0);
        }
        assert_eq!(c.history.len(), HISTORY);
        assert!((c.history[0] - 3.0).abs() < 1e-4);
    }

    #[test]
    fn programs_have_diminishing_returns_and_grow_with_the_era() {
        let p = Program::Analytics;
        let need = p.need(2010, 4);
        let third = p.effect(2010, need / 3.0, 4);
        let full = p.effect(2010, need, 4);
        let double = p.effect(2010, need * 2.0, 4);
        assert!(third > 0.0 && full > third && double > full);
        assert!(double < 2.0 * full, "diminishing");
        assert!(full / p.strongest(2010) > 0.6, "about 70% of the ceiling at the need");
        assert_eq!(p.effect(2000, 1e9, 4), 0.0, "not unlocked yet");
        assert!(Program::Design.strongest(2045) > 4.0 * Program::Analytics.strongest(2045));
        assert!(Program::Analytics.strongest(2030) > Program::Analytics.strongest(2005));
    }

    #[test]
    fn allocation_never_gives_out_more_than_everything() {
        let mut a = Allocation::default();
        assert_eq!(a.hosting(), 1.0);
        a.set(Program::Analytics, 0.7);
        a.set(Program::Optimiser, 0.7);
        assert!((a.share[1] - 0.3).abs() < 1e-6);
        assert!(a.hosting().abs() < 1e-6);
        a.set(Program::Analytics, 0.1);
        assert!((a.hosting() - 0.6).abs() < 1e-6);
    }
}
