# Starcity — Engine and world design

> How the game is built: terrain, infrastructure, rendering, the web build and where the code stands. The game itself (the pitch, the economy, the eras, the plan) is in [GAME-DESIGN.md](GAME-DESIGN.md). Section numbers here are kept stable because code comments point at them, so there are gaps (§1–2, §10–11 moved or retired).

Pillars the engine serves: hilly but buildable terrain; terrain you can change; organic settlements; transport drives the economy; instant to play (static web build, no install).

## 3. World & terrain

### 3.1 Grid

- The world is a 3D grid of blocks. The horizontal **tile** (x, z) is the unit everything is built on: one road, one track piece, one building footprint.
- The world is stored in **chunks** (e.g. 32×32 columns × N height) so meshing, saving and editing all work chunk by chunk.
- **The map is an island**: open sea all round the border. Generation sinks the terrain below sea level in a band near the edges, using a noise-wobbled distance so the coastline is irregular, with beaches where the land meets the water. The sea ring is navigable: **boats** sail round the coast between ports (§4a). The sea is **endless**: beyond the map, a sea surface and sandy seabed are drawn by the shader (generated from the vertex index, no stored geometry) and fade into the sky with distance, so the world has no visible edge. The camera can wander a map's width out to sea, and Home (or the island button) brings it back. Beaches and the seabed use sand.
- **Water**: a global sea level for now (boats sail on it, §4a). Every tile whose surface is below it shows water, so a pit dug inland below sea level fills instantly, even if it doesn't connect to the sea. Planned: a per-tile water level filled by **flood fill from the sea** (and from generated lakes). Each terrain edit re-runs a local fill, so a channel dug from the coast floods while a sealed pit stays dry (until we add rain). Rivers come later.

### 3.2 Slopes (the big difference from tyke/Minecraft)

Pure cubes give stair-step terrain, which looks bad and can't take track. Instead:

- Each **surface** block can have a **shape**, not just solid or empty: `Full`, `Slope(dir)` (4), `OuterCorner(dir)` (4), `InnerCorner(dir)` (4), and possibly the steep/diagonal cases TTD has. This is close to OpenTTD's per-tile slope set, applied to the top voxel of each column.
- Underground blocks are always full cubes, so tunnels and cuttings are ordinary voxel edits.
- Generation works on a **corner heightmap**: integer heights at tile corners, with adjacent corners never differing by more than one step. The slope shape of each surface tile then follows directly from its four corner heights, just as in TTD.
- **What can be built where:** roads and rail go on flat tiles and on straight `Slope` tiles that run along the direction of travel. Buildings need flat tiles, or they flatten the land automatically for a fee.

