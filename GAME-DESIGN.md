# Starcity — Game design

> The whole design as we understand it today: what the game is, what already exists, and what is still to build. Companion to [DESIGN.md](DESIGN.md) (engine and rendering). Numbers are first-pass dials, not promises. Items marked ✅ are implemented; everything else is planned. §11 is the implementation plan and the next steps. Last revised 2026-10-12.

**The pitch.** An OpenTTD-style transport tycoon on 3D voxel terrain, played through fifty years of the future. You start in **2000**, as the web takes off, moving coal, people and goods by road, rail, sea and air. Around 2025 the world starts to turn to chrome; **data centres** and then **AI campuses** swallow the grid; from about 2030 **spaceports** open the moon and later the asteroids, whose dust and metals feed new industries on the ground. By **2050** the technological singularity arrives: whole cities of chrome orbs, rockets at every coast, and a final megaproject to finish. The score is not money but **compute**, a meter on screen that grows from thousands to numbers with no names (§5.9). The moon is playable from about 2038, Mars by the late 2040s, and the singularity opens the solar system and then the galaxy (§10.10–10.12).

**The twist.** Where OpenTTD's late game repeats its early game, Starcity's changes verbs: you start as a **builder**, become a **manager** as compute buys you agents and forecasts, and end as a **governor** who writes the charter an automated civilisation follows, before the singularity reads what you made (§1.1). The map is the scoreboard: the same world ends as silent chrome orbs or as busy plazas, depending on what you told it to value.

**How the document is organised.** §1 is the player's role and the twist. §2–9 are the economy (money, cargo, industries, vehicles, goals). §10 is the arc of the fifty years: eras, chrome, power and data centres, space, the singularity and the moon beyond. §11 is the plan. §12 lists open questions.

## Status: what exists today

| Area | State |
|---|---|
| World | ✅ Voxel terrain with half-block slopes, water, sea level, trees; map generator; camera (orbit/pan/zoom), picking, voxel-aware rendering; native and WebAssembly (GitHub Pages). |
| Networks | ✅ Roads and rails on shared bit layers, curves and diagonals, junctions, path signals, stations, crossing rules. Building on bad slopes **levels the terrain for a fee** (`terrain::edit`, up to 3 blocks of cut/fill). |
| Towns | ✅ Generated towns that grow with service and **change with the years** (§10.7): buildings keep their build year, are retired and rebuilt, and merge into megastructures. |
| Vehicles | ✅ Trains (with signals and stations), road vehicles (queueing, 900 cap), **boats** (budgeted A* over a sea grid, cached routes, ports) and **planes** (airports, taxi/take-off/cruise/landing, shared orders). One model per mode so far. |
| Demo mode | ✅ `STARCITY_DEMO`/default: scripted rail lines, road traffic, ferry and flight lines between big towns, all back and forth. |
| Time | ✅ Calendar (a day a second), independent day/night light, speeds 1×–8× and pause, date slider (2000–2050) in the debug window. |
| Eras and chrome | ✅ `era.rs` (chrome 0 at 2025 → 1 at 2050), chrome roads and track in the terrain shader, metallic/emissive models, evolving towns. |
| Assets | ✅ glTF pipeline: 60 placeholder `.glb` models embedded at build time, artist guide in `assets/models/README.md`, tests for size, triangle budget and year coverage. |
| Economy | ❌ No money, cargo, industries, vehicle catalogue, goals or events yet. Costs are displayed but not charged; passengers are the only cargo and aren't paid for. |
| Power, data centres, space | ❌ Design only (§5.7, §5.8, §10.4). |
| UI by era | ❌ Win95 theme only (§10.3). |

Known limits: orbs can't cover roads; models' textures and animations aren't read yet; player vehicles keep their purchase-year model; the web build hasn't been profiled in a real browser (3.79 MB compressed).

## 1. What the player is doing

You run a transport company. Cargo appears at industries and people appear in towns; you build the infrastructure and buy the vehicles to move them; deliveries pay; the money buys more network. The interesting decisions, in the order a player meets them:

1. **What to carry.** A coal mine is one-way and bulky; passengers are two-way; goods are the best-paid cargo but need two supplied industries before they exist.
2. **How to carry it.** Road is cheap and slow, rail is the workhorse, boats suit coasts and oil, planes cross anything but need big airports. Each has cheap and expensive models.
3. **Where to build.** Terrain costs money (levelling, §3 of DESIGN.md), distance pays, and industries are scattered so that the best routes are never obvious.
4. **How to keep it running.** Service quality drives how much cargo industries give you, and starved industries close.

The tone to aim for is "one more line": simple rules that interlock, never a spreadsheet. If a rule can't be explained in a tooltip, it's too complicated.

### Design rules

- **Few cargo types, each with a clear role.** Twelve cargoes at the start, in four classes, with era cargo joining later (§4).
- **Two-input recipes, not deep trees.** Chains are at most three steps (mine → mill → factory → town).
- **No hidden state.** Every number the economy uses is shown in a window: production, stock, rating, income per trip.
- **Cheap to simulate.** The whole economy ticks once per economy month (30 simulated seconds); vehicles only touch it when they load or unload (§11).

### 1.1 The twist: builder, then manager, then governor

OpenTTD is a game about laying track and watching numbers go up, and its late game is usually the same verbs as its early game, only more of them. Starcity's twist is that **the verbs change as the fifty years go by**, and the thing that changes them is compute (§5.8). The player starts as a builder and ends as something stranger.

1. **Builder (2000–2015).** Hand-built, classic: lay track, place stations, buy a train, watch the first coin come in. Nothing is automated. Compute is a toy.
2. **Manager (2015–2030).** The network is too big to hold in your head, and compute can buy help: forecasts, optimisers and the first **agents** that run a line for you (§5.8). You stop placing every signal and start deciding where the company should go next, and what to spend compute on.
3. **Governor (2030–2050).** Agents build and rebuild on their own. The player stops giving orders and writes **charters**: standing priorities that the agents follow (§1.2). The late game is not "more of the same"; it is shaping what an automated civilisation does with its power. Then the singularity arrives and asks what you made.

Three systems carry the twist. They are built on things that already exist (the date slider, the demo builder, the evolving towns), which is why they are cheap to try.

**Forecast, the compute-powered ghost.** The game can already seek time (the date slider, `seek_to`). With compute, the player gets **Forecast**: clone the company's state, run it forward headless a few years at high speed, and show the *ghost*: projected income, which lines saturate, which town will outgrow its station, where the brownout will land. The map shows the ghost lines faintly over the real ones. Better compute looks further (1 year in 2005, 5 years by 2030, "the whole run" in the 2040s). It turns a late-game spreadsheet into a visual, and it makes experiments fun: *what if I build this airport?* costs nothing but compute, and the answer is a picture. Late (2040+), **Revisions** let the player rewind to a saved moment a handful of times per game and take another path, paid for in a large burst of compute. Time itself becomes a resource, which suits a game about the singularity.

**Charters: the governor's verbs (§1.2).** A policy sheet with a few sliders that agents obey.

**The Rival (parked).** *Decision: on the back burner until the core gameplay is solid; nothing else depends on it.* The demo mode's scripted builder (`scenario.rs`) already knows how to plan rail lines, pick big towns and run services. Run it, tuned and budgeted, as an **autonomous competitor** on the same map from the start, a slower, dumber one in 2000 and a smart one by the 2030s (it too gets compute and agents). It competes for the same industries, spaceports and power, and in the late game it is the other intelligence you are racing to the singularity. Players can ally with it (merge and share a grid) or buy it out. This also solves "an empty map in a sandbox": the world is alive when you arrive, and the demo mode is no longer a separate thing but the rival with the player's hand off the controls.

### 1.2 Charters and the two ledgers

As agents take over, every automated decision (build this line, close that street, redevelop that block, route power here) follows the company's **charter**: four sliders that always add up to 100%.

| Priority | Agents favour | Cost of ignoring it |
|---|---|---|
| **Profit** | High-yield routes, big towns, dense industry | Poor towns are left without service |
| **People** | Service to every town, housing and green space, short commutes | Lower income per vehicle |
| **Planet** | Clean power, fewer emissions, preserved land and coasts | Higher construction cost; slower growth |
| **Reach** | Space, the moon, the Convergence Array | Neglects the ground; brownouts |

There are two ledgers, both visible all game: **Money** (the familiar company value) and **Wellbeing** (a 0–100 score for the people: how many towns are served, how long their journeys are, power reliability, air, green space). Early on they rise together. From the 2030s they begin to pull apart, since a ground full of chrome orbs and a pure compute economy makes a great deal of money for a city of no people.

**The ending depends on what you built, not on a button.** At the end of 2050 the singularity arrives and the game reads the two ledgers and the charter:

