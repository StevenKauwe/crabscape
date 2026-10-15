"""CrabScape teaser: picture. Composites the oil repaints (a 4-painting "boil"), the lantern's light, drawn marine
snow, the typography and the level-up pop-up, and pipes frames to ffmpeg (video only; build.sh muxes the sound).

    python render.py <boil_dir> <assets_dir> <out.mp4> [--vertical] [--frames a:b] [--stills t1,t2,... --still-dir d]

The light is the reveal: the scene starts black, the jar wakes, and its light spreads out over the painting; at the
end it withdraws into the jar and goes out. "Unlit" is the same painting seen only by the deep's faint blue ambient.
"""
import argparse
import os
import subprocess
import sys
from multiprocessing import Pool

import numpy as np
from scipy.ndimage import map_coordinates, zoom as nd_zoom

import imgio
from timeline import *

ap = argparse.ArgumentParser()
ap.add_argument("boil")
ap.add_argument("assets")
ap.add_argument("out")
ap.add_argument("--vertical", action="store_true")
ap.add_argument("--frames", default=None)
ap.add_argument("--stills", default=None)
ap.add_argument("--still-dir", default=".")
ap.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 4) - 2))
args = ap.parse_args()

V = args.vertical
OW, OH = (1080, 1920) if V else (1920, 1080)
NF = int(DUR * FPS)
LANTERN = np.array([1037.0, 578.0])  # jar centre in painting pixels
BOIL_FPS = 7.5                       # repaint changes ~7.5 times a second, like a hand-drawn boil

# ---------------------------------------------------------------- assets
paint = [imgio.load(os.path.join(args.boil, f"al{k}.png")) for k in range(4)]
PH, PW, _ = paint[0].shape
def A(name):
    return imgio.load(os.path.join(args.assets, name), alpha=True)
sfx = "_v" if V else ""
TXT_EXP, TXT_WEALTH, TITLE_IMG = A(f"experience{sfx}.png"), A(f"wealth{sfx}.png"), A(f"title{sfx}.png")
POPUP = A("popup.png")
POPUP[..., :3] *= 0.93  # sit the bright UI box a hair down into the scene

# deep colour that the vertical frame's extra sky/floor falls into (the painting's own edge colour)
DEEP_TOP = paint[0][:30, 300:1600].reshape(-1, 3).mean(0) * 0.9
DEEP_BOT = paint[0][-30:, 300:1600].reshape(-1, 3).mean(0) * 0.6

# ---------------------------------------------------------------- curves
def sm(a, b, x):
    u = np.clip((x - a) / (b - a), 0.0, 1.0)
    return u * u * (3 - 2 * u)
def ease_out_back(u, k=1.9):
    u = np.clip(u, 0, 1) - 1
    return 1 + (k + 1) * u ** 3 + k * u ** 2

def camera(t):
    """zoom and the painting point at the frame centre: a slow push-in drifting towards the lantern"""
    u = sm(0.0, DUR, t) * 0.6 + (t / DUR) * 0.4
    if V:
        return 1.20 + 0.10 * u, np.array([915.0 + 45 * u, 590.0 + 5 * u])
    return 1.04 + 0.13 * u, np.array([955.0 + 55 * u, 568.0 + 14 * u])

def light(t):
    """(ambient, radius of the lantern's reach in painting px, jar intensity)"""
    amb = sm(1.6, 3.8, t) * (1 - sm(DARK[0], DARK[0] + 1.0, t))
    spark = sm(LANTERN_SPARK, LIGHT_SPREAD[0], t) * 80
    spread = sm(LIGHT_SPREAD[0], LIGHT_SPREAD[1], t) ** 1.3 * 2600
    r = spark + spread
    withdraw = sm(DARK[0], DARK[0] + 1.1, t)
    r = r * (1 - withdraw) + 70 * withdraw
    jar = sm(LANTERN_SPARK, LANTERN_SPARK + 0.5, t) * (1 - sm(DARK[1] - 0.6, DARK[1], t))
    # bacteria: an uneven flicker while waking, then a slow breath; a startled flutter when the whale calls
    flick = 1 + 0.25 * np.sin(t * 37) * np.sin(t * 23.1) * (1 - sm(LANTERN_SPARK + 0.4, LIGHT_SPREAD[0] + 0.3, t))
    breath = 1 + 0.035 * np.sin(2 * np.pi * t / 2.7)
    startle = 1 - 0.10 * sm(WHALE + 0.5, WHALE + 0.8, t) * (1 - sm(WHALE + 0.9, WHALE + 1.6, t)) * (0.6 + 0.4 * np.sin(t * 30))
    return amb, r * breath * startle, jar * flick * startle

