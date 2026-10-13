//! Events: the headlines of fifty years (GAME-DESIGN.md §10.9). Each has a start year (shifted a
//! little per game so the future isn't a calendar to memorise), a visible effect and a length.
//! What is in force is a pure function of the date, so jumping about in time stays consistent.

/// What the world is doing to the company right now: multipliers on its income and costs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mods {
    pub passengers: f32,
    pub freight: f32,
    pub running: f32,
    pub interest: f32,
    /// How fast towns grow.
    pub growth: f32,
}

impl Default for Mods {
    fn default() -> Mods {
        Mods {
            passengers: 1.0,
            freight: 1.0,
            running: 1.0,
            interest: 1.0,
            growth: 1.0,
        }
    }
}

pub struct Event {
    pub year: f32,
    pub title: &'static str,
    pub text: &'static str,
    /// Months it lasts; `None` is for good.
    pub months: Option<u32>,
    /// What it does while it lasts (and, for the pandemic, afterwards: `after`).
    pub during: Mods,
    pub after: Option<Mods>,
    /// Whether the start year moves between games.
    pub shifts: bool,
}

const fn mods(passengers: f32, freight: f32, running: f32, interest: f32, growth: f32) -> Mods {
    Mods {
        passengers,
        freight,
        running,
        interest,
        growth,
    }
}

const NONE: Mods = mods(1.0, 1.0, 1.0, 1.0, 1.0);

pub const EVENTS: [Event; 15] = [
    Event {
        year: 2001.0,
        title: "Dot-com bust",
        text: "Office demand dips. Towns grow more slowly for a year.",
        months: Some(12),
        during: mods(1.0, 1.0, 1.0, 1.0, 0.8),
        after: None,
        shifts: false,
    },
    Event {
        year: 2003.0,
        title: "Low-cost airlines take off",
        text: "Cheap flights fill the airports. Short routes pay less than they did.",
        months: None,
        during: NONE,
        after: None,
        shifts: true,
    },
    Event {
        year: 2005.0,
        title: "Fuel price spike",
        text: "Oil prices climb. Running costs are up 30% for two years.",
        months: Some(24),
        during: mods(1.0, 1.0, 1.3, 1.0, 1.0),
        after: None,
        shifts: true,
    },
    Event {
        year: 2007.0,
        title: "Smartphones everywhere",
        text: "Journey planners in every pocket: passengers choose trains a little more often.",
        months: None,
        during: mods(1.03, 1.0, 1.0, 1.0, 1.0),
        after: None,
        shifts: true,
    },
    Event {
        year: 2008.0,
        title: "Financial crisis",
        text: "Income is down 20% for eighteen months and interest rates jump. Over-borrowed companies suffer.",
        months: Some(18),
        during: mods(0.8, 0.8, 1.0, 1.5, 0.9),
        after: None,
        shifts: true,
    },
    Event {
        year: 2010.0,
        title: "E-commerce boom",
        text: "Everyone is ordering online. Freight pays 10% more for good.",
        months: None,
        during: mods(1.0, 1.1, 1.0, 1.0, 1.0),
        after: None,
        shifts: true,
    },
    Event {
        year: 2020.0,
        title: "The pandemic",
        text: "People stay home: passenger demand falls 60% for a year, then settles at 90% as remote work sticks. Freight is up 20%.",
        months: Some(12),
        during: mods(0.4, 1.2, 1.0, 1.0, 1.0),
        after: Some(mods(0.9, 1.0, 1.0, 1.0, 1.0)),
        shifts: false,
    },
    Event {
        year: 2024.0,
        title: "The AI boom",
        text: "Hosting rates triple and the first AI campuses open. Compute is suddenly worth a lot.",
        months: None,
        during: NONE,
        after: None,
        shifts: false,
    },
    Event {
        year: 2027.0,
        title: "First sub-orbital passengers",
        text: "A rocket takes tourists to the edge of space. Spaceports are now a business plan.",
        months: None,
        during: NONE,
        after: None,
        shifts: true,
    },
    Event {
        year: 2029.0,
        title: "The AI correction",
        text: "Hosting rates fall by 40% and some campuses close. The compute stays.",
        months: None,
        during: NONE,
        after: None,
        shifts: false,
    },
    Event {
        year: 2031.0,
        title: "The seas are rising",
        text: "Scientists warn: the sea will rise a block in 2036, another in 2042 and another in 2048. Low coasts will flood. Sea walls (Land toolbar) keep it out.",
        months: None,
        during: NONE,
        after: None,
        shifts: false,
    },
    Event {
        year: 2031.0,
        title: "First lunar cargo",
        text: "A freighter returns from the moon with regolith. Orbital launch is no longer a stunt.",
        months: None,
        during: NONE,
        after: None,
        shifts: true,
    },
    Event {
        year: 2035.0,
        title: "Fusion pilot plant",
        text: "A fusion pilot plant feeds the grid. Running costs ease a little everywhere.",
        months: None,
        during: mods(1.0, 1.0, 0.95, 1.0, 1.0),
        after: None,
        shifts: true,
    },
    Event {
        year: 2038.0,
        title: "The automation dividend",
        text: "Driverless everything. Running costs fall by about 40%.",
        months: None,
        during: mods(1.0, 1.0, 0.6, 1.0, 1.0),
        after: None,
        shifts: true,
    },
    Event {
        year: 2045.0,
        title: "Convergence announced",
        text: "The great project is announced. The world is waiting to see what you build.",
        months: None,
        during: NONE,
        after: None,
        shifts: false,
    },
];

