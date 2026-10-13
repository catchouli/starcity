//! The governor's verbs (GAME-DESIGN.md §1.2): the charter the agents follow, the Wellbeing ledger
//! kept beside Money, and the ending the singularity reads from both. Pure, tested without a window.

/// Four priorities that always add up to one.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Charter {
    pub weights: [f32; 4],
}

pub const PRIORITIES: [(&str, &str); 4] = [
    (
        "Profit",
        "Agents favour high-yield routes: more trains where cargo is piling up.",
    ),
    (
        "People",
        "Agents connect towns nobody serves yet, and keep shelves stocked.",
    ),
    (
        "Planet",
        "Agents build carefully: construction costs more, the air stays cleaner.",
    ),
    (
        "Reach",
        "Agents push for space and the future: research goes faster, launches pay more.",
    ),
];

pub const PROFIT: usize = 0;
pub const PEOPLE: usize = 1;
pub const PLANET: usize = 2;
pub const REACH: usize = 3;

impl Default for Charter {
    fn default() -> Charter {
        Charter { weights: [0.25; 4] }
    }
}

impl Charter {
    /// Sets one priority; the others share what's left in the proportions they had.
    pub fn set(&mut self, i: usize, value: f32) {
        let value = value.clamp(0.0, 1.0);
        let others: f32 = (0..4).filter(|&j| j != i).map(|j| self.weights[j]).sum();
        for j in 0..4 {
            if j == i {
                self.weights[j] = value;
            } else if others > 1e-6 {
                self.weights[j] *= (1.0 - value) / others;
            } else {
                self.weights[j] = (1.0 - value) / 3.0;
            }
        }
    }

    /// Which kind of thing an agent does next, from a roll between 0 and 1: weighted by Profit and People.
    pub fn agent_goal(&self, roll: f32) -> usize {
        let (p, q) = (self.weights[PROFIT], self.weights[PEOPLE]);
        if p + q <= 1e-6 {
            return PROFIT;
        }
        if roll * (p + q) < p { PROFIT } else { PEOPLE }
    }

    /// What Planet does to construction prices.
    pub fn build_cost_factor(&self) -> f32 {
        1.0 + 0.3 * self.weights[PLANET]
    }

    /// What Reach adds to launch revenue and the Tech Pace.
    pub fn launch_factor(&self) -> f32 {
        1.0 + 0.4 * self.weights[REACH]
    }

    pub fn tech_bonus(&self) -> f32 {
        self.weights[REACH]
    }
}

/// What Wellbeing is made of, each from 0 to 1.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default)]
pub struct People {
    /// Share of the population within reach of a station.
    pub served: f32,
    /// Share of the towns that want food or goods that have them.
    pub supplied: f32,
    /// How well the grid keeps up with demand.
    pub power: f32,
    /// How clean the air is: 1 with no coal burnt.
    pub clean: f32,
}

/// The Wellbeing ledger, 0 to 100.
pub fn wellbeing(p: &People) -> f32 {
    100.0 * (0.4 * p.served + 0.2 * p.supplied + 0.15 * p.power + 0.25 * p.clean).clamp(0.0, 1.0)
}

/// How the fifty years end (§1.2).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Ascension,
    Commons,
    Stagnation,
}

impl Ending {
    pub fn title(self) -> &'static str {
        match self {
            Ending::Ascension => "Ascension",
            Ending::Commons => "Commons",
            Ending::Stagnation => "Stagnation",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Ending::Ascension => {
                "Whole cities of chrome, streets empty, the network running itself in perfect silence. Rockets leave every hour. Beautiful, and a little frightening."
            }
            Ending::Commons => {
                "Crowded plazas under the chrome. Towns that kept their roads, power that doesn't run out, and machines that work for the people who live here."
            }
            Ending::Stagnation => {
                "The future arrived late. The world is left half-chrome, waiting for something that didn't get built."
            }
        }
    }
}

/// Reads the ledgers: `converged` is whether the Convergence Array was finished (§10.5).
pub fn ending(charter: &Charter, wellbeing: f32, converged: bool) -> Ending {
    if !converged {
        Ending::Stagnation
    } else if wellbeing < 50.0 && charter.weights[REACH] + charter.weights[PROFIT] >= 0.5 {
        Ending::Ascension
    } else if wellbeing >= 50.0 {
        Ending::Commons
    } else {
        Ending::Ascension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_charter_always_adds_up_to_one() {
        let mut c = Charter::default();
        c.set(PROFIT, 0.7);
        assert!((c.weights.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert!((c.weights[PEOPLE] - 0.1).abs() < 1e-5);
        c.set(PROFIT, 1.0);
        c.set(PEOPLE, 0.5);
        assert!((c.weights.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{:?}", c.weights);
        assert!((c.weights[PROFIT] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn agents_follow_the_charter() {
        let mut c = Charter::default();
        c.set(PEOPLE, 1.0);
        assert!((0..10).all(|i| c.agent_goal(i as f32 / 10.0) == PEOPLE));
        c.set(PROFIT, 1.0);
        assert!((0..10).all(|i| c.agent_goal(i as f32 / 10.0) == PROFIT));
    }

    #[test]
    fn wellbeing_and_the_three_endings() {
        let good = People {
            served: 0.9,
            supplied: 0.8,
            power: 1.0,
            clean: 0.9,
        };
        let bad = People {
            served: 0.2,
            supplied: 0.1,
            power: 0.5,
            clean: 0.2,
        };
        assert!(wellbeing(&good) > 80.0 && wellbeing(&bad) < 30.0);
        let mut greedy = Charter::default();
        greedy.set(REACH, 0.6);
        assert_eq!(ending(&greedy, wellbeing(&bad), true), Ending::Ascension);
        assert_eq!(ending(&Charter::default(), wellbeing(&good), true), Ending::Commons);
        assert_eq!(ending(&Charter::default(), wellbeing(&good), false), Ending::Stagnation);
    }
}
