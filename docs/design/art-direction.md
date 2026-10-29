# Art direction

Hand-drawn 2D: ink lines with painted fills, and oil-painted assets. Highly stylised to carry emotion (proportion,
colour and light are pushed so you feel what the crab feels). Accuracy lives in anatomy and behaviour, not in
realistic rendering.

## Rules
- **Variation lives in each shape.** Every fill and line has its own wobble and grain that moves with it and stops at
  its edge. **No full-frame filters, texture overlays or global noise**: they read as a post effect ("filter-like,
  not genuine art"). Change the drawing, not the frame.
- **Tame but bold.** Calm, confident, unassuming but never boring. Lighting does the heavy lifting and should "hit
  right": not sunset cliché.
- **Painted like a person paints.** The oil look follows human constraints: arm arcs, one paint load per stroke,
  strokes along edges not across them, the right brush for each job (flat, filbert, round, knife), small details as
  single touches, high-contrast accents kept crisp. Never "Photoshop brushes".
- **Colour key.** Warm amber = home, the crab and its lantern. Teal to near-black = the sea getting deeper. Violet and
  cyan glow = the strange deep (wild bioluminescence is blue-green; only our farmed strain glows yellow).
- **Readable at phone size.** The crab and every danger read as a silhouette at about 40 px tall.
- **Built in layers.** Foreground, play layer and several parallax background layers.
- **Cut-out puppets.** Characters are drawn in parts (rig: `art/crab/crab-parts.svg`) and animated by moving parts.
- **Paint ahead, play live.** Paint parts and backgrounds offline (lit and unlit versions); the game blends them by
  distance to the lantern at runtime, because real 2D lights are too heavy for phones.

## The player crab
A composite swimming crab (portunid paddles on the 5th legs), crusher claw on its right (screen left), cutter on its
left, holding a jar of yellow glowing bacteria. Side-on play shows its **front**. Anatomy rules and sources:
`art/crab/anatomy-notes.md` (ten legs, one-plane hinge joints, claws held off the ground, abdomen tucked under).

## Sound and music
Everything is made in code at start-up (no sound files): short impact sounds (`src/sound.rs`) and the music
(`src/music.rs`), all muffled a little, as sound is under water. The music is calm, cozy and a bit eerie, and quiet
enough never to fight the game's sounds. It never loops: it is written a beat at a time (a beat is a second) from
fixed material and a seeded generator. Settings' Audio tab has a slider per kind of sound (Master over Music, Effects and Voices)
and a choice of two music tracks, **Themes** (the default) and **Calm**. A change of track waits for the next chord.

**Calm** (`src/music/calm.rs`) is the first music, kept as it was: soft pads holding a chord for eight beats and
now and then a small bell motif with an echo; warm at home and in the shallows, lower in the basins, hollow down the
pit and in the canyon; in a fight the bells stop and a muffled heartbeat and a held uneasy note come in.

**Themes** (`src/music/themes.rs`) is one piece built from four motifs (D3 is the home note):
- **Home theme**: A D E F, then E C A (asking) or E C D (answering). Every place plays it, changed: on the flute or
  mallet at home, high on the mallet in the shallows, twice as slow on the mallet in the basins, low on the mallet down
  the pit, on cold glass in the canyon, on the flute at the lights. Its leap (A up to D) rings far off when the shell
  is nearly broken; its opening runs twice as fast, low on the mallet, in a fight.
- **Deep motif**: D A B♭ A, the home theme's leap turned upside down plus a sighing step. On the flute in the basins,
  twice as slow and low down the pit, upside down in the canyon, a fragment at home. Its first two notes sound low when
  a creature has noticed the crab.
- **Octopus**: D E♭ C♯ E C D, half-steps coiling out from D and back, low, over fifths sliding a half-step: under its
  fight only.
- **The lights**: A D E F♯ G♯ A, the home theme carried on up to the octave: the sea pens answer the home theme's
  question.

A mode per place colours them: home Dorian (warm), shallows major pentatonic (bright), basins Aeolian, pit Phrygian
(the half-step above D), canyon whole-tone (cold), lights Lydian (wonder). All keep D, so a new place starts at the
next chord on its D chord. Music comes in periods of four chords: the lead asks, a second voice answers or rests, the
lead answers, then a fragment or a rest. **Layers**: the pads and bass always; the tunes while exploring; thinner
tunes and the deep motif's falling fourth when noticed; heartbeat (fights only), an uneasy held note and the fast
home theme in a fight; the octopus motif under the octopus; the far-off leap with a nearly broken shell. The heartbeat
lasts 4 s after a fight and the unease about 10 s more. **Instruments**: pads that slowly brighten and darken with a
gentle chorus, a bass with its octave (nothing under 55 Hz), a breathy flute with a slow vibrato, a soft marimba-like
mallet, glassy bells, a light underwater space (two short muffled echoes) and a longer echo on bells and flutes.

**Stingers** (over either track, in the key playing): egg (three quick mallet notes up), level up (the home theme's
opening on glass, fast), feat ready (the home theme carried to the octave), moult hardened (E C D, settling, on the
flute), creature beaten (F E D on the mallet over a low D), the canyon found (the deep motif on glass), the lights
found (their motif on glass), death (D A F D falling on the flute) and the demo's ending card (the home theme
answering on the flute).

To hear it outside the game: `MUSIC_WAV=music.wav cargo test music -- --ignored` writes about five minutes walking
through every place, layer and stinger (`MUSIC_TRACK=calm` for the other track), with a `.txt` listing every note.

## Tools
Everything is pure Rust and `cargo run`:
- `crates/crabrig`: the crab as a parametric puppet; writes the SVG rig (`crabrig parts`) and the game's atlas +
  parts file (`crabrig export`).
- `crates/painter` (`crabpaint`): the owner's oil painter and its pure-Rust p5.brush port (fill, wash, ink stroke).
- `tools/paint-oil.sh` + `tools/oil-atlas/` oil-paint the crab's and the creatures' parts with it.
- The model sheet (`art/crab/model-sheet-v0.2.*`) is not generated by crabrig yet; that is an open task.

Taste calls on art always go to a human (`needs-human`): show options as screenshots or short clips.
