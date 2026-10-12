//! The player's company: cash, loan, the yearly books, the headquarters and
//! its levels, and the "next step" the game suggests. Pure data and rules,
//! so the economy can be tested without a window (GAME-DESIGN.md §2, §9.1, §9.2).

use crate::ui::Tool;

pub const START_CASH: i64 = 40_000;
/// The economy month, in seconds of simulation (GAME-DESIGN.md §2).
pub const MONTH_SECONDS: f32 = 30.0;

// Prices (GAME-DESIGN.md §8). Dials, not facts.
pub const ROAD_PIECE: u32 = 25;
pub const RAIL_PIECE: u32 = 50;
pub const STATION_TILE: u32 = 200;
pub const SIGNAL: u32 = 30;
/// A locomotive and two coaches.
pub const TRAIN: u32 = 5_000;
/// Moving one terrain corner by one block. Gentler than building the track itself
/// would suggest, so that crossing hills is a cost and not a wall.
pub const TERRAFORM_STEP: u32 = 25;

pub fn terraform_cost(plan: &crate::network::Plan) -> u32 {
    plan.terraform_steps * TERRAFORM_STEP
}
pub const FERRY: u32 = 6_000;
pub const PLANE: u32 = 9_000;
/// Running costs per year (charged monthly, earning or not).
const TRAIN_RUNNING: u32 = 600;
const FERRY_RUNNING: u32 = 500;
const PLANE_RUNNING: u32 = 900;
/// Upkeep per piece of road or track per year, and per station tile, port, airport.
const PIECE_UPKEEP: f32 = 2.0;
const STATION_UPKEEP: u32 = 200;
const PORT_UPKEEP: u32 = 400;
const AIRPORT_UPKEEP: u32 = 1_500;
/// Interest on the loan, per year.
const INTEREST: f32 = 0.05;
pub const LOAN_STEP: i64 = 10_000;
/// Months in the red before the warning, and before the game ends.
const WARN_MONTHS: u32 = 3;
const BANKRUPT_MONTHS: u32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Passengers,
    /// Mail and every other cargo.
    Freight,
    Construction,
    Vehicles,
    Running,
    Upkeep,
    Interest,
    Headquarters,
    /// Money from the council's contracts: seed money and completion rewards.
    Contracts,
}

impl Item {
    pub const ALL: [Item; 9] = [
        Item::Passengers,
        Item::Freight,
        Item::Contracts,
        Item::Construction,
        Item::Vehicles,
        Item::Headquarters,
        Item::Running,
        Item::Upkeep,
        Item::Interest,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Item::Passengers => "Passengers",
            Item::Freight => "Mail and freight",
            Item::Construction => "Construction",
            Item::Vehicles => "New vehicles",
            Item::Running => "Running costs",
            Item::Upkeep => "Infrastructure upkeep",
            Item::Interest => "Loan interest",
            Item::Headquarters => "Headquarters",
            Item::Contracts => "Contracts",
        }
    }

    fn index(self) -> usize {
        Item::ALL.iter().position(|&i| i == self).unwrap()
    }
}

/// One year of income and spending by category (positive numbers; income and
/// expense are kept apart by sign of the category: passengers are income).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ledger {
    amounts: [i64; 9],
}

impl Ledger {
    pub fn get(&self, item: Item) -> i64 {
        self.amounts[item.index()]
    }

    fn add(&mut self, item: Item, amount: i64) {
        self.amounts[item.index()] += amount;
    }

    pub fn income(&self) -> i64 {
        self.get(Item::Passengers) + self.get(Item::Freight) + self.get(Item::Contracts)
    }

    pub fn expenses(&self) -> i64 {
        Item::ALL
            .iter()
            .filter(|&&i| !matches!(i, Item::Passengers | Item::Freight | Item::Contracts))
            .map(|&i| self.get(i))
            .sum()
    }
}

/// What the company owns, counted once a month for running costs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Assets {
    pub trains: u32,
    pub ferries: u32,
    pub planes: u32,
    pub road_pieces: u32,
    pub rail_pieces: u32,
    pub station_tiles: u32,
    pub ports: u32,
    pub airports: u32,
}

