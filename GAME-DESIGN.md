# Starcity — Economy, industries and vehicles

> A design for the money game that sits on top of what's built. Companion to [DESIGN.md](DESIGN.md), whose §6 ("Industries & economy") this replaces. Everything here is a first pass: the numbers are starting points to tune, not promises. Written 2026-10-12.

## 1. What the player is doing

You run a transport company. Cargo appears at industries and people appear in towns; you build the infrastructure and buy the vehicles to move them; deliveries pay; the money buys more network. The interesting decisions, in the order a player meets them:

1. **What to carry.** A coal mine is one-way and bulky; passengers are two-way; goods are the best-paid cargo but need two supplied industries before they exist.
2. **How to carry it.** Road is cheap and slow, rail is the workhorse, boats suit coasts and oil, planes cross anything but need big airports. Each has cheap and expensive models.
3. **Where to build.** Terrain costs money (levelling, §3 of DESIGN.md), distance pays, and industries are scattered so that the best routes are never obvious.
4. **How to keep it running.** Service quality drives how much cargo industries give you, and starved industries close.

The tone to aim for is "one more line": simple rules that interlock, never a spreadsheet. If a rule can't be explained in a tooltip, it's too complicated.

### Design rules

- **Few cargo types, each with a clear role.** Twelve cargoes in four classes (§4).
- **Two-input recipes, not deep trees.** Chains are at most three steps (mine → mill → factory → town).
- **No hidden state.** Every number the economy uses is shown in a window: production, stock, rating, income per trip.
- **Cheap to simulate.** The whole economy ticks once per economy month (30 simulated seconds); vehicles only touch it when they load or unload (§10).

## 2. Time and money

- **Calendar** ✅ (`clock.rs`): the real Gregorian calendar, starting Sunday 1 January 1950, 08:00. One game hour is 2 seconds of simulation, so at normal speed a day takes 48 s, a week about 5½ minutes, a year about 4¾ hours. The speed buttons (pause, 1×, 2×, 4×, 8×) multiply *everything*, the clock included: at 8× a day is 6 s and a year about 36 minutes. The date shows in the status bar and drives the day/night light (DESIGN.md §7a).
- **Economy time is separate from the calendar.** The economy is paid per second of simulated time, so its rules don't care what the calendar says. In this document, *a tick is 1 simulated second, an economy month is 30 s and an economy year is 360 s* (6 minutes at 1×, 45 s at 8×). That's why a year of running costs and a year of income stay in proportion at any game speed, and why the calendar can be long enough for a day/night cycle to look good. Anything below given "per year", "per month" or in "days" means these units. Production, finances and industry changes tick monthly; vehicles are real-time.
- **Starting position:** £100,000 cash, a loan limit of £200,000 (borrow and repay in £10,000 steps), 5% a year interest on the loan, charged monthly.
- **Income:** paid on delivery, to the company, when cargo is unloaded at a station that accepts it. See §3.
- **Expenses:**
  - *Construction:* priced per piece (`network::PIECE_COST`, now £100; proposed £40 per road piece and £80 per rail piece, since a route between two towns is 100+ tiles), plus levelling at `TERRAFORM_COST` per corner-block, plus stations and terminals (§8).
  - *Vehicle purchase:* the model's price (§7).
  - *Running costs:* each vehicle pays a yearly amount, charged monthly, whether or not it's earning. About 8–12% of its price, more for faster models. This is what makes a badly used vehicle a loss, not just a non-profit.
  - *Infrastructure upkeep:* £2 per track or road piece per year, £200 per station, £400 per port, £1,500 per airport. Small, but it stops the player laying a thousand tiles to nowhere.
  - *Loan interest.*
- **Going broke:** cash below zero for three consecutive months shows a warning; below zero for twelve months ends the game ("bankrupt"). Selling a vehicle returns 60% of its value, less with age.
- **Company value** = cash + vehicles (at depreciated value) + infrastructure (at 50% of cost) − loan. It's the score, and the basis of goals (§9).

### Finance window

A Win95 window with the year's income and expenses split by category (passengers, each cargo, construction, running costs, upkeep, interest) for this year and the last three, plus balance and loan buttons. The status bar always shows cash.

## 3. How deliveries pay

For one delivered load:

```
income = units × (distance / 10) × rate(cargo) × time_factor
```

