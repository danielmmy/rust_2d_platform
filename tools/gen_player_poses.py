#!/usr/bin/env python3
"""Append **crouch**, **look-up** and **sprint** rows to the player sprite sheet.

The player sheet is a 6-column grid; its first four rows (idle / walk / jump / damage)
are hand-authored and left untouched. This derives three extra pose rows so they match
the art automatically:

  row 4 = crouch   — idle squashed vertically, anchored at the feet
  row 5 = look-up  — idle stretched a touch taller, anchored at the feet
  row 6 = sprint   — the walk frames sheared into a forward (running) lean

Result: a 6x7 sheet (`assets/sprites/player.png`). Idempotent — it always rebuilds from
the top four rows, so re-running won't stack rows. These are quick programmatic poses;
redraw rows 4-6 by hand for more character.

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
SPRINT_LEAN = 4.0  # px the top of the sprite leans forward (right) for the run


def main():
    img = Image.open(SHEET).convert("RGBA")
    w, _ = img.size
    fw = w // COLS
    fh = 40  # original frame height (160px / 4 rows); fixed so the script is idempotent
    base = img.crop((0, 0, w, fh * BASE_ROWS))  # the untouched first four rows

    out = Image.new("RGBA", (w, fh * 7), (0, 0, 0, 0))
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

        # Sprint: shear the walk frame so the top leans forward (right); feet stay planted.
        walk = base.crop((col * fw, fh, col * fw + fw, 2 * fh))
        # AFFINE maps output->input: input_x = x + (lean/fh)*y - lean (top shifts right).
        sprint = walk.transform(
            (fw, fh),
            Image.AFFINE,
            (1.0, SPRINT_LEAN / fh, -SPRINT_LEAN, 0.0, 1.0, 0.0),
            Image.NEAREST,
        )
        out.paste(sprint, (col * fw, 6 * fh))

    out.save(SHEET)
    print(f"wrote {SHEET}  ({out.size[0]}x{out.size[1]}, 6x7)")


if __name__ == "__main__":
    main()
