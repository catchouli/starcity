# Starcity — Design Doc

> Working title. A living document: started 2026-10-11 from an initial brain dump, expect it to change.

## 1. The pitch

An OpenTTD-style transport tycoon / city-growth game where the world is **real 3D voxel terrain** (Minecraft-style blocks) instead of a flat heightmap. You can build roads and railways between towns, dig tunnels through hills, and flatten land like in TTD. Industries (coal mines, timber yards, quarries, power stations, factories…) and settlements spawn around the map. Towns grow **organically and physically**: a town spreads outward from where it started and can eventually merge with its neighbours into a bigger city.

It's written in **Rust**. It compiles to **WebAssembly** and runs fast in the browser, and it ships as a **static site on GitHub Pages**, so anyone can play from a link.

## 2. Core pillars

1. **Hilly but buildable terrain.** Smooth rolling hills, not noisy high-frequency bumps. You should be able to run a railway up, down and across them, and there should be naturally flat areas where settlements form.
2. **Terrain you can change.** Dig tunnels, cut through hills, and flatten land down to a plane (like TTD's landscaping tools).
3. **Organic settlements.** Towns are networks of roads plus buildings that grow outward and can join up with each other.
4. **Transport drives the economy.** Moving people and goods between places creates economic activity, and economic activity makes towns grow faster.
5. **Instant to play.** Static web build, no install.

## 3. World & terrain

### 3.1 Grid

- The world is a 3D grid of blocks. The horizontal **tile** (x, z) is the unit everything is built on: one road, one track piece, one building footprint.
- The world is stored in **chunks** (e.g. 32×32 columns × N height) so meshing, saving and editing all work chunk by chunk.
- **The map is an island**: open sea all round the border. Generation sinks the terrain below sea level in a band near the edges, using a noise-wobbled distance so the coastline is irregular, with beaches where the land meets the water. The sea ring is navigable, so later **boats** can sail round the coast between ports (ports work like stations). The sea is **endless**: beyond the map, a sea surface and sandy seabed are drawn by the shader (generated from the vertex index, no stored geometry) and fade into the sky with distance, so the world has no visible edge. The camera can wander a map's width out to sea, and Home (or the island button) brings it back. Beaches and the seabed use sand.
- **Water**: a global sea level for now. Every tile whose surface is below it shows water, so a pit dug inland below sea level fills instantly, even if it doesn't connect to the sea. Planned: a per-tile water level filled by **flood fill from the sea** (and from generated lakes). Each terrain edit re-runs a local fill, so a channel dug from the coast floods while a sealed pit stays dry (until we add rain). Rivers come later.

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

- **Clock** ✅: a real calendar (leap years, weekday names) starting Sunday 1 January 1950, 08:00. One game hour is 2 s of simulation: a day is 48 s at normal speed, a year about 4¾ hours. The status bar shows the date and the time to the nearest quarter hour, like a Win95 tray clock.
- **Speed** ✅: pause, 1×, 2×, 4×, 8× buttons at the right of the toolbar; Space pauses and resumes at the previous speed. Speed scales the clock, trains, road vehicles, town growth and regrowth together. Each frame simulates at most 0.4 s, in steps of at most 0.1 s, so a slow frame at 8× slows the game down for a moment instead of making vehicle physics unstable or the frame worse.
- **Day/night** ✅ (`daylight.rs`): the sun crosses the sky from east to west, the sky and fog colour change with it, the light is warm at sunrise and sunset and cool and dim by moonlight, and clouds go dusky. Night is a mood, not an obstacle: the scene stays readable (about half the brightness of noon). Lamps (signals) are unshaded, so they glow. To keep it unobtrusive the cycle eases off at high speeds (full strength at 1×, 40% at 8×, so it never strobes) and **N** or View → Day/night cycle turns it off.
- **Cost on the web**: the light is a pure function of the time of day plus three uniform colours, with no shadow maps or extra passes. The scene is only re-rendered when the light has visibly changed (a colour step of one 8-bit channel, or the sun moving a few degrees), at most 10 times a second, and not at all while paused. Measured over a full day (`daylight::tests::idle_redraws_stay_rare`): about 4 scene redraws a second at 1× and at most 10 at 8×, where vehicles are redrawing the scene every frame anyway. That's well under the 15 a second the drifting clouds already cost.
- Later: lit windows at night, street and station lamps, cloud shadows, seasons.

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
- **Status bar** ✅: hint, status (tile, build cost, messages), and the date and time at the right.

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

- Source materials (4K PBR sets from ambientCG) live outside the repo in `~/assets/`. Only small processed versions are committed, in `assets/`.
- The game is viewed zoomed out, one tile at a time, so per-block textures are downscaled heavily (e.g. 4K → 128–256px). Normal and roughness maps are optional for now.
- Whenever an asset is added, `CREDITS.md` gets a line with its source URL, author and license.

## 10. Relationship to `tyke`

`tyke/` (Haskell, 2016: SDL2 + LambdaCube + imgui + reactive-banana) is kept here only as a reference and will be removed. What it contains:

- `Game/Terrain/Data.hs`: a chunk is `dimensions + Vector Bool`, indexed `x + dx*(y + dy*z)`.
- `Game/Terrain/Rendering.hs`: a naive per-block mesher that emits a face wherever the neighbour is air. It uses a two-tile texture atlas (side/top) from `grass_block.png`.
- `Game/Simulation/Camera/MixedCamera.hs`: the camera described in §9.2.
- `pipelines/blocks.lc`: a simple textured, Lambert-lit shader (light dir (−0.5, 1, 1), 0.1 ambient).

Terrain generation was never really started: it only produced random noise for one 10×1×10 chunk. So the Rust "port" is really a rewrite that keeps tyke's data layout, mesher idea, shader and camera.

## 11. Roadmap

**M0 — Skeleton** ✅
- Cargo workspace, `wgpu` + `winit` window, clear-colour frame running both natively and in the browser.
- GitHub Actions: build wasm, deploy to GitHub Pages.

**M1 — Voxel engine (tyke port)** ✅
- Chunk storage, face-culled mesher (then greedy), textured, lit blocks.
- Port the mixed ortho/perspective camera.
- egui debug overlay (FPS, camera info).

**M2 — Terrain generation** ✅ (except the settlement-site overlay)
- Corner heightmap → slope shapes → voxels.
- Slope geometry in the mesher.
- Live-tunable generation parameters and seed in egui. Iterate until the hills look right.
- Overlay showing the flatness score and candidate settlement sites.

**M3 — Win95 UI, roads & rail drawing**
- Win95 egui theme and top toolbar (§8). ✅
- Tile picking (mouse ray → tile/corner) and a hover highlight.
- Road tool: drag to draw along an axis or a 45° diagonal, with auto-connecting road tiles. ✅ Remove-road tool ✅.
- Rail tool: drag to draw track ✅, and place path signals ✅.
- Bulldoze tool and grass regrowth (first use of the fixed-tick simulation). ✅
- Tile picking and hover/drag highlight ✅. Loading dialog with download progress ✅.
- Incremental re-meshing of edited chunks.

**M4 — Terraforming**
- Raise, lower, level, dig and fill.

**M5 — Settlements** ✅
- Town founding and naming, growth along roads, buildings, passengers and service-driven growth (§5).

**M6 — Trains & stations**
- Trains ✅: placed with the train tool; follow the track graph (edge midpoints, at most 45° per step); accelerate and brake to stop points; reverse at dead ends; reserve paths through path signals and wait when blocked; crash head-on on unsignalled track; wrecks are cleared after a few seconds. Space pauses. They wander (random at junctions) until stations and orders exist.
- Stations, orders and routing ✅ (§4.4).
- Next: passengers and towns to give them somewhere to go.

**M7 — Industries & economy**
- Industry spawning, cargo chains, growth feedback.

**Later**
- Visible cars and people, merging conurbations, save/load, bridges, rivers.

## 12. Open questions

1. ~~Height step~~: half a tile (§3.2).
2. ~~Rail semantics~~: single bidirectional track per tile with block-reservation signals (§4.2–4.3).
3. ~~Signal locks~~: path reservation only (§4.3).
4. Map size target: 256², 512², 1024² tiles? This affects chunk count and memory on the web.
5. Visual style: textured blocks like tyke/Minecraft, or flat-shaded low-poly?
6. Name: "Starcity" is just the directory name for now.
7. ~~Diagonal roads and rail~~: dedicated diagonal pieces (§4.2).

## 13. Hand-off: where things stand (2026-10-12)

### Done in the latest session
- **Demo mode wired up** (`src/scenario.rs`, `StarcityApp::start_demo`): `--demo` natively, `?demo` on the web. It builds up to 12 rail lines between neighbouring towns, each with a 3-car train shuttling between its two stations, and the status bar says "Demo: N lines, M trains". The old fixed straight/45°/straight track shape never fitted the hills (every candidate hit a slope), so lines are now routed tile by tile with an A* over (tile, heading) using straights and curves (`scenario::route`). On the default 512 map it builds about 4 lines (each town gets at most one, 13 towns, spacing and slopes limit it). There's a test for that, which also runs the trains for two minutes.
- **Terraforming for rail** (§4.2 "Levelling"): `src/terrain/edit.rs` (`Corners` overlay, `apply`), `rails::plan_terraforming`, `Plan::{terraform, terraform_steps, cost}`. Tested on a generated map (levelled ground takes the track and stays consistent) and on a uniform steep ramp (refused).
- Notes corrected: the 512 map founds ~13 towns, not 29 (the cap of 24 and the spacing of size/5 apply).

### Next steps, in order
1. **Money** (M7 start). Costs already exist (`Plan::cost`) and are shown, but nothing is charged: add a balance in the status bar, charge on build, passenger income by distance, loans. Then give roads the same levelling (`roads::plan` has the same per-tile check) and let stations level their platforms.
2. **Check road vehicles visually** (native screenshot with `STARCITY_DEMO=towns`), and tune sizes, speeds and colours. Possible follow-ups: buses running between bus stops, trucks to industries, and despawning vehicles that wander far from home. Screenshots in demo mode centre on the biggest town; use `STARCITY_FOCUS` to look at a line.
3. **Check performance on the web at 512²** with the demo and hundreds of vehicles. The whole scene redraws every frame while anything moves. Look at generation/meshing time in wasm, the dynamic box upload size, and the town growth tick (`Towns::population` is O(buildings) per call, so cache per-town populations).
4. **Not yet verified live**: passenger loading and unloading (`StarcityApp::handle_arrivals` has no test), the train, station and town windows, the station and train tools in the browser, and the new levelling in the browser (it's only covered by unit tests; terrain re-meshing after a plan goes through `Changes::terrain`).
5. **Industries and cargo** (M7): coal mines and power stations etc., freight wagons, catchment for cargo.
6. **Terraforming tools** (M4): raise, lower, level and dig tools, including sloped tunnels, with water filled by flood fill from the sea (§3.1). `terrain::edit::Corners::level` is the building block.
7. Smaller items: cars and trains tilting on slopes, cloud shadows, settlement-site overlay (M2), save/load.

### Working notes
- `cargo test` validates the WGSL shader with naga. A shader error otherwise just blanks the web page.
- Dev hooks (native): `STARCITY_SCREENSHOT=out.png`, `STARCITY_SHOT_FRAME=N`, `STARCITY_DEBUG=1`, `STARCITY_ZOOM`, `STARCITY_FOCUS=dx,dz`, `STARCITY_HOUR=22` (start hour), `STARCITY_DEMO=roads|towns`; pass `--demo` for the rail-line scenario.
- `rustfmt.toml` sets 120 columns. A freshly built binary can take a while to launch on macOS, and screenshot runs sometimes take 30 s or more of wall time even though the app only uses about 2 s of CPU, so give them time.
- Generated seabed at height 0 has no blocks at all, so `World::surface` can return `None` for in-bounds sea tiles; `terrain::edit` treats those as flat at height 0.
