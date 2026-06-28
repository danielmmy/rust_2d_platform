#!/usr/bin/env python3
"""Generate the 12 parallax scenery sets into assets/scenery/.

Each set is 4 horizontally-tileable layers, 480x560:
  <set>_far.png   opaque sky gradient + distant ridge + accents (parallax 0.10)
  <set>_mid.png   silhouette band, transparent above            (parallax 0.30)
  <set>_near.png  closer, darker silhouette band                 (parallax 0.55)
  <set>_fg.png    foreground tufts/overhang, mostly transparent  (parallax 1.15)

Silhouettes use integer-frequency sine sums so the left/right edges meet (seamless
looping); scattered motifs are wrapped across the seam. Run:  python3 tools/gen_scenery.py
"""
import math
import os
import random

from PIL import Image, ImageDraw

W, H = 480, 560
OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "assets", "scenery")
os.makedirs(OUT, exist_ok=True)


def lerp(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def ridge(base, amp, comps, seed):
    """A seamless top-edge height function y(x) (smaller = higher)."""
    rnd = random.Random(seed)
    phs = [rnd.uniform(0, math.tau) for _ in comps]
    norm = sum(a for a, _ in comps)
    def y(x):
        t = x / W
        v = sum(a * math.sin(t * math.tau * f + p) for (a, f), p in zip(comps, phs))
        return base + amp * (v / norm)
    return y


def fill_below(px, yf, color, alpha=255, top=0):
    for x in range(W):
        y0 = max(top, int(yf(x)))
        for y in range(y0, H):
            px[x, y] = (*color, alpha)


SHAPES = {
    "hills": (0.46, 70, [(1, 1), (0.5, 2), (0.3, 3)]),
    "mtns":  (0.40, 120, [(1, 1), (0.7, 2), (0.6, 4), (0.4, 7)]),
    "dunes": (0.55, 55, [(1, 1), (0.45, 2)]),
    "flat":  (0.58, 26, [(1, 2), (0.4, 5)]),
}

# name, sky_top, sky_bottom, far, mid, near, fg, accent, shape
SETS = [
    ("forest_meadow", (150, 205, 235), (208, 232, 205), (120, 165, 120), (74, 140, 84), (44, 96, 60), (24, 66, 44), "sun", "trees"),
    ("deep_caves", (26, 24, 44), (44, 40, 66), (52, 48, 82), (40, 36, 64), (26, 22, 44), (16, 14, 30), "glow", "cave"),
    ("snowy_mountains", (188, 210, 234), (232, 240, 248), (180, 196, 220), (150, 170, 200), (120, 140, 176), (208, 222, 238), "snow", "mtns"),
    ("sandy_beach", (140, 206, 226), (236, 240, 210), (120, 196, 196), (224, 206, 150), (200, 178, 120), (150, 128, 84), "sun", "dunes"),
    ("desolate_desert", (224, 178, 120), (240, 214, 160), (214, 158, 104), (196, 132, 84), (160, 100, 64), (118, 72, 48), "sun", "dunes"),
    ("mushroom_hollow", (40, 30, 56), (70, 44, 78), (96, 58, 104), (70, 50, 96), (48, 36, 72), (30, 24, 50), "glow", "trees"),
    ("volcanic_depths", (40, 18, 20), (88, 32, 24), (120, 40, 28), (74, 28, 26), (44, 18, 20), (24, 12, 14), "ember", "mtns"),
    ("sunset_cliffs", (242, 150, 96), (252, 206, 150), (210, 110, 110), (150, 70, 96), (96, 48, 78), (54, 30, 56), "sun", "mtns"),
    ("crystal_grotto", (24, 30, 54), (40, 52, 86), (70, 96, 150), (54, 74, 124), (36, 50, 92), (22, 32, 64), "glow", "crystal"),
    ("autumn_woods", (210, 188, 150), (236, 220, 184), (196, 150, 96), (180, 108, 56), (140, 74, 44), (96, 50, 34), "sun", "trees"),
    ("misty_swamp", (110, 130, 120), (170, 184, 168), (120, 142, 128), (78, 104, 90), (52, 76, 66), (34, 54, 48), "mist", "trees"),
    ("starry_void", (10, 10, 26), (24, 18, 46), (40, 32, 70), (28, 24, 54), (18, 16, 40), (12, 10, 28), "stars", "flat"),
]


def scatter(draw, motif, color, edge_y, seed, count, size):
    """Place a motif along the silhouette top edge, wrapped across the seam."""
    rnd = random.Random(seed)
    for _ in range(count):
        x = rnd.uniform(0, W)
        for dx in (-W, 0, W):
            cx = x + dx
            top = edge_y(x % W)
            if motif == "tree":
                w = size * rnd.uniform(0.7, 1.2)
                draw.polygon([(cx - w, top + size * 0.2), (cx + w, top + size * 0.2), (cx, top - size)], fill=(*color, 255))
            elif motif == "crystal":
                w = size * 0.5
                draw.polygon([(cx - w, top), (cx + w, top), (cx, top - size * rnd.uniform(1.0, 2.0))], fill=(*color, 255))


def gen(name, sky_top, sky_bot, far, mid, near, fg, accent, shape):
    rnd = random.Random(hash(name) & 0xffff)

    # ---- far: opaque sky + distant ridge + accent ----
    far_img = Image.new("RGBA", (W, H), (0, 0, 0, 255))
    fp = far_img.load()
    for y in range(H):
        col = lerp(sky_top, sky_bot, y / H)
        for x in range(W):
            fp[x, y] = (*col, 255)
    fd = ImageDraw.Draw(far_img)
    if accent in ("sun", "ember"):
        sx, sy = W * 0.7, H * 0.26
        glow = lerp(sky_top, (255, 244, 210) if accent == "sun" else (255, 150, 60), 0.6)
        for r in range(90, 0, -10):
            fd.ellipse([sx - r, sy - r, sx + r, sy + r], fill=(*glow, 26))
        fd.ellipse([sx - 34, sy - 34, sx + 34, sy + 34], fill=(255, 240, 205, 255) if accent == "sun" else (255, 170, 70, 255))
    if accent == "stars" or accent == "glow":
        for _ in range(70):
            x, y = rnd.uniform(0, W), rnd.uniform(0, H * 0.8)
            s = rnd.choice([1, 1, 2])
            c = (235, 235, 255) if accent == "stars" else lerp(far, (160, 255, 220), 0.8)
            for dx in (-W, 0, W):
                fd.ellipse([x + dx - s, y - s, x + dx + s, y + s], fill=(*c, rnd.randint(120, 255)))
    far_edge = ridge(H * 0.60, 36, [(1, 1), (0.5, 2), (0.3, 3)], rnd.random())
    fill_below(fp, far_edge, far)

    base, amp, comps = SHAPES.get(shape, SHAPES["hills"])

    def band(color, base_f, amp_v, seed, motif=None, msize=46, mcount=10):
        img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        d = ImageDraw.Draw(img)
        if shape == "cave":
            # stalactites from the top instead of a ground ridge
            top_edge = ridge(H * (1 - base_f) * 0.5, amp_v, comps, seed)
            for x in range(W):
                y1 = int(top_edge(x))
                for y in range(0, max(0, y1)):
                    img.load()[x, y] = (*color, 255)
            return img
        edge = ridge(H * base_f, amp_v, comps, seed)
        fill_below(img.load(), edge, color)
        if motif:
            scatter(d, motif, lerp(color, (0, 0, 0), 0.15), edge, seed + 9, mcount, msize)
        return img

    motif = {"trees": "tree", "crystal": "crystal"}.get(shape)
    mid_img = band(mid, base + 0.06, amp * 0.7, rnd.random(), motif, 52, 8)
    near_img = band(near, base - 0.04, amp, rnd.random(), motif, 70, 9)

    # ---- fg: bottom tufts + a couple of top overhangs, mostly transparent ----
    fg_img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    fgd = ImageDraw.Draw(fg_img)
    fg_edge = ridge(H * 0.90, 26, [(1, 2), (0.5, 3), (0.4, 5)], rnd.random())
    fill_below(fg_img.load(), fg_edge, fg)
    if shape in ("trees", "crystal"):
        scatter(fgd, motif or "tree", fg, fg_edge, 7, 6, 90)
    if shape == "cave":
        for _ in range(5):  # hanging stalactites
            x = rnd.uniform(0, W)
            for dx in (-W, 0, W):
                fgd.polygon([(x + dx - 14, 0), (x + dx + 14, 0), (x + dx, rnd.uniform(60, 150))], fill=(*fg, 255))

    for suffix, im in (("far", far_img), ("mid", mid_img), ("near", near_img), ("fg", fg_img)):
        im.save(os.path.join(OUT, f"{name}_{suffix}.png"))
    return far_img, mid_img, near_img, fg_img


previews = []
for s in SETS:
    far_img, mid_img, near_img, fg_img = gen(*s)
    comp = far_img.copy()
    for layer in (mid_img, near_img, fg_img):
        comp.alpha_composite(layer)
    previews.append((s[0], comp))
print("wrote", len(SETS) * 4, "layers to", OUT)

# contact sheet (4 cols x 3 rows)
cols, rows, pad = 4, 3, 6
tw, th = W // 2, H // 2
sheet = Image.new("RGB", (cols * (tw + pad) + pad, rows * (th + pad + 12) + pad), (20, 20, 26))
dd = ImageDraw.Draw(sheet)
for i, (name, comp) in enumerate(previews):
    c, r = i % cols, i // cols
    x, y = pad + c * (tw + pad), pad + r * (th + pad + 12)
    sheet.paste(comp.convert("RGB").resize((tw, th)), (x, y + 12))
    dd.text((x + 2, y), name, fill=(230, 230, 240))
sheet.save("/tmp/claude-1000/-home-danielmmy-Workspace-rust-2d-platform/4c003525-2dd5-4027-b098-09a78857d7ca/scratchpad/scenery_sheet.png")
print("wrote contact sheet")