impl Assets {
    /// What the vehicles and infrastructure would cost new; half of the
    /// infrastructure and all of the vehicles count towards company value.
    pub fn value(&self) -> i64 {
        let vehicles = self.trains * TRAIN + self.ferries * FERRY + self.planes * PLANE;
        let infra = self.road_pieces * ROAD_PIECE
            + self.rail_pieces * RAIL_PIECE
            + self.station_tiles * STATION_TILE
            + self.ports * crate::terminals::PORT_COST
            + self.airports * crate::terminals::AIRPORT_COST;
        vehicles as i64 + infra as i64 / 2
    }
}

/// What the headquarters asks for, and gives, at each level (GAME-DESIGN.md §9.1).
pub struct Level {
    pub name: &'static str,
    /// Paid to build it.
    pub cost: u32,
    /// Lifetime passenger income that must have been earned.
    pub income: i64,
    /// Company value needed.
    pub value: i64,
    /// A town with a station of yours must have this many people.
    pub town: u32,
    pub loan: i64,
    pub vehicles: u32,
    pub unlocks: &'static str,
}

/// Levels 0 to 2 are playable; the rest arrive with the systems they unlock.
pub const LEVELS: [Level; 3] = [
    Level {
        name: "Garage",
        cost: 0,
        income: 0,
        value: 0,
        town: 0,
        loan: 0,
        vehicles: 6,
        unlocks: "Roads, railways, stations and trains",
    },
    Level {
        name: "Depot office",
        cost: 10_000,
        income: 1_000,
        value: 0,
        town: 0,
        loan: 200_000,
        vehicles: 40,
        unlocks: "A loan of up to £200,000 and up to 40 vehicles",
    },
    Level {
        name: "Regional office",
        cost: 40_000,
        income: 20_000,
        value: 100_000,
        town: 1_000,
        loan: 500_000,
        vehicles: 150,
        unlocks: "Ports, ferries, airports and planes; a loan of up to £500,000; 150 vehicles",
    },
];

/// Levels that exist in the design but are not built yet.
pub const LATER_LEVELS: &str =
    "Corporate tower, Data campus, Glass spire and the AI Citadel come with the systems they unlock.";

pub fn level(n: u8) -> &'static Level {
    &LEVELS[(n as usize).min(LEVELS.len() - 1)]
}

/// The level a tool needs, if it's locked behind the headquarters.
pub fn tool_level(tool: Tool) -> u8 {
    match tool {
        Tool::Port | Tool::Boat | Tool::Airport | Tool::Plane => 2,
        _ => 0,
    }
}

#[derive(Clone, Debug)]
pub struct Requirement {
    pub text: String,
    pub met: bool,
}

/// What the game knows about the company right now, for the checklist and
/// the next-step panel.
#[derive(Clone, Copy, Debug, Default)]
pub struct Facts {
    pub rail_pieces: u32,
    pub stations: u32,
    pub trains: u32,
    /// The biggest population of a town one of the company's stations serves.
    pub town_served: u32,
    pub vehicles: u32,
}

/// 12345 -> "£12,345".
pub fn pounds(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}£{out}", if n < 0 { "-" } else { "" })
}

/// What a contract from the town council asks for (GAME-DESIGN.md §9.3): link two towns by
/// rail and carry some people between them. The council pays `seed` money when it starts, to
/// get you going, and `reward` when it's done.
#[derive(Clone, Debug)]
pub struct Contract {
    pub from: String,
    pub to: String,
    pub seed: u32,
    pub reward: u32,
    /// Passengers to carry once the contract has started.
    pub carry: u32,
    pub done: bool,
    started_carried: u32,
}

impl Contract {
    pub fn link(from: &str, to: &str, seed: u32, reward: u32, carry: u32) -> Contract {
        Contract {
            from: from.into(),
            to: to.into(),
            seed,
            reward,
            carry,
            done: false,
            started_carried: 0,
        }
    }
}

/// Where the company stands on its current contract's two towns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkFacts {
    pub station_a: bool,
    pub station_b: bool,
    pub connected: bool,
}

pub struct Company {
    pub cash: i64,
    pub loan: i64,
    pub hq_level: u8,
    /// Everything unlocked, nothing charged (demo mode, testing).
    pub sandbox: bool,
    /// This year's books, and the previous years' (most recent first).
    pub ledger: Ledger,
    pub history: Vec<(i32, Ledger)>,
    pub year: i32,
    /// Passenger income ever earned.
    pub lifetime_income: i64,
    month_clock: f32,
    /// Consecutive months with no passenger income (for the stuck hint).
    pub quiet_months: u32,
    income_this_month: i64,
    pub months_in_red: u32,
    pub bankrupt: bool,
    /// Contracts from the council, the one being worked on first.
    pub contracts: Vec<Contract>,
    /// Passengers carried in all.
    pub carried: u32,
}