| Ending | What it looks like |
|---|---|
| **Ascension** | Money and Reach maximal, Wellbeing low: whole cities of orbs, empty streets, the network running itself in perfect silence, with rockets leaving every hour. Beautiful and a little frightening. |
| **Commons** | Balanced: crowded plazas under the chrome, towns that kept their roads, power that doesn't run out. |
| **Stagnation** | The Convergence Array unbuilt or the grid collapsed: the future arrives late, and the world is left half-chrome. |

The world is the scoreboard: the same map shows the ending, because it has been rebuilt by whatever the agents were told to value. This costs almost nothing to render (we already have towns that rebuild themselves and a chrome shader) and gives every player a different late game, which is exactly what makes a long game worth playing twice.

*Why this is fun and not just an idea.* The decisions get *more* interesting as the game goes on, because they get higher-level, not more numerous. The danger and the joy of automation is a story the genre has barely told. And all three twists are small to build on top of what exists: Forecast is a clone-and-run, the Rival is the demo builder with a budget, charters are four numbers read where costs are already computed.

## 2. Time and money

- **Calendar** ✅ (`clock.rs`): the real Gregorian calendar, starting Saturday 1 January 2000. **One calendar day is one second of simulation**, so at normal speed a month is about 30 s and a year about 6 minutes, and the game runs 2000 to 2050 in about **5 hours at 1×** (about 76 minutes at 4×, 38 at 8×). The speed buttons (pause, 1×, 2×, 4×, 8×) multiply *everything*: the calendar, vehicles, towns, the light.
- **Time of day is separate from the calendar.** The light (DESIGN.md §7b) cycles once every 120 seconds of simulation, whatever the date: slow enough to look good, and independent of the calendar so the calendar can run at the pace a 50-year game needs. The toolbar shows the date with a sun or moon beside it for the time of day, next to the speed buttons.
- **The economy uses the same time.** *A tick is 1 simulated second, an economy month is 30 s and an economy year is 360 s*, close to the calendar's own month and year, so "monthly" and "yearly" in this document line up with what the date shows. Anything below given "per year", "per month" or in "days" means these units. Production, finances and industry changes tick monthly; vehicles are real-time.
- **Starting position:** £100,000 cash, a loan limit of £200,000 (borrow and repay in £10,000 steps), 5% a year interest on the loan, charged monthly.
- **Income:** paid on delivery, to the company, when cargo is unloaded at a station that accepts it. See §3.
- **Expenses:**
  - *Construction:* priced per piece (`network::PIECE_COST`, now £100; proposed £40 per road piece and £80 per rail piece, since a route between two towns is 100+ tiles), plus levelling at `TERRAFORM_COST` per corner-block, plus stations and terminals (§8).
  - *Vehicle purchase:* the model's price (§7).
  - *Running costs:* each vehicle pays a yearly amount, charged monthly, whether or not it's earning. About 8–12% of its price, more for faster models. This is what makes a badly used vehicle a loss, not just a non-profit.
  - *Infrastructure upkeep:* £2 per track or road piece per year, £200 per station, £400 per port, £1,500 per airport. Small, but it stops the player laying a thousand tiles to nowhere.
  - *Loan interest.*
- **Going broke:** cash below zero for three consecutive months shows a warning; below zero for twelve months ends the game ("bankrupt"). Selling a vehicle returns 60% of its value, less with age.
- **Company value** = cash + vehicles (at depreciated value) + infrastructure (at 50% of cost) − loan. It is the early score and the basis of the first goals (§9); from the AI era the score is **compute** (§5.9), and money becomes what you spend to get it. Money stays meaningful all game; it just stops being the point.

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

Twelve starting cargoes in four classes. A vehicle model carries one class (some only one cargo), so class decides which wagons, trucks and ships work (§7).

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

**Era cargo** (appear with their industries, §5.7–5.8 and §10.4; the first four sit in the General class, Hydrogen with Bulk):

| Cargo | Rate | Fresh / stale | From → to | Arrives |
|---|---|---|---|---|
| Silicon | 3.6 | 100 / 150 | Silica quarry → chip fab | 2000 |
| Chips | 7.2 | 40 / 90 | Chip fab → data centres, factories | 2000 |
| Containers | 5.0 | 80 / 120 | Container port → anywhere (replaces Goods on long hauls) | 2000 |
| Parcels | 6.0 | 15 / 40 | Logistics hub → towns | 2012 |
| Lithium / Batteries | 4.0 / 6.0 | 100 / 150 | Lithium mine → battery gigafactory → vehicle makers | 2016 |
| Hydrogen | 4.8 | 60 / 100 | Hydrogen plant → rockets, power | 2022 |
| Regolith | 2.6 | 200 / 300 | Lunar base → regolith works (§10.4) | 2030 |
| Lunar glass | 6.8 | 100 / 150 | Regolith works → chrome building, spaceports | 2032 |
| Helium-3 | 8.0 | 80 / 120 | Lunar base → fusion plant | 2034 |
| Rare metals | 9.0 | 100 / 150 | Asteroid mining → nano-fabricator | 2040 |
| Water ice | 3.0 | 150 / 200 | Asteroid mining → water works, rocket fuel | 2040 |
| Nano-goods | 12.0 | 30 / 70 | Nano-fabricator → big towns | 2040 |

*Power* and *Compute* are not cargo: they are services measured in MW and compute-units (§5.7, §5.8).

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

### 5.7 Power

Electricity is a **service, not cargo**: nothing is hauled, but it limits what you can build. It is the part of the game that grows from a footnote in 2000 to the main constraint by 2040.

- **Sources** produce megawatts (MW): *Power station* (coal or fuel delivered by the player, 100 MW, runs only while it has stock), *Wind farm* (2005, 30 MW, free but variable), *Solar farm* (2012, 40 MW, daytime only: the day/night light already exists to drive this), *Hydrogen plant* (2022, 80 MW), *Fusion plant* (2035, 400 MW, needs Helium-3, §10.4), *Solar array* (space-made units, 2040, 600 MW).
- **Consumers** draw MW: towns a little (more per person), steel mills 20, data centres 5–400 (§5.8), spaceports 40–150 (§10.4), chip fabs 30.
- **Grid.** Version one pools every source and consumer within **30 tiles** of each other (the range is drawn as a faint circle when you select a source). Version two lets you build **pylon lines** as another network layer (like roads and rails) when a site is remote. A grid's supply is shared in proportion to demand; a short grid **browns out**: its industries make less in proportion, and nearby towns grow more slowly. The industry window shows "Power 62%".
- **Cost on the web:** one number per source and consumer, a grid recomputed only when something is built or monthly, no per-frame work.

### 5.8 Data centres and compute

The thread that links the 2000s to the singularity: where the internet lives.

```
Silica quarry ──► Chip fab ◄── power ──► Data centre / AI campus ──► Compute
   (silicon)       (chips)                      ▲   (goods or chips + power)
                                    Power ──────┘
```

| Industry | Years | Takes | Makes | Notes |
|---|---|---|---|---|
| Silica quarry | 2000 | | Silicon | Sand and rock; a plain primary |
| Chip fab | 2000 | Silicon **and** power (30 MW) | Chips | 4×4, needs flat ground and clean water within 20 tiles |
| Server farm | 2000–2015 | Goods (servers), 5 MW | Compute | Small, 3×3, near towns |
| Data centre | 2005+ | Goods or **Chips**, 20–80 MW | Compute | Cloud era: 4×4, chips triple the compute |
| AI campus | 2020+ | **Chips**, 150–400 MW | Compute ×20 | 6×6 hypercampus; first appears in the 2020s; the largest consumer of power in the game |

**Compute is the ultimate resource.** It is not hauled and it isn't money. It is made in **compute-units per month (CU)** by data centres, kept in a company-wide pool (not stockpiled for long: unspent CU decays 20% a month, so you use it or lose it), and **spent by allocating it to programs**. A program is a standing instruction to the company's software; you give it a share of your CU with a slider in the **Compute window**, and it applies its effect to the whole network until you take it away. The same pool pays for everything, so the question is always *what is my compute for?*

How it becomes extremely powerful: programs unlock with the era (they are models like any other: a `from` year, shifted by the Tech Pace), and their strength **rises faster than their cost**, from an early curiosity to the thing that runs the company.

