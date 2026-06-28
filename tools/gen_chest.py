#!/usr/bin/env python3
"""Draw the treasure-chest sprite (`assets/sprites/chest.png`).

A small, flat pixel chest — a wooden body with a domed lid, gold bands and a lock.
Single frame; the game tints/scales it via `custom_size`. Run: python3 tools/gen_chest.py
"""

import os

from PIL import Image, ImageDraw

OUT = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "assets", "sprites", "chest.png")
)
S = 32  # frame size

WOOD = (122, 78, 42, 255)
WOOD_DK = (86, 52, 26, 255)
GOLD = (235, 196, 84, 255)
GOLD_DK = (170, 132, 40, 255)
OUTLINE = (40, 26, 14, 255)


def main():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    left, right = 5, 26
    lid_top, lid_bot, body_bot = 9, 17, 27

    # Body.
    d.rectangle([left, lid_bot, right, body_bot], fill=WOOD, outline=OUTLINE)
    # Domed lid (rounded top).
    d.rounded_rectangle([left, lid_top, right, lid_bot + 2], radius=4, fill=WOOD_DK, outline=OUTLINE)
    d.rectangle([left, lid_bot, right, lid_bot + 1], fill=WOOD_DK, outline=OUTLINE)
    # Gold bands down the sides and centre.
    for x in (left + 3, right - 4):
        d.rectangle([x, lid_top + 2, x + 1, body_bot - 1], fill=GOLD_DK)
    cx = (left + right) // 2
    d.rectangle([cx - 1, lid_top, cx, body_bot - 1], fill=GOLD)
    # Lock plate at the seam.
    d.rectangle([cx - 2, lid_bot - 2, cx + 2, lid_bot + 3], fill=GOLD, outline=OUTLINE)
    d.point((cx, lid_bot), fill=OUTLINE)

    img.save(OUT)
    print(f"wrote {OUT}  ({S}x{S})")


if __name__ == "__main__":
    main()
