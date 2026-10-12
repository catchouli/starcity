#!/usr/bin/env python3
"""Writes the placeholder models in assets/models/ as glTF binary (.glb).

These are stand-ins for art. Artists replace any file with their own model
(see assets/models/README.md); the game picks up every .glb in the folder.
Run this script only to regenerate the placeholders:

    python3 tools/make_placeholder_models.py

Standard library only. Each model is one node per part (so it opens in Blender
as separate, named objects) under a root node carrying the game's metadata as
flat custom properties (glTF "extras").

Coordinates are glTF's (Y up): +X is the direction a vehicle faces, +Y is up,
the ground is y = 0, and 1 unit is 1 map tile.
"""

import json
import math
import os
import struct
import sys

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "models")


# --- Materials ----------------------------------------------------------------


def lin(c):
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def hexrgb(h):
    h = h.lstrip("#")
    return [lin(int(h[i : i + 2], 16)) for i in (0, 2, 4)]


MATERIALS = {}


def mat(name, hexcolor, metallic=0.0, roughness=0.85, emissive=0.0):
    """A PBR material. `emissive` > 0 makes it glow in its own colour."""
    MATERIALS[name] = dict(color=hexrgb(hexcolor), metallic=metallic, roughness=roughness, emissive=emissive)
    return name


# "Paint" is tinted by each vehicle's own colour in the game.
PAINT = mat("Paint", "#d8d8d8", roughness=0.5)
BLACK = mat("Black", "#1c1c20")
TYRE = mat("Tyre", "#17171a", roughness=0.95)
GLASS = mat("Glass", "#3d5a78", metallic=0.3, roughness=0.15)
GLASS2 = mat("GlassModern", "#4f86b8", metallic=0.55, roughness=0.1)
WHITE = mat("White", "#ececf0")
CONCRETE = mat("Concrete", "#9a9a9c")
DARK_CONCRETE = mat("DarkConcrete", "#6c6c70")
BRICK = mat("Brick", "#a85a40")
BRICK2 = mat("Brick2", "#8d4a38")
PLASTER = mat("Plaster", "#e6dcc4")
PLASTER2 = mat("Plaster2", "#d9cfae")
TIMBER = mat("Timber", "#b08a5a")
ROOF_RED = mat("RoofRed", "#8e3a2c")
ROOF_GREY = mat("RoofGrey", "#5c5e66")
ROOF_GREEN = mat("RoofGreen", "#465246")
SOLAR = mat("Solar", "#1c2a48", metallic=0.4, roughness=0.2)
STEEL = mat("Steel", "#b4b9c2", metallic=0.7, roughness=0.35)
CHROME = mat("Chrome", "#e8ecf2", metallic=1.0, roughness=0.06)
GOLD = mat("ChromeWarm", "#f0e2c0", metallic=1.0, roughness=0.1)
CHROME_BLUE = mat("ChromeBlue", "#9fc4ee", metallic=1.0, roughness=0.08)
GLOW = mat("Glow", "#7fe8ff", emissive=1.0)
GLOW_WARM = mat("GlowWarm", "#ffd890", emissive=1.0)
LAMP_RED = mat("LampRed", "#ff3a28", emissive=1.0)
LAMP_GREEN = mat("LampGreen", "#3aff5e", emissive=1.0)
ASPHALT = mat("Asphalt", "#3e3e44")
APRON = mat("Apron", "#707078")
LINE = mat("Line", "#ececec")
WOOD = mat("Wood", "#7c5c3e")
CRANE = mat("Crane", "#c87828")
HULL = mat("Hull", "#1e345c")
RED = mat("Red", "#be342a")
GREEN = mat("Green", "#2c6e44")
DARKGREEN = mat("DarkGreen", "#1c4a2e")


# --- Geometry -----------------------------------------------------------------


class Part:
    def __init__(self, name, material, positions, normals, indices):
        self.name, self.material = name, material
        self.positions, self.normals, self.indices = positions, normals, indices


def quad(p, n, a, b, c, d):
    """Appends a quad (a, b, c, d anticlockwise seen from outside, normal n)."""
    base = len(p["pos"])
    for v in (a, b, c, d):
        p["pos"].append(v)
        p["nrm"].append(n)
    p["idx"] += [base, base + 1, base + 2, base, base + 2, base + 3]


def new():
    return {"pos": [], "nrm": [], "idx": []}


def finish(name, material, p):
    return Part(name, material, p["pos"], p["nrm"], p["idx"])


def box(name, material, c, s):
    """An axis-aligned box centred at c with full size s."""
    (cx, cy, cz), (sx, sy, sz) = c, [v / 2 for v in s]
    x0, x1, y0, y1, z0, z1 = cx - sx, cx + sx, cy - sy, cy + sy, cz - sz, cz + sz
    p = new()
    quad(p, (1, 0, 0), (x1, y0, z1), (x1, y0, z0), (x1, y1, z0), (x1, y1, z1))
    quad(p, (-1, 0, 0), (x0, y0, z0), (x0, y0, z1), (x0, y1, z1), (x0, y1, z0))
    quad(p, (0, 1, 0), (x0, y1, z1), (x1, y1, z1), (x1, y1, z0), (x0, y1, z0))
    quad(p, (0, -1, 0), (x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1))
    quad(p, (0, 0, 1), (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1))
    quad(p, (0, 0, -1), (x1, y0, z0), (x0, y0, z0), (x0, y1, z0), (x1, y1, z0))
    return finish(name, material, p)