| Program | Years | What it does | Early strength → late strength |
|---|---|---|---|
| **Hosting** | 2000+ | Sells spare compute: £ per CU per month, paid directly | £40 per CU in 2000 → £1,500 in 2026 (the boom) → £600 after the correction (2029) |
| **Route analytics** | 2003+ | Station ratings and `time_factor` slightly up where service is good | +3% → +12% (2025) |
| **Forecasting** | 2008+ | Warns of next year's production and growth; from 2015 the **Forecast ghost** (§1.1) runs the company forward and draws projected lines and income; how far it looks grows with the era | 1 year → the whole run; −30% industry build costs |
| **Fleet optimiser** | 2015+ | Reduces vehicle running cost and queueing (signal/junction timing) | −5% → −25% |
| **Autonomy** | 2024+ | Unlocks autonomous vehicles (they need compute in proportion to their number); autonomous fleets cost less to run | each autonomous vehicle draws 0.05 CU |
| **Generative design** | 2026+ | Plans and prices terrain levelling and routes: cheaper construction, auto-proposed lines (the planner assistant) | −10% → −50% on levelling and track |
| **Agents** | 2028+ | A manager agent for a line, a town or an industry: it buys and replaces vehicles, adds feeder lines, rebalances orders, within a budget you set. The compute cost scales with the number of agents | automates the late-game micromanagement so a 500-vehicle company is playable |
| **Research** | 2030+ | Each full bar of CU spent on research pulls the introduction year of every model and industry forward (the **Tech Pace** meter) | up to four years by 2050 |
| **Synthesis** | 2038+ | Converts CU + power into scarce things directly: Nano-goods, Rare-metal substitutes, Hydrogen; a "print it instead of hauling it" option | a dial between hauling and computing |
| **Convergence** | 2046+ | The compute commitment the Convergence Array needs (§10.5) | the end goal |

**The shape of it.** Until about 2020 a few CU buy modest conveniences: it's hosting income and small percentages, a small data centre is an optional extra. From 2020, each tier of campus multiplies output ×20, and programs scale with the **size of the company** (an optimiser saves 25% of *all* running costs, an agent manages *every* line), so the value of a CU is proportional to what it is applied to, which keeps growing. In 2030 a player with ten campuses and a hundred vehicles runs a company that is partly self-managing; in 2045 compute is what separates a player who is doing things by hand from one who is directing. At the same time its cost is **power, chips and heat**, which are all earned the hard way (§5.7), so compute is the one resource with no ceiling except the other three.

**Why it isn't a runaway.** Programs cost CU *per month*, rising with the fleet and the map; a naive player who runs everything finds their CU doesn't stretch; a power shortage (brownout) cuts compute in proportion; and each program's effect has diminishing returns above a certain share (a 100% allocation to one program gives about 60% more than a 30% one). So there's a real allocation question every era, and what is best changes with the year.

The tension for the player (about compute): an AI campus draws as much power as a city and wants a steady stream of chips, but it makes money, speeds the tech clock and brings a boom. Build too many for the grid and everything browns out. Build none and the future arrives later.

**The AI boom** is also a world event (§10.9): from about 2024 hosting rates triple and chip demand jumps, with the first AI campuses; around 2029 a **correction** cuts hosting rates 40% and some campuses close, but the compute dividend stays. A player who over-built in the boom feels it; a patient one doesn't.

### 5.9 Compute through the eras: the meter and the scale

**Compute is the score.** Money buys things; compute is what you are *for*. A large counter sits at the top of the screen next to the clock, always visible: the **Compute meter**, showing the pool, the rate per second and a tiny sparkline on a log axis (a straight line means exponential growth). Like the best idle games, it should feel good to watch: the digits roll, the rate ticks up with every campus you energise, thresholds fire a short banner ("**1 PetaCU**: the first model trains"), and from the AI boom the number grows so fast the unit changes under you (kCU, MCU, GCU, TCU, PCU, ECU... then names we make up: *Brim*, *Halo*, *Sol*).

**The problem to solve:** in 2000 compute is a dull utility; in 2026 a large share of it is nearly general-purpose labour; by 2050 it is the thing the world is made of. One resource has to scale across eleven orders of magnitude and change in *meaning* along the way. The answer is to change what compute **does** at each stage, and what it is **bottlenecked by**, while keeping the meter and the allocation sliders the same.

| Era | What compute is | What you spend it on | Rate (CU/s) | Bottleneck |
|---|---|---|---|---|
| **Servers** 2000–2012 | Utility. Information services: deterministic, linear. | Hosting income, route analytics, maps. A small convenience. | 10⁰–10³ | Money and Goods |
| **Cloud** 2012–2022 | Software that scales with your operation. | Forecasting, the fleet optimiser, early autonomy. Value ∝ fleet size. | 10³–10⁶ | Chips and Power |
| **Models** 2022–2030 | **Two kinds appear.** *Training* (a cumulative total that unlocks capability tiers, permanent) and *Inference* (a monthly flow that runs agents). The AI boom and the correction. | Agents, generative design, autonomous fleets. Capability tiers jump at thresholds: each tier unlocks a program. | 10⁶–10⁹ | Grid power |
| **Acceleration** 2030–2040 | Compute designs better compute: chips, power, materials, cities. The growth rate itself grows. | Research (the Tech Pace), synthesis, charters (§1.2), automated cities. | 10⁹–10¹³ | Land, heat, rare metals (space) |
| **Singularity** 2040–2050 | The planet's economy. Orbital and lunar compute. | Everything: it is the currency (below). The Convergence Array. | 10¹³–10¹⁸ | Energy and matter |
| **Expansion** 2050+ | Stellar. | Probes, colonies, the galaxy (§10.11). | 10¹⁸+ | The sun |

**How the number grows (the cookie-clicker engine).** Monthly compute is, in one line:

`CU = Σ sites ( chips × efficiency(year) × power_ratio × campus_scale ) × (1 + self_improvement)`

- *efficiency(year)* doubles about every 2.5 years (the chip generations), so an old campus quietly gets better at no cost;
- *campus_scale* is ×1 (server farm), ×5 (data centre), ×20 (AI campus), ×200 (gigacampus, 2032+), ×10⁴ (orbital or lunar compute, 2040+): building the next tier is the "buy the next upgrade" step;
- *self_improvement* is zero until the Models era, then proportional to the Training total: compute improves the chips that make compute. This is the one super-exponential term and the heart of the singularity, kept deliberately small until 2028 so the game doesn't explode before the AI boom;
- *power_ratio* (§5.7) caps it. A short grid is the main way a clever player's number stops growing, which is what keeps the late game a *logistics* game: the way to more compute is more power, more chips, more land and more matter, brought in by trains, ships and rockets.

The shape is: slow and legible for the first twenty years, a visible kink at the boom (2024–28), a flat spell at the correction (2029, the *rate* dips though the total keeps rising, as with the efficiency gains that rescued real AI after similar scares), then steeper than ever.

**When compute becomes the currency.** Around 2035 the economy re-prices itself in compute: construction, vehicles and services are quoted in CU as well as £, and Synthesis (§5.8) lets compute pay for things directly. £ doesn't vanish; it becomes the unit of the old world, and the Finance window shows both. Mechanically this just means the cost tables have a CU column that starts at zero and becomes the main price; narratively it's the moment the player notices that the game has changed what it is about.

**Training tiers** (capability thresholds on the cumulative total; names are placeholders). Each unlocks a program (§5.8) and a line in the news ticker.

| Tier | Training total | Unlocks |
|---|---|---|
| 0 Index | 10³ | Hosting, route analytics |
| 1 Forecast | 10⁶ | The Forecast ghost (a year ahead) |
| 2 Assist | 10⁹ | Fleet optimiser, generative design |
| 3 Agent | 10¹² | Agents (a line at a time), autonomous fleets |
| 4 Manager | 10¹⁵ | Agents that run a whole town or industry; charters (§1.2) |
| 5 Architect | 10¹⁸ | Automated city design and construction; synthesis |
| 6 Convergence | 10²¹ | The Convergence Array (§10.5) |

### 5.10 Cities that grow very big

A town can grow far past today's few hundred people if the player works at it. This is where the economy and the map meet: a city is the sum of everything you bring to it.

**Sizes.** village (< 300) → town (300) → city (2,000) → metropolis (20,000) → megacity (200,000) → **arcology city** (2 million+, orbs). The size shows on the label and unlocks things: a metropolis can host an airport hub and an AI campus, a megacity a spaceport link, an arcology city a vertical farm district and a hyper-expressway interchange.

**What makes a town grow.** A **Prosperity** score, recomputed monthly, drives growth speed (up to ×5, then ×10 for powerhouses). It sums, with diminishing returns per source:
- *Connections:* distinct lines, weighted by the size of the places they link (a link to another metropolis counts for more than ten to hamlets);
- *Service:* passengers carried and station ratings (as now);
- *Supply:* food, goods, fuel, parcels, and later Chips and Nano-goods delivered. The valuable cargo you deliver counts by its **value**, so a city that receives Nano-goods grows like a boom town;
- *Local industry:* output of industries within 30 tiles. Clusters (a steel mill, a factory and a chip fab together) multiply: each type present adds 10%;
- *Compute:* a data centre or AI campus nearby (jobs, §5.8); power reliability.

A town that is deliberately fed becomes an **economic powerhouse** (Prosperity high for a year): it pays more per delivery (+15%), spawns new industries nearby, and gets its own events ("**Ashford is now a megacity**"). The player doesn't tell a town to grow; they *build a reason* for it to, which is the same fantasy as OpenTTD, scaled up.

