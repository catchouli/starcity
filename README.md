# Starcity

**▶ Play in your browser: https://catchouli.github.io/starcity/**

A transport tycoon / city-growth game on voxel terrain, in Rust, playable in the browser.
See [GAME-DESIGN.md](GAME-DESIGN.md) for the game and [ENGINE.md](ENGINE.md) for the engine and [CREDITS.md](CREDITS.md) for asset licenses.

## Running

Native:

```sh
cargo run --release
```

Web (needs `rustup target add wasm32-unknown-unknown` and [trunk](https://trunkrs.dev)):

```sh
trunk serve --release     # http://127.0.0.1:8080
trunk build --release     # static site in dist/
```

Demo mode builds rail lines between neighbouring towns, ferry lines between ports and flights between the biggest cities, with vehicles on each:
`cargo run --release -- --demo`, or add `?demo` to the web URL.

Pushing to `main` builds the static site and deploys it to GitHub Pages
(enable Pages with "GitHub Actions" as the source in the repo settings).

## Controls

Movement keys go by physical position, so WASD/QE stay in the same place on Dvorak, AZERTY and so on. Letter shortcuts (R, G, C) follow the letter on the key.

| Input | Action |
|---|---|
| WASD / arrows, left-drag | Pan |
| Q / E, right-drag | Rotate (right-drag also tilts) |
| Scroll | Zoom towards the cursor |
| R | Toggle perspective / orthographic |
| F1 | Wireframe (shows the greedy-meshed quads) |
| G | Tile grid |
| C | Clouds on/off |
| F3 | Debug window (stats, render settings, terrain generator) |
| Esc | Back to the pointer tool |
| Home / island button | Centre the view on the island |
| Signal tool | Click track to place a signal facing you, click again to turn it round, right-click to remove |
| Rail tool | Drag to lay track. Ground with the wrong slope is levelled for an extra cost (shown in the status bar) unless it's too steep |
| Port tool | Click flat coastal land to build a port |
| Ferry tool | Click water to place a ferry; click it to give it ports to visit |
| Airport tool | Click to build an airport (levels the ground for a price); O turns the runway |
| Plane tool | Click an airport to park a plane; click it to give it airports to visit |
| Station tool | Drag along straight track or flat ground to build platforms |
| Train tool | Click plain track to place a train heading away from you |
| Pointer click | Open a train (orders, stop/go), a station or a town |
| Space | Pause / resume the game (the clock, trains, vehicles, towns) |
| Speed buttons | Pause, 1×, 2×, 4×, 8× game speed (right of the toolbar) |
| N | Day/night cycle on/off |
| Bulldoze tool | Click a tile, or drag a rectangle, to clear it to dirt. Grass grows back from the edges. |

## Development

- `cargo test` runs the unit tests and validates the WGSL shader with naga. A
  shader error otherwise just leaves the web page blank.
- Debug window (F3): a **Date** slider from 2000 to 2050 seeks through time (forward plays the years, back re-ages what was built later). `STARCITY_YEAR=2046 cargo run --release -- --demo` starts in a year.
- `STARCITY_SCREENSHOT=out.png cargo run`: render a few frames natively, save a
  screenshot and exit. Add `STARCITY_DEBUG=1` to open the debug window.

## Layout

- `src/terrain/`: block storage, map generation, chunk meshing
- `src/render.rs`, `src/terrain.wgsl`: wgpu renderer (WebGPU, WebGL2 fallback)
- `src/camera.rs`: orbit camera with perspective↔ortho dolly zoom
- `src/app.rs`: eframe app, input and debug UI
- `src/ui/`: Win95-style theme, windows, menu bar, toolbar and status bar
- `src/picking.rs`: mouse ray to terrain tile
- `src/sim.rs`: fixed-tick simulation (bulldozing, grass regrowth)
- `src/clock.rs`: game date and time, speeds
- `src/daylight.rs`: sun, sky and light colour through the day
- `src/network.rs`: shared road/rail layers, edges and build plans
- `src/roads.rs`, `src/rails.rs`: road and track pieces, drag planning, signals
- `src/trains.rs`: train movement, routing, path-signal reservations, crashes
- `src/vehicles.rs`: cars, buses and trucks on the road graph
- `src/scenario.rs`: demo rail lines between towns (`--demo` / `?demo`)
- `src/terminals.rs`: ports and airports, orders
- `src/sea.rs`: water grid and boat pathfinding
- `src/boats.rs`, `src/planes.rs`: ferries and planes
- `src/app/craft.rs`: tools, windows, props and demo for ports, airports, ferries, planes
- `src/models.rs`, `src/props.rs`, `src/era.rs`: the glTF model catalog, turning game state into placed models, the eras
- `assets/models/`: the `.glb` models (placeholders: see its README to replace them); `tools/make_placeholder_models.py` regenerates them
- `src/stations.rs`: stations and platform planning
- `src/towns.rs`: town founding, names, growth, buildings, catchment
- `src/clouds.rs`: tiling voxel cloud field
- `loader.mjs`: download progress for the loading dialog in `index.html`
- `tools/process_textures.sh`: downscales source textures into `assets/`
