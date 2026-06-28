#!/usr/bin/env python3
"""Append **crouch** and **look-up** rows to the player sprite sheet.

The player sheet is a 6-column grid; its first four rows (idle / walk / jump / damage)
are hand-authored and left untouched. This derives two extra pose rows from the **idle**
row so they match the art automatically:

  row 4 = crouch   — idle squashed vertically, anchored at the feet
  row 5 = look-up  — idle stretched a touch taller, anchored at the feet

Result: a 6x6 sheet (`assets/sprites/player.png`). Idempotent — it always rebuilds from
the top four rows, so re-running won't stack rows. These are quick programmatic poses;
redraw rows 4-5 by hand for more character.

Run:  python3 tools/gen_player_poses.py
"""

import os

from PIL import Image

SHEET = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "assets", "sprites", "player.png")
)
COLS = 6
BASE_ROWS = 4  # idle, walk, jump, damage (hand-authored)
CROUCH_SCALE = 0.62  # crouch height as a fraction of a frame
LOOKUP_SCALE = 1.08  # look-up stretch (bottom-anchored; the few px over the top clip)


def main():
    img = Image.open(SHEET).convert("RGBA")
    w, _ = img.size
    fw = w // COLS
    fh = 40  # original frame height (160px / 4 rows); fixed so the script is idempotent
    base = img.crop((0, 0, w, fh * BASE_ROWS))  # the untouched first four rows

    out = Image.new("RGBA", (w, fh * 6), (0, 0, 0, 0))
    out.paste(base, (0, 0))

    for col in range(COLS):
        idle = base.crop((col * fw, 0, col * fw + fw, fh))

        # Crouch: squash vertically, keep the feet on the ground.
        ch = max(1, int(fh * CROUCH_SCALE))
        squashed = idle.resize((fw, ch), Image.NEAREST)
        crouch = Image.new("RGBA", (fw, fh), (0, 0, 0, 0))
        crouch.paste(squashed, (0, fh - ch), squashed)
        out.paste(crouch, (col * fw, 4 * fh))

        # Look-up: stretch a little taller, anchored at the feet (top few px clip off).
        lh = max(fh, int(fh * LOOKUP_SCALE))
        stretched = idle.resize((fw, lh), Image.NEAREST)
        look = Image.new("RGBA", (fw, fh), (0, 0, 0, 0))
        look.paste(stretched, (0, fh - lh), stretched)
        out.paste(look, (col * fw, 5 * fh))

    out.save(SHEET)
    print(f"wrote {SHEET}  ({out.size[0]}x{out.size[1]}, 6x6)")


if __name__ == "__main__":
    main()
