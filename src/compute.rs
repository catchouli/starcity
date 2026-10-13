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
}