impl Company {
    pub fn new(year: i32) -> Company {
        Company {
            cash: START_CASH,
            loan: 0,
            hq_level: 0,
            sandbox: false,
            ledger: Ledger::default(),
            history: Vec::new(),
            year,
            lifetime_income: 0,
            month_clock: 0.0,
            quiet_months: 0,
            income_this_month: 0,
            months_in_red: 0,
            bankrupt: false,
            contracts: Vec::new(),
            carried: 0,
        }
    }

    /// The contract being worked on, if any.
    pub fn contract(&self) -> Option<&Contract> {
        self.contracts.iter().find(|c| !c.done)
    }

    /// Queues contracts and starts the first, paying its seed money.
    pub fn offer_contracts(&mut self, contracts: Vec<Contract>) -> Option<String> {
        self.contracts = contracts;
        self.start_next()
    }

    fn start_next(&mut self) -> Option<String> {
        let carried = self.carried;
        let c = self.contracts.iter_mut().find(|c| !c.done)?;
        c.started_carried = carried;
        let (text, seed) = (
            format!(
                "New contract: link {} and {}. The council pays {} now and {} when it's done.",
                c.from,
                c.to,
                pounds(c.seed as i64),
                pounds(c.reward as i64)
            ),
            c.seed,
        );
        self.cash += seed as i64;
        self.ledger.add(Item::Contracts, seed as i64);
        Some(text)
    }

    /// What the current contract asks, and how far along it is.
    pub fn contract_requirements(&self, link: &LinkFacts, facts: &Facts) -> Vec<Requirement> {
        let Some(c) = self.contract() else { return Vec::new() };
        let carried = self.carried.saturating_sub(c.started_carried).min(c.carry);
        vec![
            Requirement {
                text: format!("A station in {}", c.from),
                met: link.station_a,
            },
            Requirement {
                text: format!("A station in {}", c.to),
                met: link.station_b,
            },
            Requirement {
                text: "Track joining the two stations".into(),
                met: link.connected,
            },
            Requirement {
                text: "A train running".into(),
                met: facts.trains > 0,
            },
            Requirement {
                text: format!("Carry {} passengers ({carried} so far)", c.carry),
                met: carried >= c.carry,
            },
        ]
    }

    /// Checks the current contract; pays the reward and starts the next one when it's done.
    pub fn update_contract(&mut self, link: &LinkFacts, facts: &Facts) -> Vec<String> {
        let mut messages = Vec::new();
        if self.contract().is_none() || self.contract_requirements(link, facts).iter().any(|r| !r.met) {
            return messages;
        }
        let c = self.contracts.iter_mut().find(|c| !c.done).unwrap();
        c.done = true;
        let (reward, text) = (
            c.reward,
            format!(
                "Contract complete: {} and {} are linked. The council pays {}.",
                c.from,
                c.to,
                pounds(c.reward as i64)
            ),
        );
        self.cash += reward as i64;
        self.ledger.add(Item::Contracts, reward as i64);
        messages.push(text);
        messages.extend(self.start_next());
        messages
    }

    pub fn loan_limit(&self) -> i64 {
        if self.sandbox {
            1_000_000
        } else {
            level(self.hq_level).loan
        }
    }

    pub fn vehicle_limit(&self) -> u32 {
        if self.sandbox {
            900
        } else {
            level(self.hq_level).vehicles
        }
    }

    pub fn tool_unlocked(&self, tool: Tool) -> bool {
        self.sandbox || self.hq_level >= tool_level(tool)
    }

    pub fn can_afford(&self, cost: u32) -> bool {
        self.sandbox || self.cash >= cost as i64
    }

    /// Pays for something. Returns false (and charges nothing) if there isn't the money.
    pub fn spend(&mut self, item: Item, cost: u32) -> bool {
        if cost == 0 {
            return true;
        }
        if !self.can_afford(cost) {
            return false;
        }
        if !self.sandbox {
            self.cash -= cost as i64;
            self.ledger.add(item, cost as i64);
        }
        true
    }

