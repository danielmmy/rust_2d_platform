#!/usr/bin/env python3
"""Draw the treasure-chest sprites (`assets/sprites/chest.png` + `chest_open.png`).

Small flat pixel chests — a wooden body with gold bands and a lock. Two states: closed
(what you walk up to) and open (left behind after you take the prize). The game tints/scales
them via `custom_size`. Run: python3 tools/gen_chest.py
"""

import os

from PIL import Image, ImageDraw

DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "assets", "sprites"))
S = 32  # frame size

WOOD = (122, 78, 42, 255)
WOOD_DK = (86, 52, 26, 255)
GOLD = (235, 196, 84, 255)
GOLD_DK = (170, 132, 40, 255)
INSIDE = (28, 18, 10, 255)
OUTLINE = (40, 26, 14, 255)


def draw_chest(is_open: bool) -> Image.Image:
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    left, right = 5, 26
    lid_top, lid_bot, body_bot = 9, 17, 27

    # Body (both states).
    d.rectangle([left, lid_bot, right, body_bot], fill=WOOD, outline=OUTLINE)
    for x in (left + 3, right - 4):
        d.rectangle([x, lid_bot + 1, x + 1, body_bot - 1], fill=GOLD_DK)

    if is_open:
        # Dark interior opening at the top of the body, with a gold gleam.
        d.rectangle([left + 1, lid_bot - 3, right - 1, lid_bot + 2], fill=INSIDE, outline=OUTLINE)
        d.rectangle([left + 4, lid_bot - 1, left + 7, lid_bot + 1], fill=GOLD)
        # Lid flipped up and back (a thin plank near the top).
        d.rounded_rectangle([left, 2, right, 7], radius=3, fill=WOOD_DK, outline=OUTLINE)
        cx = (left + right) // 2
        d.rectangle([cx - 1, 2, cx, 7], fill=GOLD)
    else:
        # Domed lid + centre gold band + lock plate.
        d.rounded_rectangle([left, lid_top, right, lid_bot + 2], radius=4, fill=WOOD_DK, outline=OUTLINE)
        d.rectangle([left, lid_bot, right, lid_bot + 1], fill=WOOD_DK, outline=OUTLINE)
        cx = (left + right) // 2
        d.rectangle([cx - 1, lid_top, cx, body_bot - 1], fill=GOLD)
        d.rectangle([cx - 2, lid_bot - 2, cx + 2, lid_bot + 3], fill=GOLD, outline=OUTLINE)
        d.point((cx, lid_bot), fill=OUTLINE)
    return img


def main():
    draw_chest(False).save(os.path.join(DIR, "chest.png"))
    draw_chest(True).save(os.path.join(DIR, "chest_open.png"))
    print(f"wrote chest.png + chest_open.png in {DIR}  ({S}x{S})")


if __name__ == "__main__":
    main()