**How cities physically get big.** Three things, each also a task for the engine (not yet built):
1. **Taller:** the floor limit rises with the era (14 today; 40 by 2035; 90 by 2050), and people per floor rise too (8 → 30), so the same footprint holds several times more people. Merging into orbs (§10.7) is how this looks.
2. **Wider:** towns *extend their streets*: when a town's buildings fill its blocks it adds a ring road or a new district (the town generator already knows how to lay a grid; it needs to run again as the city grows). Roads that already exist stay; new ones are cheap for the town to build.
3. **Denser infrastructure:** big cities want elevated lanes, hyper-expressway interchanges and vertiports on rooftops (§7.6). They need those to keep growing, which gives the player something to build for a city that has outgrown its roads.

A well-fed city is the thing to stare at: a few thousand people in 2000, and, if you've worked at it, a 200,000-person forest of towers and orbs at night by 2040.

## 6. Passengers and mail

Passengers behave as now (buildings near a station trickle people, trains unload everyone at each stop, §5 of DESIGN.md) with two changes:

- **Destinations are implied**: a load of passengers is paid on distance from where they boarded, as in §3, so long routes between big towns pay more, which is the reason to link cities, not hamlets.
- **Mail** is generated in parallel at a fifth of the rate, and carried by mail wagons/vans and aircraft holds. A mail van is the cheapest way to get a small town paying its way.

## 7. Vehicle models

Every mode has two or three models. Cheaper ones are slower, smaller and cost less to run; dearer ones are the opposite. The aim is a real choice, not a ladder you just climb:

- **Cheap and small** wins when the cargo is thin (a village, a small farm), when the line is short, or when you're nearly broke.
- **Big and fast** wins on long routes and busy ones: the `time_factor` (§3) and capacity both scale in its favour, but its price and running costs are paid even when it runs half empty.
- **Break-even guidance** is in each table: roughly, the dearer model should earn 2–3× more per year on a long, busy route and *less* than the cheap one on a short, thin one.

All numbers are per vehicle. Speeds are in tiles per second (the unit the code already uses; trains today run at 3.0, road vehicles 1.1–1.6). Running cost is per year. Models appear by year, in eras (§10); the tables below are the models of the opening era plus the tier each later generation slots into.

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

### 7.6 Late-game transport: flying cars, hyper expressways, ultra-fast rail

From about 2030 the new vehicles are not just faster models of the old ones: they come with new kinds of **infrastructure**, so there is always something new to build. This is how the player "dominates the globe" with the network.

| Mode | Years | Infrastructure | Speed (tiles/s) | Notes |
|---|---|---|---|---|
| **Flying cars** (eVTOL air taxis, then personal pods) | 2028+ | **Vertiports** (a pad, £4,000, on the ground or on a rooftop), and **skylanes** (corridors you draw between two points) | 6–9 | No track. Capacity of a skylane is limited by how much compute you give **air-traffic control** (§5.8): more compute, more cars in the air, safely. They carry few people but fast, which suits the time-factor on passengers |
| **Hyper expressway** | 2035+ | An **elevated, multi-lane autonomous road** (a new network layer, drawn on poles over terrain and over ordinary roads), junction interchanges, power at each node | 10–14 | £400 a piece, expensive but a straight line that ignores slopes (no levelling), and it can cross town, rail and road. Autonomous pods and convoys only. Its capacity is vast, so it is the thing that lets a city of a million exist |
| **Maglev** | 2030+ | Maglev track (a rail variant) | 8–12 | Replaces the Hawk-class electrics on the busiest lines |
| **Hyperloop / vactrain** | 2040+ | Evacuated tube track, an elevated or buried layer | 20–40 | Ultra-high-speed rail that makes 300-tile routes take seconds of game time, and trans-region lines possible (§10.12) |
| **Orbital and lunar links** | 2040+ | Spaceports, then a space elevator (2046+) | n/a | §10.4, §10.10 |

The design point is that each new mode is the answer to a problem the previous one now has: roads jam, so you build rail; rail doesn't cross seas, so you fly; cities outgrow their roads, so you lift traffic above them; continents are far apart, so you build the vacuum. Everything draws power (§5.7), which ties this back to the grid and to compute.

**Engine notes.** The hyper expressway is a third network layer beside roads and rails, drawn at a fixed height above the ground (the renderer already draws elevated pieces as separate models). Flying cars and skylanes reuse the `planes.rs` waypoint machinery at low altitude with no airport (vertiports are tiny terminals); only the capacity rule is new. Neither needs terrain levelling.

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

- **The big goal is compute** (§5.9): the total produced, the current rate, and the highest Training tier reached. Each tier is a milestone (a banner, an unlock, a line in the news). It's what the game is about from the 2020s; before that, money is the goal and compute is a toy.
- **Yearly goals** shown in a window, three at a time, rotating: "Carry 5,000 passengers this year", "Deliver 1,000 t of coal", "Connect two towns of 1,000+", "Reach company value £250,000". Completing one pays a small bonus (£5,000) and unlocks the next tier. They're suggestions, never forced.
- **Company rating** (0–100%) from vehicle age, profit and goals met; a sensible scoreboard line.
- **Scenario modes** later: "Reach £1M", "Supply every town with goods", on a fixed seed.

## 10. Eras: from the web to the singularity

The game starts on **1 January 2000**, as the web is taking off, and runs to **2050**, when technology reaches the singularity. The calendar is the story: models, buildings, industries, the interface and the look of the world all change with the year, so a long game feels like moving through a future, not just buying bigger vehicles. The 2000 start also suits the Win95-style UI, which reads as the early-2000s desktop it's imitating.

### 10.1 The arc

| Era | Years | Feel | What arrives |
|---|---|---|---|
| **Dot-com** | 2000–2012 | Grounded and plain. Win2000 grey UI, boxy vehicles. | The §7 models as they are: diesel trains, buses, turboprops and jets, ferries. Mail still matters. **Server farms and the first data centres** (§5.8). |
| **Broadband** | 2012–2025 | Cleaner and flatter. Glossy buttons give way to flat ones. | Electric trains and buses, hybrid trucks, larger jets, container-style ships. Cloud data centres, solar and wind, lithium. **The first AI campuses (2020+).** |
| **Chrome dawn** | 2025–2038 | The first metal appears: vehicles and tall buildings gain a sheen. | Maglev trains, autonomous trucks, electric and VTOL aircraft, hydrofoil ferries. **The AI boom (2024–29)**. Industries get automated. **Spaceports (2027), orbital lifters (2030), the first moon cargo (2031) and regolith industries**, fusion plants (2035). |
| **Chrome age** | 2038–2048 | Everything reflective: towers of mirrored glass, polished vehicles, a chrome UI. | Hyperloop pods, autonomous convoys, fusion power. **Asteroid mining (2040)**, nano-fabricators, space-made solar arrays, lunar cities in outline (§10.4). |
| **Singularity** | 2048–2050 | Gleaming, strange, accelerating. | The final megaproject (§10.5). Production and growth curves steepen. |

Dates are the *centre* of each introduction; each game shifts every model by a random ±3 years, so no two games unlock in the same order and the player can't plan from a memorised list. A model shows as "Coming around 2031" in the buy window, so there's something to look forward to.

### 10.2 Technology and models

Models (§7) gain an **introduction year** and a **retirement year**. A new generation doesn't replace the old: it joins it, with its own price and its own place on the cheap-to-expensive scale (§7). That keeps §7's rule that the cheap option can still be right. Old models can still be bought until they retire; vehicles already running keep running, with higher running costs as they age.

Technology also moves the world: a new model can need new infrastructure (maglev needs maglev track, electric trains need overhead wire, spaceports need a launch complex), and costs fall about 3% a year for established kit, so mid-game purchases get cheaper while new models start dear.

Keying this to the calendar is deliberate. The pace problem it creates is in §12.

### 10.3 Chrome: the look of the future

The world turns chrome gradually: a little from 2025, extremely by 2050. It's built (✅) from three parts, all driven by data an artist controls.