    pub fn earn(&mut self, item: Item, amount: u32) {
        self.cash += amount as i64;
        self.ledger.add(item, amount as i64);
        if matches!(item, Item::Passengers | Item::Freight) {
            self.lifetime_income += amount as i64;
            self.income_this_month += amount as i64;
        }
    }

    pub fn borrow(&mut self) -> bool {
        if self.loan + LOAN_STEP > self.loan_limit() {
            return false;
        }
        self.loan += LOAN_STEP;
        self.cash += LOAN_STEP;
        true
    }

    pub fn repay(&mut self) -> bool {
        if self.loan < LOAN_STEP || self.cash < LOAN_STEP {
            return false;
        }
        self.loan -= LOAN_STEP;
        self.cash -= LOAN_STEP;
        true
    }

    /// Cash plus vehicles and half the infrastructure, less the loan.
    pub fn value(&self, assets: &Assets) -> i64 {
        self.cash + assets.value() - self.loan
    }

    /// What the next headquarters level asks for, and whether each is met.
    pub fn requirements(&self, assets: &Assets, facts: &Facts) -> Vec<Requirement> {
        let next = self.hq_level as usize + 1;
        let Some(l) = LEVELS.get(next) else {
            return Vec::new();
        };
        let mut out = vec![Requirement {
            text: format!("£{} to build it", l.cost),
            met: self.cash >= l.cost as i64,
        }];
        if l.income > 0 {
            out.push(Requirement {
                text: format!(
                    "Earn £{} from deliveries (£{} so far)",
                    l.income,
                    self.lifetime_income.min(l.income)
                ),
                met: self.lifetime_income >= l.income,
            });
        }
        if l.value > 0 {
            out.push(Requirement {
                text: format!("Company value £{}", l.value),
                met: self.value(assets) >= l.value,
            });
        }
        if l.town > 0 {
            out.push(Requirement {
                text: format!("Serve a town of {} people with a station", l.town),
                met: facts.town_served >= l.town,
            });
        }
        out
    }

    pub fn can_upgrade(&self, assets: &Assets, facts: &Facts) -> bool {
        let reqs = self.requirements(assets, facts);
        !reqs.is_empty() && reqs.iter().all(|r| r.met)
    }

    /// Pays for the next level and raises it.
    pub fn upgrade(&mut self, assets: &Assets, facts: &Facts) -> bool {
        if !self.can_upgrade(assets, facts) {
            return false;
        }
        let cost = level(self.hq_level + 1).cost;
        if !self.spend(Item::Headquarters, cost) {
            return false;
        }
        self.hq_level += 1;
        true
    }

    /// Advances the economy clock by `dt` seconds. Once a month charges
    /// running costs, upkeep and interest, and tracks time in the red.
    /// Returns the messages to show ("bankrupt" is also set on the company).
    pub fn tick(&mut self, dt: f32, year: i32, assets: &Assets) -> Vec<String> {
        let mut messages = Vec::new();
        if year != self.year {
            let old = std::mem::take(&mut self.ledger);
            self.history.insert(0, (self.year, old));
            self.history.truncate(3);
            self.year = year;
        }
        self.month_clock += dt;
        while self.month_clock >= MONTH_SECONDS {
            self.month_clock -= MONTH_SECONDS;
            self.month(assets, &mut messages);
        }
        messages
    }

    fn month(&mut self, assets: &Assets, messages: &mut Vec<String>) {
        if !self.sandbox {
            let running =
                (assets.trains * TRAIN_RUNNING + assets.ferries * FERRY_RUNNING + assets.planes * PLANE_RUNNING) as f32
                    / 12.0;
            let upkeep = ((assets.road_pieces + assets.rail_pieces) as f32 * PIECE_UPKEEP
                + (assets.station_tiles * STATION_UPKEEP) as f32 / 3.0
                + (assets.ports * PORT_UPKEEP + assets.airports * AIRPORT_UPKEEP) as f32)
                / 12.0;
            let interest = self.loan as f32 * INTEREST / 12.0;
            for (item, amount) in [
                (Item::Running, running),
                (Item::Upkeep, upkeep),
                (Item::Interest, interest),
            ] {
                let amount = amount.round() as i64;
                self.cash -= amount;
                self.ledger.add(item, amount);
            }
        }
        if self.income_this_month == 0 {
            self.quiet_months += 1;
        } else {
            self.quiet_months = 0;
        }
        self.income_this_month = 0;
        if self.cash < 0 && !self.sandbox {
            self.months_in_red += 1;
            if self.months_in_red == WARN_MONTHS {
                messages.push(
                    "You've been in the red for 3 months. Earn money, sell a vehicle or you'll go bankrupt.".into(),
                );
            }
            if self.months_in_red >= BANKRUPT_MONTHS {
                self.bankrupt = true;
            }
        } else {
            self.months_in_red = 0;
        }
    }