def unlit_of(c):
    lum = (c * np.array([0.3, 0.59, 0.11], np.float32)).sum(-1, keepdims=True)
    return (lum + (c - lum) * 0.4) * 0.30 * np.array([0.72, 0.86, 1.18], np.float32)

# ---------------------------------------------------------------- marine snow (drawn particles)
rng = np.random.default_rng(42)
NP = 320
P_X = rng.uniform(-100, OW + 100, NP)
P_Y = rng.uniform(-100, OH + 100, NP)
P_D = rng.uniform(0, 1, NP) ** 1.6           # depth: 0 far, 1 near
P_VY = 6 + 22 * P_D + rng.uniform(0, 6, NP)  # sinking speed px/s
P_SW = rng.uniform(4, 16, NP) * (0.4 + P_D)  # sway amplitude
P_SF = rng.uniform(0.08, 0.25, NP)           # sway rate
P_PH = rng.uniform(0, 2 * np.pi, NP)
P_SIG = 0.7 + 1.9 * P_D
P_BR = rng.uniform(0.5, 1.0, NP)
# a handful of large out-of-focus motes close to the lens
big = rng.choice(NP, 9, replace=False)
P_SIG[big] = rng.uniform(5, 9, 9); P_BR[big] *= 0.22; P_D[big] = 1.0

def splat(img, x, y, sig, col, alpha):
    r = int(3 * sig + 1)
    xi, yi = int(round(x)), int(round(y))
    x0, x1, y0, y1 = max(xi - r, 0), min(xi + r + 1, img.shape[1]), max(yi - r, 0), min(yi + r + 1, img.shape[0])
    if x0 >= x1 or y0 >= y1:
        return
    yy, xx = np.mgrid[y0:y1, x0:x1]
    g = np.exp(-(((xx - x) ** 2) / (2 * sig * sig) + ((yy - y) ** 2) / (2 * (sig * 1.25) ** 2))) * alpha
    img[y0:y1, x0:x1] += g[..., None] * col  # light adds

def over(img, rgba, x, y, alpha=1.0):
    """composite an RGBA sprite with its top-left at (x, y)"""
    h, w, _ = rgba.shape
    xi, yi = int(round(x)), int(round(y))
    x0, y0 = max(xi, 0), max(yi, 0)
    x1, y1 = min(xi + w, img.shape[1]), min(yi + h, img.shape[0])
    if x0 >= x1 or y0 >= y1 or alpha <= 0:
        return
    s = rgba[y0 - yi:y1 - yi, x0 - xi:x1 - xi]
    a = s[..., 3:4] * alpha
    img[y0:y1, x0:x1] = img[y0:y1, x0:x1] * (1 - a) + s[..., :3] * a

# level-up sparks: a small celebratory burst from the pop-up's icon
SP_N = 30
sp_ang = rng.uniform(-np.pi * 0.95, -np.pi * 0.05, SP_N)
sp_v = rng.uniform(260, 720, SP_N)
sp_life = rng.uniform(0.45, 0.95, SP_N)
sp_col = np.where(rng.uniform(0, 1, SP_N)[:, None] < 0.35, [1.0, 0.97, 0.85], [1.0, 0.78, 0.32])

U, Vv = np.meshgrid(np.arange(OW, dtype=np.float32), np.arange(OH, dtype=np.float32))