**Decided: half-height steps.** One block is 1 tile wide × 0.5 tall, so a single slope is about 27°, which is gentle enough for track (TTD's proportions). The constant lives in one place (`terrain::BLOCK_HEIGHT`) in case we want to tune it.

### 3.3 Map generation

Goals: smooth large hills, broad flat areas, few sharp features.

Proposed pipeline:

1. **Base height:** low-frequency fBm (Perlin/OpenSimplex) with few octaves (2–4), plus light domain warping so hills look less blobby.
2. **Flattening:** a second low-frequency "flatness" noise, which is the other half of getting broad flat areas. Where it's high, squash heights towards a plateau or terrace level.
3. **Quantise** to integer corner heights.
4. **Slope constraint pass:** clamp so adjacent corners differ by at most 1. This removes impassable cliffs and gives the TTD look.
5. **Coast:** blend down to a seabed below sea level near the map edges (§3.1).
6. **Classify:** compute a per-tile "flatness" score and use it to choose settlement sites (large flat regions, spaced apart). Industries are placed with type-specific rules (e.g. quarries in hills, power stations near water or flat land).
7. **Fill voxels:** below the surface use stone, dirt, then a grass top layer. Maybe ore layers later for mines.

All generation is **seeded and deterministic**. A debug panel (egui) exposes the seed and noise parameters so terrain can be tuned live.

### 3.4 Terraforming tools

- Raise or lower a corner or tile (TTD-style).
- Level an area to a target height.
- Dig and fill blocks (tunnels, cuttings, embankments).
- **Sloped tunnels:** a tunnel can ramp down from the surface (a sloped floor block inside the hill) and then level off underground. Underground blocks are still full cubes; only the ramp blocks have slope shapes. So slope shapes aren't just for the surface: any block whose floor a road or rail runs on can have one.
- Every edit re-meshes only the affected chunk(s).
- **Bulldoze**: drag a rectangle (TTD-style) to clear everything in it (roads, track, later buildings) down to bare ground. Cleared grass becomes **dirt**.
- **Regrowth**: bare dirt slowly turns back into grass from the edges. Each simulation tick, a dirt tile with grass neighbours has a chance to turn to grass, and the chance rises with the number of grass neighbours. A large cleared patch fills in from its borders over a few minutes. Tiles at or below sea level don't regrow.

## 4. Infrastructure

### 4.1 Roads

- **Two-way**: one tile holds a lane in each direction, so cars can pass each other.
- **Pieces**: each tile has 8 road bits. Four *halves* join the tile centre to an edge midpoint, and combine into straights, bends, T-junctions and crossroads. Four *diagonals* join two adjacent edge midpoints across a corner, and a 45° road is a staircase of them. Pieces meet at edge midpoints, which later forms the graph cars drive on.
- **Drawing**: drag from a tile; mostly along an axis gives a straight road, nearer 45° gives a diagonal. The end tiles get the half pointing inwards, plus the outward half if a road already reaches that edge. That gives T-junctions, bends and continuations without special cases.
- **Diagonals off existing road**: dragging diagonally from a dead end turns that last tile into a diagonal piece, so the road bends 45° where it enters the tile and carries straight on. From a through road or junction, the road continues to the tile's far edge along its own axis, then turns 45°. A diagonal leaving through a dead end's open edge replaces the stub. From an empty tile there are two possible 45° lines; the planner picks the one that joins up at the far end.
- **Fit rules (v1)**: anything on flat ground; on a slope only straight pieces running up the slope; diagonals need flat ground; nothing on water. **All or nothing**: if any piece doesn't fit, the whole preview turns red, the status bar says why, and nothing is built. Track can level misfit ground for a fee (§4.2); roads can't yet.
- **Rendering**: roads are drawn by the terrain shader from a per-tile `Rgba8Uint` texture (r = roads, g = rails, b/a = drag preview), as a distance field around the pieces. They follow slopes exactly, cost no extra geometry, and placing one rewrites a single texel.
- Towns are made of the same roads the player builds.

### 4.2 Railways

- One track segment takes a whole tile, **with borders on each side** (ballast/fence).
- **Pieces** (TTD-style): every piece joins two edge midpoints. There are two straights and four corner diagonals, so no 90° turns and nothing stops at a tile's centre. Junctions (switches) are pieces sharing an edge midpoint.
- **Drawing** follows the road rules: drag straight or at 45°, all or nothing. A diagonal off a straight starts at the edge the straight leads to; from an existing diagonal it carries on along the same line. A diagonal arriving where track already reaches its entry edge joins there. Track is drawn by the shader (ballast, sleepers, rails) from the `g` channel of the infrastructure texture. Roads and track can't share a tile yet (level crossings later).
- **Levelling** ✅: where the ground has the wrong slope, the rail tool levels it instead of refusing. Each misfit tile is flattened (to the median of its corners, or its neighbour's height so the track stays level), and surrounding corners move just enough to keep every step to a neighbour at one block or less. Terraforming is priced: a piece costs £100 and each corner moved by one block costs £150, shown in the status bar while dragging. It's refused ("the ground is too steep") if a corner would move more than 3 blocks or more than 160 corners would move, which happens when a flat bench would drag a whole hillside with it, and if it would disturb roads, buildings or track beside it. The plan carries the corner changes (`Plan::terraform`) and `terrain::edit::apply` writes them. Roads and stations don't level yet.
- **Single track per tile.** Trains can run either way along it. A double-track line is two parallel rail tiles that the player links with junctions.
- **Collisions are real.** If two trains meet head-on on unsignalled track, they crash. Safety is the player's job.
- **Diagonal pieces**: a 45° drag builds dedicated diagonal track (TTD-style half-tile pieces running corner to corner), not a staircase of straight tiles.

### 4.3 Signals: path signals only

- One kind of signal, like OpenTTD's **path signals**. Each faces one way along its piece of track. Trains may only pass it travelling that way, and **nothing may enter a signalled block from behind**, which prevents accidental head-on crashes.
- Signals split track into **blocks**: the stretch between one signal and the next.
- When a train reaches a signal, it tries to **reserve a path through the block ahead**, up to the next signal. If any piece of that path is reserved by another train, it waits. Reservations are released as the train's tail clears them. Because only the path is locked, trains can use different routes through the same junction at the same time.
- On unsignalled track nothing stops trains, so they can still crash head-on.
- **Placing** ✅: with the signal tool, click plain track (not a junction) to place a signal facing the camera; click it again to turn it round; right-click to remove it. Building track that would change a signal's tile is blocked, and removing track takes its signals with it.
- Signals are drawn as small 3D props (post, head, lamp) with the instanced box renderer, which trains will use too.

### 4.4 Stations & orders

- **Stations** ✅: the station tool drags a straight run of platform tiles along an axis. Platforms lay their own track or reuse matching straight track, and need flat ground with no roads, junctions or signals. New platforms touching a station join it. They're drawn as platforms either side of the track plus a small station building, with a floating Win95 tooltip-style name label. Removing the track removes the platforms, and empty stations disappear (and drop out of train orders). Click a station with the pointer to rename it. Later, stations will serve the towns and industries within a catchment radius, and take their names from the nearest town.
- **Orders** ✅: each train has a list of stations to visit in turn. Click a train with the pointer to open its window: status, orders (add, reorder, delete), stop/go, find and remove.
- **Routing** ✅: a train computes a shortest-distance map to its next station over the track graph (reverse Dijkstra), rebuilt when its target or the track changes. Its choices at junctions, both on open track and when reserving paths through signals, follow that map. It brakes so its head stops at the far end of the platform, loads for a few seconds, then heads for the next order. A path reservation for a station-bound train ends at the platform end. If the next stop is only reachable behind it, the train turns round at the station (so terminus stations work). A train with no orders wanders.
- Later: loading and unloading actual passengers and cargo, conditional orders, timetables.

## 4a. Ports, airports, boats and planes

Simple vehicles that follow a list of stops, as a base for the different models in GAME-DESIGN.md §7. Ports and airports are both **terminals** (`terminals.rs`): named, with passengers waiting, served by a catchment of 4 tiles like stations, and visited in order by whatever has them in its **orders**. Passenger loading works as for trains: everyone unloads (crediting the nearest town) and up to the capacity boards.

- **Ports** ✅: the port tool needs a flat, free coastal land tile with a berth in the water beside it, in a body of water of at least 200 tiles (no ponds). £2,500. Drawn as a quay, warehouse, crane and a pier into the berth.
- **Ferries** ✅ (`boats.rs`): placed on water with the ferry tool, 40 passengers, 2.5 tiles/s. A ferry sails a route found by A* over the water grid, ties up for 4 s, then heads for its next port. Position follows the route exactly (so it never crosses land) while the hull swings round at a limited turn rate. It tells you when no sea route exists ("No way to reach X by sea") and retries if the terrain changes. Ferries don't collide.
- **Airports** ✅: a 10 × 3 tile strip: a runway with a dashed line, an apron, a terminal building and a control tower. £8,000 plus levelling the ground under it (the same corner-height machinery as track, so it's refused where the ground is too steep or something is in the way). Click to build, **O** turns the runway; the preview shows the footprint and the price.
- **Planes** ✅ (`planes.rs`): placed on an airport with the plane tool, 40 passengers, cruising at 9 tiles/s. Each leg is a precomputed list of waypoints: taxi to the runway end that points at the destination, roll and lift off, climb out, cruise, descend along an approach that lines up with the destination runway (landing from the side facing where it came from), roll out and taxi to the apron. Cruise height is above the highest terrain, and waypoints are lifted to stay clear of hills. Planes don't see each other. A shadow on the ground shows where they are.
- **Orders** ✅: click a ferry or plane for a window like the train's: status, load, add/reorder/delete stops, stop/go, find, remove. Click a port or airport to rename it and see who's waiting. Bulldozing a terminal removes it; parked planes go with it and others divert or are lost.

**Cost on the web** (measured natively, so allow two to three times that in wasm):
- The water grid (`sea.rs`) is built on first use, about 7 ms, and patched where terrain edits change a tile (it relabels connected bodies of water, which also makes "can a boat get there at all?" a lookup, so no search ever runs for an impossible route).
- Route searches run a fixed 3,000 node expansions per frame at most (a map-spanning route takes about 5 ms in total over several frames, 0.5 ms per call), and routes are cached per pair of berths (reversed for the way back), so a ferry line searches once.
- 200 ferries cost 36 µs a frame and 200 planes 20 µs (`two_hundred_boats_are_cheap`, `two_hundred_planes_are_cheap`). Their props (a few boxes each) go in the dynamic box buffer; ports and airports in the static one.
- Nothing here runs when nothing sails or flies, apart from the clock.

## 5. Settlements

- **Founding** ✅: at map generation, towns are placed on the flattest dry land, spaced apart (about one per 9,000 tiles²), with generated English-style names (prefix + suffix: Ashford, Kingsley, Thornbury…). Each starts as a crossroads and gets a head start of 30–120 growth steps. The name belongs to that founding point, shown as a label with the population.
- **Growth** ✅: each step a town does one of the following: extends a road from its network (using the same road planner as the player, keeping a one-tile gap from parallel roads so houses fit between), builds a house on a free flat tile beside a road, or adds a floor to a building near its centre (the floor limit rises with population, so centres get taller). The town's reach grows with its population. Because towns use the shared road layer, they join up with player roads and, as they grow, with each other into larger conurbations.
- **Buildings** ✅: drawn as boxes with roofs in a few colour schemes; buildings of four or more floors become grey-blue offices. Buildings block player roads, track and stations; bulldozing removes them.
- **Growth rate** ✅: a slow base rate plus a boost from recent passenger service (passengers delivered to the town's stations, decaying over time).
- **Passengers** ✅: buildings within 4 tiles of a station's platforms generate waiting passengers there (about one trip per person per 100 s, capped). Trains unload everyone at each stop (crediting the nearest town) and load up to their capacity (30 per carriage). New stations take the nearest town's name, with a compass point if it's taken.
- Later: passengers with destinations, station ratings, mail, town zoning (shops and industry), and towns building their own stations.

## 6. Industries & economy

- Primary industries (coal mine, timber yard, quarry, farm…) produce cargo.
- Secondary industries (power station, sawmill, factory…) take in cargo and may produce goods.
- Towns produce and accept passengers and mail, and accept goods.
- Industries spawn randomly (with placement rules), and may open and close over time.
- Economic activity in a region feeds back into town growth (§5).

## 7. Simulation (later)

- Individual cars driving on roads, people commuting between towns.
- Start abstract (cargo and passenger counts moving between stations) and add visible agents once the core loop works.

## 7a. Atmosphere

- **Clouds**: blocky voxel clouds with sloped edges (the same block shapes as the terrain), generated from noise, drifting slowly over the map at a fixed altitude. They fade out near the mouse cursor so they never get in the way of building. ✅ Implemented as a seamlessly tiling cloud `World` holding a few small clouds (clusters of domes in cube-sized blocks, four grey tones), meshed with the terrain mesher and drawn per chunk at a wind offset. They fade with an ordered dither (no sorting), near the cursor, past the map edge and when zoomed in. Idle redraws are capped at 15 fps while they drift, and C toggles them. Later: cloud shadows on the ground.

## 7b. Time and light

- **Clock** ✅: a real calendar (leap years, weekday names) starting Saturday 1 January 2000. **One calendar day is one second of simulation**: a month about 30 s, a year about 6 minutes at normal speed, and 2000 to 2050 in about 5 hours (about 76 minutes at 4×). The date is shown at the top right, in a sunken pane beside the speed buttons, with a sun or moon for the time of day.
- **Time of day is separate** ✅: the light cycles once every 120 s of simulation whatever the date, so the calendar can be as fast as a 50-year game needs while the day/night still looks good.
- **Speed** ✅: pause, 1×, 2×, 4×, 8× buttons at the right of the toolbar; Space pauses and resumes at the previous speed. Speed scales the calendar, the light, trains, road vehicles, boats, planes, town growth and regrowth together. Each frame simulates at most 0.4 s, in steps of at most 0.1 s, so a slow frame at 8× slows the game down for a moment instead of making vehicle physics unstable or the frame worse.
- **Seeking** ✅: the debug window (F3) has a **Date** slider from 2000 to 2050 (and buttons for 2000, 2012, 2025, 2038, 2050) that sets the date. Forward plays the years in between; back re-ages everything built later (§7c, GAME-DESIGN.md §10.7).
- **Day/night** ✅ (`daylight.rs`): the sun crosses the sky from east to west once every two minutes of simulation, the sky and fog colour change with it, the light is warm at sunrise and sunset and cool and dim by moonlight, and clouds go dusky. Night is a mood, not an obstacle: the scene stays readable (about half the brightness of noon). Lamps (signals) are unshaded, so they glow. To keep it unobtrusive the cycle eases off at high speeds (full strength at 1×, 40% at 8×, so it never strobes) and **N** or View → Day/night cycle turns it off.
- **Cost on the web**: the light is a pure function of the time of day plus three uniform colours, with no shadow maps or extra passes. The scene is only re-rendered when the light has visibly changed (a colour step of one 8-bit channel, or the sun moving a few degrees), at most 10 times a second, and not at all while paused. Measured over a full cycle (`daylight::tests::idle_redraws_stay_rare`, assuming a faster 48 s cycle than the real 120 s, so the real rate is lower): about 4 scene redraws a second at 1× and at most 10 at 8×, where vehicles are redrawing the scene every frame anyway. That's well under the 15 a second the drifting clouds already cost.
- Later: lit windows at night, street and station lamps, cloud shadows, seasons.

## 7c. Eras and style

The game runs from 2000 to 2050. As the years pass the world modernises and turns **chrome** (a little from 2025, extremely by 2050), and the late game opens **space**: spaceports, rockets and space resources, ending in a 2050 singularity goal. The design is in GAME-DESIGN.md §10; what's built:

- **Models by year** ✅ (`models.rs`, `props.rs`, `era.rs`): buildings, road vehicles, trains, ferries, planes, stations, ports and airports are glTF models chosen by the years in their metadata. Buildings and vehicles keep the look of the year they were built or bought in; stations, ports and airports are refurbished with the current year.
- **Towns change** ✅ (`towns.rs`): each building has a build year, old buildings are redeveloped over the years, and tall ones can merge into 2×2 and 3×3 megastructures (the orbs). Vehicle fleets turn over after 8–18 years.
- **Chrome** ✅: chrome is the glTF *metallic* value of a material, which the shader turns into a sky reflection, so it's controlled entirely by the art. Roads and track follow `era::chrome(year)` in the terrain shader (steel-blue glassy surfaces with glowing cyan lines).
- **Not yet**: the UI theme following the eras, vehicles for sale by year (the player's new trains, ferries and planes already take the newest model; there's no buy window yet), space, events.

## 8. UI

- **Windows 95 look**: grey (#c0c0c0) surfaces, 2px bevelled borders (light top-left, dark bottom-right; pressed buttons invert), navy title bars with white text, square corners, no transparency, and a pixel-style sans font.
- **Toolbar along the top**, like a Win95 app: a menu bar, then a row of bevelled icon buttons grouped into sections. Picking a tool makes it the active cursor tool; Esc or right-click returns to the pointer.
- First tool sections:
  - **Roads**: draw road. Drag from a start tile; the road follows a straight line along an axis, or a 45° diagonal, depending on the drag direction. Release to build. Plus a remove-road tool.
  - **Rail**: draw track (same straight or 45° dragging, with dedicated diagonal pieces), place signal, remove.
  - **Land**: bulldoze (clear a rectangle to bare dirt).
  - Later: terraform (raise, lower, level, dig), stations, demolish.
- Dialogs and info panels (town info, train orders) are Win95 windows inside the game view.
- The debug window (F3) stays separate from the game UI.
- **Status bar** ✅: hint and status (tile, build cost, messages). The date and time live in the toolbar, next to the speed buttons.

## 9. Tech stack

| Concern | Choice | Notes |
|---|---|---|
| Language | Rust (stable) | |
| Rendering | `wgpu` | WebGPU in modern browsers, WebGL2 fallback, native Vulkan/Metal/DX12 |
| Windowing/input | `winit` | Works on web and native |
| Debug UI | `egui` | Fills the role imgui had in tyke |
| Math | `glam` | |
| Noise | `noise` crate or a small custom OpenSimplex | Needs to be deterministic across platforms |
| Web build | `trunk` (or `wasm-bindgen` + `wasm-pack`) | Outputs a static `dist/` |
| Hosting | GitHub Pages via a GitHub Actions workflow | |
| Textures | [ambientCG](https://ambientcg.com) (CC0) | Downscaled by `tools/process_textures.sh`. **Every third-party asset is listed in `CREDITS.md` with its source and license**, even CC0 ones. |

**Why not Bevy?** It's a real option (ECS, UI, asset pipeline), but it brings large wasm binaries and long compile times. The core of this game is custom voxel meshing and a custom simulation, so plain `wgpu` keeps things lean and fast on the web. Revisit if the UI/ECS work gets heavy.

### 9.1 Web performance notes

- **Meshing:** greedy meshing (merge coplanar faces) for full cubes, plus dedicated geometry for slope shapes. Only re-mesh dirty chunks.
- **Threads:** wasm threads need `SharedArrayBuffer`, which requires COOP/COEP headers. GitHub Pages can't set those. The workaround is `coi-serviceworker`. Plan to stay single-threaded at first and keep per-frame work bounded, e.g. a meshing budget per frame.
- Keep wasm size down: LTO, `wasm-opt`, size-optimised dependencies, and only the eframe features we use. The current bundle is ~6.9 MB (3.2 MB gzipped). Most of it is the WebGL2 fallback (wgpu-core plus naga translating WGSL to GLSL). Going WebGPU-only would shrink it a lot but exclude older browsers.
- **Render on demand**: the 3D view is only re-rendered when the camera or world changes, so an idle game costs almost nothing. When the simulation runs, it should still only re-render chunks or objects that changed.
- **Compact vertices**: 8 bytes (u16 x, y, z, material and flags). Normals are derived in the fragment shader, and one quad index buffer is shared by all chunks.
- **Greedy meshing** of flat tops and water. Use F1 (wireframe) to see the merged quads.
- **Meshing cost**: a 256² map meshes in ~11 ms natively and ~22 ms in wasm. Columns skip their buried solid core, and edits only re-mesh the affected chunks.

### 9.2 Camera (carried over from tyke)

tyke's best piece is its camera. It should be ported as-is:

- An isometric-ish default view (yaw π/6, pitch −π/6).
- A **mixed perspective/orthographic projection**: a dolly zoom tweens the FOV toward 0 while keeping the screen width at the focal point constant, then snaps to true ortho (toggled with `R`).
- WASD pans relative to the camera's yaw, Q/E rotate, the scroll wheel zooms (changes distance), and left-drag orbits.

### 9.3 Assets

- **Models are glTF binaries** (`.glb`) in `assets/models/`, authored in Blender; as much of the look of the game as possible is data in those files. See `assets/models/README.md` for the conventions, the custom properties (role, kind, from/to years, floors, footprint, weight) and how to export. `build.rs` embeds every `.glb` in the program, so there's nothing to download separately on the web, and adding a model needs no code change. The shipped files are placeholders generated by `tools/make_placeholder_models.py`; artists replace them.
- **How they render**: all models go in one vertex buffer and one index buffer. Placed models (position, yaw, scale, tint) are instanced, one draw per model per layer (static: buildings, stations, terminals; dynamic: everything that moves, rebuilt each frame). Materials map to a single shader: base colour, metallic and roughness (sky reflection), emissive (glow), and `Paint` materials take the instance's colour. Triangle budgets per role are enforced by a test.
- **Textures** (terrain only today): source materials (4K PBR sets from ambientCG) live outside the repo in `~/assets/`; only small processed versions are committed, in `assets/`. The game is viewed zoomed out, so per-block textures are downscaled heavily (4K → 128–256px). Whenever an asset is added, `CREDITS.md` gets a line with its source URL, author and license.
- **Not yet for models**: textures, animation (wheels, propellers), level of detail. The loader is built to add them without breaking existing files.

## 10. Relationship to `tyke`

`tyke/` (Haskell, 2016) is kept in the repo only as a reference for the original camera (§9.2) and will be removed. Nothing else from it is used.

## 11. Engine to do

Done: skeleton and CI, voxel engine, terrain generation, Win95 UI, roads, rail, signals, bulldozer, settlements, trains, stations and orders (see §13 for the rest). Still open:
- **Terraforming tools** (raise, lower, level, dig; sloped tunnels; flood-fill water, §3.1).
- Incremental re-meshing of edited chunks, settlement-site overlay (M2).
- Visible cars and people, merging conurbations, save/load, bridges, rivers.
- Everything on the game side is planned in GAME-DESIGN.md §11.

## 12. Open questions

1. ~~Height step~~: half a tile (§3.2).
2. ~~Rail semantics~~: single bidirectional track per tile with block-reservation signals (§4.2–4.3).
3. ~~Signal locks~~: path reservation only (§4.3).
4. Map size target: 256², 512², 1024² tiles? This affects chunk count and memory on the web.
5. Visual style: textured blocks like tyke/Minecraft, or flat-shaded low-poly?
6. Name: "Starcity" is just the directory name for now.
7. ~~Diagonal roads and rail~~: dedicated diagonal pieces (§4.2).

## 13. Hand-off: where things stand (2026-10-12)

### Done in the latest sessions
- **Demo mode** (`--demo` / `?demo`, `scenario.rs`, `StarcityApp::start_demo`): rail lines between neighbouring towns (A* routed round hills), ferry lines between coastal towns (a ferry at each end), and flights between the biggest cities (airports levelled flat beside them, a plane at each end of each route). On the default map: about 4 rail lines, 8 ferries, 10 planes.
- **Terraforming for rail** (§4.2 "Levelling"): `terrain/edit.rs` (corner overlay, `apply`), `rails::plan_terraforming`, `Plan::cost`. Airports use it too.
- **Clock, speed and day/night** (§7b): `clock.rs`, `daylight.rs`.
- **Models, eras and chrome** (§7c, §9.3, GAME-DESIGN.md §10): `models.rs`, `props.rs`, `era.rs`, `build.rs`, `assets/models/*.glb` (60 placeholders from `tools/make_placeholder_models.py`), town redevelopment, a date slider in the debug window.
- **Ports, ferries, airports and planes** (§4a): `terminals.rs`, `sea.rs`, `boats.rs`, `planes.rs`, and `app/craft.rs` for the tools, windows, props and demo.
- **Company, money and the headquarters** (GAME-DESIGN.md §2, §9.1, §9.2; Phase 4): `company.rs` (cash, loan, yearly books, HQ levels 0–2 and their checklists, fares, the next-step goals and stuck hints; all pure and tested), `hq.rs` (the garage on a 4×4 plot in the company town), `app/company_ui.rs` (charging, income, Finances, Headquarters and Next Step windows), `Role::Hq` models (`buildings/hq_*.glb`). Tools are charged; ports, airports, ferries and planes need the Regional office; `--demo` and `STARCITY_DEMO` run in free sandbox mode.
- **Rail routing and gentle terraforming** (this session): dragging the rail tool from any point to any other now finds a route (`rails::plan_route`, A* over straights and curves, avoiding water, houses and roads, with level crossings where a straight track crosses a straight road), then lays it on a gentle grade (`grade_profile`: dynamic programming for the height along the line, steps of one block at most, ramps only on straight tiles, ground under and beside built things held fixed). `Corners` got configurable limits and frozen corners. Test: `towns_can_be_joined_across_any_terrain` (about 88% of hard town pairs; the rest report NoRoute, Slope or Disturbs).
- **Cargo and stations** (GAME-DESIGN.md §3–5; Phase 5): `cargo.rs` (cargo table, fares, `Stock`, per-cargo station `Service` and rating), `exchange.rs` (`accrue` and `arrive`, shared by the app and the headless economy test `a_first_railway_pays_for_itself_in_about_a_year_or_two`). Trains carry passengers or mail; stations show waiting cargo and ratings.
- **Industries and power** (GAME-DESIGN.md §5; Phase 6): `industry.rs` (kinds, recipes, placement with `Corners` levelling, monthly production, deliveries), `power.rs`, `app/industry_ui.rs` (monthly step, industry window), `Role::Industry` models (`industries/*.glb`), `exchange::collect` and room-limited `arrive`; contracts generalised to supply contracts (`ContractKind`).
- **Vehicle catalogue** (GAME-DESIGN.md §7; Phase 7, trains): `catalog.rs`, `app/catalog_ui.rs` (Buy a train window), per-train speed/price/running cost; Assistant docked panel (`company_ui::assistant_panel`); price beside the cursor (`cursor_label`); loading progress bars (`loading_labels`, `Train::flow`); `scenario::build_freight` for demo freight lines.
- **Web build**: a blank page with no console error means a shader or pipeline problem that naga does not catch; the see-through dither once did this (an array lookup in `fs_model`). Always load `trunk build --release` output in a browser after GPU changes.
- **Council contracts** (GAME-DESIGN.md §9.3): seed money up front and a reward on completion drive the opening; `company::Contract`, `app/company_ui.rs`.
- **GAME-DESIGN.md**: the money game: costs, cargo, industries and supply chains, vehicle models in cheap/fast tiers, goals, and the implementation order. Written but not built.

### Next steps, in order
1. **Replace the placeholders with art, and extend the era system** (GAME-DESIGN.md §10): textures and animation in the model loader; a buy window so the player picks vehicle models by year; the UI theme following the eras; events and the new industries and cargo; space.
2. ~~**Money**~~ Done for passengers (Phase 4). Still to do: roads and stations levelling slopes like rail and airports, selling vehicles, upkeep per vehicle age, and tuning with a headless economy test (the payback targets in GAME-DESIGN.md §8).
3. **Cargo and stations**, then **industries** (GAME-DESIGN.md plan steps 2–3), then the **vehicle catalogue**: the ferry, plane, train and road vehicle become the first tier of each mode.
4. **Check performance on the web** at 512² with the demo, hundreds of vehicles, boats, planes and the day/night cycle, in a real browser. Everything above is measured natively only. The whole scene redraws every frame while anything moves, so watch generation/meshing time in wasm, the dynamic box upload size, and the town growth tick (`Towns::population` is O(buildings) per call, so cache per-town populations).
5. **Not yet verified live in a browser**: the Finances, Headquarters and Next Step windows and the greyed-out toolbar buttons (only a native screenshot of the opening has been checked), passenger loading and unloading (`handle_arrivals`, `handle_craft_arrivals` have no tests), the train, station, town, ferry, plane and terminal windows, and the station, train, port, airport, ferry and plane tools.
6. **Terraforming tools** (M4): raise, lower, level and dig tools, including sloped tunnels, with water filled by flood fill from the sea (§3.1). `terrain::edit::Corners::level` is the building block; `Sea::refresh` already follows terrain edits.
7. Smaller items: cars and trains tilting on slopes, cloud shadows, lit windows at night, planes and ferries avoiding each other, bridges, settlement-site overlay (M2), save/load.

### Working notes
- `cargo test` validates the WGSL shader with naga. A shader error otherwise just blanks the web page.
- Dev hooks (native): `STARCITY_YEAR=2046` (start in a year, with the towns grown and rebuilt), `STARCITY_SCREENSHOT=out.png`, `STARCITY_SHOT_FRAME=N`, `STARCITY_DEBUG=1`, `STARCITY_ZOOM`, `STARCITY_FOCUS=dx,dz`, `STARCITY_HOUR=22` (start hour), `STARCITY_DEMO=roads|towns`; pass `--demo` for the demo scenario. With `STARCITY_DEMO=towns` the camera centres on the biggest town, which is where the demo's first airport and port are.
- `rustfmt.toml` sets 120 columns. A freshly built binary can take a while to launch on macOS, and screenshot runs sometimes take 30 s or more of wall time even though the app only uses about 2 s of CPU, so give them time (large `STARCITY_SHOT_FRAME` values can take minutes).
- Generated seabed at height 0 has no blocks at all, so `World::surface` can return `None` for in-bounds sea tiles; `terrain::edit` and `sea` treat those as flat at height 0 / water.