    /// The one thing to do next (GAME-DESIGN.md §9.2): a goal, and where to do it.
    pub fn next_step(&self, assets: &Assets, facts: &Facts, link: &LinkFacts) -> NextStep {
        if self.bankrupt {
            return NextStep::plain("Your company is bankrupt", "Start again from the File menu.");
        }
        if let Some(c) = self.contract() {
            let (from, to) = (&c.from, &c.to);
            if facts.rail_pieces < 12 {
                return NextStep::tool(
                    &format!("Contract: lay a railway from {from} towards {to}"),
                    &format!(
                        "Pick the Build railway tool and drag from open ground by {from} to open ground by {to}. It finds the way and levels the ground. The council has paid you {} to get started.",
                        pounds(c.seed as i64)
                    ),
                    Tool::Rail,
                );
            }
            if !link.station_a {
                return NextStep::tool(
                    &format!("Contract: build a station in {from}"),
                    &format!("Pick the Build station tool and drag along the track at the edge of {from}."),
                    Tool::Station,
                );
            }
            if !link.station_b {
                return NextStep::tool(
                    &format!("Contract: build a station in {to}"),
                    &format!("Pick the Build station tool and drag along the track at the edge of {to}."),
                    Tool::Station,
                );
            }
            if !link.connected {
                return NextStep::tool(
                    "Contract: join the two stations with track",
                    "The stations aren't connected yet. Lay track from one station to the other.",
                    Tool::Rail,
                );
            }
            if facts.trains < 1 {
                return NextStep::tool(
                    "Contract: buy a train",
                    "Pick the Place train tool and click plain track. Then click the train and add both stations to its orders.",
                    Tool::Train,
                );
            }
            return NextStep::plain(
                &format!("Contract: carry {} passengers", c.carry),
                &format!(
                    "Click your train and add the stations in {from} and {to} to its orders. When it has carried enough, the council pays you {}.",
                    pounds(c.reward as i64)
                ),
            );
        }
        if self.hq_level == 0 {
            if facts.rail_pieces < 12 {
                return NextStep::tool(
                    "Lay a railway between two towns",
                    "Pick the Build railway tool and drag along the ground from one town towards another.",
                    Tool::Rail,
                );
            }
            if facts.stations < 2 {
                return NextStep::tool(
                    "Build a station at each end",
                    "Pick the Build station tool and drag along the track beside a town.",
                    Tool::Station,
                );
            }
            if facts.trains < 1 {
                return NextStep::tool(
                    "Buy a train",
                    "Pick the Place train tool and click plain track. Click the train to give it stops.",
                    Tool::Train,
                );
            }
            if self.lifetime_income < level(1).income {
                return NextStep::plain(
                    "Earn your first £1,000",
                    "Click your train and add both stations to its orders: it earns by carrying people between them.",
                );
            }
            return NextStep::hq(
                "Build your Depot office",
                "Click your garage and upgrade it: it opens the loan and bigger fleets.",
            );
        }
        if let Some(next) = LEVELS.get(self.hq_level as usize + 1) {
            let reqs = self.requirements(assets, facts);
            if let Some(unmet) = reqs.iter().find(|r| !r.met) {
                return NextStep::plain(
                    &format!("Grow towards the {}", next.name),
                    &format!("Next: {}.", unmet.text.to_lowercase()),
                );
            }
            return NextStep::hq(
                &format!("Build your {}", next.name),
                "Click your headquarters and upgrade it.",
            );
        }
        NextStep::plain("Keep growing", LATER_LEVELS)
    }

    /// A gentle hint when the company seems stuck (checked monthly; §9.2).
    pub fn stuck_hint(&self, facts: &Facts) -> Option<&'static str> {
        if self.sandbox || self.bankrupt {
            return None;
        }
        if facts.trains > 0 && self.quiet_months >= 3 {
            return Some("No passenger income for 3 months. Check the train has two stations in its orders.");
        }
        if self.cash < 500 && facts.trains == 0 && self.cash < self.loan_limit() {
            return Some("Running low on money. Bulldoze things you don't need, or earn from a train.");
        }
        None
    }
}

