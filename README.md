# Starcity

**▶ Play in your browser: https://catchouli.github.io/starcity/**

A transport tycoon / city-growth game on voxel terrain, in Rust, playable in the browser.
See [DESIGN.md](DESIGN.md) for the plan and [CREDITS.md](CREDITS.md) for asset licenses.

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
| Bulldoze tool | Click a tile, or drag a rectangle, to clear it to dirt. Grass grows back from the edges. |

## Development

- `cargo test` runs the unit tests and validates the WGSL shader with naga. A
  shader error otherwise just leaves the web page blank.
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
- `src/roads.rs`: road pieces and drag planning
- `src/clouds.rs`: tiling voxel cloud field
- `loader.mjs`: download progress for the loading dialog in `index.html`
- `tools/process_textures.sh`: downscales source textures into `assets/`
