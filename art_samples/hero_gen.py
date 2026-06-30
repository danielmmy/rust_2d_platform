"""Blocky 8-bit hero (Celeste-ish): flat colour blocks, silhouette outline only, animated
procedurally so we can emit many fluid in-between frames.

Scale: 1 logical px = 2 in-game px (chunky 8-bit), 1 tile = 32px = 16 logical px.
Standing ≈ 27 logical px ≈ 1.7 tiles; crouched = 16 logical px = 1 tile.

Run as a script to (re)generate the review art into this folder."""
import math, os
from PIL import Image, ImageDraw, ImageFont

OUT = os.path.dirname(os.path.abspath(__file__))
W, H = 40, 40
CX, FEET = 20, 34

# standing skeleton heights
HIP_Y = FEET - 9
SH_Y = FEET - 18
HEAD_TOP = SH_Y - 8

HAIR  = (62, 45, 35)
SKIN  = (243, 199, 156)
COAT  = (70, 140, 172)
PANTS = (56, 58, 96)
SHOE  = (34, 32, 44)
BLADE = (206, 212, 226)
EYE   = (30, 26, 40)
OUTLINE = (22, 18, 30)
BG = (48, 52, 72)

def blank(): return Image.new("RGBA", (W, H), (0, 0, 0, 0))
def put(px, x, y, c):
    if 0 <= x < W and 0 <= y < H: px[x, y] = c
def block(px, x, y, w, h, c):
    for i in range(w):
        for j in range(h): put(px, x + i, y + j, c)