/// A suggestion for the Next Step panel.
#[derive(Clone, Debug)]
pub struct NextStep {
    pub title: String,
    pub detail: String,
    /// A tool to pick, which the toolbar highlights.
    pub tool: Option<Tool>,
    /// Points at the headquarters.
    pub hq: bool,
}

impl NextStep {
    fn plain(title: &str, detail: &str) -> NextStep {
        NextStep {
            title: title.into(),
            detail: detail.into(),
            tool: None,
            hq: false,
        }
    }
    fn tool(title: &str, detail: &str, tool: Tool) -> NextStep {
        NextStep {
            title: title.into(),
            detail: detail.into(),
            tool: Some(tool),
            hq: false,
        }
    }
    fn hq(title: &str, detail: &str) -> NextStep {
        NextStep {
            title: title.into(),
            detail: detail.into(),
            tool: None,
            hq: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_company_starts_with_a_garage_and_no_loan() {
        let c = Company::new(2000);
        assert_eq!(c.cash, START_CASH);
        assert_eq!(c.hq_level, 0);
        assert_eq!(c.loan_limit(), 0);
        assert!(!Company::new(2000).borrow());
        assert!(c.tool_unlocked(Tool::Rail));
        assert!(!c.tool_unlocked(Tool::Airport));
    }

    #[test]
    fn spending_checks_the_money_and_the_books_balance() {
        let mut c = Company::new(2000);
        assert!(c.spend(Item::Construction, 30_000));
        assert!(!c.spend(Item::Vehicles, 20_000), "only 10,000 left");
        assert_eq!(c.cash, 10_000);
        c.earn(Item::Passengers, 500);
        assert_eq!(c.cash, 10_500);
        assert_eq!(c.ledger.income(), 500);
        assert_eq!(c.ledger.expenses(), 30_000);
    }

    #[test]
    fn sandbox_is_free_and_unlocked() {
        let mut c = Company::new(2000);
        c.sandbox = true;
        assert!(c.spend(Item::Vehicles, 10_000_000));
        assert_eq!(c.cash, START_CASH);
        assert!(c.tool_unlocked(Tool::Plane));
    }

    #[test]
    fn the_depot_office_opens_the_loan_once_the_first_pounds_are_earned() {
        let mut c = Company::new(2000);
        let (a, f) = (Assets::default(), Facts::default());
        assert!(!c.can_upgrade(&a, &f), "nothing earned yet");
        c.earn(Item::Passengers, 1_000);
        assert!(c.can_upgrade(&a, &f));
        assert!(c.upgrade(&a, &f));
        assert_eq!(c.hq_level, 1);
        assert_eq!(c.cash, START_CASH + 1_000 - 10_000);
        assert!(c.borrow());
        assert_eq!(c.loan, 10_000);
        assert!(c.repay());
        assert_eq!(c.loan, 0);
    }

    #[test]
    fn the_regional_office_needs_a_town_and_value() {
        let mut c = Company::new(2000);
        c.hq_level = 1;
        c.cash = 200_000;
        c.lifetime_income = 50_000;
        let assets = Assets::default();
        let mut facts = Facts::default();
        assert!(!c.can_upgrade(&assets, &facts), "no town of 1,000 served");
        facts.town_served = 1_200;
        assert!(c.can_upgrade(&assets, &facts));
        assert!(c.upgrade(&assets, &facts));
        assert!(c.tool_unlocked(Tool::Airport));
    }

    #[test]
    fn running_costs_and_interest_are_charged_monthly() {
        let mut c = Company::new(2000);
        c.hq_level = 1;
        c.borrow();
        let assets = Assets {
            trains: 2,
            ..Default::default()
        };
        let before = c.cash;
        c.tick(MONTH_SECONDS * 12.0, 2000, &assets);
        // A year: 2 trains at 600 plus 5% on 10,000.
        let expected = 2 * 600 + 500;
        assert!((before - c.cash - expected).abs() <= 6, "paid {}", before - c.cash);
        assert!(c.ledger.get(Item::Running) > 0 && c.ledger.get(Item::Interest) > 0);
    }

    #[test]
    fn a_new_year_files_the_old_books() {
        let mut c = Company::new(2000);
        c.earn(Item::Passengers, 700);
        c.tick(1.0, 2001, &Assets::default());
        assert_eq!(c.ledger.income(), 0);
        assert_eq!(c.history[0].0, 2000);
        assert_eq!(c.history[0].1.income(), 700);
    }

    #[test]
    fn twelve_months_in_the_red_ends_the_game() {
        let mut c = Company::new(2000);
        c.cash = -100;
        let mut warned = false;
        for _ in 0..12 {
            warned |= !c.tick(MONTH_SECONDS, 2000, &Assets::default()).is_empty();
        }
        assert!(warned && c.bankrupt);
        let mut c2 = Company::new(2000);
        c2.cash = -100;
        c2.tick(MONTH_SECONDS * 2.0, 2000, &Assets::default());
        c2.cash = 100;
        c2.tick(MONTH_SECONDS, 2000, &Assets::default());
        assert_eq!(c2.months_in_red, 0, "recovering resets the count");
    }

    #[test]
    fn the_next_step_leads_from_nothing_to_the_depot_office() {
        let mut c = Company::new(2000);
        let a = Assets::default();
        let mut f = Facts::default();
        assert_eq!(c.next_step(&a, &f, &LinkFacts::default()).tool, Some(Tool::Rail));
        f.rail_pieces = 30;
        assert_eq!(c.next_step(&a, &f, &LinkFacts::default()).tool, Some(Tool::Station));
        f.stations = 2;
        assert_eq!(c.next_step(&a, &f, &LinkFacts::default()).tool, Some(Tool::Train));
        f.trains = 1;
        assert!(
            c.next_step(&a, &f, &LinkFacts::default())
                .title
                .contains("first £1,000")
        );
        c.earn(Item::Passengers, 1_200);
        assert!(c.next_step(&a, &f, &LinkFacts::default()).hq);
        c.upgrade(&a, &f);
        // Now it names the next unmet requirement instead of leaving the player stuck.
        let step = c.next_step(&a, &f, &LinkFacts::default());
        assert!(step.title.contains("Regional office"), "{}", step.title);
        assert!(!step.detail.is_empty());
    }

    #[test]
    fn a_quiet_company_gets_a_hint() {
        let mut c = Company::new(2000);
        let f = Facts {
            trains: 1,
            ..Default::default()
        };
        assert!(c.stuck_hint(&f).is_none());
        c.tick(MONTH_SECONDS * 3.0, 2000, &Assets::default());
        assert!(c.stuck_hint(&f).is_some());
        c.earn(Item::Passengers, 10);
        c.tick(MONTH_SECONDS, 2000, &Assets::default());
        assert!(c.stuck_hint(&f).is_none());
    }

    #[test]
    fn a_council_contract_pays_seed_money_then_a_reward_and_leads_the_way() {
        let mut c = Company::new(2000);
        let msg = c.offer_contracts(vec![
            Contract::link("Ashford", "Brookley", 20_000, 15_000, 50),
            Contract::link("Brookley", "Calder", 25_000, 25_000, 100),
        ]);
        assert!(msg.unwrap().contains("Ashford"));
        assert_eq!(c.cash, START_CASH + 20_000, "seed money is paid up front");
        let a = Assets::default();
        let mut f = Facts::default();
        let mut link = LinkFacts::default();
        // The next step walks through the contract.
        assert_eq!(c.next_step(&a, &f, &link).tool, Some(Tool::Rail));
        f.rail_pieces = 40;
        let step = c.next_step(&a, &f, &link);
        assert!(step.title.contains("station in Ashford"), "{}", step.title);
        link.station_a = true;
        assert!(c.next_step(&a, &f, &link).title.contains("station in Brookley"));
        link.station_b = true;
        link.connected = true;
        f.trains = 1;
        assert!(c.update_contract(&link, &f).is_empty(), "nobody carried anyone yet");
        c.carried = 60;
        let before = c.cash;
        let messages = c.update_contract(&link, &f);
        assert!(messages[0].contains("complete"), "{messages:?}");
        // The reward, then the next contract's seed money.
        assert_eq!(c.cash, before + 15_000 + 25_000);
        assert_eq!(c.contract().unwrap().from, "Brookley");
        assert!(c.ledger.get(Item::Contracts) >= 60_000);
        // Its passengers count from when it started, not before.
        assert!(!c.contract_requirements(&link, &f).last().unwrap().met);
    }
}