- **Models carry their own look** ✅. Props are glTF models (`assets/models/`, authored in Blender, see `assets/models/README.md`), and a material's standard glTF **metallic** value is what makes it chrome: the shader mirrors the sky gradient (horizon to zenith, ground below, a bright streak where the sun is) in proportion to *metallic*, softened by *roughness*; *emissive* materials glow (windows, lane lights, engine strips); materials named `Paint` take each vehicle's colour. Nothing about chrome is in code: a new generation is a new file whose materials are more metallic. Which generation is in use is by the years written into each file (§10.7).
- **Roads and track follow a single era value** ✅ (`era::chrome`: 0 until 2025, 1 by 2050): asphalt cools to a glassy steel-blue with white kerbs and glowing cyan lane lines, ballast becomes a pale guideway with glowing magnetic strips. One uniform and a few lines in the terrain shader.
- **The UI follows, delayed from the real world** (planned, ✅ for the first look): the windows themselves move through the desktops of the time, but about ten years late, as if the company's software upgraded when it could afford to. Layout never changes, only the paint, so muscle memory survives (`ui/win95.rs` centralises it).

  | Years | Look | Notes |
  |---|---|---|
  | **2000–2010** | **Win95 classic** ✅ | Grey bevels, navy title bars, 3D buttons. The start look |
  | **2010–2020** | **Glass** | Translucent, glossy frames with soft shadows and rounded corners; the Aero-and-iOS-6 mood. Gradient buttons |
  | **2020–2030** | **Sleek** | Flat, rounded, a soft material feel; large type, generous padding, subtle shadows (the current macOS style, Sonoma and later). A thin accent colour |
  | **2030–2040** | **Holo** *(new)* | Windows without frames: thin luminous edges, panels that float with a faint grid behind them, content in cool white and cyan. Controls light up near the cursor |
  | **2040–2048** | **Liquid chrome** *(new)* | Windows are slabs of polished metal that reflect the sky and the sunset, edges that ripple when you move them; the world's own chrome shader (§10.3) drawn in 2D |
  | **2048+** | **Ambient** *(new)* | The frames fade: panels dissolve into annotations on the map itself (labels, curves and numbers that sit on lines and buildings), with a few glass anchors for the rest. The interface becomes part of the world, which is what the singularity looks like |

  The Compute meter is always on top and is the first thing to change at each step.

  *Cost on the web:* real background blur is expensive in egui and on mobile GPUs, so Glass and Sleek fake it with gradients and a baked noise; Holo and Liquid chrome are single draw calls per panel with a cheap shader. An option in settings keeps "Classic" for anyone who prefers it. `win95.rs` becomes a `Theme` trait (`bevel`, `title_bar`, `button`, `panel`) with one struct per look, blended for 6 months when the year crosses a boundary.
- **Day/night** (DESIGN.md §7b) already shows it: chrome picks up the sunset, emissive strips glow at night, and the sky reflected in a chrome orb changes with the time of day. Later: lit windows on every era's buildings.
- **Cost on the web:** one extra uniform and a few shader lines; the model fragment shader is a gradient lookup, with no textures, no extra passes and no per-object work. Terrain, water and clouds are unchanged.

### 10.4 Space

Space is a second economy layered on the first, not a replacement for it. It begins as an expensive curiosity around 2027, loses money for years, and becomes the most profitable thing you can run by the 2040s as the technology matures. Its industries come back down to Earth and feed the chains of §5.

**Launch sites.** *Spaceports* are terminals like ports and airports (`terminals.rs`), but hungry:

| Site | Years | Price | Footprint | Needs |
|---|---|---|---|---|
| Sub-orbital pad | 2027+ | £250,000 | 8×8, levelled | 40 MW, 40 kl fuel per launch |
| Orbital spaceport | 2030+ | £600,000 | 12×12, levelled | 150 MW, 200 kl fuel per launch, a stock of Lunar glass for the pad (2032+) |
| Lunar-capable complex | 2032+ | £1,200,000 | 14×14 plus a gantry | 250 MW, Hydrogen or Fuel, Water ice (2040+ cheaper) |

Rules of the site, all visible in the spaceport window as red/green checks:

- **Power:** the grid in range must have enough spare MW (§5.7). A spaceport on a short grid launches slowly, or not at all. This is why the fusion plant matters.
- **Resources:** fuel (and Hydrogen later) must be delivered by the player. A launch takes its fuel from the pad's stock, so an empty pad idles. A refinery and tanker line to the spaceport is a real job.
- **Site:** a flat area away from towns: a 15-tile safety zone, within which towns lose growth while a pad is active; coastal sites launch over water and are the only choice for the bigger lifters (no pad inland after 2030 unless the zone is 25 tiles).
- **Noise and weather:** storms (§10.9) delay launches; launches visibly shake the nearest buildings' windows (cosmetic).

**Rockets** are vehicles (`rockets.rs`, modelled on `planes.rs`): they stand on a pad, lift off with a plume and climb out of sight, and return after a trip time with cargo exchanged. They cost £300,000–£1.5 million and carry 60–250 units.

| Rocket | Years | Price | Capacity | Notes |
|---|---|---|---|---|
| Hopper (sub-orbital) | 2027 | £300,000 | 60 units | Ballistic, back in minutes: tourists and fast parcels across the map |
| Orbital lifter | 2030 | £500,000 | 150 units | Orbit and stations |
| Lunar shuttle | 2031 | £900,000 | 120 units | Moon |
| Deep-space hauler | 2040 | £1,500,000 | 250 units | Asteroids; long trips (a game year or more) |

