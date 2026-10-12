//! Game time: a calendar date and time of day that tick up with the simulation.
//!
//! One game hour is [`SECONDS_PER_HOUR`] seconds of simulation, so at normal
//! speed a day takes 48 s, a week about 5½ minutes and a year about 4¾ hours.
//! The speed buttons multiply everything (vehicles, towns, the clock itself):
//! at 8× a day is 6 s and a year about 36 minutes.
//!
//! The calendar is the real Gregorian one, with leap years, starting on
//! Sunday 1 January 1950. Weeks and months are for display and, later, for
//! the economy's accounting periods (see GAME-DESIGN.md).

/// Simulation seconds in one game hour.
pub const SECONDS_PER_HOUR: f64 = 2.0;
const START_YEAR: i32 = 1950;
/// Time of day the game starts at, so the first view is in morning light.
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
pub struct DateTime {
    pub year: i32,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    /// 0 = Sunday.
    pub weekday: u32,
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

impl DateTime {
    pub fn date_string(&self) -> String {
        format!(
            "{} {} {} {}",
            WEEKDAYS[self.weekday as usize],
            self.day,
            MONTHS[self.month as usize - 1],
            self.year
        )
    }

    /// Time to the nearest quarter hour, so the display changes twice a second
    /// at normal speed, not every frame.
    pub fn time_string(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute / 15 * 15)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// Game hours since midnight on 1 January of the start year.
    hours: f64,
}

impl Default for Clock {
    fn default() -> Clock {
        Clock::starting_at(START_HOUR)
    }
}

impl Clock {
    pub fn starting_at(hour: f64) -> Clock {
        Clock { hours: hour }
    }

    /// Advances by `sim_dt` seconds of simulation time.
    pub fn tick(&mut self, sim_dt: f32) {
        self.hours += sim_dt as f64 / SECONDS_PER_HOUR;
    }

    /// Advances by `real_dt` seconds at `speed`; returns the simulation time
    /// that passed, which is what everything else should step by.
    #[cfg(test)]
    pub fn advance(&mut self, real_dt: f32, speed: Speed) -> f32 {
        let sim = real_dt * speed.factor();
        self.tick(sim);
        sim
    }

    /// How far through the day it is: 0 at midnight, 0.5 at noon.
    pub fn day_fraction(&self) -> f32 {
        (self.hours / 24.0).fract() as f32
    }

    pub fn date_time(&self) -> DateTime {
        let days = (self.hours / 24.0).floor() as i64;
        let (year, month, day) = civil_from_days(days_from_civil(START_YEAR, 1, 1) + days);
        let minutes = ((self.hours - days as f64 * 24.0) * 60.0) as u32;
        DateTime {
            year,
            month,
            day,
            hour: minutes / 60 % 24,
            minute: minutes % 60,
            weekday: (days_from_civil(START_YEAR, 1, 1) + days + 4).rem_euclid(7) as u32,
        }
    }

    /// Real seconds until the displayed time (quarter hours) next changes.
    pub fn seconds_to_next_display_change(&self, speed: Speed) -> f32 {
        let quarter = (self.hours * 4.0).fract();
        let sim = (1.0 - quarter) * SECONDS_PER_HOUR / 4.0;
        (sim / speed.factor().max(0.001) as f64) as f32
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
    fn starts_on_a_sunday_morning_in_1950() {
        let t = Clock::default().date_time();
        assert_eq!(t.date_string(), "Sun 1 Jan 1950");
        assert_eq!(t.time_string(), "08:00");
    }

    #[test]
    fn a_day_is_48_seconds_and_the_calendar_has_leap_years() {
        let mut c = Clock::starting_at(0.0);
        let at = |c: &Clock| c.date_time().date_string();
        c.advance(48.0, Speed::X1);
        assert_eq!(at(&c), "Mon 2 Jan 1950");
        // Faster speeds pass the same game time in less real time.
        let mut d = Clock::starting_at(0.0);
        d.advance(6.0, Speed::X8);
        assert_eq!(at(&d), "Mon 2 Jan 1950");
        // 1952 was a leap year: 1 Jan 1952 + 59 days is 29 Feb.
        let mut c = Clock::starting_at(0.0);
        c.advance((365.0 + 365.0 + 59.0) * 48.0, Speed::X1);
        assert_eq!(at(&c), "Fri 29 Feb 1952");
        c.advance(48.0, Speed::X1);
        assert_eq!(at(&c), "Sat 1 Mar 1952");
        // Paused time doesn't move.
        let before = c.date_time();
        assert_eq!(c.advance(100.0, Speed::Paused), 0.0);
        assert_eq!(c.date_time(), before);
    }

    #[test]
    fn the_time_of_day_wraps() {
        let mut c = Clock::starting_at(23.0);
        assert_eq!(c.date_time().time_string(), "23:00");
        c.advance(3.0, Speed::X1);
        let t = c.date_time();
        assert_eq!((t.day, t.time_string().as_str()), (2, "00:30"));
        assert!((c.day_fraction() - 0.5 / 24.0).abs() < 1e-4);
    }
}