- `distance` is in tiles, straight-line between the pickup station and the delivery station (not the route taken, so detours don't pay).
- `rate` is per unit per 10 tiles (§4).
- `time_factor` rewards speed. Each cargo has a `fresh` and a `stale` number of seconds: full pay while the cargo's age (seconds since loaded) is within `fresh`, then falling linearly to a floor of 30% by `fresh + stale`. Passengers and food are impatient; coal and ore barely care.

This gives faster, more expensive vehicles a real job: on long routes the time factor is where they earn their price (§7).

**Worked example.** 120 passengers carried 100 tiles in 33 s (fresh 20, stale 60): `120 × 10 × 2.4 × 0.93 ≈ £2,680`. A coal train with 6 hoppers (120 t) over the same distance, one way, `120 × 10 × 3.4 × 0.95 ≈ £3,900`, and it comes back empty, so a full passenger train earns more per trip but needs more people to exist (§6).

Cargo that isn't taken: station stock waits, loses rating while it waits, and is discarded past a cap (§6).

## 4. Cargo

Twelve cargoes in four classes. A vehicle model carries one class (some only one cargo), so class decides which wagons, trucks and ships work (§7).

| Class | Cargo | Rate (£ per unit per 10 tiles) | Fresh / stale (seconds) | Unit |
|---|---|---|---|---|
| People | Passengers | 2.4 | 20 / 60 | person |
| People | Mail | 3.0 | 20 / 50 | bag |
| Bulk | Coal | 3.4 | 100 / 150 | tonne |
| Bulk | Iron ore | 3.2 | 100 / 150 | tonne |
| Bulk | Grain | 3.0 | 40 / 80 | tonne |
| Bulk | Oil | 3.2 | 60 / 120 | kilolitre |
| Bulk | Fuel | 4.4 | 60 / 100 | kilolitre |
| General | Timber | 3.4 | 80 / 120 | tonne |
| General | Steel | 4.8 | 80 / 120 | tonne |
| General | Planks | 4.4 | 70 / 110 | tonne |
| Fresh | Food | 4.8 | 20 / 40 | tonne |
| General | Goods | 6.4 | 30 / 70 | crate |

Value rises along a chain (coal 3.4 → steel 4.8 → goods 6.4), so finishing the chain pays, but only if the player supplies every input.

*Mail* is a second cargo from towns, generated by the same buildings as passengers at a fifth of the rate. It's what makes small aircraft worthwhile on thin routes. *Oil* comes from offshore rigs (§5), which is the reason for tankers.

## 5. Industries

### 5.1 The chains

```
Coal mine ──► Power station                      (coal; fuel optional)
         └──► Steel mill ◄── Iron ore mine       (1 ore + 1 coal → 1 steel)
                   └──► Factory ◄── Sawmill ◄── Forest   (1 steel + 1 planks → 2 goods)
                                                      (1 timber → 1 planks)
Farm ──► Food plant ──► towns                    (1 grain → 1 food)
Oil rig (offshore) ──► Refinery (coastal) ──► towns, power stations   (1 oil → 1 fuel)
Factory ──► towns                                 (goods)
```

| Industry | Kind | Takes | Makes | Notes |
|---|---|---|---|---|
| Coal mine | primary | | Coal | Hills, rock |
| Iron ore mine | primary | | Iron ore | Mountains, rock |
| Forest | primary | | Timber | Grassland, damp |
| Farm | primary | | Grain | Flat grassland, near towns |
| Oil rig | primary | | Oil | Open sea, only reachable by boat |
| Power station | final | Coal, Fuel | (power) | Towns within 40 tiles grow 15% faster; also pays for the coal |
| Steel mill | secondary | Iron ore **and** Coal | Steel | Needs both; output follows the scarcer input |
| Sawmill | secondary | Timber | Planks | |
| Food plant | secondary | Grain | Food | Near a town |
| Factory | secondary | Steel **and** Planks | Goods | Needs both; 1 + 1 → 2 goods, the only industry that multiplies |
| Refinery | secondary | Oil | Fuel | On the coast, with a berth for tankers |

**Why this is interesting but not complicated.** Each industry is a one-line rule. The depth comes from three things the player controls: *which* chain to build, *how far* apart the stages are (a mill 40 tiles from a mine is cheap, 150 tiles pays more but costs more), and *balancing two inputs* (a steel mill with only coal sits idle, and a surplus of one input piles up and gets refused, a visible signal that you need more of the other).

### 5.2 Production and service rating

Each primary industry makes cargo every month: `production` units per month at 100% rating. The **station rating** (0–100%) of a station in its catchment decides what share of that the station actually receives. Rating comes from:

| Factor | Effect |
|---|---|
| Speed of the last vehicle to load (faster is better) | up to +17% |
| Days since a vehicle last loaded | up to +25% (just loaded) to −50% (long ago) |
| Cargo waiting at the station | +10% when under 100 units, to −20% above 1,500 |
| Age of the last vehicle | up to +10% for a new vehicle |
| Statue or similar bonus | later |

Cargo goes to the best-rated station covering the industry, or is **split between the stations covering it in proportion to their ratings**, so competing lines (or your own two) share a mine rather than one winning outright.

**Production drifts.** Once a month each industry has a small chance of a production change. If its best station's rating is over 60% the change is up (+3% to +15%) with 70% odds, otherwise down. A well-served mine slowly grows; a neglected one shrinks.

**Closing.** A primary industry whose production has fallen under 20% of its start and whose rating has stayed under 15% for 24 months announces closure ("Coal mine near Ashford is closing") and disappears the following year. Secondary industries close when their inputs have been unserved for 36 months. When an industry closes, the map spawns a replacement elsewhere (§5.4), so there's always somewhere new to build.

### 5.3 Secondary industries and stock

A secondary industry holds a **stock** of each input (cap 400 units). Unloading adds to stock; each month it converts as much as it can: `min(stock of each input) × recipe`. Its output goes to the stations covering it exactly like a primary's does.

- A station only **accepts** an input while the industry's stock of it is under the cap, shown by the cargo icon in the station window.
- Towns **accept** food, goods and fuel (§5.5), power stations accept coal and fuel, and a final industry's acceptance is unlimited.

### 5.4 Where industries go

Placement is a map-generation step (after towns) plus a monthly check afterwards.

**Primaries** appear in **resource fields**: broad blobs of low-frequency noise, one noise map per resource, so there are regions where coal is likely and others where it isn't. That gives the map a geography the player can read: "the coal is in the northern hills".

| Industry | Terrain rule | Spacing rule |
|---|---|---|
| Coal mine | surface height in the upper half; coal field noise high | ≥ 40 tiles from other coal mines |
| Iron ore mine | upper quarter of heights; ore field noise high | ≥ 60 tiles from other ore mines |
| Forest | grass, mid heights, damp noise high | forests are clusters of 2–4 sites, 6–10 tiles apart |
| Farm | flat grass, ≥ 15 tiles from rock | ≥ 20 tiles from other farms, within 60 tiles of a town |
| Oil rig | open sea ≥ 12 tiles from land | ≥ 50 tiles apart |

**Secondaries** are placed *after* the primaries that feed them, with an aim: each should be **40–140 tiles** from at least one supplier, and no more than two suppliers within 40, so the player always has a worthwhile first route and never a trivial one.

| Industry | Terrain rule |
|---|---|
| Power station | flat, within 10 tiles of water or within 25 tiles of a town |
| Steel mill | flat, not near a town (< 20 tiles) |
| Sawmill | flat grass, 40–120 tiles from a forest |
| Food plant | flat, within 15 tiles of a town |
| Factory | flat, within 20 tiles of a town |
| Refinery | flat coast tile with water within 3 tiles and a body of water the rigs connect to |

**Footprints.** Industries are small: 3×3 to 4×4 tiles, flattened on placement with the same levelling machinery as airports (`terrain::edit`). A site that needs more than 3 blocks of cut or fill is rejected and another is tried.

**Counts on the default 512² map:** roughly 4 coal, 3 ore, 5 forests, 8 farms, 3 oil rigs; 3 power stations, 2 steel mills, 3 sawmills, 3 food plants, 2 factories, 2 refineries (about 40 in all). Counts scale with area.

**After the start.** Every game month there's a 4% chance of a new industry, biased towards filling gaps: a primary with no consumer in range gets one, a secondary with no supplier gets one. The player can also **fund a new industry** from a menu (£40,000 for a primary, placed randomly in a place that suits it, near their network), which is the late-game way to keep growing.

### 5.5 Towns as customers

Town growth already depends on passenger service (`Town::served`). Extend it, so towns are another end of the chains:

- Towns **accept** food, goods and fuel at any station within the catchment of their buildings.
- A town above **500 people** grows at 40% speed unless it received food in the last 3 game months; above **1,500** it also wants goods.
- Delivering what a town wants is paid like any delivery, and also boosts growth, which means more passengers, so serving a town with food and goods feeds its passenger traffic. The two economies feed each other.
- Power stations within 40 tiles speed up growth (15%), a reason to build one near a town and not just near the mine.

### 5.6 Stations, catchment and what stations handle

A station collects from, and delivers to, every industry or town building within its **catchment radius** of any of its tiles.

| Station | Catchment | Handles |
|---|---|---|
| Rail station (platforms) | 3 tiles | everything |
| Road stop (new) | 2 tiles | everything (trucks and buses) |
| Port | 4 tiles | everything but passengers' mail only if a town is in range |
| Airport | 6 tiles | passengers and mail only |

Waiting cargo is stored **per cargo** at the station (`waiting: [f32; N]` becomes a small map), capped at 1,000 per cargo; rating (§5.2) falls as it fills. Vehicles load the cargo their class carries, up to capacity, in order of waiting amount.

## 6. Passengers and mail

Passengers behave as now (buildings near a station trickle people, trains unload everyone at each stop, §5 of DESIGN.md) with two changes:

- **Destinations are implied**: a load of passengers is paid on distance from where they boarded, as in §3, so long routes between big towns pay more, which is the reason to link cities, not hamlets.
- **Mail** is generated in parallel at a fifth of the rate, and carried by mail wagons/vans and aircraft holds. A mail van is the cheapest way to get a small town paying its way.

## 7. Vehicle models

Every mode has two or three models. Cheaper ones are slower, smaller and cost less to run; dearer ones are the opposite. The aim is a real choice, not a ladder you just climb:

- **Cheap and small** wins when the cargo is thin (a village, a small farm), when the line is short, or when you're nearly broke.
- **Big and fast** wins on long routes and busy ones: the `time_factor` (§3) and capacity both scale in its favour, but its price and running costs are paid even when it runs half empty.
- **Break-even guidance** is in each table: roughly, the dearer model should earn 2–3× more per year on a long, busy route and *less* than the cheap one on a short, thin one.

All numbers are per vehicle. Speeds are in tiles per second (the unit the code already uses; trains today run at 3.0, road vehicles 1.1–1.6). Running cost is per year. Models are all available from the start; **eras** (models appearing over the years) are a possible later layer (§11).

### 7.1 Trains

A train is a locomotive plus up to a limit of wagons. Locomotives set speed and haulage; wagons set what's carried.

| Locomotive | Price | Speed | Max wagons | Running | Notes |
|---|---|---|---|---|---|
| Kestrel (steam) | £6,000 | 3.0 | 5 | £600 | The default today |
| Hawk (diesel) | £11,000 | 4.5 | 8 | £1,000 | Best all-rounder |
| Falcon (electric) | £20,000 | 6.0 | 10 | £1,700 | Needs overhead wire (later); fastest |

| Wagon | Price | Capacity | Carries |
|---|---|---|---|
| Coach | £600 | 30 pax | Passengers |
| Mail van | £500 | 25 bags | Mail |
| Hopper | £500 | 20 t | Coal, iron ore, grain |
| Tanker | £650 | 20 kl | Oil, fuel |
| Flatbed | £550 | 20 t | Timber, steel, planks |
| Goods van | £600 | 20 crates | Goods |
| Refrigerated van | £750 | 20 t | Food |

*Check:* a Kestrel with 4 coaches (£8,400) hauls 120 passengers at 3.0; a Hawk with 8 coaches (£15,800) hauls 240 at 4.5, which pays 3× per trip and ≈ 2× as often per hour on a 150-tile route. It only wins if there are 240 people to carry.

### 7.2 Road vehicles

| Vehicle | Price | Speed | Capacity | Running | Notes |
|---|---|---|---|---|---|
| Minibus | £1,800 | 1.4 | 16 pax | £250 | Villages, feeder lines |
| Bus | £3,200 | 1.7 | 32 pax | £400 | |
| Coach | £5,500 | 2.1 | 48 pax | £650 | Long routes |
| Small truck | £1,500 | 1.3 | 10 units | £220 | |
| Truck | £3,000 | 1.6 | 20 units | £380 | |
| Heavy truck | £5,000 | 1.9 | 32 units | £600 | |

A truck is built for one cargo class (bulk, general, fresh, tanker), priced at the base figure, with a 10% premium for tanker and 25% for refrigerated bodies. Road lines run on the existing road graph; vehicles queue (already in), which is the road's real limit: a dozen trucks on one mine's route makes a jam, which is the cue to build a railway.

### 7.3 Boats

| Boat | Price | Speed | Capacity | Running | Notes |
|---|---|---|---|---|---|
| Ferry | £6,000 | 2.0 | 40 pax | £500 | |
| Large ferry | £16,000 | 2.8 | 120 pax | £1,300 | |
| Coaster | £10,000 | 1.8 | 60 units | £800 | Any non-oil cargo class, one at a time |
| Bulk carrier | £28,000 | 2.4 | 200 units | £2,000 | |
| Tanker | £12,000 | 1.8 | 80 kl | £900 | Oil, fuel |
| Large tanker | £34,000 | 2.4 | 240 kl | £2,300 | |

Boats are slow, but they need no track and carry the most per pound. They matter most for oil (rigs are only reachable by sea) and for coastal towns. Ports cost £2,500.

### 7.4 Aircraft

| Aircraft | Price | Speed | Capacity | Range | Running | Airport |
|---|---|---|---|---|---|---|
| Sparrow (light prop) | £9,000 | 7 | 16 pax | 150 tiles | £900 | Small |
| Kite (turboprop) | £24,000 | 9 | 40 pax + 10 mail | 300 | £2,200 | Small |
| Condor (jet) | £60,000 | 14 | 100 pax + 20 mail | any | £6,000 | Large |
| Freighter (later) | £50,000 | 11 | 40 units | any | £4,500 | Large |

Planes earn the same per second of flying as a train of the same pax-speed product (a Kite on a 300-tile route is roughly a Hawk with 8 coaches) but need no track, so they win where track is expensive: across sea, over mountains, between far-apart cities. They need airports at each end, which cost money and flat land.

| Airport | Price | Footprint | Takes | Catchment |
|---|---|---|---|---|
| Small | £8,000 | 10 × 3 (the one that exists) | Sparrow, Kite | 6 |
| Large | £25,000 | 14 × 6, two parallel runways, so two planes can land and take off at once | all | 8 |

### 7.5 Buying and managing vehicles

- **Build vehicle window** (Win95): a list of models for the selected mode, with a picture, price, speed, capacity, running cost and the cargo it carries; "Buy" places it where the tool says (a train on track, a boat on water, a plane on an airport, a road vehicle on a road or stop).
- **Vehicle window** (already there for trains): adds model, age, yearly profit and a "sell" button; boats and planes get the same.
- **Replace** (later): swap a model on a whole route in one go.
- **Breakdowns and servicing** (later, optional): vehicles lose reliability with age and slow down or stop for a few days when they break; depots service them. This turns model choice into a long-term question (cheap models age faster) without changing the core loop, so it's deliberately left for after the economy works.

## 8. Costs summary (starting values)

| Item | Cost |
|---|---|
| Road piece / rail piece | £40 / £80 |
| Levelling | £150 per corner moved one block |
| Rail station platform tile | £300 each |
| Road stop | £250 |
| Port | £2,500 |
| Small / large airport | £8,000 / £25,000 (+ levelling) |
| Fund a new industry | £40,000 |
| Signal | £50 |

These interlock through the payback target below. Treat them as dials, not facts.

**Tuning target:** on a decent first route (two 500-person towns, 100 tiles apart) a steam train line (≈ £20,000 all-in) should pay for itself in about an economy year (360 s of simulated time: 6 minutes at 1×, 45 s at 8×); a coal route about 1.3 years; a first bus line (≈ £10,000 including road) about 1.5 years. If the first line isn't profitable inside the first five minutes, the opening is too slow.

A **headless economy test** should enforce this: build a scripted route on a generated map, run the simulation for N game years at 100× speed, and assert profit lands in range, so rebalancing can't silently break the start of the game.

## 9. Goals, progress and the end of a session

The game is a sandbox first, but a little structure keeps sessions interesting:

- **Yearly goals** shown in a window, three at a time, rotating: "Carry 5,000 passengers this year", "Deliver 1,000 t of coal", "Connect two towns of 1,000+", "Reach company value £250,000". Completing one pays a small bonus (£5,000) and unlocks the next tier. They're suggestions, never forced.
- **Company rating** (0–100%) from vehicle age, profit and goals met; a sensible scoreboard line.
- **Scenario modes** later: "Reach £1M", "Supply every town with goods", on a fixed seed.

## 10. Implementation plan

Order of work, each step playable on its own:

0. **Clock and day/night** ✅. Date and time in the status bar, speed buttons, a light that follows the time of day.
1. **Money.** `Company { cash, loan }`; charge `Plan::cost` on build (refuse if short, show why); a monthly tick; cash in the status bar; the finance window with loans; income from passengers by distance. *Also*: stations record the day and origin of boarding passengers so payment is per distance.
2. **Cargo and stations.** Generalise `Station.waiting: f32` to per-cargo stock; vehicles carry a cargo class; `handle_arrivals` pays out; rating factors; catchment query for industries, as `Towns::population_near` does for buildings. Keep the existing passenger behaviour as the one-cargo case.
3. **Industries.** `industry.rs` with `IndustryKind` (a `const` table of recipe, terrain rule, footprint, colour, cargo in/out), `Industries` (list, by-tile layer, monthly update), placement at generation (resource noise fields, footprints levelled with `terrain::edit`), and stock/acceptance. Static boxes for rendering; the industry window (production, stock, rating, transported %).
4. **Vehicle catalogue.** A `const` model table per mode (price, speed, capacity, class, running cost); the build-vehicle window; vehicles take their stats from their model. Boats and planes use the same table (see below).
5. **Town demands** (§5.5) and the power-station bonus.
6. **Industry life** (§5.2 closures, §5.4 spawning, fund-new-industry) and goals (§9).
7. **Breakdowns, depots, eras, replace** (§7.5), as optional polish.

### Data shapes (sketch)

```rust
enum Cargo { Passengers, Mail, Coal, IronOre, Grain, Oil, Fuel, Timber, Steel, Planks, Food, Goods }
enum CargoClass { People, Bulk, General, Fresh }       // what a vehicle can carry

struct IndustryKind { name, footprint: (u8, u8), inputs: &[(Cargo, u16)], outputs: &[(Cargo, u16)],
                      terrain: Rule, spacing: u16, colour: [u8; 3] }
struct Industry { id, kind, tiles: Rect, stock: [u32; N], produced_last_month: u32,
                  transported_last_month: u32, production: u32, rating_low_months: u16 }

struct Model { name, mode: Mode, price: u32, running: u32, speed: f32, capacity: u16,
               class: CargoClass, tier: u8 }
const MODELS: &[Model] = &[ /* tables in §7 */ ];

struct Company { cash: i64, loan: i64, year_ledger: Ledger }
```

### Web performance budget

The web build is a hard constraint (DESIGN.md §9.1), so the economy is designed around it:

- **Industries are few** (about 40–80), static, drawn as a few boxes each in the static box buffer. No per-frame cost.
- **The economy runs monthly**: one pass over industries and stations every 30 game seconds, well under a millisecond. Vehicles touch it only on arrival.
- **Catchment lookups are cached.** A station records which industries and buildings it covers when it's built or changes, not every tick (`population_near` is the pattern, but cached).
- **No new per-frame loops over the map.** Resource noise fields are evaluated only at generation; spawning checks a handful of candidate sites per month.
- **Vehicle counts stay bounded** (the existing cap of 900 road vehicles, small fleets for boats, planes and trains), and everything that moves is already drawn through the dynamic box buffer.
- **Pathfinding is budgeted** (boats already do this: a fixed number of A* expansions per frame, with cached routes), and the same pattern applies to anything new that searches.

## 11. Open questions

1. **Eras or all models from day one?** All at once keeps the first session simple; eras give a long game a reason to keep buying. Proposed: all at once now, eras later.
2. **Distance: straight-line or route length?** Straight-line stops players padding routes but rewards the shortest track; route length rewards winding lines. Proposed: straight-line.
3. **Competing companies.** The market-share split (§5.2) would let AI companies exist; do we want them, or is the split just a rule between a player's own stations?
4. **Mail** might be cut if passengers alone give enough for aircraft to do.
5. **Scale of the numbers.** Everything in §3–8 should be tuned against the headless economy test in §8 once money exists, not by hand.