def gable(name, material, c, s):
    """A gable roof (triangular prism) with its ridge along X."""
    (cx, cy, cz), (sx, sy, sz) = c, [v / 2 for v in s]
    x0, x1, y0, y1, z0, z1 = cx - sx, cx + sx, cy - sy, cy + sy, cz - sz, cz + sz
    p = new()
    k = math.hypot(sy * 2, sz)
    nz = (0, sz / k, (sy * 2) / k)
    quad(p, nz, (x0, y0, z1), (x1, y0, z1), (x1, y1, cz), (x0, y1, cz))
    nz2 = (0, sz / k, -(sy * 2) / k)
    quad(p, nz2, (x1, y0, z0), (x0, y0, z0), (x0, y1, cz), (x1, y1, cz))
    # End triangles.
    base = len(p["pos"])
    for v in ((x1, y0, z1), (x1, y0, z0), (x1, y1, cz)):
        p["pos"].append(v)
        p["nrm"].append((1, 0, 0))
    p["idx"] += [base, base + 1, base + 2]
    base = len(p["pos"])
    for v in ((x0, y0, z0), (x0, y0, z1), (x0, y1, cz)):
        p["pos"].append(v)
        p["nrm"].append((-1, 0, 0))
    p["idx"] += [base, base + 1, base + 2]
    return finish(name, material, p)


def sphere(name, material, c, r, lat=10, lon=20, half=False):
    """An ellipsoid with smooth normals; `half` keeps only the upper half (a dome)."""
    (cx, cy, cz), (rx, ry, rz) = c, r if isinstance(r, (tuple, list)) else (r, r, r)
    p = new()
    top = math.pi / 2 if half else math.pi
    for i in range(lat + 1):
        th = top * i / lat
        for j in range(lon + 1):
            ph = 2 * math.pi * j / lon
            nx, ny, nz = math.sin(th) * math.cos(ph), math.cos(th), math.sin(th) * math.sin(ph)
            p["pos"].append((cx + nx * rx, cy + ny * ry, cz + nz * rz))
            ln = math.sqrt((nx / rx) ** 2 + (ny / ry) ** 2 + (nz / rz) ** 2)
            p["nrm"].append((nx / rx / ln, ny / ry / ln, nz / rz / ln))
    for i in range(lat):
        for j in range(lon):
            a = i * (lon + 1) + j
            b = a + lon + 1
            # Seen from outside the order is a, a+1, b+1, b for increasing theta from the top.
            p["idx"] += [a, b + 1, a + 1, a, b, b + 1]
    return finish(name, material, p)


def cyl(name, material, c, r, h, axis="y", seg=16, rtop=None):
    """A cylinder (or a cone frustum if `rtop` is given) along an axis."""
    cx, cy, cz = c
    rt = r if rtop is None else rtop
    p = new()

    def pt(a, t, rad):
        u, v = math.cos(a) * rad, math.sin(a) * rad
        s = t * h - h / 2
        if axis == "y":
            return (cx + u, cy + s, cz + v)
        if axis == "x":
            return (cx + s, cy + u, cz + v)
        return (cx + u, cy + v, cz + s)

    def dirn(a):
        u, v = math.cos(a), math.sin(a)
        return {"y": (u, 0, v), "x": (0, u, v), "z": (u, v, 0)}[axis]

    for i in range(seg):
        a0, a1 = 2 * math.pi * i / seg, 2 * math.pi * (i + 1) / seg
        base = len(p["pos"])
        for a, t, rad in ((a0, 0, r), (a1, 0, r), (a1, 1, rt), (a0, 1, rt)):
            p["pos"].append(pt(a, t, rad))
            p["nrm"].append(dirn(a))
        # Wind so the outside faces out whichever axis it is on.
        p["idx"] += [base, base + 1, base + 2, base, base + 2, base + 3] if axis != "y" else [base, base + 2, base + 1, base, base + 3, base + 2]
    axis_n = {"y": (0, 1, 0), "x": (1, 0, 0), "z": (0, 0, 1)}[axis]
    for t, rad, sign in ((1, rt, 1), (0, r, -1)):
        centre = pt(0, t, 0)
        base = len(p["pos"])
        p["pos"].append(centre)
        p["nrm"].append(tuple(sign * v for v in axis_n))
        for i in range(seg + 1):
            a = 2 * math.pi * i / seg
            p["pos"].append(pt(a, t, rad))
            p["nrm"].append(tuple(sign * v for v in axis_n))
        for i in range(seg):
            if (sign > 0) == (axis != "y"):
                p["idx"] += [base, base + 1 + i, base + 2 + i]
            else:
                p["idx"] += [base, base + 2 + i, base + 1 + i]
    return finish(name, material, p)


# --- glTF writer -----------------------------------------------------------------


