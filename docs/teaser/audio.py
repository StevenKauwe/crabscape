"""CrabScape teaser soundtrack, synthesised from scratch with numpy/scipy (no samples, no downloads).
Writes a 48 kHz stereo float WAV. Loudness is mastered later by ffmpeg (two-pass loudnorm, linear)."""
import sys
import numpy as np
from scipy.signal import butter, sosfilt, fftconvolve
from scipy.io import wavfile
from timeline import *

SR = 48000
N = int(DUR * SR)
t = np.arange(N) / SR
rng = np.random.default_rng(7)

def lp(x, f, order=2):
    return sosfilt(butter(order, f, "low", fs=SR, output="sos"), x)
def hp(x, f, order=2):
    return sosfilt(butter(order, f, "high", fs=SR, output="sos"), x)
def bp(x, lo, hi, order=2):
    return sosfilt(butter(order, [lo, hi], "band", fs=SR, output="sos"), x)
def smooth(a, b, x):
    u = np.clip((x - a) / (b - a), 0, 1)
    return u * u * (3 - 2 * u)
def env_ad(tt, a, d):  # attack then exponential decay, tt relative time
    return np.where(tt < 0, 0, np.where(tt < a, tt / max(a, 1e-9), np.exp(-(tt - a) / d)))
def hz(note):  # midi -> Hz
    return 440.0 * 2 ** ((note - 69) / 12)

def reverb_ir(seconds, decay, bright, seed):
    r = np.random.default_rng(seed)
    n = int(seconds * SR)
    tt = np.arange(n) / SR
    ir = r.standard_normal((2, n)) * np.exp(-tt / decay)
    ir = np.stack([lp(ch, bright) for ch in ir])
    ir[:, : int(0.012 * SR)] *= np.linspace(0, 1, int(0.012 * SR))
    return ir / np.sqrt((ir ** 2).sum(axis=1, keepdims=True))

def place(dst, sig, at, gain=1.0, pan=0.0):
    i = int(at * SR)
    n = min(len(sig), N - i)
    if n <= 0: return
    l, r = np.cos((pan + 1) * np.pi / 4), np.sin((pan + 1) * np.pi / 4)
    dst[0, i:i + n] += sig[:n] * gain * l * 1.414
    dst[1, i:i + n] += sig[:n] * gain * r * 1.414

music = np.zeros((2, N))
sfx = np.zeros((2, N))

# ---- the deep: a low drone (A1 with its fifth), slowly breathing, plus the sea's rumble
drone_env = smooth(0.0, 2.5, t) * (1 - smooth(DARK[0] + 0.6, DARK[1] + 0.4, t) * 0.55)
drone = np.zeros(N)
for f, a in [(55.0, 1.0), (55.0 * 1.003, 0.7), (82.4, 0.45), (110.0, 0.25), (164.8, 0.08)]:
    ph = rng.uniform(0, 2 * np.pi)
    drone += a * np.sin(2 * np.pi * f * t + ph + 0.4 * np.sin(2 * np.pi * 0.07 * t + ph))
drone *= (0.85 + 0.15 * np.sin(2 * np.pi * 0.19 * t)) * drone_env
rumble = lp(rng.standard_normal(N), 180, 4) * 3.0 * drone_env
rumble2 = bp(rng.standard_normal(N), 300, 900) * 0.25 * (0.5 + 0.5 * np.sin(2 * np.pi * 0.11 * t)) * drone_env
for ch in (0, 1):
    music[ch] += 0.08 * drone + 0.22 * rumble + 0.15 * rumble2
# (no inter-channel delay: keeps the bed mono-safe; the reverb gives the width)

# ---- warm pad: chords that open as the light spreads and lift for the tagline
def pad_voice(freqs, start, end, att, rel, gain):
    out = np.zeros(N)
    e = smooth(start, start + att, t) * (1 - smooth(end, end + rel, t))
    for f in freqs:
        for det in (-0.004, 0.0, 0.0037):
            ph = rng.uniform(0, 2 * np.pi)
            ff = f * (1 + det)
            # soft saw: a few harmonics, rolled off
            for k in range(1, 6):
                out += np.sin(2 * np.pi * ff * k * t + ph * k) / (k ** 1.6)
    return lp(out, 1400, 2) * e * gain / len(freqs)
