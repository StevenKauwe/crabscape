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

## Tools today
Everything is pure Rust and `cargo run` (nothing outside this repo):
- `crates/crabrig`: the crab as a parametric puppet; writes the SVG rig (`crabrig parts`) and the game's atlas +
  parts file (`crabrig export`).
- `crates/painter` (`crabpaint`): the owner's oil painter and its pure-Rust p5.brush port (fill, wash, ink stroke).
- `tools/paint-oil-crab.sh` + `tools/oil-atlas/` oil-paint the parts with it.
- The model sheets (`art/crab/model-sheet*.png`) were drawn by the old JS (in git history); porting the sheet to
  crabrig + the Rust brush is an open task.

Taste calls on art always go to a human (`needs-human`): show options as screenshots or short clips.
