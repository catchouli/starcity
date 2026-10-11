# Starcity

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

| Input | Action |
|---|---|
| WASD / arrows, left-drag | Pan |
| Q / E, right-drag | Rotate (right-drag also tilts) |
| Scroll | Zoom |
| R | Toggle perspective / orthographic |
| F1 | Wireframe (shows the greedy-meshed quads) |
| G | Tile grid |
| F3 | Debug window (stats, render settings, terrain generator) |

## Layout

- `src/terrain/`: block storage, map generation, chunk meshing
- `src/render.rs`, `src/terrain.wgsl`: wgpu renderer (WebGPU, WebGL2 fallback)
- `src/camera.rs`: orbit camera with perspective↔ortho dolly zoom
- `src/app.rs`: eframe app, input and debug UI
- `tools/process_textures.sh`: downscales source textures into `assets/`