Am9 = [hz(n) for n in (45, 52, 57, 59, 60, 64)]        # A2 E3 A3 B3 C4 E4
Fmaj7 = [hz(n) for n in (41, 48, 53, 57, 60, 64, 71)]  # F2 C3 F3 A3 C4 E4 B4 (lydian shimmer)
Aadd9 = [hz(n) for n in (45, 52, 57, 59, 61, 64)]      # A major, open, for the title
pad = pad_voice(Am9, 1.8, WHALE - 0.4, 3.0, 1.6, 0.26)
pad += pad_voice(Fmaj7, WHALE - 0.6, DARK[0] + 0.3, 2.6, 2.2, 0.17)
pad += pad_voice(Aadd9, TITLE - 0.4, END_FADE[0], 1.2, 1.6, 0.22)
music[0] += pad
music[1] += np.roll(pad, 330)

# ---- a sparse glass-marimba motif as the light reaches out
def mallet(f, dur=2.5, bright=1.0):
    tt = np.arange(int(dur * SR)) / SR
    s = (np.sin(2 * np.pi * f * tt) * np.exp(-tt / 0.9)
         + 0.35 * bright * np.sin(2 * np.pi * f * 3.98 * tt) * np.exp(-tt / 0.18)
         + 0.12 * bright * np.sin(2 * np.pi * f * 9.2 * tt) * np.exp(-tt / 0.05))
    return s * np.minimum(1, tt / 0.003)
motif = [(2.15, 76, -0.3), (2.85, 72, 0.25), (3.55, 69, -0.1), (4.25, 71, 0.3), (4.95, 64, -0.2)]
for at, n, pan in motif:
    place(music, mallet(hz(n)), at, 0.13, pan)
# after the whale: two low, slow notes like distant buoys
place(music, mallet(hz(57), 4, 0.5), WHALE_ANSWER + 1.2, 0.06, -0.2)
place(music, mallet(hz(52), 4, 0.5), WHALE_ANSWER + 2.1, 0.05, 0.2)

# ---- the lantern waking: a soft shimmer of tiny glints
glint = np.zeros(N)
for k in range(26):
    at = LANTERN_SPARK + rng.uniform(0, 1.6) ** 1.4
    f = rng.uniform(2400, 5200)
    tt = np.arange(int(0.6 * SR)) / SR
    g = np.sin(2 * np.pi * f * tt) * np.exp(-tt / 0.12) * np.minimum(1, tt / 0.004)
    i = int(at * SR); glint[i:i + len(g)] += g * rng.uniform(0.3, 1.0)
swell = bp(rng.standard_normal(N), 1500, 6000) * smooth(LANTERN_SPARK, LIGHT_SPREAD[0] + 0.6, t) * (1 - smooth(LIGHT_SPREAD[0] + 0.6, LIGHT_SPREAD[1], t))
glint = glint * 0.03 + swell * 0.04
place(music, glint, 0, 1.0, 0.25)

# ---- reverb for the music bed (a big dark hall: underwater space)
ir_music = reverb_ir(4.0, 1.3, 3500, 3)
wet = np.stack([fftconvolve(music[c], ir_music[c])[:N] for c in (0, 1)])
music = music * 0.75 + wet * 0.55

# ---- DINK: a bright, metallic level-up flourish (original): three quick rising struck-metal notes, the top one
# brightest and longest, each with a crisp strike transient and inharmonic bar/bell partials, then a sparkle tail.
def struck(f, dur, decay, bright, seed):
    r = np.random.default_rng(seed)
    n = int(dur * SR); tt = np.arange(n) / SR
    # free-bar + bell partials (inharmonic), each with its own decay; a twin a few cents off makes it shimmer
    partials = [(1.0, 1.0, 1.0), (1.0035, 0.55, 1.0), (2.0, 0.35, 0.6), (2.756, 0.5 * bright, 0.42),
                (4.07, 0.28 * bright, 0.3), (5.404, 0.30 * bright, 0.22), (8.933, 0.16 * bright, 0.12),
                (13.34, 0.08 * bright, 0.06)]
    s = np.zeros(n)
    for m, a, d in partials:
        if f * m < 20000:
            s += a * np.sin(2 * np.pi * f * m * tt + r.uniform(0, 6.28)) * np.exp(-tt / (decay * d))
    # the strike: a 3 ms burst of bright noise and a tick of high partial
    k = int(0.004 * SR)
    s[:k] += hp(r.standard_normal(k), 4000) * np.exp(-np.arange(k) / (0.0008 * SR)) * 1.4
    return s * np.minimum(1, tt / 0.0004) * np.minimum(1, (dur - tt) / 0.4)  # no truncation click
