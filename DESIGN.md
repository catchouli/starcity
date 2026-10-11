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
- Water: a global sea level to start with. Rivers and lakes come later.

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
5. **Classify:** compute a per-tile "flatness" score and use it to choose settlement sites (large flat regions, spaced apart). Industries are placed with type-specific rules (e.g. quarries in hills, power stations near water or flat land).
6. **Fill voxels:** below the surface use stone, dirt, then a grass top layer. Maybe ore layers later for mines.

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
- 1 tile wide, and they **auto-connect** to orthogonally adjacent road tiles (the tile picks straight, corner, T or cross from its neighbours).
- Towns are made of the same roads the player builds.

### 4.2 Railways

- One track segment takes a whole tile, **with borders on each side** (ballast/fence).
- **Single track per tile.** Trains can run either way along it. A double-track line is two parallel rail tiles that the player links with junctions.
- **Collisions are real.** If two trains meet head-on on unsignalled track, they crash. Safety is the player's job.
- **Diagonal pieces**: a 45° drag builds dedicated diagonal track (TTD-style half-tile pieces running corner to corner), not a staircase of straight tiles.

### 4.3 Signals: block reservation

- Signals split track into **blocks**: the stretch between one signal and the next.
- When a train reaches a signal, it tries to take an **exclusive lock on the track ahead up to the next signal**. If it gets the lock, it proceeds. If another train holds any part of that block, it waits at the signal.
- The lock is released as the train's tail clears the block.
- With this one rule, players can build two parallel lines with crossovers and junctions, and signal them correctly to keep everything safe.
- Design note: if the lock covers the *whole path through the block*, junctions need no special signal types (this is OpenTTD's "path signal" idea, but simpler). Decide later whether a lock covers the whole block or only the reserved path. Per-path locking allows more traffic through junctions.

### 4.4 Stations & routes

- Players place **stations** next to towns or industries. Each station serves the settlements and industries within a catchment radius, and trains load and unload there.
- Stations are multi-tile platforms on track.
- **Train routes/orders:** each train gets a programmable list of stops (go to A, load, go to B, unload…). Start with simple ordered stop lists and add conditions later.
- Tunnels can follow any dug-out path, since the terrain is real voxels. Bridges come later.

## 5. Settlements

- A settlement is **seeded** at a point with a name. **The name belongs to that origin point.** There's no fixed town boundary.
- Growth: new roads extend from the existing road network and buildings fill the plots beside roads. Density and building size go up near the centre over time.
- **Merging:** when two settlements grow into each other, their road networks connect and they effectively become one conurbation. Each original name still marks its own district (like London boroughs).
- **Growth rate** depends on local economic activity: passengers carried, goods delivered, nearby industry production and station service quality. Well-connected towns boom.

## 6. Industries & economy

- Primary industries (coal mine, timber yard, quarry, farm…) produce cargo.
- Secondary industries (power station, sawmill, factory…) take in cargo and may produce goods.
- Towns produce and accept passengers and mail, and accept goods.
- Industries spawn randomly (with placement rules), and may open and close over time.
- Economic activity in a region feeds back into town growth (§5).

## 7. Simulation (later)

- Individual cars driving on roads, people commuting between towns.
- Start abstract (cargo and passenger counts moving between stations) and add visible agents once the core loop works.

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
- Win95 egui theme and top toolbar (§8).
- Tile picking (mouse ray → tile/corner) and a hover highlight.
- Road tool: drag to draw along an axis or a 45° diagonal, with auto-connecting road tiles.
- Rail tool: drag to draw track, and place signals.
- Bulldoze tool and grass regrowth (first use of the fixed-tick simulation). ✅
- Tile picking and hover/drag highlight ✅. Loading dialog with download progress ✅.
- Incremental re-meshing of edited chunks.

**M4 — Terraforming**
- Raise, lower, level, dig and fill.

**M5 — Settlements**
- Settlement seeding and naming, basic outward growth along roads.

**M6 — Trains & stations**
- Stations, trains with block-reservation signalling, passengers between towns.

**M7 — Industries & economy**
- Industry spawning, cargo chains, growth feedback.

**Later**
- Visible cars and people, merging conurbations, save/load, bridges, rivers.

## 12. Open questions

1. ~~Height step~~: half a tile (§3.2).
2. ~~Rail semantics~~: single bidirectional track per tile with block-reservation signals (§4.2–4.3).
3. Signal locks: whole block or reserved path only (§4.3)?
4. Map size target: 256², 512², 1024² tiles? This affects chunk count and memory on the web.
5. Visual style: textured blocks like tyke/Minecraft, or flat-shaded low-poly?
6. Name: "Starcity" is just the directory name for now.
7. ~~Diagonal roads and rail~~: dedicated diagonal pieces (§4.2).
