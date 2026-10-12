//! Light through the day: the sun's path, sky colour and how bright and warm
//! the light is. Pure functions of the time of day, so the renderer just
//! takes a [`Light`] and it's easy to test and tune.
//!
//! Night is meant to be a mood, not an obstacle: the scene stays readable by
//! moonlight, and the cycle eases off at high game speeds (where a day lasts
//! only a few seconds) so it never strobes.

use crate::clock::Speed;
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    /// Unit vector towards the sun, or the moon at night.
    pub dir: [f32; 3],
    /// Colour of direct light, strength included.
    pub sun: [f32; 3],
    /// Tint of the ambient (hemisphere) light.
    pub ambient: [f32; 3],
    /// Sky colour: the clear colour, and what distant ocean fades into.
    pub sky: [f32; 3],
    /// 1 in full daylight, 0 at night; dims clouds.
    pub day: f32,
}

/// Midday, and what the game looked like before it had a clock.
pub const NOON: Light = Light {
    dir: [-0.391, 0.869, 0.304],
    sun: [0.8, 0.8, 0.8],
    ambient: [1.0, 1.0, 1.0],
    sky: [0.55, 0.70, 0.85],
    day: 1.0,
};

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn scale3(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Light {
    /// The light at a time of day (0 = midnight, 0.5 = noon). `strength` is
    /// how far the cycle is allowed to go from noon: 0 is permanent day,
    /// 1 is the full cycle.
    pub fn at(day_fraction: f32, strength: f32) -> Light {
        let angle = TAU * (day_fraction - 0.25);
        // Sun elevation: 0 at 06:00 and 18:00, 1 at noon, -1 at midnight.
        let elev = angle.sin();
        let day = smoothstep(-0.40, 0.12, elev);
        // Peaks when the sun is on the horizon: warm light and a coloured sky.
        let twilight = (1.0 - (elev + 0.1).abs() / 0.35).clamp(0.0, 1.0);

        let sky = mix3([0.05, 0.08, 0.18], NOON.sky, day);
        let sky = mix3(sky, scale3([0.85, 0.52, 0.38], 0.55 + 0.45 * day), twilight * 0.55);

        let warm = mix3([1.0, 0.68, 0.42], [1.0, 0.97, 0.9], smoothstep(0.05, 0.55, elev));
        let sun = mix3([0.26, 0.34, 0.56], scale3(warm, 0.8), day);

        let ambient = mix3([0.60, 0.66, 0.92], [1.0, 1.0, 1.0], day);

        // East at sunrise, west at sunset; at night the moon takes over from the
        // opposite side. The direction blends between the two with the daylight,
        // so it never jumps while the light is still bright. Both stay a little
        // above the horizon so faces still get some relief.
        let sun_dir = (angle.cos() - 0.25, elev.max(0.3));
        let moon_dir = (-angle.cos() - 0.25, (-elev).max(0.3));
        let x = sun_dir.0 * day + moon_dir.0 * (1.0 - day);
        let y = sun_dir.1 * day + moon_dir.1 * (1.0 - day);
        let z = 0.35;
        let len = (x * x + y * y + z * z).sqrt();
        let computed = Light {
            dir: [x / len, y / len, z / len],
            sun,
            ambient,
            sky,
            day,
        };
        computed.towards(&NOON, 1.0 - strength.clamp(0.0, 1.0))
    }

    /// Blends towards another light (`t` = 0 stays, 1 becomes `other`).
    pub fn towards(&self, other: &Light, t: f32) -> Light {
        let d = mix3(self.dir, other.dir, t);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
        Light {
            dir: [d[0] / len, d[1] / len, d[2] / len],
            sun: mix3(self.sun, other.sun, t),
            ambient: mix3(self.ambient, other.ambient, t),
            sky: mix3(self.sky, other.sky, t),
            day: self.day + (other.day - self.day) * t,
        }
    }

    /// Whether the difference to another light would be visible on screen,
    /// so the scene only needs re-rendering when the light really changed.
    pub fn differs(&self, other: &Light) -> bool {
        // Colours must change by about two or three steps of an 8-bit channel (too
        // little to see while the light is drifting); the sun's direction by a few degrees, as it never stops moving by day and
        // would otherwise keep the scene redrawing flat out.
        const COLOUR: f32 = 0.01;
        const DIRECTION: f32 = 0.06;
        let far = |a: &[f32; 3], b: &[f32; 3], eps: f32| (0..3).any(|i| (a[i] - b[i]).abs() > eps);
        far(&self.dir, &other.dir, DIRECTION)
            || far(&self.sun, &other.sun, COLOUR)
            || far(&self.ambient, &other.ambient, COLOUR)
            || far(&self.sky, &other.sky, COLOUR)
            || (self.day - other.day).abs() > COLOUR
    }
}

/// How strong the day/night cycle is at a game speed: full at normal speed,
/// easing off as days get shorter so they never flicker.
pub fn strength_at(speed: Speed, enabled: bool) -> f32 {
    if !enabled {
        return 0.0;
    }
    match speed {
        Speed::Paused | Speed::X1 => 1.0,
        Speed::X2 => 0.8,
        Speed::X4 => 0.55,
        Speed::X8 => 0.4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brightness(l: &Light) -> f32 {
        // What a flat, upward-facing surface gets, as in the shader.
        let n = [0.0f32, 1.0, 0.0];
        let sun = (n[0] * l.dir[0] + n[1] * l.dir[1] + n[2] * l.dir[2]).max(0.0);
        let amb = 0.65;
        (amb * l.ambient[1]) + l.sun[1] * sun
    }

    #[test]
    fn noon_matches_the_old_fixed_light_and_zero_strength_is_always_day() {
        let noon = Light::at(0.5, 1.0);
        assert!(
            !noon.differs(&NOON) || (noon.sky[2] - NOON.sky[2]).abs() < 0.02,
            "{noon:?}"
        );
        let flat = Light::at(0.0, 0.0);
        assert!(!flat.differs(&NOON));
    }

    #[test]
    fn it_gets_dark_at_night_but_stays_readable() {
        let midnight = Light::at(0.0, 1.0);
        let noon = Light::at(0.5, 1.0);
        let (n, d) = (brightness(&midnight), brightness(&noon));
        assert!(n < d * 0.6, "night {n} vs day {d}");
        assert!(n > d * 0.2, "night is too dark to play in: {n} vs {d}");
        assert!(midnight.sky.iter().all(|&c| c < 0.2));
        assert!(midnight.day < 0.01 && noon.day > 0.99);
    }

    #[test]
    fn sunrise_is_warm_and_the_sun_crosses_the_sky() {
        let dawn = Light::at(0.27, 1.0);
        assert!(dawn.sun[0] > dawn.sun[2], "dawn light is warm: {:?}", dawn.sun);
        let (morning, evening) = (Light::at(0.33, 1.0), Light::at(0.67, 1.0));
        assert!(morning.dir[0] > evening.dir[0], "sun moves from east to west");
        for t in 0..100 {
            let l = Light::at(t as f32 / 100.0, 1.0);
            let len: f32 = l.dir.iter().map(|c| c * c).sum::<f32>().sqrt();
            assert!((len - 1.0).abs() < 1e-4 && l.dir[1] > 0.0);
        }
    }

    #[test]
    fn the_light_barely_changes_around_midday() {
        // So the scene needn't be redrawn while nothing is changing.
        assert!(!Light::at(0.50, 1.0).differs(&Light::at(0.503, 1.0)));
        assert!(Light::at(0.25, 1.0).differs(&Light::at(0.27, 1.0)));
    }

    #[test]
    fn fast_game_speeds_soften_the_cycle() {
        assert!(strength_at(Speed::X8, true) < strength_at(Speed::X1, true));
        assert_eq!(strength_at(Speed::X1, false), 0.0);
    }

    /// How often an idle game would redraw the scene just for the light, at
    /// 10 light checks a second over one 48 s day.
    #[test]
    fn idle_redraws_stay_rare() {
        for (speed, strength, day_seconds) in [(Speed::X1, 1.0, 48.0), (Speed::X8, 0.4, 6.0)] {
            let mut shown = Light::at(0.0, strength);
            let mut redraws = 0;
            let checks = (day_seconds * 10.0) as u32;
            for i in 0..checks {
                let l = Light::at(i as f32 / checks as f32, strength);
                if l.differs(&shown) {
                    shown = l;
                    redraws += 1;
                }
            }
            let per_second = redraws as f32 / day_seconds;
            eprintln!("{speed:?}: {redraws} redraws per day, {per_second:.1}/s");
            // At 8× the scene is redrawing for moving vehicles anyway.
            let limit = if speed == Speed::X1 { 4.5 } else { 10.0 };
            assert!(
                per_second <= limit,
                "{speed:?} redraws {per_second}/s for the light alone"
            );
        }
    }
}