notes = [(0.000, hz(88), 0.35, 0.7, 0.8),   # E6
         (0.055, hz(93), 0.40, 0.85, 0.9),  # A6
         (0.110, hz(100), 0.75, 1.0, 1.25)] # E7, the bright top note that rings
dink = np.zeros(int(3.0 * SR))
for k, (at, f, dec, br, g) in enumerate(notes):
    sgl = struck(f, 2.2, dec, br, 40 + k)
    i = int(at * SR); dink[i:i + len(sgl)] += sgl[: len(dink) - i] * g
# sparkle tail: a scatter of tiny high glints over the top note's ring
for k in range(40):
    at = 0.11 + rng.uniform(0, 0.55) ** 1.3
    f = rng.uniform(6000, 12500)
    tt = np.arange(int(0.25 * SR)) / SR
    gl = np.sin(2 * np.pi * f * tt) * np.exp(-tt / 0.04) * np.minimum(1, tt / 0.001)
    i = int(at * SR); dink[i:i + len(gl)] += gl * rng.uniform(0.05, 0.22) * (1 - at / 0.9)
dink /= np.abs(dink).max()
ir_small = reverb_ir(1.4, 0.4, 9000, 5)
dink *= np.minimum(1, (len(dink) / SR - np.arange(len(dink)) / SR) / 0.3)
dink = np.pad(dink, (0, int(1.4 * SR)))  # room for the shimmer's reverb tail
dink_st = np.stack([dink + 0.45 * fftconvolve(dink, ir_small[c])[:len(dink)] for c in (0, 1)])
place(sfx, dink_st[0], DINK, 0.36, -0.15)
place(sfx, dink_st[1], DINK, 0.36, 0.15)
# little celebratory crackle as the sparks fly
crk = np.zeros(int(1.2 * SR))
for k in range(18):
    i = int(rng.uniform(0.05, 0.9) ** 1.5 * SR)
    m = int(0.01 * SR)
    crk[i:i + m] += hp(rng.standard_normal(m), 3000) * np.exp(-np.arange(m) / (0.002 * SR)) * rng.uniform(0.2, 1)
place(sfx, crk, POPUP_IN + 0.02, 0.12, 0.3)  # crackle with the sparks

# ---- WHALE: a huge, distant call. Slow sweeps on a low fundamental, harmonics shaped by a moving formant,
# a little roughness, then the ocean between us: low-pass, long dark reverb and late echoes.
def whale_call(dur, contour, formant, seed, rough=0.25):
    r = np.random.default_rng(seed)
    n = int(dur * SR)
    tt = np.arange(n) / SR
    ks, fs = zip(*contour)
    f0 = np.interp(tt / dur, ks, fs)
    f0 *= 1 + 0.012 * np.sin(2 * np.pi * 5.3 * tt) * smooth(0.2, 0.6, tt / dur)  # vibrato as it sustains
    ph = 2 * np.pi * np.cumsum(f0) / SR
    fc = np.interp(tt / dur, [0, 0.5, 1], formant)
    s = np.zeros(n)
    for k in range(1, 14):
        fk = f0 * k
        amp = np.exp(-((fk - fc) / (0.9 * fc)) ** 2) + 0.6 / k
        s += amp * np.sin(k * ph + r.uniform(0, 6.28))
    s *= 1 + rough * lp(r.standard_normal(n), 40)  # grain
    a = smooth(0, 0.12, tt / dur) * (1 - smooth(0.7, 1.0, tt / dur))
    return s * a