def thick(px, a, b, w, c):
    (x0, y0), (x1, y1) = a, b
    n = max(abs(x1 - x0), abs(y1 - y0), 1)
    for s in range(n + 1):
        t = s / n
        block(px, round(x0 + (x1 - x0) * t) - w // 2, round(y0 + (y1 - y0) * t) - w // 2, w, w, c)
def add_outline(img):
    px = img.load(); out = img.copy(); opx = out.load()
    for y in range(H):
        for x in range(W):
            if px[x, y][3] == 0:
                for dx, dy in ((-1,0),(1,0),(0,-1),(0,1),(-1,-1),(1,1),(-1,1),(1,-1)):
                    nx, ny = x+dx, y+dy
                    if 0 <= nx < W and 0 <= ny < H and px[nx, ny][3] > 0:
                        opx[x, y] = OUTLINE; break
    return out

def head(px, sh, htop, face, h=8, eye_dy=4):
    block(px, sh[0]-3, htop, 7, h, SKIN)              # head
    block(px, sh[0]-4, htop-1, 9, 3, HAIR)            # hair cap (static)
    block(px, (sh[0]-4 if face > 0 else sh[0]+2), htop-1, 3, 6, HAIR)   # back tuft
    block(px, sh[0] + (1 if face>0 else -2), htop+eye_dy, 2, 2, EYE)    # eye

def render(face, sh, hip, htop, bleg, fleg, barm, farm, head_h=8, eye_dy=4, sword=None):
    """Joints: *leg/*arm are ((knee/elbow), (foot/hand)). Draws back→front for a side view."""
    img = blank(); px = img.load()
    thick(px, sh, barm[0], 3, COAT); thick(px, barm[0], barm[1], 3, SKIN)        # back arm
    thick(px, hip, bleg[0], 4, PANTS); thick(px, bleg[0], bleg[1], 4, PANTS)     # back leg
    block(px, bleg[1][0]-2, bleg[1][1]-1, 5, 2, SHOE)
    thick(px, hip, sh, 7, COAT); block(px, sh[0]-3, sh[1]-1, 7, 3, COAT)         # torso
    head(px, sh, htop, face, head_h, eye_dy)
    thick(px, hip, fleg[0], 4, PANTS); thick(px, fleg[0], fleg[1], 4, PANTS)     # front leg
    block(px, fleg[1][0]-2, fleg[1][1]-1, 5, 2, SHOE)
    thick(px, sh, farm[0], 3, COAT); thick(px, farm[0], farm[1], 3, SKIN)        # front arm
    if sword:                                                                    # blade in front hand
        thick(px, sword[0], sword[1], 2, BLADE)
        block(px, sword[0][0]-1, sword[0][1]-1, 3, 3, SHOE)                      # small guard
    return add_outline(img)

# --- animation builders: each returns a list of frames ---------------------

def loco(phase, stride, lift, bob_amp, lean):
    bob = -abs(math.cos(phase * 2 * math.pi)) * bob_amp
    shY = round(SH_Y + bob); hipY = round(HIP_Y + bob)
    sh = (CX + round(lean) * (1), shY); hip = (CX, hipY)
    htop = HEAD_TOP + (shY - SH_Y)
    def leg(ph):
        s = math.sin(ph * 2 * math.pi)
        return (CX + round(s*stride*0.5), FEET-4), (CX + round(s*stride), FEET - round(max(0, s)*lift))
    def arm(ph):
        s = math.sin(ph * 2 * math.pi)
        return (CX + round(s*2), shY+3), (CX + round(s*4), shY + 5 - round(s*2))
    return render(1, sh, hip, htop, leg(phase+0.5), leg(phase), arm(phase), arm(phase+0.5))

def anim_idle():
    out = []
    for i in range(4):
        bob = round(math.sin(i/4 * 2*math.pi))
        shY = SH_Y + bob; hipY = HIP_Y + bob; htop = HEAD_TOP + bob
        out.append(render(1, (CX, shY), (CX, hipY), htop,
                          ((CX-1, FEET-4), (CX-1, FEET)), ((CX+1, FEET-4), (CX+1, FEET)),
                          ((CX, shY+3), (CX, shY+6)), ((CX+3, shY+3), (CX+3, shY+6))))
    return out

def anim_walk():  return [loco(i/6, 3, 2, 1, 0) for i in range(6)]
def anim_run():   return [loco(i/6, 5, 4, 2, 1) for i in range(6)]
def anim_crouch_walk():
    out = []
    for i in range(6):
        s = math.sin(i/6 * 2*math.pi)
        top = FEET - 16; sh = (CX, top+6); hip = (CX, FEET-5)
        img = blank(); px = img.load()
        for d, ph in ((0, i/6+0.5), (1, i/6)):
            ss = math.sin(ph * 2*math.pi)
            kn = (CX + round(3 + ss*2), FEET-3); ft = (CX + round(d + ss*3), FEET)
            thick(px, hip, kn, 4, PANTS); thick(px, kn, ft, 4, PANTS)
            block(px, ft[0]-2, FEET-1, 5, 2, SHOE)
        thick(px, hip, sh, 7, COAT); block(px, sh[0]-3, sh[1]-1, 7, 3, COAT)
        thick(px, sh, (CX+4, sh[1]+3), 3, COAT); thick(px, (CX+4, sh[1]+3), (CX+5, hip[1]), 3, SKIN)
        head(px, sh, top, 1, 6)
        out.append(add_outline(img))
    return out

def anim_crouch():
    top = FEET - 16; sh = (CX, top+6); hip = (CX, FEET-5)
    img = blank(); px = img.load()
    for d in (0, 1):
        kn = (CX + 3 + d, FEET-3); ft = (CX + d, FEET)
        thick(px, hip, kn, 4, PANTS); thick(px, kn, ft, 4, PANTS)
        block(px, ft[0]-2, FEET-1, 5, 2, SHOE)
    thick(px, hip, sh, 7, COAT); block(px, sh[0]-3, sh[1]-1, 7, 3, COAT)
    thick(px, sh, (CX+4, sh[1]+3), 3, COAT); thick(px, (CX+4, sh[1]+3), (CX+5, hip[1]), 3, SKIN)
    head(px, sh, top, 1, 6)
    return [add_outline(img)]

def anim_lookup():
    # standing, chin up: eye near the top of the face, arms at the sides
    return [render(1, (CX, SH_Y), (CX, HIP_Y), HEAD_TOP,
                   ((CX-1, FEET-4), (CX-1, FEET)), ((CX+1, FEET-4), (CX+1, FEET)),
                   ((CX, SH_Y+3), (CX, SH_Y+6)), ((CX+2, SH_Y+3), (CX+2, SH_Y+6)),
                   eye_dy=1)]

def anim_jump():
    poses = [
        # launch: coiled, knees bent, arms back-low
        dict(sh=(CX, SH_Y+1), hip=(CX, HIP_Y+1), bl=((CX-3, FEET-5),(CX-2, FEET)), fl=((CX+3, FEET-5),(CX+2, FEET)),
             ba=((CX-2, SH_Y+4),(CX-3, SH_Y+7)), fa=((CX+2, SH_Y+4),(CX+3, SH_Y+7))),
        # rising: legs tucked up, arms reaching up
        dict(sh=(CX, SH_Y-1), hip=(CX, HIP_Y-1), bl=((CX-2, FEET-7),(CX-1, FEET-4)), fl=((CX+1, FEET-7),(CX+2, FEET-3)),
             ba=((CX-1, SH_Y),(CX-2, SH_Y-4)), fa=((CX+1, SH_Y),(CX+2, SH_Y-4))),
        # apex: body stretched, legs easing down, arms out
        dict(sh=(CX, SH_Y-1), hip=(CX, HIP_Y-1), bl=((CX-2, FEET-4),(CX-2, FEET-1)), fl=((CX+2, FEET-4),(CX+2, FEET-1)),
             ba=((CX-3, SH_Y+2),(CX-4, SH_Y+1)), fa=((CX+3, SH_Y+2),(CX+4, SH_Y+1))),
        # falling: legs reaching down, arms up to brace
        dict(sh=(CX, SH_Y), hip=(CX, HIP_Y), bl=((CX-2, FEET-3),(CX-1, FEET)), fl=((CX+2, FEET-3),(CX+1, FEET)),
             ba=((CX-2, SH_Y+1),(CX-3, SH_Y-3)), fa=((CX+2, SH_Y+1),(CX+3, SH_Y-3))),
    ]
    out = []
    for p in poses:
        htop = HEAD_TOP + (p['sh'][1] - SH_Y)
        out.append(render(1, p['sh'], p['hip'], htop, p['bl'], p['fl'], p['ba'], p['fa']))
    return out

def anim_damage():
    out = []
    for i in range(4):
        j = (i % 2) * 2 - 1                      # shake ±1
        lean = -2                                # knocked backward (faces right)
        sh = (CX + lean, SH_Y - 1); hip = (CX, HIP_Y)
        htop = HEAD_TOP + (sh[1]-SH_Y) - 1
        out.append(render(1, sh, hip, htop,
                          ((CX-2+j, FEET-4), (CX-3+j, FEET)), ((CX+2, FEET-5), (CX+2, FEET-1)),
                          ((CX-2, sh[1]-2), (CX-3, sh[1]-5)), ((CX+2, sh[1]-2), (CX+3, sh[1]-5)),
                          eye_dy=3))
    return out

def anim_dash():
    # horizontal burst: body low and stretched forward, trailing leg back, arms swept back
    sh = (CX+2, SH_Y+2); hip = (CX, HIP_Y+1); htop = HEAD_TOP + 2
    return [render(1, sh, hip, htop,
                   ((CX-3, FEET-3),(CX-5, FEET)), ((CX+3, FEET-3),(CX+5, FEET-1)),
                   ((CX-2, sh[1]+2),(CX-5, sh[1]+1)), ((CX+1, sh[1]+2),(CX-2, sh[1]+2)))]

def anim_attack():
    # front hand swings a blade through a down-forward arc
    poses = [  # (hand, blade-tip)
        ((CX+1, SH_Y+1), (CX-3, SH_Y-6)),    # wind up (raised behind)
        ((CX+4, SH_Y),   (CX+11, SH_Y-3)),   # forward-up
        ((CX+5, SH_Y+3), (CX+13, SH_Y+5)),   # strike (down-forward)
        ((CX+4, SH_Y+5), (CX+9, SH_Y+10)),   # follow through
    ]
    out = []
    for hand, tip in poses:
        elb = (CX + (hand[0]-CX)//2, hand[1]-1)
        out.append(render(1, (CX, SH_Y), (CX, HIP_Y), HEAD_TOP,
                          ((CX-2, FEET-4),(CX-2, FEET)), ((CX+2, FEET-4),(CX+2, FEET)),
                          ((CX, SH_Y+3),(CX-1, SH_Y+6)), (elb, hand), sword=(hand, tip)))
    return out

def anim_pogo():
    # airborne down-stab: blade straight down below the feet, legs tucked
    out = []
    for i in range(2):
        b = i  # tiny bob
        hand = (CX+2, HIP_Y+1+b); tip = (CX+2, FEET+5+b)
        out.append(render(1, (CX, SH_Y), (CX, HIP_Y), HEAD_TOP,
                          ((CX-2, FEET-6),(CX-1, FEET-3)), ((CX+2, FEET-6),(CX+3, FEET-3)),
                          ((CX-2, SH_Y+2),(CX-3, SH_Y-1)), ((CX+1, SH_Y+3), hand), sword=(hand, tip)))
    return out

ANIMS = [
    ("idle", anim_idle(), 5), ("walk", anim_walk(), 10), ("run", anim_run(), 15),
    ("jump", anim_jump(), 8), ("fall→land", anim_jump()[2:] + anim_idle()[:1], 8),
    ("crouch", anim_crouch(), 2), ("crouch-walk", anim_crouch_walk(), 9),
    ("look-up", anim_lookup(), 2), ("dash", anim_dash(), 10),
    ("attack", anim_attack(), 14), ("pogo", anim_pogo(), 10), ("damage", anim_damage(), 14),
]

# --- review art ------------------------------------------------------------

def gif(frames, name, scale=7, ms=80):
    bg = Image.new("RGBA", (W*scale, H*scale), BG+(255,)); imgs=[]
    for f in frames:
        c = bg.copy(); c.alpha_composite(f.resize((W*scale, H*scale), Image.NEAREST)); imgs.append(c.convert("RGB"))
    imgs[0].save(os.path.join(OUT, name), save_all=True, append_images=imgs[1:], duration=ms, loop=0)

def contact_sheet():
    fnt = ImageFont.load_default()
    sc = 3; cell = 40*sc; lblw = 88; maxf = max(len(f) for _, f, _ in ANIMS)
    img = Image.new("RGBA", (lblw + maxf*(cell+3) + 4, len(ANIMS)*(cell+3) + 4), BG+(255,))
    d = ImageDraw.Draw(img)
    for r, (name, frames, _) in enumerate(ANIMS):
        y = 4 + r*(cell+3)
        d.text((6, y + cell//2 - 4), name, font=fnt, fill=(235, 235, 245))
        for c, f in enumerate(frames):
            img.alpha_composite(f.resize((cell, cell), Image.NEAREST), (lblw + c*(cell+3), y))
    img.save(os.path.join(OUT, "all_anims.png"))

def montage_gif():
    fnt = ImageFont.load_default()
    cols = 4; sc = 3; cell = 40*sc; rows = (len(ANIMS)+cols-1)//cols
    cw, ch = cell, cell + 14
    T = 24
    base = Image.new("RGBA", (cols*cw + 4, rows*ch + 4), BG+(255,))
    frames_out = []
    for t in range(T):
        fr = base.copy(); d = ImageDraw.Draw(fr)
        for i, (name, frs, fps) in enumerate(ANIMS):
            cx0 = 2 + (i % cols)*cw; cy0 = 2 + (i//cols)*ch
            idx = int((t/T) * max(1, len(frs)) * (fps/8.0)) % len(frs)
            fr.alpha_composite(frs[idx].resize((cell, cell), Image.NEAREST), (cx0, cy0))
            d.text((cx0+3, cy0+cell+2), name, font=fnt, fill=(235, 235, 245))
        frames_out.append(fr.convert("RGB"))
    frames_out[0].save(os.path.join(OUT, "all_anims.gif"), save_all=True,
                       append_images=frames_out[1:], duration=90, loop=0)

# Rows in baking order — must match the `first = row*6` clip table in src/anim.rs.
BAKE_ROWS = [
    ("idle", anim_idle()), ("walk", anim_walk()), ("jump", anim_jump()),
    ("damage", anim_damage()), ("crouch", anim_crouch()), ("look-up", anim_lookup()),
    ("sprint", anim_run()), ("crouch-walk", anim_crouch_walk()), ("dash", anim_dash()),
    ("attack", anim_attack()), ("pogo", anim_pogo()),
]
COLS = 6

def bake_sheet(path):
    """Write the player sprite sheet: COLS columns × len(BAKE_ROWS) rows of W×H frames."""
    sheet = Image.new("RGBA", (COLS * W, len(BAKE_ROWS) * H), (0, 0, 0, 0))
    for r, (_, frames) in enumerate(BAKE_ROWS):
        for c, f in enumerate(frames[:COLS]):
            sheet.paste(f, (c * W, r * H))
    sheet.save(path)
    print(f"baked {path}  ({sheet.size[0]}x{sheet.size[1]}, {COLS}x{len(BAKE_ROWS)})")

if __name__ == "__main__":
    bake_sheet(os.path.join(OUT, "player_sheet.png"))
    contact_sheet()
    montage_gif()
    for name, frames, fps in ANIMS:
        gif(frames, f"anim_{name.replace('→','_').replace('-','_')}.gif", 7, int(1000/max(fps,1)))
    print("wrote all_anims.png, all_anims.gif, and per-anim gifs")