**Why spaceports start unprofitable.** Each launch has an *operating cost* (crew, range, refurbishment, the fuel you don't deliver) that falls as the technology improves, while the destinations pay a fixed "orbital distance" (§3):

| Destination | Orbital distance | Years |
|---|---|---|
| Orbital station | 400 tiles-equivalent | 2027 |
| Moon (lunar base) | 1,500 | 2031 |
| Asteroid belt | 6,000 | 2040 |

Launch operating cost per orbital launch: about **£120,000 in 2030**, falling in steps: booster reuse (~2033, −40%), rapid turnaround (~2037, −50%), orbital lifter infrastructure (~2044, −70%), reaching about **£8,000 by 2050**. A load of 150 Goods to orbit pays about £38,000: so in 2030 a launch loses money on ordinary cargo and only profits with the dear stuff (Chips, Nano-goods, people); by the late 2030s it breaks even on goods; by the 2040s it is the best-paid route in the game. Reliability improves the same way: **failure chance per launch** falls from 6% (2027) to 0.3% (2050); a failure destroys the cargo and the rocket and closes the pad for a month (a visible explosion, and a reason to insure). Early adopters pay to learn, and the Tech Pace meter (§5.8) speeds this curve.

**Off-map destinations** are virtual industries with no tile, always present once their era opens (their stock and production are a few numbers each):

- **Orbital station** (2027): accepts people, goods, food, fuel, chips; produces *Solar power units* (2040).
- **Lunar base** (2031): accepts goods, food, people, fuel, hydrogen; produces **Regolith** (2031), **Helium-3** (2034) and Lunar glass is made on Earth from regolith. The base grows over the years (its demand rises, a small model sits on the pad's skyline).
- **Asteroid mining** (2040): accepts fuel and goods; produces **Rare metals** and **Water ice**. Far, slow, rich.

**Space resources feed ground industries** (extra steps in the §5 chains, never a replacement, so a player who skips space still has a complete game):

| Industry | Years | Takes | Makes | Notes |
|---|---|---|---|---|
| Regolith works | 2032 | Regolith + power | **Lunar glass** (and Metals) | The moon-dust industry: lunar glass is the material of chrome buildings and the spaceport pad |
| Chrome foundry | 2034 | Lunar glass + Steel | Chrome plate | Towns supplied with chrome plate redevelop into orbs and towers faster (a growth bonus, §10.7) |
| Fusion plant | 2035 | Helium-3 | 400 MW | A power source with a big town-growth bonus |
| Nano-fabricator | 2040 | Rare metals + Goods | **Nano-goods** | The best-paid cargo; accepted by towns above 5,000 |
| Water works | 2040 | Water ice | Food and Hydrogen | Cheaper than farms near big towns |
| Solar array receiver | 2040 | Solar power units | 600 MW | Largest source in the game |

**The player's decision:** space is a long investment. A pad, a power source, a fuel supply and a rocket together cost millions before the first cargo lands; early on the money is better spent on the ground. But a player who starts early, learns the economics while the launch cost is still high, and builds the fuel and power they will need is rewarded when the curve turns, and the singularity (§10.5) needs the whole chain.

**Cost on the web:** a handful of rockets (a few boxes plus a short plume), no off-map world to simulate, and a few numbers per off-map destination. Launch plumes are particles on the dynamic buffer, and a bounded count.

### 10.5 The singularity

By 2048 a megaproject, the **Convergence Array**, becomes available to build: a large structure that needs enormous deliveries over a couple of game years (Rare metals, Nano-goods, Helium-3, Lunar glass, and a large power draw) and a **compute commitment** from the company's data centres and AI campuses (§5.8). Finishing it before the end of 2050 triggers the singularity, whose ending (Ascension, Commons or Stagnation) depends on the charter and the two ledgers (§1.2): the sky and UI complete their shift to full chrome, a short credits sequence plays, and the game offers to continue as a post-singularity sandbox (industries produce without inputs, towns grow without limit, the player keeps building), or, later, to carry on to the moon (§10.10). Missing it just continues the sandbox with a note. It's a goal (§9), never a gate.

### 10.6 Implementation notes

- `clock.rs` runs from 2000 and `era.rs` maps a year to the era name and the 0–1 chrome value ✅.
- Vehicle models for sale (§7) will take their introduction and retirement years from the model files too (`from` / `to`), plus a per-game random shift of ±3 years; availability is a pure function of the date.
- The era value is a uniform in `Globals` next to the light (`daylight::Light` is the model: a pure function of the date, applied in one place); `era.rs` holds the timeline.
- Space uses the existing terminal and orders machinery (`terminals.rs`): a `Site::Spaceport`, a `rockets.rs` modelled on `planes.rs` (a precomputed list of waypoints for lift-off and landing, plus a timed absence), and off-map destinations as entries in the industry table with no tile.
- Power is a `power.rs` module (sources, consumers, grids by radius, a monthly solve) used by industries, data centres and spaceports; compute and the Tech Pace meter live in the company state; both are pure data a test can drive.
- Introduction years are shifted by the Tech Pace as well as the per-game random: `available(model, year) = year >= model.from + shift - pace`.

### 10.7 Towns that change with the years ✅

Buildings keep the look of the year they were built in, so the old ones stay while new ones go up in the current style, and over time the old stock is replaced. The world is a record of its own history.

- **Every building has a build year.** The towns that exist at the start have grown over the 30 years before 2000, so their buildings span 1970–2000, with the oldest nearest the centre (concrete blocks, brick, flat roofs).
- **Models are chosen by year, floors and footprint**, from metadata in each file (`from`, `to`, `min_floors`, `max_floors`, `tiles_x`, `tiles_z`; see `assets/models/README.md`). New houses and towers use the current generation. If no model suits (an artist hasn't made a 2031 house yet), the nearest generation is used, so the game never has a hole in it.
- **Redevelopment:** roughly every building is rebuilt once in 20 years (a steady trickle: a town of 100 buildings has one replaced about every 73 seconds at 1×), but only if the replacement would look different, and only once it's 12 years old. A rebuilt building takes the style of that year.
- **Merging into megastructures:** once there are models with bigger footprints (2×2 landmark towers and orbs from about 2036, 3×3 orbs from 2042), a tall building can be redeveloped together with its neighbours, as long as the whole area is flat land at one height, with no road or track, and every tile is free or a small building of the same town. Population counts once per building and scales with area (a 3×3 orb with 12 floors houses 864 people), so the late-game cities are dense and tall, with whole districts of massive chrome orbs. Orbs can't cover a road, so cities of orbs form where towns have wide blocks; letting redevelopment close a quiet street is a good later addition.
- **Each year tall buildings get taller** (the floor limit rises from 8 to 14), and Towns grow faster the better they're served.
- **Seeking time**: the debug window (F3) has a date slider from 2000 to 2050. Moving it forward plays the years in between (growth, redevelopment, fleets); moving it back re-ages whatever was built later, so the world looks as it did then. Developers can also start at a year with `STARCITY_YEAR=2046`.

### 10.8 Vehicles and buildings by era

Each family shows what real-world designs the placeholders stand for (late 20th century up to now, 2026) and the extrapolation after. Artists replace the placeholder file for the family; suggested extra models are listed so there's more than one look per generation. File names are `assets/models/<folder>/<name>.glb`.

| Family | 1990s (still around in 2000) | 2000s | 2010s | 2020s (to 2026) | 2030s | 2040s |
|---|---|---|---|---|---|---|
| **Cars** (`vehicles/car_*`) | Boxy hatchbacks, saloons and estates, mostly in silver, red, blue | Rounder bodies, minivans, the first SUVs | Crossovers, hybrids, LED lights | Electric: smooth, closed grilles, light bars, charging ports | Autonomous pods with glass canopies, no steering wheel | Chrome capsules, glowing underlines, hovering a hand above the road |
| **Buses** (`vehicles/bus_*`) | Step-entrance single-deckers, bright liveries | Low-floor buses with big windows; bendy buses on trunk routes | Diesel-hybrids with roof battery boxes, double-deckers in cities | Battery-electric with pantograph chargers, destination LEDs | Cab-less autonomous shuttles | Chrome pod-buses with glowing skirts |
| **Trucks** (`vehicles/truck_*`) | Rigid box lorries and cab-over tractors | Aerodynamic cabs, curtain-siders | Electric delivery vans (the parcels boom) | Electric heavy trucks, platooning convoys | Autonomous haulers with a sensor bar, no cab | Chrome maglev haulers |
| **Trains** (`trains/loco_*`, `coach_*`) | Diesel-electric locos with yellow noses, push-pull coaches, a diesel high-speed set | Electric multiple units, the first bullet-nose high-speed trains | Sleek 300 km/h electrics, battery-hybrid regional units | Hydrogen trains, driverless metros | Maglev: chrome noses, glowing skirts | Hyperloop pods: chrome capsules in a tube |
| **Ferries** (`boats/ferry_*`) | Roll-on/roll-off car ferries, hovercraft | Fast catamarans | Hybrid ferries | Battery ferries, the first foiling ones | Hydrofoils, autonomous | Chrome skimmers, autonomous |
| **Aircraft** (`planes/plane_*`) | Twin turboprop commuters, narrow-body jets | Double-deck giants, regional jets | Composite airliners with raked wingtips | Electric commuters, eVTOL air taxis | Supersonic business jets, blended-wing airliners | Hypersonic chrome planes; the first rockets (§10.4) |
| **Houses** (`buildings/house_*`, `pod_*`) | Brick semis, flat-roofed bungalows, red-tile suburbia | Pitched-roof estates, conservatories | Cubic white boxes, timber cladding | Solar roofs, charging porches | Rounded pods, green roofs | Chrome domes with glowing doors |
| **Mid-rise** (`block_*`) | Concrete flats, brutalist blocks | Rendered flats with balconies | White-and-glass blocks, solar trim | Metal-panel blocks | Steel and chrome-trimmed blocks | Chrome blocks with gold trim |
| **Towers** (`office_*`, `tower_*`) | Steel-and-glass offices | Glass curtain-wall towers | Taller glass towers with rooftop solar | Chrome-shafted towers | Twisted chrome landmarks (2×2) | Round chrome towers with glowing bands, orbs (2×2, 3×3) |
| **Stations, ports, airports** (`props/station_*`, `terminals/*`) | Brick station buildings, concrete platforms, working cranes | Same, refurbished | Glass canopies | Metal and glass | Chrome terminals with glowing platform edges | Domed chrome terminals |

Suggested models to add (each is one file with its own years): a bendy bus (2002–2018), a double-decker (2005–2030), a minivan (2000–2015), a tram (2010+, once tram track exists), a battery-electric loco with a pantograph (2018–2030), a tilting high-speed train (2005–2020), a container ship and a car carrier (2000+, for the cargo game), an air taxi eVTOL (2028–2045), a rocket (2032+), and for buildings a second and third variant of each generation so streets vary.

### 10.9 Content between 2000 and 2050

What the player meets along the way. Years are when a thing first appears, with a random shift of a few years per game, so the future isn't a calendar to memorise.

**Events** show as Win95-style headline pop-ups ("**Broadband arrives in Ashford**", "**Oil price shock: running costs up for two years**"), each with a simple, visible effect and usually a decision to make:

| When | Event | Effect |
|---|---|---|
| 2001 | Dot-com bust | Office demand dips for a year: towers grow more slowly |
| 2000s | Broadband spreads | Towns with fibre grow a little faster; commuter passengers fall about 5%, goods rise |
| 2005 | Cloud and broadband data centres | Data centres available; towns with one boom |
| 2003 | Low-cost airlines | Airport passenger demand up; short flights pay less |
| 2005–2012 | Fuel price spikes | Diesel and aviation running costs up 30% for a couple of years; nudges players towards electric |
| 2008 | Financial crisis | All income down 20% for 18 months; loan interest up: a test for over-borrowed players |
| 2007+ | Smartphones | Journey planners: station ratings up where there is a good service |
| 2010s | E-commerce | Parcels and mail grow; goods demand up; a new cargo, Parcels |
| 2020 | The pandemic | Passenger demand falls 60% for a year and recovers to about 90% for good (remote work); freight up 20% |
| 2020s | Low-emission zones | Diesel vehicles are banned from a town's centre; electric ones aren't |
| 2024–29 | **The AI boom** | Hosting rates triple, chip prices jump, the first AI campuses; electricity demand surges and power prices rise 40% |
| 2024+ | First autonomous pilots | Autonomous road vehicles appear for sale |
| 2027 | First commercial sub-orbital flights | Sub-orbital pads available (§10.4) |
| 2029 | AI correction | Hosting rates fall 40%, some campuses close; the compute dividend remains |
| 2030 | Orbital lifters; first lunar landings | Orbital spaceports available; the moon opens a year later |
| 2030+ | Sea levels rise | Coasts flood slowly: low coastal tiles become water (the sea grid already follows terrain edits), threatening ports, coastal stations and shore towns; seawalls become a thing to buy |
| 2030+ | Storms and floods | Random events ground flights, close a route or flood a low station for a month |
| 2031 | First lunar cargo | Lunar base; regolith for sale; regolith works in 2032 |
| 2035 | Fusion pilot plants | Helium-3 fusion plants; towns near them boom |
| 2040 | Asteroid prospectors | Deep-space haulers; Rare metals and Water ice |
| 2038+ | Automation dividend | Running costs fall about 40%; driver-less vehicles; towns shed some commuter traffic |
| 2045 | "Convergence" announced | The final megaproject opens (§10.5) |

**New industries and cargo along the way** (in addition to §5): *Container port* (2000), *Wind farm* (2005, power), *Biofuel plant* (2008, grain to fuel), *Data centre and AI campus* (§5.8), *Chip fab* and *Silica quarry* (2000), *Logistics hub* (2010+, goods to parcels), *Solar farm* (2012), *Lithium mine and Battery gigafactory* (2015–2020, lithium to batteries, which electric vehicles and the 2030s power industries need), *Hydrogen plant* (2022), *Vertical farm* (2020+, inside towns, food in small amounts), *Fusion plant* (2035), and the space chain (§10.4): *Regolith works* (2032), *Chrome foundry* (2034), *Nano-fabricator*, *Water works* and *Solar array receiver* (2040). New cargo: see the era table in §4.

**Technology and policy:**
- **Power:** the grid (§5.7) is the constraint that grows through the game: data centres, chip fabs, spaceports and megacities all need it, so each era needs new generation.
- **Electrification:** electric vehicles need chargers (a charging depot building, cheap, at a stop); electric trains need overhead wire; maglev needs its own track. These are the new things to build each era, so the player is always laying something new.
- **Subsidies and carbon tax:** from the 2020s the government subsidises electric vehicles in some towns and taxes diesel in others, shifting what pays.
- **Automation:** autonomous vehicles cost more but need no drivers (lower running costs); an early adopter bonus.
- **Mega-cities:** from the 2030s merged towns can become a *megacity* (population 20,000+), with its own demands (an orbital link by 2045; a spaceport within range is the status symbol).

**Look and feel:** the toolbar icons, window chrome and cursor shift through the desktops of the time (§10.3); the news ticker uses period language; the music (later) moves from synth pop through electronic to ambient. Each era has a one-line tagline in the debug window and a small banner on entering it.

### 10.10 The moon and beyond: surprising the player

"Whoa, I'm on the moon." The space chain (§10.4) starts as a few rockets leaving the map. Then, around 2038, a new button appears. The player should be genuinely surprised at how much more game there is.

**The moon (playable from about 2038).** The moon is a second map of the same kind (voxel terrain, no water, no weather), reached by rocket. Switching maps is a button on the status bar (§10.13), the lunar and Earth economies run side by side, and the same company owns both. Play there is a different flavour of the same game:
- **Domes.** Nothing outside a dome supports people. A **dome** claims an area (12×12 and up), seals it and costs power and water; inside it towns grow as on Earth, in the style of the era. **Sealed tunnels and maglev** link domes (built like rails); vehicles outside run in vacuum and fast, in low gravity.
- **Lunar industry.** The moon's regolith, Helium-3 and Water ice (from the polar craters) become **real mines on the lunar map**, replacing the virtual off-map base of §10.4. It consumes Earth's goods, people and Nano-goods through the **orbital lift** (a rocket route, later a space elevator). Domes want Lunar glass, compute wants power, and the sun on the moon is a two-week day.
- **Look.** A black sky with Earth in it and no atmosphere haze. `daylight.rs` is a pure function, so a lunar light is one new function.

**Mars (about 2046).** The same, but with a thin atmosphere, dust storms, and a six-month window for the transfer. Colonies here are *slower* to build and produce *Water ice* and *Metals*, and are the first place where the player's colony will not pay off for a decade. The point of Mars is patience.

**The solar system map (about 2048).** A zoomed-out **system view**: sun, planets, moons and the belt as nodes. The player draws **space lanes** between bodies and assigns autonomous freight drones (the same Orders machinery as trains and planes, scaled up: each leg has a delay in game time, with launch windows). The *transport game* is preserved all the way out: it's still pick-up and delivery, only on celestial scales.

**Autonomous space drones (2040+).** The late vehicle: self-routing freight and mining craft with no pilot, run by the player's agents. They need no spaceport on arrival, only a cradle. They build the first asteroid mines and, later, space-made solar arrays.

### 10.11 The galaxy

The final surprise. After the singularity (2050), the system view zooms out again, and the map becomes the galaxy. This is deliberately open-ended and not for the first release, but the design must make it a *continuation*, not a different game.

- **Seed ships.** The player builds slow **interstellar vessels** that carry a seed (a small self-sufficient factory plus a library of the company's designs). It takes years of game time to reach another star; on arrival it builds a dome-and-industry outpost and the player may switch to a new system map. Each system is a new map (see Level, §11).
- **Purpose, not paperclips.** What makes this interesting and not a "turn everything into one thing" game: **seed ships carry the charter** (§1.2). A fleet launched under *Profit* behaves differently from one under *People*: Profit-minded seeds strip-mine and move on; People-minded seeds build gardens and slowly grow; Planet-minded seeds avoid inhabited or living worlds. The player sees, a hundred game years later, the effect of the values they wrote down. The parked **Rival** (§1.1) would be the cautionary mirror, a maximiser which converts what it finds into more of itself. If the Rival is built (it is parked), meeting it is the late-game conflict, and it is optional to win.
- **No unbounded replication.** Seed ships can build one more ship per year per system at most, and every colony has an upkeep: expansion is a managed network, not a plague.
- **The galaxy view** is a stylised star map (a few hundred systems), each simulated in a cheap abstract mode (a handful of numbers) while the player is elsewhere.

### 10.12 The globe: dominating the map

On Earth the 512² map is a region. When regions become the unit, the game gets a **globe layer**: a strategic view of many regions (continents, islands), each its own `Level`, joined by **trunk lines**: hyper expressways, vactrains, skylanes and sea lanes (§7.6) that cross the map edge. The player starts in one region and the others are partly built by the Rival and by the world. "Dominating the globe" is taking over these regions' power, ports and cities, and tying them with ultra-high-speed networks. Unviewed regions simulate in a cheap economy-only mode (monthly numbers, no vehicles), so the web build pays for one fully simulated map at a time.

### 10.13 Moving on: a choice, with an invitation

Each new stage (the moon, Mars, the system map, the galaxy, the globe) is **the player's choice**. The game never forces the jump, but it is designed to *lead* the player there, because the point of the game is the distant endgame. And the choice is **not one-way**: the player can switch between stages at any time, and everything keeps running.

**The invitation.** Tools of persuasion, in order of subtlety:
- **A sight line.** Rockets leave the map from the 2020s, a lunar base glows on the moon in the sky from 2031 and a station ring appears overhead. The player sees where they could go for years before they can.
- **Dangled rewards.** Each stage needs a visible, specific thing the old stage can't provide: lunar glass for chrome cities, Helium-3 for fusion, Rare metals for Nano-goods. The industry windows show them as locked inputs ("Needs Rare metals: asteroid mining 2040").
- **Pressure from success.** The ceilings of the Earth economy are real: grid power, land and heat cap compute (§5.9), and the news ticker says so ("*Ashford's grid is at its limit*"). The easiest way to keep the number growing is up.
- **The banner.** The first time the prerequisite is met, a single pop-up: "**The lunar landing site is ready**: go there?" with a button. It never nags afterwards.
- **A small head start.** The first build on a new map is cheap and its first deliveries pay a bonus, so trying it is rewarded, not punished.

**Staying and returning.** The Earth map keeps simulating while the player is away, in a **cheap abstract mode** (monthly economy only, no vehicles moving; §12 globe notes) when it isn't on screen, and the vehicle and train positions are restored on return. A single status-bar switcher (Earth · Moon · Mars · System · Galaxy) goes anywhere already unlocked, and the company's totals (money, compute, charter) are shared. A player who hates the moon can play Earth for 50 years and still see the singularity; a player who loves it can leave Earth on autopilot (agents), a design in itself.

**What stays gated.** Only *timeline* things: a stage can't be entered before its technology year (§10.1), and the endgame (galaxy) needs the Convergence Array (§10.5). Within those limits, every jump is free.

## 11. Implementation plan and next steps

Each phase is playable and testable on its own; later phases lean on earlier ones. Every phase ends with the same checks: `cargo test` (incl. shader validation), `cargo check --target wasm32-unknown-unknown`, a native screenshot run, a frame-time check, and a `trunk build --release` size note.

### Done ✅

0. **Foundations:** terrain, networks, signals, stations, towns, trains and road vehicles, demo mode, rail levelling.
1. **Sea and air:** sea grid with budgeted A*, ports, ferries, airports, planes, demo ferry and flight lines.
2. **Time and light:** calendar, speeds, independent day/night, date slider.
3. **Eras and assets:** glTF pipeline, 60 placeholder models and artist guide, era/chrome shading, towns that renew and merge, vehicles that retire.

### Next steps, in order

**Phase 4 — Money.** `company.rs`: `Company { cash, loan, ledger }`. Charge `Plan::cost` when building (refuse if short and say why); monthly tick for running costs, upkeep and interest; cash in the status bar; the Finance window (§2). Passengers pay by distance (§3): stations record origin and boarding time. *Test:* a scripted route earns a profit within the §8 range. *Touches:* `app.rs`, `network.rs`, `stations`, `ui/`.

**Phase 5 — Cargo and stations.** `cargo.rs` with the `Cargo` table (§4); `Station.waiting` becomes per-cargo; vehicles carry a class; rating factors; catchment cache. Passengers remain the one-cargo case. *Test:* rating and catchment unit tests.

**Phase 6 — Industries and power.** `industry.rs` (the const kind table, placement with resource noise and `terrain::edit` levelling, monthly update, stock, closure/spawn); the industry window; `power.rs` with sources, consumers, radius grids and brownouts (§5.7). Static boxes first, then glTF models in `assets/models/industries/` (add a `Role::Industry` and a `kind` to the loader). *Test:* placement rules on generated maps; a chain delivers; a short grid cuts output.

**Phase 7 — Vehicle catalogue and the build window.** The `Model` table (§7) with `from`/`to` years in the glb files; the Build Vehicle window with "coming around 2031" entries; vehicles read their stats from the table; boats and planes use the same table. Players choose the generation they buy.

**Phase 8 — Towns as customers, goals, industry life.** §5.5, §5.2 closures and spawning, fund-a-new-industry, §9 goals.

**Phase 9 — Data centres and compute.** *Start with the meter:* the Compute meter, rate and log sparkline in the toolbar, driven by the formula of §5.9 with the Servers-era numbers, so the idle-game feel is there before the economy is.  The silica → chips → data-centre chain (§5.8) as ordinary industries; `compute.rs` with the CU pool (decay, allocation sliders) and the Compute window; first programs (hosting, analytics, forecasting, fleet optimiser); the Tech Pace meter shifting introduction years. *Test:* compute pays; brownouts cut CU; the allocation of a program changes its effect with diminishing returns. Later programs (autonomy, generative design, agents, synthesis) are added one at a time as the systems they touch arrive.

**Phase 9b — Forecast.** `forecast.rs`: clone the simulation state, run it N years headless in budgeted slices (so the web UI stays responsive), render ghost lines and a projected-income chart. *Test:* a forecast of an idle company matches the real run within tolerance. (The Rival, `scenario.rs` grown into `rival.rs`, is **parked** until the core game is solid; see §1.1 and §12.)

**Phase 10a — Agents, charters and the two ledgers.** Agents (§5.8) that build and run lines through the same `Plan` API the player uses; the four-slider charter window; the Wellbeing score from town and grid data; the endings (§1.2). *Test:* charters change what agents build in a seeded scenario, deterministically.

**Phase 10 — Events and the UI by era.** An events engine (headlines, effects with durations; §10.9), and the `Theme` trait with the six looks of §10.3 (Win95 classic, Glass, Sleek, then Holo, Liquid chrome and Ambient), blended at the boundaries. *Pure functions of the date plus a seed, testable.*

**Phase 11 — Space.** `Site::Spaceport`, `rockets.rs`, off-map destinations, the launch-cost curve and failure model, space industries and cargo (§10.4), then the Convergence Array. *Test:* the economics table: a goods launch loses money in 2030 and makes it by 2040. **Before this phase**, move the world, networks, towns and vehicles into a **`Level`** struct, a mechanical refactor, so that a second map becomes possible.

**Phase 12 — Late-game transport and big cities.** The hyper expressway layer, vertiports and skylanes, maglev and vactrain tracks (§7.6); the Prosperity score, size tiers, taller and wider cities, towns that extend their streets (§5.10). Compute as currency (§5.9).

**Phase 13 — The moon, Mars and the system map.** Second and third `Level`s, domes, sealed networks, the orbital lift, the system view and space drones (§10.10).

**Phase 14 — The globe and the galaxy.** Regions and trunk lines (§10.12), seed ships and the galaxy view (§10.11).

**Polish, any time:** breakdowns and depots, replace, overhead wire, terraforming tools, textures and animation from glb.

**Cross-cutting, any time:** profile the web build in a real browser (the one unverified thing); the headless economy test (§8) from Phase 4; replace placeholder art as the artists deliver (files only, no code); the 2× default speed question (§12).

### Data shapes (sketch)

```rust
enum Cargo { Passengers, Mail, Coal, IronOre, Grain, Oil, Fuel, Timber, Steel, Planks, Food, Goods,
             Silicon, Chips, Containers, Parcels, Lithium, Batteries, Hydrogen,
             Regolith, LunarGlass, Helium3, RareMetals, WaterIce, NanoGoods }
enum CargoClass { People, Bulk, General, Fresh }       // what a vehicle can carry

struct IndustryKind { name, footprint: (u8, u8), inputs: &[(Cargo, u16)], outputs: &[(Cargo, u16)],
                      power_mw: i16,                  // + produces, - consumes
                      years: (u16, u16), terrain: Rule, spacing: u16, colour: [u8; 3] }
struct Industry { id, kind, tiles: Rect, stock: [u32; N], produced_last_month: u32,
                  transported_last_month: u32, production: u32, rating_low_months: u16 }

struct Model { name, mode: Mode, price: u32, running: u32, speed: f32, capacity: u16,
               class: CargoClass, tier: u8, from: u16, to: u16 }
const MODELS: &[Model] = &[ /* tables in §7 */ ];

struct Company { cash: i64, loan: i64, year_ledger: Ledger,
                 compute: Compute }                    // pool, decay, per-program allocation
struct Program { kind: ProgramKind, share: f32 }       // a slider; effect = f(share, fleet, year)
struct Level { world, roads, rails, towns, vehicles, industries, power, .. }  // phase 11
```

### Web performance budget

The web build is a hard constraint (DESIGN.md §9.1), so the economy is designed around it:

- **Industries are few** (about 40–80), static, drawn as a few boxes each in the static box buffer. No per-frame cost.
- **The economy runs monthly**: one pass over industries and stations every 30 game seconds, well under a millisecond. Vehicles touch it only on arrival.
- **Catchment lookups are cached.** A station records which industries and buildings it covers when it's built or changes, not every tick (`population_near` is the pattern, but cached).
- **No new per-frame loops over the map.** Resource noise fields are evaluated only at generation; spawning checks a handful of candidate sites per month.
- **Vehicle counts stay bounded** (the existing cap of 900 road vehicles, small fleets for boats, planes and trains), and everything that moves is already drawn through the dynamic box buffer.
- **Power and compute are monthly sums:** one pass over sources, consumers and data centres each economy month; programs apply as multipliers read in the places that already compute costs, with no per-frame loops.
- **Rockets are bounded** (a few at a time), plumes are a capped particle count on the dynamic buffer.
- **Pathfinding is budgeted** (boats already do this: a fixed number of A* expansions per frame, with cached routes), and the same pattern applies to anything new that searches.

## 12. Open questions

1. ~~**Pacing of the 50-year arc.**~~ Decided: a calendar day per second, light cycling separately (§2), about 5 hours to 2050 at 1×. Still open: whether the default speed should be 2× and whether to add 16×.
2. ~~**Eras: all models from day one, or by year?**~~ Decided: by year, with a per-game shift and the Tech Pace (§10.1, §5.8).
3. **Distance: straight-line or route length?** Proposed: straight-line.
4. **Competing companies.** The market-share split (§5.2) would let AI companies exist; do we want them, or is the split only between the player's own stations? (A rival "AI company" is thematically appealing in the 2030s.)
5. **Mail** might be cut if passengers alone give enough for aircraft to do.
6. **Compute balance.** The programs table (§5.8) is the most speculative part of the design: how strong is too strong? Needs playtests, and the headless economy test should include a compute scenario.
7. **Are orbs gated by lunar glass?** §10.4 makes supplied towns redevelop faster; a stricter version would make orbs impossible without it. Visually that risks cities that never go chrome, so the proposed rule is a bonus, not a gate.
8. **Power grid:** radius pooling first, pylons later. Is that enough, or do pylons belong in the first version?
9. **Scale of the numbers.** Everything in §3–8 and §10.4 should be tuned against headless tests once money exists, not by hand.
10. **The Rival (parked).** Kept in the design (§1.1, §10.11) but deliberately not scheduled: revisit once money, industries, compute and agents work. If revived, it's the cautionary maximiser mirror; the galaxy section reads fine without it.
11. ~~**Compute scale.**~~ Decided: keep the unit-change spectacle (kCU → ... → Brim), as idle-game players already read it. A log-axis meter option stays a cheap fallback if playtests find it confusing.
12. **How far to take the twist?** Charters and endings (§1.2) assume the agents' behaviour can be steered and tested; the first version could ship with two ledgers and three endings, with agents as hand-written heuristics. Is that enough, or should agents be more surprising (they have a mind of their own)?
13. **Revisions:** rewinding is powerful, and might undermine consequence. Limited use (three per game) and a heavy compute price are the proposal.
14. ~~**The moon and the globe: same save?**~~ Decided: same save, optional, and reversible (§10.13).
