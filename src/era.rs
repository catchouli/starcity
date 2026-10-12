//! The eras of the game, 2000 to 2050, as a function of the year.
//!
//! Which models are in use is decided by the years written into each model
//! file (see `models.rs`), so artists control it. What lives here is the
//! shared timeline: the era names, and the single "chrome" value (0 until
//! 2025, 1 by 2050) that the shaders use for roads and track, and that the
//! UI theme will follow.

/// How chrome the world is, from 0 (before 2025) to 1 (2050 and after).
pub fn chrome(year: f64) -> f32 {
    let t = ((year - 2025.0) / 25.0).clamp(0.0, 1.0) as f32;
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Era {
    DotCom,
    Broadband,
    ChromeDawn,
    ChromeAge,
    Singularity,
}

impl Era {
    pub fn of(year: i32) -> Era {
        match year {
            ..=2011 => Era::DotCom,
            2012..=2024 => Era::Broadband,
            2025..=2037 => Era::ChromeDawn,
            2038..=2047 => Era::ChromeAge,
            _ => Era::Singularity,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Era::DotCom => "Dot-com",
            Era::Broadband => "Broadband",
            Era::ChromeDawn => "Chrome dawn",
            Era::ChromeAge => "Chrome age",
            Era::Singularity => "Singularity",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_is_nothing_until_2025_and_everything_by_2050() {
        assert_eq!(chrome(2000.0), 0.0);
        assert_eq!(chrome(2025.0), 0.0);
        assert!(chrome(2030.0) < 0.2, "{}", chrome(2030.0));
        assert!((chrome(2037.5) - 0.5).abs() < 1e-6);
        assert!(chrome(2045.0) > 0.8);
        assert_eq!(chrome(2050.0), 1.0);
        assert_eq!(chrome(2080.0), 1.0);
        // It only ever goes up.
        let mut last = 0.0;
        for i in 0..=100 {
            let c = chrome(2020.0 + i as f64 * 0.4);
            assert!(c >= last);
            last = c;
        }
    }

    #[test]
    fn eras_follow_the_calendar() {
        assert_eq!(Era::of(2000), Era::DotCom);
        assert_eq!(Era::of(2024), Era::Broadband);
        assert_eq!(Era::of(2025), Era::ChromeDawn);
        assert_eq!(Era::of(2040), Era::ChromeAge);
        assert_eq!(Era::of(2050), Era::Singularity);
    }
}