def write_glb(path, name, parts, meta):
    used = []
    for part in parts:
        if part.material not in used:
            used.append(part.material)
    buf = bytearray()
    views, accessors, meshes, nodes = [], [], [], []

    def add_view(data, target):
        while len(buf) % 4:
            buf.append(0)
        views.append({"buffer": 0, "byteOffset": len(buf), "byteLength": len(data), "target": target})
        buf.extend(data)
        return len(views) - 1

    for part in parts:
        pos = b"".join(struct.pack("<3f", *v) for v in part.positions)
        nrm = b"".join(struct.pack("<3f", *v) for v in part.normals)
        idx = b"".join(struct.pack("<I", i) for i in part.indices)
        lo = [min(v[k] for v in part.positions) for k in range(3)]
        hi = [max(v[k] for v in part.positions) for k in range(3)]
        accessors.append({"bufferView": add_view(pos, 34962), "componentType": 5126, "count": len(part.positions), "type": "VEC3", "min": lo, "max": hi})
        a_pos = len(accessors) - 1
        accessors.append({"bufferView": add_view(nrm, 34962), "componentType": 5126, "count": len(part.normals), "type": "VEC3"})
        a_nrm = len(accessors) - 1
        accessors.append({"bufferView": add_view(idx, 34963), "componentType": 5125, "count": len(part.indices), "type": "SCALAR"})
        a_idx = len(accessors) - 1
        meshes.append({"name": part.name, "primitives": [{"attributes": {"POSITION": a_pos, "NORMAL": a_nrm}, "indices": a_idx, "material": used.index(part.material), "mode": 4}]})
        nodes.append({"name": part.name, "mesh": len(meshes) - 1})

    root = {"name": name, "children": list(range(len(parts))), "extras": meta}
    nodes.append(root)
    gltf = {
        "asset": {"version": "2.0", "generator": "starcity tools/make_placeholder_models.py"},
        "scene": 0,
        "scenes": [{"name": name, "nodes": [len(nodes) - 1]}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": [pbr(m) for m in used],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(buf)}],
    }
    js = json.dumps(gltf, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    buf.extend(b"\0" * (-len(buf) % 4))
    out = struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(buf))
    out += struct.pack("<II", len(js), 0x4E4F534A) + js
    out += struct.pack("<II", len(buf), 0x004E4942) + bytes(buf)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(out)


def pbr(name):
    m = MATERIALS[name]
    d = {"name": name, "pbrMetallicRoughness": {"baseColorFactor": m["color"] + [1.0], "metallicFactor": m["metallic"], "roughnessFactor": m["roughness"]}}
    if m["emissive"] > 0:
        d["emissiveFactor"] = [min(1.0, c * m["emissive"]) for c in m["color"]]
    return d


MODELS = []


def model(path, role, kind, parts, **meta):
    """Registers a model. Metadata is written as flat custom properties:
    role, kind, from/to (years it is built in), floors, tiles, weight."""
    extras = {"role": role, "kind": kind}
    for k, v in meta.items():
        extras[k] = v
    MODELS.append((path, parts, extras))


# --- Buildings (each centred on its footprint; 1 tile = 1 unit) ---------------------


def windows_band(prefix, mat_, w, d, y0, floors, floor_h, inset=0.0):
    """Dark window strips on all four faces of a w x d block."""
    parts = []
    for i in range(floors):
        y = y0 + floor_h * (i + 0.5)
        parts.append(box(f"{prefix}_win_f{i}", mat_, (0, y, 0), (w + 0.012 - inset, floor_h * 0.45, d + 0.012 - inset)))
    return parts


def house(name, years, walls, roof, floors, pitched=True, variant=0):
    h = 0.30 * floors
    parts = [box("Walls", walls, (0, h / 2, 0), (0.62, h, 0.58))]
    if pitched:
        parts.append(gable("Roof", roof, (0, h + 0.10, 0), (0.70, 0.20, 0.66)))
    else:
        parts.append(box("Roof", roof, (0, h + 0.03, 0), (0.68, 0.06, 0.64)))
    parts.append(box("Door", WOOD, (0.0, 0.10, 0.296), (0.10, 0.20, 0.02)))
    parts.append(box("Window", GLASS, (0.2, 0.13 + (0.3 if floors > 1 else 0), 0.296), (0.12, 0.10, 0.02)))
    if variant == 1:
        parts.append(box("Garage", CONCRETE, (-0.32, 0.12, 0.2), (0.16, 0.24, 0.28)))
    model(name, "building", "house", parts, **{"from": years[0], "to": years[1], "min_floors": 1, "max_floors": 2, "tiles_x": 1, "tiles_z": 1, "weight": 1})


def block(name, years, walls, roof, floors_range, glass=GLASS, w=0.74, metallic_trim=None):
    lo, hi = floors_range
    n = min(hi, lo + 2)
    fh = 0.28
    h = n * fh
    parts = [box("Body", walls, (0, h / 2, 0), (w, h, w))]
    parts += windows_band("Body", glass, w, w, 0.04, n, fh)
    parts.append(box("Roof", roof, (0, h + 0.03, 0), (w + 0.04, 0.06, w + 0.04)))
    if metallic_trim:
        parts.append(box("Trim", metallic_trim, (0, h + 0.09, 0), (w * 0.5, 0.06, w * 0.5)))
    model(name, "building", "block", parts, **{"from": years[0], "to": years[1], "min_floors": lo, "max_floors": hi, "tiles_x": 1, "tiles_z": 1, "weight": 1})


def tower(name, years, walls, glass, floors_range, round_=False, rooftop=None, w=0.7):
    lo, hi = floors_range
    n = min(hi, 12) if hi < 99 else 12
    fh = 0.30
    h = n * fh
    parts = []
    if round_:
        parts.append(cyl("Shaft", walls, (0, h / 2, 0), w / 2, h, seg=20))
        for i in range(n):
            parts.append(cyl(f"Band_{i}", glass, (0, 0.2 + fh * i, 0), w / 2 + 0.006, fh * 0.4, seg=20))
        parts.append(cyl("Crown", rooftop or STEEL, (0, h + 0.05, 0), w / 2 + 0.03, 0.1, seg=20))
    else:
        parts.append(box("Shaft", walls, (0, h / 2, 0), (w, h, w)))
        parts += windows_band("Shaft", glass, w, w, 0.04, n, fh)
        parts.append(box("Crown", rooftop or STEEL, (0, h + 0.05, 0), (w * 0.8, 0.1, w * 0.8)))
        parts.append(box("Mast", STEEL, (0, h + 0.3, 0), (0.03, 0.5, 0.03)))
    model(name, "building", "tower", parts, **{"from": years[0], "to": years[1], "min_floors": lo, "max_floors": hi, "tiles_x": 1, "tiles_z": 1, "weight": 1})


def pod_house(name, years):
    parts = [sphere("Dome", CHROME, (0, 0, 0), (0.34, 0.36, 0.34), half=True)]
    parts.append(cyl("Base", DARK_CONCRETE, (0, 0.04, 0), 0.36, 0.08))
    parts.append(box("Door", GLOW, (0.31, 0.12, 0), (0.04, 0.16, 0.08)))
    parts.append(sphere("Cap", GOLD, (0, 0.34, 0), (0.08, 0.05, 0.08)))
    model(name, "building", "house", parts, **{"from": years[0], "to": years[1], "min_floors": 1, "max_floors": 2, "tiles_x": 1, "tiles_z": 1, "weight": 1})


def orb(name, years, tiles, radius, material=CHROME, floors=(8, 99)):
    t = tiles
    r = radius
    parts = [cyl("Plinth", DARK_CONCRETE, (0, 0.06, 0), t * 0.5 - 0.04, 0.12, seg=24)]
    parts.append(sphere("Orb", material, (0, 0.12 + r * 0.92, 0), r, lat=16, lon=28))
    parts.append(cyl("Ring", CHROME_BLUE, (0, 0.12 + r * 0.92, 0), r * 1.12, 0.05, seg=36))
    parts.append(box("Gate", GLOW, (r * 0.9, 0.2, 0), (0.06, 0.2, 0.2)))
    parts.append(cyl("Spire", STEEL, (0, 0.12 + r * 1.84 + 0.2, 0), 0.025, 0.5, seg=8))
    model(name, "building", "orb", parts, **{"from": years[0], "to": years[1], "min_floors": floors[0], "max_floors": floors[1], "tiles_x": tiles, "tiles_z": tiles, "weight": 1})


def landmark(name, years, tiles=2):
    t = tiles
    parts = []
    y = 0.0
    for i, (s, mtl) in enumerate([(0.9, STEEL), (0.78, CHROME), (0.64, STEEL), (0.5, CHROME), (0.36, STEEL)]):
        h = 0.5
        parts.append(box(f"Tier_{i}", mtl, (0.04 * i, y + h / 2, 0.03 * i), (s * t * 0.5, h, s * t * 0.5)))
        parts.append(box(f"TierGlass_{i}", GLASS2, (0.04 * i, y + h / 2, 0.03 * i), (s * t * 0.5 + 0.012, h * 0.4, s * t * 0.5 + 0.012)))
        y += h
    parts.append(cyl("Spire", CHROME, (0.2, y + 0.4, 0.15), 0.03, 0.8, seg=8, rtop=0.005))
    model(name, "building", "landmark", parts, **{"from": years[0], "to": years[1], "min_floors": 8, "max_floors": 99, "tiles_x": t, "tiles_z": t, "weight": 1})


# Late 20th century.
house("buildings/house_1970", (1900, 1984), BRICK, ROOF_GREY, 1, pitched=False)
house("buildings/house_1985", (1985, 1999), PLASTER2, ROOF_RED, 2, variant=1)
block("buildings/block_1975", (1950, 1999), CONCRETE, DARK_CONCRETE, (3, 5), glass=BLACK)
block("buildings/office_1990", (1985, 1999), CONCRETE, ROOF_GREY, (6, 99), glass=GLASS, w=0.7)
# Dot-com: 2000-2011.
house("buildings/house_2000_a", (2000, 2011), PLASTER, ROOF_RED, 2)
house("buildings/house_2000_b", (2000, 2011), BRICK2, ROOF_GREEN, 1, variant=1)
block("buildings/block_2000", (2000, 2011), PLASTER2, ROOF_GREY, (3, 5))
tower("buildings/office_2000", (2000, 2011), CONCRETE, GLASS, (6, 99))
# Broadband and smartphones: 2012-2024.
house("buildings/house_2012", (2012, 2024), WHITE, ROOF_GREY, 2, pitched=False, variant=1)
block("buildings/block_2012", (2012, 2024), WHITE, ROOF_GREY, (3, 5), glass=GLASS2, metallic_trim=SOLAR)
tower("buildings/tower_2015", (2012, 2024), WHITE, GLASS2, (6, 99), rooftop=SOLAR)
# Chrome dawn: 2025-2037.
house("buildings/house_2025", (2025, 2037), WHITE, SOLAR, 2, pitched=False, variant=1)
block("buildings/block_2025", (2025, 2037), STEEL, SOLAR, (3, 5), glass=GLASS2, metallic_trim=CHROME)
tower("buildings/tower_2028", (2025, 2037), CHROME, GLASS2, (6, 99), rooftop=CHROME)
landmark("buildings/landmark_2030", (2028, 2060), tiles=2)
# Chrome age: 2038 on.
pod_house("buildings/pod_2038", (2038, 2100))
block("buildings/block_2038", (2038, 2100), CHROME, CHROME_BLUE, (3, 5), glass=GLASS2, metallic_trim=GOLD)
tower("buildings/tower_2040", (2038, 2100), CHROME, GLOW, (6, 99), round_=True, rooftop=GOLD)
orb("buildings/orb_2x2_2040", (2036, 2100), 2, 0.8, material=CHROME)
orb("buildings/orb_3x3_2046", (2042, 2100), 3, 1.3, material=CHROME_BLUE)


# --- Road vehicles (face +X, about 0.3-0.7 tiles long) ---------------------------------


def wheels(prefix, xs, z, r=0.04, y=0.04):
    return [cyl(f"{prefix}_w{i}_{s}", TYRE, (x, y, z * s), r, 0.03, axis="z", seg=10) for i, x in enumerate(xs) for s in (-1, 1)]


def car(name, years, body_len=0.28, style=0):
    parts = []
    if style == 0:  # boxy hatchback
        parts += [box("Body", PAINT, (0, 0.07, 0), (body_len, 0.07, 0.15)), box("Cabin", GLASS, (-0.02, 0.135, 0), (body_len * 0.55, 0.06, 0.13))]
    elif style == 1:  # rounded hybrid
        parts += [box("Body", PAINT, (0, 0.07, 0), (body_len, 0.065, 0.15)), sphere("Cabin", GLASS, (-0.01, 0.12, 0), (body_len * 0.3, 0.05, 0.065), lat=6, lon=12, half=True)]
    elif style == 2:  # glossy EV
        parts += [box("Body", PAINT, (0, 0.065, 0), (body_len, 0.055, 0.15)), sphere("Canopy", GLASS2, (0, 0.1, 0), (body_len * 0.38, 0.06, 0.07), lat=6, lon=14, half=True), box("LightBar", GLOW, (body_len / 2, 0.08, 0), (0.01, 0.01, 0.12))]
    else:  # chrome pod
        parts += [sphere("Pod", CHROME, (0, 0.085, 0), (body_len * 0.5, 0.07, 0.085), lat=8, lon=16), box("Glow", GLOW, (0, 0.03, 0), (body_len * 0.7, 0.01, 0.11))]
    parts += wheels("car", [-0.08, 0.08], 0.082) if style < 3 else []
    model(name, "road_vehicle", "car", parts, **{"from": years[0], "to": years[1], "weight": 1})


def bus(name, years, style=0):
    parts = []
    if style == 0:
        parts += [box("Body", PAINT, (0, 0.12, 0), (0.58, 0.18, 0.17)), box("Windows", GLASS, (0, 0.15, 0), (0.56, 0.06, 0.172))]
    elif style == 1:
        parts += [box("Body", PAINT, (0, 0.12, 0), (0.62, 0.17, 0.17)), box("Windows", GLASS2, (0, 0.15, 0), (0.60, 0.06, 0.172)), box("Roof", SOLAR, (0, 0.215, 0), (0.4, 0.015, 0.12))]
    elif style == 2:
        parts += [box("Body", PAINT, (0, 0.12, 0), (0.66, 0.17, 0.17)), box("Windows", GLASS2, (0, 0.15, 0), (0.64, 0.06, 0.172)), box("Glow", GLOW, (0.33, 0.1, 0), (0.01, 0.02, 0.15))]
    else:
        parts += [sphere("Pod", CHROME, (0, 0.14, 0), (0.34, 0.12, 0.1), lat=8, lon=18), box("Glow", GLOW, (0, 0.05, 0), (0.5, 0.012, 0.13)), box("Paint", PAINT, (0, 0.14, 0), (0.3, 0.02, 0.106))]
    parts += wheels("bus", [-0.2, 0.2], 0.09, 0.045, 0.045) if style < 3 else []
    model(name, "road_vehicle", "bus", parts, **{"from": years[0], "to": years[1], "weight": 1})


def truck(name, years, style=0):
    parts = []
    if style == 0:
        parts += [box("Cab", PAINT, (0.18, 0.11, 0), (0.15, 0.15, 0.16)), box("Windscreen", GLASS, (0.255, 0.14, 0), (0.01, 0.06, 0.14)), box("Cargo", WHITE, (-0.08, 0.13, 0), (0.34, 0.18, 0.17))]
    elif style == 1:
        parts += [box("Cab", PAINT, (0.2, 0.11, 0), (0.16, 0.16, 0.16)), box("Windscreen", GLASS2, (0.28, 0.14, 0), (0.01, 0.07, 0.14)), box("Cargo", STEEL, (-0.08, 0.13, 0), (0.38, 0.18, 0.17))]
    elif style == 2:  # autonomous: no cab, a sensor bar
        parts += [box("Body", PAINT, (0, 0.12, 0), (0.6, 0.17, 0.17)), box("Sensors", GLOW, (0.30, 0.15, 0), (0.01, 0.015, 0.14)), box("Cargo", STEEL, (-0.05, 0.22, 0), (0.45, 0.05, 0.15))]
    else:
        parts += [sphere("Hauler", CHROME, (0, 0.12, 0), (0.34, 0.1, 0.1), lat=8, lon=18), box("Glow", GLOW, (0, 0.05, 0), (0.56, 0.012, 0.13)), box("Paint", PAINT, (0, 0.12, 0), (0.4, 0.02, 0.106))]
    parts += wheels("truck", [-0.22, -0.06, 0.2] if style < 3 else [], 0.09, 0.045, 0.045)
    model(name, "road_vehicle", "truck", parts, **{"from": years[0], "to": years[1], "weight": 1})


car("vehicles/car_1995", (1990, 2004), style=0)
car("vehicles/car_2005", (2005, 2017), style=1)
car("vehicles/car_2018", (2018, 2033), style=2)
car("vehicles/car_2034", (2034, 2100), style=3)
bus("vehicles/bus_1998", (1990, 2009), style=0)
bus("vehicles/bus_2010", (2010, 2023), style=1)
bus("vehicles/bus_2024", (2024, 2037), style=2)
bus("vehicles/bus_2038", (2038, 2100), style=3)
truck("vehicles/truck_1998", (1990, 2011), style=0)
truck("vehicles/truck_2012", (2012, 2027), style=1)
truck("vehicles/truck_2028", (2028, 2041), style=2)
truck("vehicles/truck_2042", (2042, 2100), style=3)


# --- Train cars (0.8 long, face +X, centred; the origin is the middle of the car) ----------


def loco(name, years, style):
    L = 0.8
    parts = [box("Chassis", BLACK, (0, 0.08, 0), (L - 0.08, 0.05, 0.22))]
    if style == 0:  # diesel
        parts += [box("Body", PAINT, (0, 0.2, 0), (L - 0.1, 0.17, 0.25)), box("Cab", GLASS, (0.3, 0.24, 0), (0.1, 0.07, 0.25)), box("Stripe", YELLOW(), (0.0, 0.19, 0), (L - 0.1, 0.03, 0.252)), box("Fan", STEEL, (-0.22, 0.31, 0), (0.18, 0.03, 0.18))]
    elif style == 1:  # electric
        parts += [box("Body", PAINT, (0, 0.2, 0), (L - 0.1, 0.17, 0.25)), box("Nose", PAINT, (0.34, 0.18, 0), (0.1, 0.12, 0.23)), box("Windscreen", GLASS2, (0.37, 0.22, 0), (0.02, 0.06, 0.2)), box("Pantograph", STEEL, (-0.05, 0.34, 0), (0.1, 0.08, 0.03)), box("Wire", BLACK, (-0.05, 0.38, 0), (0.12, 0.012, 0.18))]
    elif style == 2:  # maglev
        parts += [sphere("Nose", CHROME, (0.2, 0.19, 0), (0.28, 0.12, 0.13), lat=8, lon=16), box("Body", PAINT, (-0.12, 0.19, 0), (0.5, 0.2, 0.25)), box("Glow", GLOW, (0, 0.1, 0), (L - 0.1, 0.012, 0.26)), box("Skirt", STEEL, (0, 0.1, 0), (L - 0.06, 0.04, 0.2))]
    else:  # pod train
        parts += [sphere("Capsule", CHROME, (0, 0.19, 0), (L / 2 - 0.02, 0.13, 0.14), lat=10, lon=20), box("Glow", GLOW, (0, 0.09, 0), (L - 0.1, 0.012, 0.22)), box("Paint", PAINT, (0, 0.19, 0), (0.5, 0.02, 0.2))]
    model(name, "train_car", "loco", parts, **{"from": years[0], "to": years[1], "weight": 1})


def coach(name, years, style):
    L = 0.8
    parts = [box("Chassis", BLACK, (0, 0.08, 0), (L - 0.08, 0.05, 0.22))]
    if style == 0:
        parts += [box("Body", PAINT, (0, 0.21, 0), (L - 0.04, 0.2, 0.24)), box("Windows", GLASS, (0, 0.25, 0), (L - 0.08, 0.06, 0.242))]
    elif style == 1:
        parts += [box("Body", PAINT, (0, 0.21, 0), (L - 0.04, 0.2, 0.24)), box("Windows", GLASS2, (0, 0.25, 0), (L - 0.06, 0.07, 0.242)), box("Roof", SOLAR, (0, 0.32, 0), (0.5, 0.012, 0.16))]
    elif style == 2:
        parts += [box("Body", PAINT, (0, 0.2, 0), (L - 0.04, 0.2, 0.25)), box("Windows", GLASS2, (0, 0.24, 0), (L - 0.06, 0.07, 0.252)), box("Glow", GLOW, (0, 0.1, 0), (L - 0.05, 0.012, 0.26))]
    else:
        parts += [sphere("Capsule", CHROME, (0, 0.19, 0), (L / 2 - 0.02, 0.13, 0.14), lat=10, lon=20), box("Glow", GLOW, (0, 0.09, 0), (L - 0.1, 0.012, 0.22)), box("Paint", PAINT, (0, 0.19, 0), (0.5, 0.02, 0.2))]
    model(name, "train_car", "coach", parts, **{"from": years[0], "to": years[1], "weight": 1})


def YELLOW():
    return mat("Yellow", "#e8c020")


loco("trains/loco_diesel_1995", (1980, 2011), 0)
loco("trains/loco_electric_2012", (2012, 2029), 1)
loco("trains/loco_maglev_2030", (2030, 2041), 2)
loco("trains/loco_pod_2042", (2042, 2100), 3)
coach("trains/coach_1995", (1980, 2011), 0)
coach("trains/coach_2012", (2012, 2029), 1)
coach("trains/coach_2030", (2030, 2041), 2)
coach("trains/coach_2042", (2042, 2100), 3)


# --- Boats and planes (face +X) -------------------------------------------------------


def ferry(name, years, style):
    if style == 0:
        parts = [box("Hull", HULL, (0, 0.06, 0), (0.72, 0.12, 0.28)), box("Bow", HULL, (0.3, 0.05, 0), (0.2, 0.08, 0.2)), box("Cabin", WHITE, (-0.06, 0.17, 0), (0.28, 0.12, 0.22)), box("Windows", GLASS, (-0.06, 0.2, 0), (0.286, 0.04, 0.222)), cyl("Funnel", RED, (-0.12, 0.3, 0), 0.04, 0.12, seg=10)]
    elif style == 1:  # catamaran
        parts = [box("HullL", HULL, (0, 0.06, -0.1), (0.76, 0.12, 0.09)), box("HullR", HULL, (0, 0.06, 0.1), (0.76, 0.12, 0.09)), box("Deck", WHITE, (0, 0.17, 0), (0.62, 0.1, 0.3)), box("Windows", GLASS2, (0, 0.2, 0), (0.64, 0.04, 0.302))]
    else:  # hydrofoil
        parts = [sphere("Hull", CHROME, (0, 0.13, 0), (0.4, 0.09, 0.15), lat=8, lon=16), box("Foil", STEEL, (0.0, 0.03, 0), (0.1, 0.02, 0.4)), box("Glow", GLOW, (0, 0.1, 0), (0.7, 0.012, 0.18))]
    model(name, "boat", "ferry", parts, **{"from": years[0], "to": years[1], "weight": 1})


def plane(name, years, style):
    if style == 0:  # turboprop
        parts = [cyl("Fuselage", WHITE, (0, 0.12, 0), 0.06, 0.8, axis="x", seg=12), box("Wing", STEEL, (0.02, 0.12, 0), (0.18, 0.015, 0.9)), box("Tail", RED, (-0.36, 0.22, 0), (0.1, 0.16, 0.015)), box("TailPlane", STEEL, (-0.36, 0.14, 0), (0.1, 0.012, 0.3)), cyl("EngineL", STEEL, (0.08, 0.1, -0.22), 0.025, 0.14, axis="x", seg=8), cyl("EngineR", STEEL, (0.08, 0.1, 0.22), 0.025, 0.14, axis="x", seg=8)]
    elif style == 1:  # jet
        parts = [cyl("Fuselage", WHITE, (0, 0.12, 0), 0.06, 0.86, axis="x", seg=12), box("Wing", STEEL, (-0.02, 0.1, 0), (0.22, 0.015, 0.84)), box("Tail", RED, (-0.38, 0.23, 0), (0.1, 0.17, 0.015)), box("TailPlane", STEEL, (-0.38, 0.14, 0), (0.1, 0.012, 0.3)), cyl("EngineL", STEEL, (0.0, 0.07, -0.2), 0.035, 0.18, axis="x", seg=10), cyl("EngineR", STEEL, (0.0, 0.07, 0.2), 0.035, 0.18, axis="x", seg=10)]
    elif style == 2:  # flying wing
        parts = [sphere("Wing", WHITE, (0, 0.1, 0), (0.42, 0.045, 0.5), lat=6, lon=16), box("Glow", GLOW, (0.2, 0.1, 0), (0.02, 0.012, 0.5)), box("Fin", STEEL, (-0.2, 0.17, 0), (0.1, 0.08, 0.015))]
    else:  # chrome
        parts = [sphere("Fuselage", CHROME, (0, 0.12, 0), (0.48, 0.07, 0.08), lat=8, lon=16), sphere("Wing", CHROME_BLUE, (-0.04, 0.115, 0), (0.18, 0.02, 0.5), lat=6, lon=16), box("Glow", GLOW, (0.4, 0.12, 0), (0.02, 0.02, 0.1))]
    model(name, "plane", "airliner", parts, **{"from": years[0], "to": years[1], "weight": 1})


ferry("boats/ferry_1995", (1980, 2014), 0)
ferry("boats/ferry_2015", (2015, 2033), 1)
ferry("boats/ferry_2034", (2034, 2100), 2)
plane("planes/plane_1995", (1980, 2009), 0)
plane("planes/plane_2010", (2010, 2027), 1)
plane("planes/plane_2028", (2028, 2041), 2)
plane("planes/plane_2042", (2042, 2100), 3)


# --- Terminals ----------------------------------------------------------------------------
# A port faces +X: the water (berth) is on the +X side of its tile, centred on the tile.


def port(name, years, style):
    modern = style > 0
    parts = [box("Quay", CONCRETE, (0, 0.03, 0), (1.0, 0.06, 1.0)), box("Warehouse", mat("Warehouse", "#a88060") if not modern else STEEL, (-0.14, 0.22, 0), (0.56, 0.32, 0.56)), box("WarehouseRoof", ROOF_GREY if not modern else SOLAR, (-0.14, 0.4, 0), (0.62, 0.05, 0.62))]
    parts += [box("CraneMast", CRANE, (0.32, 0.4, 0), (0.05, 0.8, 0.05)), box("CraneJib", CRANE, (0.46, 0.78, 0), (0.4, 0.04, 0.05))]
    if modern:
        parts += [box("Glow", GLOW, (0.5, 0.075, 0.45), (0.9, 0.012, 0.012))]
    model(name, "terminal", "port", parts, **{"from": years[0], "to": years[1], "weight": 1})


def airport(name, years, style):
    # Local frame: u along the runway (0..10), v across (0..3); the origin is the footprint's
    # north-west corner. Row 0 is the runway, row 1 the apron, row 2 the terminal row.
    modern = style > 0
    L = 10.0
    parts = [box("Runway", ASPHALT, (L / 2, 0.02, 0.5), (L - 0.1, 0.03, 0.84)), box("Apron", APRON, (L / 2, 0.014, 1.5), (6.4, 0.024, 0.92)), box("TaxiwayW", mat("Taxiway", "#60606a"), (1.0, 0.014, 1.0), (0.44, 0.024, 1.04)), box("TaxiwayE", mat("Taxiway", "#60606a"), (L - 1.0, 0.014, 1.0), (0.44, 0.024, 1.04))]
    for i in range(5):
        parts.append(box(f"Dash_{i}", LINE, (1.6 + i * 1.7, 0.037, 0.5), (0.7, 0.008, 0.05)))
    if modern:
        parts += [box("Terminal", CHROME, (L / 2 - 1.2, 0.22, 2.5), (3.2, 0.4, 0.6)), box("TerminalGlass", GLASS2, (L / 2 - 1.2, 0.24, 2.5), (3.22, 0.16, 0.602)), sphere("TerminalDome", CHROME_BLUE, (L / 2 - 1.2, 0.42, 2.5), (1.5, 0.3, 0.32), half=True, lat=6, lon=16), cyl("Tower", CHROME, (L / 2 + 2.2, 0.45, 2.5), 0.1, 0.9, seg=12), sphere("Cab", GLOW, (L / 2 + 2.2, 0.95, 2.5), (0.2, 0.1, 0.2), half=True, lat=6, lon=14)]
    else:
        parts += [box("Terminal", mat("TerminalWall", "#d7d7de"), (L / 2 - 1.2, 0.2, 2.5), (3.2, 0.4, 0.6)), box("TerminalRoof", mat("TerminalRoof", "#5a6482"), (L / 2 - 1.2, 0.42, 2.5), (3.32, 0.04, 0.68)), box("Tower", mat("TowerWall", "#c8c8cd"), (L / 2 + 2.2, 0.38, 2.5), (0.24, 0.76, 0.24)), box("TowerCab", mat("TowerGlass", "#3c5a78", metallic=0.3, roughness=0.2), (L / 2 + 2.2, 0.84, 2.5), (0.4, 0.14, 0.4))]
    model(name, "terminal", "airport", parts, **{"from": years[0], "to": years[1], "tiles_x": 10, "tiles_z": 3, "weight": 1})


port("terminals/port_1995", (1980, 2029), 0)
port("terminals/port_2030", (2030, 2100), 1)
airport("terminals/airport_1995", (1980, 2029), 0)
airport("terminals/airport_2030", (2030, 2100), 1)


# --- Props ------------------------------------------------------------------------------------
# Station platforms run along X. A signal stands at the tile centre, lamp towards +X.


def station_platform(name, years, style):
    modern = style > 0
    parts = [box("PlatformL", CONCRETE if not modern else STEEL, (0, 0.07, -0.4), (1.0, 0.14, 0.18)), box("PlatformR", CONCRETE if not modern else STEEL, (0, 0.07, 0.4), (1.0, 0.14, 0.18))]
    if modern:
        parts += [box("EdgeGlowL", GLOW, (0, 0.145, -0.31), (1.0, 0.01, 0.01)), box("EdgeGlowR", GLOW, (0, 0.145, 0.31), (1.0, 0.01, 0.01))]
    model(name, "prop", "station_platform", parts, **{"from": years[0], "to": years[1], "weight": 1})


def station_building(name, years, style):
    modern = style > 0
    if not modern:
        parts = [box("Walls", mat("StationWall", "#decda5"), (0, 0.3, 0), (0.84, 0.48, 0.4)), box("Roof", mat("StationRoof", "#963e32"), (0, 0.58, 0), (0.96, 0.1, 0.52)), box("Door", WOOD, (0, 0.14, -0.205), (0.18, 0.28, 0.02))]
    else:
        parts = [box("Body", CHROME, (0, 0.28, 0), (0.84, 0.44, 0.4)), box("Glass", GLASS2, (0, 0.3, 0), (0.86, 0.2, 0.402)), sphere("Roof", CHROME_BLUE, (0, 0.5, 0), (0.46, 0.14, 0.24), half=True, lat=6, lon=14)]
    model(name, "prop", "station_building", parts, **{"from": years[0], "to": years[1], "weight": 1})


def signal(name, lamp):
    # Stands at the side of the track; the lamp faces back along it (-X), towards approaching trains.
    parts = [box("Post", STEEL, (0, 0.45, 0), (0.07, 0.9, 0.07)), box("Head", BLACK, (0, 0.98, 0), (0.12, 0.32, 0.18)), box("Lamp", lamp, (-0.065, 1.04, 0), (0.024, 0.13, 0.13))]
    model(name, "prop", "signal_red" if lamp == LAMP_RED else "signal_green", parts, **{"from": 1900, "to": 2100, "weight": 1})


def pier(name, years, style):
    # Runs along X, centred; the game puts it at sea level, reaching out from a port.
    parts = [box("Deck", WOOD if style == 0 else STEEL, (0, 0.07, 0), (0.9, 0.04, 0.26))]
    for i, x in enumerate((-0.4, 0.0, 0.4)):
        parts += [box(f"Post_{i}_L", DARK_CONCRETE, (x, 0.0, -0.11), (0.04, 0.2, 0.04)), box(f"Post_{i}_R", DARK_CONCRETE, (x, 0.0, 0.11), (0.04, 0.2, 0.04))]
    if style:
        parts.append(box("Glow", GLOW, (0, 0.095, 0.12), (0.9, 0.008, 0.008)))
    model(name, "prop", "pier", parts, **{"from": years[0], "to": years[1], "weight": 1})


def shadow(name):
    parts = [box("Shadow", mat("ShadowDark", "#16200f"), (0, 0.004, 0), (0.8, 0.008, 0.18))]
    model(name, "prop", "shadow", parts, **{"from": 1900, "to": 2200, "weight": 1})


station_platform("props/station_platform_1995", (1900, 2029), 0)
station_platform("props/station_platform_2030", (2030, 2100), 1)
station_building("props/station_building_1995", (1900, 2029), 0)
station_building("props/station_building_2030", (2030, 2100), 1)
signal("props/signal_red", LAMP_RED)
signal("props/signal_green", LAMP_GREEN)
pier("props/pier_1995", (1900, 2029), 0)
pier("props/pier_2030", (2030, 2100), 1)
shadow("props/plane_shadow")


# --- Headquarters (GAME-DESIGN.md 9.1): one model per level, centred on a 4 x 4 plot ----------


def hq_garage(name):
    parts = [box("Yard", CONCRETE, (0, 0.015, 0), (3.7, 0.03, 3.7)),
             box("Shed", mat("GarageWall", "#c9c2b0"), (-0.4, 0.34, -0.5), (1.9, 0.68, 1.3)),
             box("ShedRoof", ROOF_GREY, (-0.4, 0.7, -0.5), (2.05, 0.06, 1.45)),
             box("Door", mat("GarageDoor", "#8a8f99", metallic=0.4, roughness=0.5), (-0.4, 0.26, 0.17), (1.1, 0.5, 0.03)),
             box("SignPost", WOOD, (1.2, 0.3, 1.3), (0.06, 0.6, 0.06)),
             box("Sign", mat("SignBoard", "#2a4f9e"), (1.2, 0.62, 1.3), (0.7, 0.28, 0.05))]
    model(name, "hq", "hq_0", parts, **{"from": 1900, "to": 2200, "level": 0, "tiles_x": 4, "tiles_z": 4, "weight": 1})


def hq_depot_office(name):
    parts = [box("Yard", CONCRETE, (0, 0.015, 0), (3.8, 0.03, 3.8)),
             box("Office", mat("OfficeWall", "#d6d0c2"), (-0.3, 0.55, -0.4), (2.0, 1.1, 1.5)),
             box("Windows", GLASS, (-0.3, 0.7, 0.355), (1.7, 0.35, 0.02)),
             box("Windows2", GLASS, (-0.3, 0.3, 0.355), (1.7, 0.3, 0.02)),
             box("Roof", ROOF_GREY, (-0.3, 1.12, -0.4), (2.14, 0.06, 1.64)),
             box("Depot", mat("DepotWall", "#a9b0bb"), (1.15, 0.3, -0.3), (1.0, 0.6, 1.8)),
             box("DepotDoor", BLACK, (1.15, 0.22, 0.61), (0.7, 0.42, 0.03)),
             box("SignPost", WOOD, (-1.5, 0.35, 1.4), (0.06, 0.7, 0.06)),
             box("Sign", mat("SignBoard", "#2a4f9e"), (-1.5, 0.72, 1.4), (0.8, 0.3, 0.05))]
    model(name, "hq", "hq_1", parts, **{"from": 1900, "to": 2200, "level": 1, "tiles_x": 4, "tiles_z": 4, "weight": 1})


def hq_regional_office(name):
    parts = [box("Plaza", mat("Plaza", "#b9b9bf"), (0, 0.015, 0), (3.9, 0.03, 3.9)),
             box("Block", mat("TowerWall", "#c4c8d0", metallic=0.35, roughness=0.3), (-0.2, 1.1, -0.3), (2.2, 2.2, 1.8)),
             box("GlassFront", GLASS, (-0.2, 1.1, 0.605), (2.0, 2.0, 0.02)),
             box("GlassSide", GLASS, (0.905, 1.1, -0.3), (0.02, 2.0, 1.6)),
             box("Roof", STEEL, (-0.2, 2.24, -0.3), (2.3, 0.08, 1.9)),
             box("Wing", mat("WingWall", "#d6d0c2"), (1.4, 0.4, 0.2), (1.0, 0.8, 1.4)),
             box("SignBoard", mat("SignBoard", "#2a4f9e"), (-0.2, 2.0, 0.63), (1.2, 0.2, 0.03)),
             box("LobbyGlow", GLOW_WARM, (-0.2, 0.2, 0.62), (0.9, 0.3, 0.02))]
    model(name, "hq", "hq_2", parts, **{"from": 1900, "to": 2200, "level": 2, "tiles_x": 4, "tiles_z": 4, "weight": 1})


hq_garage("buildings/hq_0_garage")
hq_depot_office("buildings/hq_1_depot_office")
hq_regional_office("buildings/hq_2_regional_office")


def main():
    for path, parts, extras in MODELS:
        write_glb(os.path.join(OUT, path + ".glb"), os.path.basename(path), parts, extras)
    print(f"wrote {len(MODELS)} models to {os.path.normpath(OUT)}")


if __name__ == "__main__":
    sys.exit(main())
