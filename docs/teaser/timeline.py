"""The teaser's beats, in seconds. Shared by the picture (render.py) and the sound (audio.py) so they stay in sync."""
DUR = 17.0
FPS = 30
LANTERN_SPARK = 1.2      # bacteria in the jar wake: a pinprick of light
LIGHT_SPREAD = (2.0, 4.8)  # the lantern's light reaches out across the scene
EXPERIENCE = 5.0         # "Experience…" fades in
DINK = 6.3               # the level-up dink sounds...
POPUP_IN = DINK + 0.12   # ...and the pop-up springs in just after it, with the sparks
POPUP_OUT = 8.6          # pop-up leaves
WHALE = 8.6              # whale call begins (slow swell)
WEALTH = 9.2             # "the wealth of the deep." fades in, under the call's rise
WHALE_ANSWER = 11.6      # a fainter, farther answer
DARK = (12.4, 14.0)      # light withdraws into the lantern, then out
TITLE = 14.3             # CRABSCAPE on black
END_FADE = (16.0, 17.0)