fn mul(a: Mods, b: Mods) -> Mods {
    Mods {
        passengers: a.passengers * b.passengers,
        freight: a.freight * b.freight,
        running: a.running * b.running,
        interest: a.interest * b.interest,
        growth: a.growth * b.growth,
    }
}

/// When event `i` starts in the game with this seed: its year, nudged by up to a year either way.
pub fn start(i: usize, seed: u32) -> f32 {
    let e = &EVENTS[i];
    if !e.shifts {
        return e.year;
    }
    let mut h = seed.wrapping_mul(0x9E37_79B1) ^ (i as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 13;
    e.year + (h % 3) as f32 - 1.0
}

/// What is in force at `date` (a year and fraction), in the game with this seed.
pub fn active(date: f32, seed: u32) -> Mods {
    let mut m = Mods::default();
    for (i, e) in EVENTS.iter().enumerate() {
        let from = start(i, seed);
        if date < from {
            continue;
        }
        match e.months {
            None => m = mul(m, e.during),
            Some(n) => {
                if date < from + n as f32 / 12.0 {
                    m = mul(m, e.during);
                } else if let Some(after) = e.after {
                    m = mul(m, after);
                }
            }
        }
    }
    m
}

/// Storms (§10.9): from 2030, about one month in twenty has one somewhere over the map. Flights are
/// grounded and launches held while it lasts. A pure function of the month and the game's seed.
pub fn storm(date: f32, seed: u32) -> bool {
    if date < 2030.0 {
        return false;
    }
    let month = (date * 12.0).floor() as u32;
    let mut h = month.wrapping_mul(0x9E37_79B1) ^ seed.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 13;
    h % 20 == 0
}

/// The events whose effects are in force at `date`, for the Finances window.
pub fn active_titles(date: f32, seed: u32) -> Vec<&'static str> {
    EVENTS
        .iter()
        .enumerate()
        .filter(|(i, e)| {
            let from = start(*i, seed);
            date >= from
                && e.during != NONE
                && match e.months {
                    None => true,
                    Some(n) => date < from + n as f32 / 12.0,
                }
        })
        .map(|(_, e)| e.title)
        .collect()
}

/// Events that start after `date` and within `years` of it, soonest first: (start, index).
pub fn coming(date: f32, years: f32, seed: u32) -> Vec<(f32, usize)> {
    let mut out: Vec<(f32, usize)> = (0..EVENTS.len())
        .map(|i| (start(i, seed), i))
        .filter(|&(s, _)| s > date && s <= date + years)
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Events that started in `(from, to]`, as indices, for headlines. A jump of more than a year (the
/// date slider) reports nothing, so seeking doesn't flood the screen.
pub fn started_between(from: f32, to: f32, seed: u32) -> Vec<usize> {
    if to - from > 1.0 {
        return Vec::new();
    }
    (0..EVENTS.len())
        .filter(|&i| {
            let s = start(i, seed);
            s > from && s <= to
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_in_force_before_the_first_event_and_the_crisis_bites_then_passes() {
        assert_eq!(active(2000.5, 1), Mods::default());
        let crisis = (0..EVENTS.len()).find(|&i| EVENTS[i].title == "Financial crisis").unwrap();
        let s = start(crisis, 5);
        let during = active(s + 0.5, 5);
        assert!(during.passengers < 0.9 && during.interest > 1.2);
        assert!(active(s + 3.0, 5).interest < during.interest);
    }

    #[test]
    fn the_pandemic_leaves_a_lasting_dent_and_automation_cuts_costs() {
        let m = active(2021.5, 3);
        assert!((m.passengers - 0.9 * 1.03 * 1.0).abs() < 0.05, "{m:?}");
        assert!(active(2045.0, 3).running < 0.7);
    }

    #[test]
    fn start_years_shift_by_at_most_a_year_between_games_but_not_for_fixed_ones() {
        let boom = EVENTS.iter().position(|e| e.title == "The AI boom").unwrap();
        for seed in 0..20 {
            assert_eq!(start(boom, seed), 2024.0);
        }
        let fuel = EVENTS.iter().position(|e| e.title == "Fuel price spike").unwrap();
        let years: std::collections::BTreeSet<i32> = (0..40).map(|s| start(fuel, s) as i32).collect();
        assert!(years.len() >= 2 && years.iter().all(|y| (2004..=2006).contains(y)), "{years:?}");
    }

    #[test]
    fn headlines_fire_once_as_time_passes_and_not_when_seeking() {
        let all: usize = (2000..2050)
            .map(|y| started_between(y as f32, y as f32 + 1.0, 9).len())
            .sum();
        assert!(all >= EVENTS.len() - 1 && all <= EVENTS.len(), "{all}");
        assert!(started_between(2000.0, 2050.0, 9).is_empty());
    }
}

#[cfg(test)]
mod forecast_tests {
    use super::*;

    #[test]
    fn storms_come_after_2030_now_and_then() {
        assert!((0..240).all(|m| !storm(2000.0 + m as f32 / 12.0, 3)));
        let stormy = (0..240).filter(|&m| storm(2030.0 + m as f32 / 12.0, 3)).count();
        assert!((4..=30).contains(&stormy), "{stormy} stormy months in twenty years");
    }

    #[test]
    fn the_forecast_sees_what_is_coming_within_its_horizon() {
        let soon = coming(2007.5, 1.0, 4);
        assert!(soon.iter().all(|&(s, _)| s > 2007.5 && s <= 2008.5));
        let far = coming(2007.5, 20.0, 4);
        assert!(far.len() > soon.len());
        assert!(far.windows(2).all(|w| w[0].0 <= w[1].0), "soonest first");
    }
}