def frame(i):
    t = i / FPS
    z, C = camera(t)
    X = C[0] + (U - OW / 2) / z
    Y = C[1] + (Vv - OH / 2) / z
    k = int(t * BOIL_FPS) % 4
    src = paint[k]
    lit = np.stack([map_coordinates(src[..., c], [Y, X], order=1, mode="nearest") for c in range(3)], -1)
    if V:  # beyond the painting: its own deep colour, top and bottom
        inside = sm(-10, 120, Y) * sm(-10, 120, PH - Y)
        deep = np.where((Y < PH / 2)[..., None], DEEP_TOP, DEEP_BOT).astype(np.float32)
        lit = lit * inside[..., None] + deep * (1 - inside[..., None])
    amb, R, jar = light(t)
    d = np.hypot(X - LANTERN[0], Y - LANTERN[1])
    feather = max(140.0, 0.6 * R)
    w = sm(-feather, 0.25 * feather, R - d) if R > 1 else np.zeros_like(d)
    unl = unlit_of(lit) * amb
    img = unl + (lit * jar - unl) * w[..., None]
    # the jar's breath of warm light in the water around it
    dark = 1 - sm(150, 900, R)  # while the scene is still dark the jar's halo in the water is what we see
    glow = jar * (0.05 + 0.015 * np.sin(2 * np.pi * t / 2.7) + 0.10 * dark) * np.exp(-(d / (240.0 - 90 * dark)) ** 2)
    img += glow[..., None] * np.array([1.0, 0.6, 0.28], np.float32)
    # the opening spark: before the light spreads, the jar is a pinprick
    if t < LIGHT_SPREAD[0] + 0.5 and jar > 0:
        Ls = (LANTERN - C) * z + [OW / 2, OH / 2]
        pin = jar * (1 - sm(LIGHT_SPREAD[0], LIGHT_SPREAD[0] + 0.5, t))
        splat(img, Ls[0], Ls[1], 7 * z, np.array([1.0, 0.85, 0.55]), 0.22 * pin)

    # ---- marine snow, in front of the scene; it is seen only where light reaches it
    Lsx, Lsy = (LANTERN - C) * z + [OW / 2, OH / 2]
    z0 = camera(0)[0]
    push = (z / z0) ** (0.6 + 0.8 * P_D)
    px = (P_X + P_SW * np.sin(2 * np.pi * P_SF * t + P_PH))
    py = (P_Y + P_VY * t) % (OH + 200) - 100
    px = OW / 2 + (px - OW / 2) * push
    py = OH / 2 + (py - OH / 2) * push
    # the whale's pressure wave rolls through the water, left to right
    wave_t = t - (WHALE + 0.7) - (px / OW) * 0.9
    px = px + 16 * (0.3 + P_D) * np.exp(-((wave_t - 0.6) / 0.6) ** 2) * np.sin(np.clip(wave_t, 0, None) * 3.0)
    for j in range(NP):
        xq = C[0] + (px[j] - OW / 2) / z
        yq = C[1] + (py[j] - OH / 2) / z
        dq = np.hypot(xq - LANTERN[0], yq - LANTERN[1])
        reach = float(sm(-max(140.0, 0.6 * R), 0.25 * max(140.0, 0.6 * R), R - dq)) if R > 1 else 0.0
        ds = np.hypot(px[j] - Lsx, py[j] - Lsy) / z
        warm = jar * reach * np.exp(-(ds / 330.0) ** 2)
        cool = 0.10 * amb + 0.20 * reach * jar
        col = warm * np.array([1.0, 0.72, 0.38]) * 0.9 + cool * np.array([0.55, 0.65, 0.75])
        a = P_BR[j] * (0.35 + 0.65 * P_D[j])
        if a * col.max() > 0.004:
            splat(img, px[j], py[j], P_SIG[j], col, a)

    # ---- typography
    if V:
        ty_exp, ty_wealth = 250, 365
        ex_x = OW / 2 - TXT_EXP.shape[1] / 2
        we_x = OW / 2 - TXT_WEALTH.shape[1] / 2
    else:
        ty_exp = ty_wealth = 120
        gap = 26
        total = (TXT_EXP.shape[1] - 80) + gap + (TXT_WEALTH.shape[1] - 80)
        ex_x = OW / 2 - total / 2 - 40
        we_x = ex_x + TXT_EXP.shape[1] - 80 + gap
    out_a = 1 - sm(DARK[0] - 0.2, DARK[0] + 0.8, t)
    a_exp = sm(EXPERIENCE, EXPERIENCE + 0.9, t) * out_a
    a_we = sm(WEALTH, WEALTH + 1.1, t) * out_a
    over(img, TXT_EXP, ex_x, ty_exp + 10 * (1 - sm(EXPERIENCE, EXPERIENCE + 1.2, t)), a_exp)
    over(img, TXT_WEALTH, we_x, ty_wealth + 10 * (1 - sm(WEALTH, WEALTH + 1.4, t)), a_we)

    # ---- level-up pop-up + sparks
    if POPUP_IN - 0.05 <= t < POPUP_OUT + 0.6:
        u = (t - POPUP_IN) / 0.24
        s = 0.72 + 0.28 * ease_out_back(u)
        a = sm(POPUP_IN, POPUP_IN + 0.07, t) * (1 - sm(POPUP_OUT, POPUP_OUT + 0.45, t))
        cx, cy = OW / 2, (1560 if V else 935) + 18 * sm(POPUP_OUT, POPUP_OUT + 0.45, t)
        spr = POPUP if abs(s - 1) < 1e-3 else np.clip(nd_zoom(POPUP, (s, s, 1), order=1), 0, 1)
        over(img, spr, cx - spr.shape[1] / 2, cy - spr.shape[0] / 2, a)
        # sparks fly from the icon
        ix, iy = cx - POPUP.shape[1] / 2 + 110, cy - POPUP.shape[0] / 2 + 100
        ts = t - POPUP_IN
        if 0 < ts < 1.0:
            for j in range(SP_N):
                if ts > sp_life[j]:
                    continue
                f = 1 - ts / sp_life[j]
                for back in (0.0, 0.025, 0.05):
                    tb = max(ts - back, 0)
                    x = ix + np.cos(sp_ang[j]) * sp_v[j] * tb
                    y = iy + np.sin(sp_ang[j]) * sp_v[j] * tb + 900 * tb * tb
                    splat(img, x, y, 2.6 - back * 20, sp_col[j], 0.9 * f * (1 - back * 10))

    # ---- title on black
    if t >= TITLE - 0.1:
        a = sm(TITLE, TITLE + 1.0, t)
        ty = (OH / 2 - TITLE_IMG.shape[0] / 2 - 30) + 8 * (1 - sm(TITLE, TITLE + 1.6, t))
        over(img, TITLE_IMG, OW / 2 - TITLE_IMG.shape[1] / 2, ty, a)
        ra = sm(TITLE + 0.5, TITLE + 1.5, t)
        rw = int(120 * sm(TITLE + 0.5, TITLE + 1.6, t))
        ry = int(ty + TITLE_IMG.shape[0] + 26)
        if rw > 0:
            img[ry:ry + 3, OW // 2 - rw // 2:OW // 2 + rw // 2] = (
                img[ry:ry + 3, OW // 2 - rw // 2:OW // 2 + rw // 2] * (1 - ra) + np.array([0.86, 0.69, 0.40]) * ra)

    img *= 1 - sm(END_FADE[0], END_FADE[1], t)
    # dither: keeps the dark water from banding in 8-bit
    img += (np.random.default_rng(i).random(img.shape[:2], np.float32)[..., None] - 0.5) / 255.0
    return (np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8)

if __name__ == "__main__":
    if args.stills:
        os.makedirs(args.still_dir, exist_ok=True)
        for ts in args.stills.split(","):
            i = int(round(float(ts) * FPS))
            imgio.save(os.path.join(args.still_dir, f"still_{'v_' if V else ''}{float(ts):05.2f}.png"), frame(i).astype(np.float32) / 255)
        sys.exit(0)
    a, b = (0, NF)
    if args.frames:
        a, b = map(int, args.frames.split(":"))
    enc = subprocess.Popen(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{OW}x{OH}",
                            "-r", str(FPS), "-i", "-", "-vf", "scale=out_color_matrix=bt709:out_range=tv", "-c:v", "libx264", "-preset", "slow", "-crf", "15",
                            "-tune", "film", "-pix_fmt", "yuv420p", "-colorspace", "bt709", "-color_primaries", "bt709",
                            "-color_trc", "bt709", args.out], stdin=subprocess.PIPE)
    with Pool(args.jobs) as pool:
        for n, f in enumerate(pool.imap(frame, range(a, b), chunksize=2)):
            enc.stdin.write(f.tobytes())
            if n % 60 == 0:
                print(f"frame {a + n}/{b}", file=sys.stderr, flush=True)
    enc.stdin.close()
    enc.wait()
