#!/usr/bin/env python3
"""Generate the 12 parallax scenery sets into assets/scenery/<set>/.

Each set has four tileable layers (mix-and-match per room via the level builder):
  far.png   wide, seamless sky gradient + faint distant haze (no discrete features)
  mid.png   silhouette band, transparent above
  near.png  closer, darker silhouette band
  fg.png    SPARSE foreground tufts along the bottom edge (drawn in front of the player)

Silhouettes/haze use integer-frequency sine sums so the edges meet (seamless looping);
scattered motifs wrap across the seam. Run:  python3 tools/gen_scenery.py
"""
import math
import os
import random

from PIL import Image, ImageDraw

H = 560
FAR_W = 1440   # wide so the sky never visibly repeats across a room
W = 960        # mid / near / fg width (>= the 960 viewport, so no on-screen repeat)
OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "assets", "scenery")


def lerp(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def ridge(width, base, amp, comps, seed):
    rnd = random.Random(seed)
    phs = [rnd.uniform(0, math.tau) for _ in comps]
    norm = sum(a for a, _ in comps)
    def y(x):
        t = x / width
        v = sum(a * math.sin(t * math.tau * f + p) for (a, f), p in zip(comps, phs))
        return base + amp * (v / norm)
    return y


def fill_below(px, width, yf, color):
    for x in range(width):
        for y in range(max(0, int(yf(x))), H):
            px[x, y] = (*color, 255)


SHAPES = {
    "hills": (0.46, 70, [(1, 2), (0.5, 3), (0.3, 5)]),
    "mtns":  (0.40, 120, [(1, 2), (0.7, 3), (0.6, 6), (0.4, 11)]),
    "dunes": (0.55, 55, [(1, 1), (0.45, 3)]),
    "flat":  (0.58, 26, [(1, 3), (0.4, 7)]),
}

# name, sky_top, sky_bottom, far, mid, near, fg, shape
SETS = [
    ("forest_meadow", (150, 205, 235), (208, 232, 205), (150, 188, 160), (74, 140, 84), (44, 96, 60), (26, 70, 46), "trees"),
    ("deep_caves", (26, 24, 44), (44, 40, 66), (40, 36, 60), (40, 36, 64), (26, 22, 44), (16, 14, 30), "cave"),
    ("snowy_mountains", (188, 210, 234), (232, 240, 248), (196, 210, 230), (150, 170, 200), (120, 140, 176), (210, 224, 240), "mtns"),
    ("sandy_beach", (140, 206, 226), (236, 240, 210), (170, 214, 214), (224, 206, 150), (200, 178, 120), (120, 170, 120), "dunes"),
    ("desolate_desert", (224, 178, 120), (244, 222, 175), (224, 178, 128), (196, 132, 84), (160, 100, 64), (118, 72, 48), "dunes"),
    ("mushroom_hollow", (40, 30, 56), (70, 44, 78), (64, 46, 84), (70, 50, 96), (48, 36, 72), (30, 24, 50), "trees"),
    ("volcanic_depths", (40, 18, 20), (88, 32, 24), (74, 30, 26), (74, 28, 26), (44, 18, 20), (24, 12, 14), "mtns"),
    ("sunset_cliffs", (242, 150, 96), (252, 214, 162), (220, 132, 116), (150, 70, 96), (96, 48, 78), (54, 30, 56), "mtns"),
    ("crystal_grotto", (24, 30, 54), (40, 52, 86), (48, 64, 110), (54, 74, 124), (36, 50, 92), (22, 32, 64), "crystal"),
    ("autumn_woods", (210, 188, 150), (238, 224, 190), (200, 162, 110), (180, 108, 56), (140, 74, 44), (96, 50, 34), "trees"),
    ("misty_swamp", (110, 130, 120), (172, 186, 170), (130, 150, 138), (78, 104, 90), (52, 76, 66), (34, 54, 48), "trees"),
    ("starry_void", (10, 10, 26), (24, 18, 46), (30, 26, 56), (28, 24, 54), (18, 16, 40), (12, 10, 28), "flat"),
]


def scatter(draw, motif, color, edge_y, width, seed, count, size):
    rnd = random.Random(seed)
    for _ in range(count):
        x = rnd.uniform(0, width)
        for dx in (-width, 0, width):
            cx, top = x + dx, edge_y(x % width)
            if motif == "tree":
                w = size * rnd.uniform(0.7, 1.2)
                draw.polygon([(cx - w, top + size * 0.2), (cx + w, top + size * 0.2), (cx, top - size)], fill=(*color, 255))
            elif motif == "crystal":
                w = size * 0.45
                draw.polygon([(cx - w, top), (cx + w, top), (cx, top - size * rnd.uniform(1.0, 2.0))], fill=(*color, 255))


def gen(name, sky_top, sky_bot, far, mid, near, fg, shape):
    rnd = random.Random(hash(name) & 0xffff)
    folder = os.path.join(OUT, name)
    os.makedirs(folder, exist_ok=True)

    # ---- far: wide seamless sky gradient + a faint distant haze (no discrete feature) --
    far_img = Image.new("RGBA", (FAR_W, H), (0, 0, 0, 255))
    fp = far_img.load()
    for y in range(H):
        col = lerp(sky_top, sky_bot, y / H)
        for x in range(FAR_W):
            fp[x, y] = (*col, 255)
    haze = ridge(FAR_W, H * 0.66, 24, [(1, 2), (0.5, 4), (0.3, 6)], rnd.random())
    haze_col = lerp(far, sky_bot, 0.35)
    fill_below(fp, FAR_W, haze, haze_col)

    base, amp, comps = SHAPES.get(shape, SHAPES["hills"])

    def band(color, base_f, amp_v, seed, motif=None, msize=60, mcount=14):
        img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        d = ImageDraw.Draw(img)
        if shape == "cave":
            top_edge = ridge(W, H * (1 - base_f) * 0.5, amp_v, comps, seed)
            p = img.load()
            for x in range(W):
                for y in range(0, max(0, int(top_edge(x)))):
                    p[x, y] = (*color, 255)
            return img
        edge = ridge(W, H * base_f, amp_v, comps, seed)
        fill_below(img.load(), W, edge, color)
        if motif:
            scatter(d, motif, lerp(color, (0, 0, 0), 0.12), edge, W, seed + 9, mcount, msize)
        return img

    motif = {"trees": "tree", "crystal": "crystal"}.get(shape)
    mid_img = band(mid, base + 0.06, amp * 0.7, rnd.random(), motif, 58, 12)
    near_img = band(near, base - 0.04, amp, rnd.random(), motif, 78, 14)

    # ---- fg: SPARSE tufts along the bottom edge (drawn in FRONT of the player) ----------
    fg_img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    fgd = ImageDraw.Draw(fg_img)
    frnd = random.Random(hash(name) ^ 0x5151)
    if shape == "cave":
        # a few stalactites hang from the top edge
        for _ in range(7):
            x = frnd.uniform(0, W)
            h = frnd.uniform(50, 150)
            for dx in (-W, 0, W):
                fgd.polygon([(x + dx - 16, 0), (x + dx + 16, 0), (x + dx, h)], fill=(*fg, 255))
    else:
        # scattered blades/clumps rooted at the bottom; mostly empty so it never hides the player
        for _ in range(22):
            x = frnd.uniform(0, W)
            h = frnd.uniform(40, 110)
            lean = frnd.uniform(-14, 14)
            wdt = frnd.uniform(7, 14)
            col = lerp(fg, (0, 0, 0), frnd.uniform(0.0, 0.2))
            for dx in (-W, 0, W):
                bx = x + dx
                fgd.polygon([(bx - wdt, H), (bx + wdt, H), (bx + lean, H - h)], fill=(*col, 255))

    far_img.save(os.path.join(folder, "far.png"))
    mid_img.save(os.path.join(folder, "mid.png"))
    near_img.save(os.path.join(folder, "near.png"))
    fg_img.save(os.path.join(folder, "fg.png"))
    return far_img, mid_img, near_img, fg_img


previews = []
for s in SETS:
    far_img, mid_img, near_img, fg_img = gen(*s)
    comp = far_img.crop((0, 0, W, H)).copy()
    for layer in (mid_img, near_img, fg_img):
        comp.alpha_composite(layer)
    previews.append((s[0], comp))
print("wrote", len(SETS), "sets to", OUT)

cols, rows, pad = 4, 3, 6
tw, th = W // 3, H // 3
sheet = Image.new("RGB", (cols * (tw + pad) + pad, rows * (th + pad + 12) + pad), (20, 20, 26))
dd = ImageDraw.Draw(sheet)
for i, (name, comp) in enumerate(previews):
    c, r = i % cols, i // cols
    x, y = pad + c * (tw + pad), pad + r * (th + pad + 12)
    sheet.paste(comp.convert("RGB").resize((tw, th)), (x, y + 12))
    dd.text((x + 2, y), name, fill=(230, 230, 240))
sheet.save("/tmp/claude-1000/-home-danielmmy-Workspace-rust-2d-platform/4c003525-2dd5-4027-b098-09a78857d7ca/scratchpad/scenery_sheet.png")
print("wrote contact sheet")
