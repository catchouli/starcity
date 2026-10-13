//! The launch economy (GAME-DESIGN.md §10.4): spaceports start as expensive curiosities that lose
//! money on every launch and become the best-paid thing in the game as the technology matures.
//! Pure arithmetic, tested without a window.

/// What an orbital launch costs to run (£): crew, range and refurbishment, falling in steps as boosters
/// are reused and turnaround speeds up. Linear between the anchor years.
pub fn operating_cost(year: f32) -> f32 {
    const POINTS: [(f32, f32); 6] = [
        (2027.0, 150_000.0),
        (2030.0, 120_000.0),
        (2033.0, 72_000.0),
        (2037.0, 36_000.0),
        (2044.0, 10_800.0),
        (2050.0, 8_000.0),
    ];
    if year <= POINTS[0].0 {
        return POINTS[0].1;
    }
    for w in POINTS.windows(2) {
        let ((y0, c0), (y1, c1)) = (w[0], w[1]);
        if year <= y1 {
            return c0 + (c1 - c0) * (year - y0) / (y1 - y0);
        }
    }
    POINTS[5].1
}

/// Goods carried per launch and what the orbital station pays for them (about £38,000 a load).
pub const PAYLOAD: u32 = 150;
pub const REVENUE: u32 = 38_000;

/// Fuel burnt per launch (kl).
pub const FUEL: u32 = 40;

/// Chance a launch fails: 6% in 2027, falling to 0.3% by 2050.
pub fn failure_chance(year: f32) -> f32 {
    let t = ((year - 2027.0) / 23.0).clamp(0.0, 1.0);
    0.06 + (0.003 - 0.06) * t
}

/// Upkeep of a spaceport, a month (£).
pub const UPKEEP: u32 = 8_000;

/// A launch's result: what it earned, what it cost to run, and whether it failed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Launch {
    pub revenue: u32,
    pub cost: u32,
    pub failed: bool,
}

/// `roll` is a number from 0 to 1 that decides failure.
pub fn launch(year: f32, roll: f32) -> Launch {
    let failed = roll < failure_chance(year);
    Launch {
        revenue: if failed { 0 } else { REVENUE },
        cost: operating_cost(year).round() as u32,
        failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launches_lose_money_in_2030_break_even_in_the_late_2030s_and_pay_by_the_2040s() {
        let net = |y: f32| REVENUE as f32 - operating_cost(y);
        assert!(net(2030.0) < -60_000.0, "{}", net(2030.0));
        assert!(net(2035.0) < 0.0);
        assert!(net(2038.0).abs() < 8_000.0, "{}", net(2038.0));
        assert!(net(2045.0) > 20_000.0);
        assert!(net(2050.0) > 29_000.0);
    }

    #[test]
    fn the_cost_never_rises_and_reliability_improves() {
        let mut last = f32::MAX;
        for y in 2026..=2051 {
            let c = operating_cost(y as f32);
            assert!(c <= last);
            last = c;
        }
        assert!((failure_chance(2027.0) - 0.06).abs() < 1e-6);
        assert!((failure_chance(2050.0) - 0.003).abs() < 1e-6);
        assert!(launch(2030.0, 0.0).failed && !launch(2030.0, 0.99).failed);
        assert_eq!(launch(2030.0, 0.0).revenue, 0);
    }
}
