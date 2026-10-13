# Models

Everything you see on the map apart from the terrain, water and roads is a **glTF binary (`.glb`) model** in this folder. They're plain files made for Blender: change a file, rebuild, and the game picks it up. No code changes are needed to replace a model, to add a new generation of bus, or to add a new kind of house.

The models here are **placeholders** made by `tools/make_placeholder_models.py` (they are original, CC0). Replace any of them with real art. The script is only for regenerating the placeholders; don't run it over your work (it will overwrite the placeholder files, and it's the only thing that does).

## The quick way: replace a placeholder

1. In Blender, **File → Import → glTF 2.0** the file you're replacing (say `buildings/hq_2_regional_office.glb`). It comes in at the right size, facing the right way, with its metadata attached as custom properties on the root object.
2. Model over it, then delete the placeholder parts.
3. **File → Export → glTF 2.0**, Format **glTF Binary (.glb)**, and save over the same file name.

That's all. If "Custom Properties" isn't ticked on export, nothing is lost: the game reads anything missing from **`models.json`** in this folder, which lists every model's metadata by file name. If it is ticked, the properties in the file win, so you can change a model's years or footprint either in Blender or in `models.json`. (Tested with Blender's own import and export.)

## Adding or replacing a model

1. Make the model in Blender (conventions below).
2. Add the custom properties (below) to the **root object**, or add a line for the new file to `models.json` (the file name without `.glb`, then its properties). Without either, the folder decides the `role`, the file name is the `kind`, and everything else takes the defaults below.
3. **File → Export → glTF 2.0 (.glb)**. In the export options: Format **glTF Binary (.glb)**, Include → **Custom Properties** ticked, Transform → **+Y Up** (the default), Mesh → **Apply Modifiers**, Normals ticked, **no** textures (see "Not yet").
4. Save it in the right folder with the same name to replace a model, or any new name to add one.
5. `cargo test` checks it loads and is a sensible size. Run the game with `cargo run --release -- --demo`, open the debug window with **F3** and drag the **Date** slider (2000 to 2050) to see your model in the years it's used.

`build.rs` embeds every `.glb` under this folder into the program (so the web build needs no extra downloads), so just rebuild.

## Folders and what the models are

| Folder | Role | What goes there |
|---|---|---|
| `buildings/` | building | Houses, mid-rise blocks, towers, landmarks and orbs for the towns |
| `vehicles/` | road_vehicle | `car`, `bus`, `truck` |
| `trains/` | train_car | `loco` (the engine) and `coach` (every other car) |
| `boats/` | boat | `ferry` |
| `planes/` | plane | `airliner` |
| `terminals/` | terminal | `port` (one tile) and `airport` (a 10 × 3 strip) |
| `props/` | prop | `station_platform`, `station_building`, `signal_red`, `signal_green`, `pier`, `shadow` |

The folder sets the default `role`; the file name is the default `kind`.

## Conventions

- **Units:** one Blender unit is one map tile. A house is about 0.6 across, a bus about 0.6 long. Trains are exactly 0.8 long per car.
- **Up is +Z in Blender** (+Y in glTF; the exporter converts). The **ground is at Z = 0**; nothing should go below it (a pier's posts are the one exception).
- **Facing:** vehicles, boats, planes and train cars **face +X** in Blender. Signals stand at the side of the track with their lamp facing −X. A port faces +X: the water is on its +X side.
- **Origin:**
  - Vehicles, trains, boats, planes, signals and platforms: the middle of the thing, on the ground.
  - Buildings: the **centre of the footprint** on the ground. A 1 × 1 building spans ±0.5; a 3 × 3 orb spans ±1.5.
  - Port: the centre of its tile. Pier: the middle of the pier (the game places it at sea level).
  - **Airport: the north-west corner** of its 10 × 3 strip, with the runway along +X. See "Airport layout" below.
- **Keep it small.** Triangle budgets (checked by a test): buildings 4,000, vehicles and train cars 1,500, boats and planes 3,000, terminals 8,000. Hundreds are on screen at once, in a browser. Shared flat shading is cheap; don't sculpt.
- **Smooth or flat?** The game uses your normals as exported, so a chrome orb wants smooth shading and a box wants flat.

## Custom properties (the metadata)

Set these on the root object (Object properties → Custom Properties) and export with "Custom Properties" on. All are optional except where noted; numbers can be integers or text.

| Property | Meaning | Default |
|---|---|---|
| `role` | `building`, `road_vehicle`, `train_car`, `boat`, `plane`, `terminal`, `prop` | from the folder |
| `kind` | What it is within the role: `house`, `block`, `tower`, `landmark`, `orb` for buildings (informational), `car`/`bus`/`truck`, `loco`/`coach`, `ferry`, `airliner`, `port`/`airport`, and for props `station_platform`, `station_building`, `signal_red`, `signal_green`, `pier`, `shadow` | the file name |
| `from`, `to` | The years it's in use: built (buildings) or bought (vehicles), inclusive | 1900, 2200 |
| `min_floors`, `max_floors` | Buildings: the floor counts it suits (1–2 a house, 3–5 mid-rise, 6+ tower) | 1, 255 |
| `tiles_x`, `tiles_z` | Buildings: its footprint in tiles | 1, 1 |
| `weight` | How often it's chosen among equally good models | 1 |

How a model is chosen: for the year and (for buildings) the floors and footprint, the game takes every model that fits and picks one by weight. Give two houses the same years and both appear, mixed. If nothing fits a year, the nearest generation is used, so nothing is ever missing. A building keeps the look of the year it was built in; old buildings are replaced over the years. Vehicles keep the look of the year they were bought in. Stations, ports and airports follow the current year.

Make a **generation** by giving a model its own `from` and `to`, so the file for the next one begins the year the last one ends. The placeholders show the pattern: `bus_1998` (1990–2009), `bus_2010` (2010–2023), `bus_2024` (2024–2037), `bus_2038` (2038+).

## Materials

Use plain Principled BSDF materials. The game reads:

- **Base Color** → the colour.
- **Metallic** → how chrome it is. Metallic 1, roughness 0.05 is mirror chrome: it reflects the sky and the sun. Use it freely from 2025 on; it's the look of the future.
- **Roughness** → how sharp the reflection is.
- **Emission** (colour and Strength) → glows in its own colour, whatever the light (windows, lane lights, engine strips). Strength about 1 is plenty.
- A material whose **name starts with `Paint`** is **tinted by the vehicle's own colour** (so one bus model comes in every colour). Make its base colour near-white.
- Vertex colours (COLOR_0) multiply the base colour if present.

### Not yet

Textures are not read yet (Base Color images are ignored): use materials and vertex colours. Animation is not read either (wheels don't turn). Both are planned; the loader is built so adding them won't break existing files.

## Airport layout

An airport model is one 10 × 3 tile strip, drawn with its **north-west corner at the origin** (0, 0), the runway along +X. In Blender (Z up) the strip is X from 0 to 10 and Y from 0 to −3 (glTF's +Z is Blender's −Y). Rows, from the runway: row 0 (the runway, one tile deep), row 1 (the apron where planes park, at the middle, around X = 5), row 2 (the terminal and tower). The placeholder shows exactly this. The game rotates the whole strip when the player turns the runway.

## Checking your work

`cargo test` runs these on every model (they fail with a message that names the file):

- it loads, has triangles, valid indices and sane normals;
- it isn't bigger than a model of its kind should be, and stays above the ground;
- it's within its triangle budget;
- there is always a model for every kind in every year from 1990 to 2100 (so deleting the only 2040 bus would fail);
- chrome turns up when it should in the building generations.

In the game: the **F3 debug window** has a **Date** slider and buttons for 2000, 2012, 2025, 2038 and 2050. The slider plays the years forward (towns grow and are redeveloped, fleets are replaced) and winds back (re-aging what was built later). `STARCITY_YEAR=2046 cargo run --release -- --demo` starts in a year, `STARCITY_HOUR=22` starts at night.