WP = 0.75  # pitch: a fourth lower than the first cut
call = whale_call(3.8, [(0, 150 * WP), (0.18, 210 * WP), (0.45, 330 * WP), (0.62, 300 * WP), (0.85, 170 * WP), (1.0, 120 * WP)], [320, 560, 280], 11)
call2 = whale_call(2.4, [(0, 120 * WP), (0.3, 95 * WP), (0.7, 70 * WP), (1.0, 55 * WP)], [210, 240, 160], 12, 0.35)  # a low groan tail
whale = np.zeros(int(9 * SR))
whale[: len(call)] += call
o = int(2.9 * SR); whale[o:o + len(call2)] += call2 * 0.7
# grit: gentle asymmetric tape-style saturation before the ocean's low-pass, so it thickens rather than fizzes
whale /= np.abs(whale).max()
whale = (np.tanh(2.6 * (whale + 0.08)) - np.tanh(2.6 * 0.08)) / np.tanh(2.6)
whale = lp(whale, 1100, 4)
whale = hp(whale, 32)
ir_whale = reverb_ir(6.0, 2.2, 1600, 9)
wv = np.stack([fftconvolve(whale, ir_whale[c])[: len(whale)] for c in (0, 1)])
dry = np.stack([whale, whale])
whale_st = dry * 0.9 + wv * 0.8
for d, g in [(0.43, 0.35), (0.97, 0.22), (1.6, 0.12)]:   # sea-floor and surface echoes
    k = int(d * SR)
    whale_st[0, k:] += wv[1, :-k] * g; whale_st[1, k:] += wv[0, :-k] * g
whale_st /= np.abs(whale_st).max()
place(sfx, whale_st[0], WHALE, 1.1, -0.25)
place(sfx, whale_st[1], WHALE, 1.1, 0.25)
# the sub: pressure you feel more than hear
sub = np.sin(2 * np.pi * 38 * t) * smooth(WHALE, WHALE + 1.5, t) * (1 - smooth(WHALE + 2.5, WHALE + 5.0, t))
place(sfx, sub, 0, 0.10)
# the answer, farther away
ans = whale_call(2.8, [(0, 180 * WP), (0.3, 260 * WP), (0.6, 240 * WP), (1.0, 140 * WP)], [300, 420, 240], 21)
ans = lp(np.pad(ans, (0, int(5 * SR))), 700, 4)
av = np.stack([fftconvolve(ans, ir_whale[c])[: len(ans)] for c in (1, 0)])
av /= np.abs(av).max()
place(sfx, av[0], WHALE_ANSWER, 0.13, 0.6)
place(sfx, av[1], WHALE_ANSWER, 0.13, 0.2)

def bell(f, dur=1.8, decay=0.55):
    tt = np.arange(int(dur * SR)) / SR
    partials = [(1.0, 1.0, 1.0), (2.0, 0.5, 0.7), (2.76, 0.32, 0.45), (5.40, 0.18, 0.25), (8.93, 0.08, 0.12)]
    s = sum(a * np.sin(2 * np.pi * f * m * tt) * np.exp(-tt / (decay * d)) for m, a, d in partials)
    return s * np.minimum(1, tt / 0.0015)

# ---- title: a soft low boom and one bell, like a door shutting far below
boom = np.sin(2 * np.pi * 46 * t[: int(3 * SR)] * (1 - 0.15 * t[: int(3 * SR)] / 3)) * env_ad(t[: int(3 * SR)], 0.02, 0.9)
place(sfx, boom, TITLE - 0.05, 0.30)
tb = bell(hz(69), 4.0, 2.0) * 0.6 + bell(hz(76), 4.0, 1.6) * 0.25
tbv = np.stack([fftconvolve(tb, ir_music[c])[: len(tb)] for c in (0, 1)])
place(sfx, tb * 0.4 + tbv[0] * 0.6, TITLE, 0.14, -0.15)
place(sfx, tb * 0.4 + tbv[1] * 0.6, TITLE, 0.14, 0.15)

# ---- mix: duck the bed a touch under the dink and the whale so they cut through
duck = 1 - 0.25 * smooth(DINK - 0.03, DINK, t) * (1 - smooth(DINK + 0.3, DINK + 1.2, t))
duck -= 0.45 * smooth(WHALE, WHALE + 0.6, t) * (1 - smooth(WHALE + 3.0, WHALE + 4.5, t))
mix = music * duck * 1.0 + sfx
fade = 1 - smooth(END_FADE[0], END_FADE[1], t)
fade *= smooth(0, 0.05, t)
mix *= fade
mix = hp(mix, 25)
mix = np.tanh(mix / np.abs(mix).max() * 1.2) / np.tanh(1.2) * 0.7  # gentle soft-clip, headroom for loudnorm
wavfile.write(sys.argv[1] if len(sys.argv) > 1 else "teaser.wav", SR, mix.T.astype(np.float32))
