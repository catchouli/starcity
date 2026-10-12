//! Game time: a calendar date and time of day that tick up with the simulation.
//!
//! The **calendar** runs one day per second of simulation, so at normal speed
//! a month is about 30 s and a year about 6 minutes, in step with the
//! economy's own months and years (GAME-DESIGN.md §2). The game spans 2000 to
//! 2050, so that's about 5 hours at 1×, and under an hour at 8×.
//!
//! The **light** (time of day) is separate: one cycle every
//! [`LIGHT_CYCLE_SECONDS`] of simulation, slow enough to look good. The speed
//! buttons multiply both, along with vehicles, towns and everything else.
//!
//! The calendar is the real Gregorian one, with leap years, starting on
//! Saturday 1 January 2000.

/// Simulation seconds in one calendar day.
pub const SECONDS_PER_DAY: f64 = 1.0;
/// Simulation seconds for the light to go round once (a full day and night).
pub const LIGHT_CYCLE_SECONDS: f32 = 120.0;
const START_YEAR: i32 = 2000;
/// Hour of the day the light starts at, so the first view is in morning light.
pub const START_HOUR: f64 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    Paused,
    X1,
    X2,
    X4,
    X8,
}

impl Speed {
    pub fn factor(self) -> f32 {
        match self {
            Speed::Paused => 0.0,
            Speed::X1 => 1.0,
            Speed::X2 => 2.0,
            Speed::X4 => 4.0,
            Speed::X8 => 8.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Speed::Paused => "Pause",
            Speed::X1 => "1×",
            Speed::X2 => "2×",
            Speed::X4 => "4×",
            Speed::X8 => "8×",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    /// 0 = Sunday.
    pub weekday: u32,
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

impl Date {
    pub fn date_string(&self) -> String {
        format!(
            "{} {} {} {}",
            WEEKDAYS[self.weekday as usize],
            self.day,
            MONTHS[self.month as usize - 1],
            self.year
        )
    }
}

/// The calendar and the light cycle run separately: the calendar fast enough
/// for a 50-year game, the light slowly enough to look good.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// Calendar days since 1 January of the start year.
    days: f64,
    /// Time of day for the light: 0 at midnight, 0.5 at noon.
    light_phase: f32,
}

impl Default for Clock {
    fn default() -> Clock {
        Clock::starting_at(START_HOUR)
    }
}

impl Clock {
    /// A clock at the start date, with the light at this hour of the day.
    pub fn starting_at(hour: f64) -> Clock {
        Clock {
            days: 0.0,
            light_phase: (hour / 24.0).rem_euclid(1.0) as f32,
        }
    }

    /// Jumps to a date given as a year with a fraction (2025.5 is mid-2025).
    pub fn set_year_f64(&mut self, year: f64) {
        let y = year.floor() as i32;
        let in_year = (days_from_civil(y + 1, 1, 1) - days_from_civil(y, 1, 1)) as f64;
        self.days = (days_from_civil(y, 1, 1) - days_from_civil(START_YEAR, 1, 1)) as f64 + (year - y as f64) * in_year;
    }

    /// Advances by `sim_dt` seconds of simulation time.
    pub fn tick(&mut self, sim_dt: f32) {
        self.days += sim_dt as f64 / SECONDS_PER_DAY;
        self.light_phase = (self.light_phase + sim_dt / LIGHT_CYCLE_SECONDS).rem_euclid(1.0);
    }

    /// Advances by `real_dt` seconds at `speed`; returns the simulation time
    /// that passed, which is what everything else should step by.
    #[cfg(test)]
    pub fn advance(&mut self, real_dt: f32, speed: Speed) -> f32 {
        let sim = real_dt * speed.factor();
        self.tick(sim);
        sim
    }

    /// The time of day the light shows: 0 at midnight, 0.5 at noon.
    pub fn light_phase(&self) -> f32 {
        self.light_phase
    }

    pub fn date(&self) -> Date {
        let days = self.days.floor() as i64;
        let (year, month, day) = civil_from_days(days_from_civil(START_YEAR, 1, 1) + days);
        Date {
            year,
            month,
            day,
            weekday: (days_from_civil(START_YEAR, 1, 1) + days + 4).rem_euclid(7) as u32,
        }
    }

    /// The year with the part-way through it as a fraction (2025.5 is about 1 July 2025).
    pub fn year_f64(&self) -> f64 {
        let d = self.date();
        let start = days_from_civil(d.year, 1, 1);
        let end = days_from_civil(d.year + 1, 1, 1);
        let today = days_from_civil(d.year, d.month, d.day);
        d.year as f64 + (today - start) as f64 / (end - start) as f64
    }

    /// Real seconds until the date next changes.
    pub fn seconds_to_next_day(&self, speed: Speed) -> f32 {
        let left = (1.0 - self.days.fract()) * SECONDS_PER_DAY;
        (left / speed.factor().max(0.001) as f64) as f32
    }
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = (y - (m <= 2) as i32) as i64;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m as i64 + if m > 2 { -3 } else { 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (yoe + era * 400) as i32 + (m <= 2) as i32;
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_on_a_saturday_in_2000_with_the_light_at_8am() {
        let c = Clock::default();
        assert_eq!(c.date().date_string(), "Sat 1 Jan 2000");
        assert!((c.light_phase() - 8.0 / 24.0).abs() < 1e-6);
        assert!((c.year_f64() - 2000.0).abs() < 1e-9);
    }

    #[test]
    fn a_day_is_a_second_and_the_calendar_has_leap_years() {
        let mut c = Clock::default();
        let at = |c: &Clock| c.date().date_string();
        c.advance(1.0, Speed::X1);
        assert_eq!(at(&c), "Sun 2 Jan 2000");
        // Faster speeds pass the same game time in less real time.
        let mut d = Clock::default();
        d.advance(0.125, Speed::X8);
        assert_eq!(at(&d), "Sun 2 Jan 2000");
        // 2000 was a leap year (divisible by 400): 1 Jan + 59 days is 29 Feb.
        let mut c = Clock::default();
        c.advance(59.0, Speed::X1);
        assert_eq!(at(&c), "Tue 29 Feb 2000");
        c.advance(1.0, Speed::X1);
        assert_eq!(at(&c), "Wed 1 Mar 2000");
        // 2100 is not: the day after 28 Feb 2100 is 1 Mar.
        let mut c = Clock::default();
        c.advance(100.0 * 365.0 + 25.0 + 31.0 + 27.0, Speed::X1);
        assert_eq!(at(&c).split_once(' ').unwrap().1, "28 Feb 2100");
        c.advance(1.0, Speed::X1);
        assert_eq!(at(&c).split_once(' ').unwrap().1, "1 Mar 2100");
        // Paused time doesn't move.
        let before = c.date();
        assert_eq!(c.advance(100.0, Speed::Paused), 0.0);
        assert_eq!(c.date(), before);
    }

    #[test]
    fn a_game_to_2050_takes_about_five_hours_at_normal_speed() {
        let mut c = Clock::default();
        // 2000 to 2049 has 13 leap years: 1 Jan 2050 is day 18,263.
        let days = 50.0 * 365.0 + 13.0;
        c.advance(days as f32 * SECONDS_PER_DAY as f32, Speed::X1);
        assert_eq!(c.date().date_string(), "Sat 1 Jan 2050");
        assert!((c.year_f64() - 2050.0).abs() < 1e-9, "{}", c.year_f64());
        let hours = days * SECONDS_PER_DAY / 3600.0;
        assert!((4.5..5.5).contains(&hours), "{hours} hours");
    }

    #[test]
    fn the_calendar_can_be_set_to_any_date() {
        let mut c = Clock::default();
        c.set_year_f64(2025.0);
        assert_eq!(c.date().date_string(), "Wed 1 Jan 2025");
        c.set_year_f64(2025.5);
        assert_eq!((c.date().year, c.date().month), (2025, 7));
        assert!((c.year_f64() - 2025.5).abs() < 0.01);
        c.set_year_f64(2000.0);
        assert_eq!(c.date().date_string(), "Sat 1 Jan 2000");
        c.set_year_f64(2050.0);
        assert_eq!(c.date().date_string(), "Sat 1 Jan 2050");
    }

    #[test]
    fn the_light_cycles_independently_of_the_calendar() {
        let mut c = Clock::starting_at(23.0);
        c.advance(LIGHT_CYCLE_SECONDS / 24.0 * 1.5, Speed::X1);
        assert!((c.light_phase() - 0.5 / 24.0).abs() < 1e-4, "{}", c.light_phase());
        // A whole light cycle is four calendar months.
        let mut c = Clock::default();
        c.advance(LIGHT_CYCLE_SECONDS, Speed::X1);
        assert!((c.light_phase() - Clock::default().light_phase()).abs() < 1e-4);
        assert_eq!((c.date().month, c.date().day), (4, 30), "{:?}", c.date());
        assert!(c.seconds_to_next_day(Speed::X8) <= 0.125 + 1e-6);
    }
}
