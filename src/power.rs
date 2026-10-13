//! Electricity as a service, not cargo (GAME-DESIGN.md §5.7): sources make
//! megawatts, consumers draw them, and everything within reach of each other
//! shares. A short grid browns out: its consumers run slower.

/// How far (tiles) a source reaches its consumers.
pub const REACH: f32 = 30.0;

/// Even with no grid, machines run on their own generators at this fraction.
pub const GENERATOR_FLOOR: f32 = 0.3;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Source {
    pub at: (f32, f32),
    pub mw: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Consumer {
    pub at: (f32, f32),
    pub mw: f32,
}

/// For each consumer, the share (0 to 1) of what it wants that the grid supplies.
/// Each source's output is shared among the consumers in its reach in proportion to their demand.
pub fn solve(sources: &[Source], consumers: &[Consumer]) -> Vec<f32> {
    let dist = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let mut received = vec![0.0f32; consumers.len()];
    for s in sources {
        let in_reach: Vec<usize> = (0..consumers.len())
            .filter(|&i| dist(s.at, consumers[i].at) <= REACH)
            .collect();
        let demand: f32 = in_reach.iter().map(|&i| consumers[i].mw).sum();
        if demand <= 0.0 {
            continue;
        }
        // Never hand out more than a consumer wants.
        let each = (s.mw / demand).min(1.0);
        for &i in &in_reach {
            received[i] += consumers[i].mw * each;
        }
    }
    consumers
        .iter()
        .zip(&received)
        .map(|(c, r)| if c.mw <= 0.0 { 1.0 } else { (r / c.mw).min(1.0) })
        .collect()
}

/// How fast a machine runs given its grid share: the generator floor at the least.
pub fn efficiency(supplied: f32) -> f32 {
    supplied.max(GENERATOR_FLOOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_in_reach_powers_a_consumer() {
        let s = [Source {
            at: (0.0, 0.0),
            mw: 100.0,
        }];
        let c = [Consumer {
            at: (10.0, 10.0),
            mw: 20.0,
        }];
        assert_eq!(solve(&s, &c), vec![1.0]);
    }

    #[test]
    fn out_of_reach_means_no_grid_and_the_generator_floor() {
        let s = [Source {
            at: (0.0, 0.0),
            mw: 100.0,
        }];
        let c = [Consumer {
            at: (100.0, 0.0),
            mw: 20.0,
        }];
        let supplied = solve(&s, &c);
        assert_eq!(supplied, vec![0.0]);
        assert_eq!(efficiency(supplied[0]), GENERATOR_FLOOR);
    }

    #[test]
    fn a_short_grid_browns_out_everyone_in_proportion() {
        let s = [Source {
            at: (0.0, 0.0),
            mw: 30.0,
        }];
        let c = [
            Consumer {
                at: (5.0, 0.0),
                mw: 20.0,
            },
            Consumer {
                at: (0.0, 5.0),
                mw: 40.0,
            },
        ];
        let supplied = solve(&s, &c);
        assert!(
            (supplied[0] - 0.5).abs() < 1e-5 && (supplied[1] - 0.5).abs() < 1e-5,
            "{supplied:?}"
        );
    }

    #[test]
    fn two_sources_add_up_and_no_one_gets_more_than_they_want() {
        let s = [
            Source {
                at: (0.0, 0.0),
                mw: 100.0,
            },
            Source {
                at: (10.0, 0.0),
                mw: 100.0,
            },
        ];
        let c = [Consumer {
            at: (5.0, 0.0),
            mw: 20.0,
        }];
        assert_eq!(solve(&s, &c), vec![1.0]);
        let weak = [
            Source {
                at: (0.0, 0.0),
                mw: 5.0,
            },
            Source {
                at: (10.0, 0.0),
                mw: 5.0,
            },
        ];
        assert!((solve(&weak, &c)[0] - 0.5).abs() < 1e-5);
    }
}
