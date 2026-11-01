# Underwater cells: disposable draft

This is an opt-in, single-player experiment using the actual CrabScape game and crab. On a computer:

```sh
cargo run --release --bin crabscape -- --cells
```

On the draft's browser preview, add `?cells` to the URL. The normal game remains the default. The opening section (x = 0–2560 world units) uses a 640 × 360 material grid at four world units per cell. Its initial floor and fixed rocks come from the existing level. Hold Shift/dodge and a direction to dig; Shift + Down digs into loose ground, and jump pushes back out. On a touch screen, drag the DIG joystick in the direction you want to burrow. Drag up or hold JUMP to escape. The opening has a nearby water surface, bubbles and a current-driven stalk that can be gripped and climbed by jumping again beside it. The remaining level uses its original terrain and physics.

The real crab samples the current, moves loose material and creates wakes. Sand and mixed-density mud fall, slide, and can be carried by the current; light mud lifts more easily and neighboring mud grains cohere. Fine visual silt does not add to the material count. The large experimental stalk follows the current and supplies the crab's climbable grip; the smaller grid kelp is decorative. Water fills empty cells. The fluid field, lift, cohesion, reaction forces and crab collision are approximations, not a conservation-accurate fluid or rigid-body solver.

**Do not merge this as a complete conversion.** Terrain resets on reload, normal saves and replay recording are disabled in this mode, and co-op is disabled because terrain changes are not synchronized. The grid is a bounded opening-area test: the boundary, original level objects, lighting/memory, phone performance, asset style and larger crabs need further work. Ground-walking animals sample the same smoothed support surface and stir the water, but their full physics and all world objects are not yet coupled to the cell world. The opening-only surface is fixed in world coordinates, so its position on screen varies with device shape and zoom; it changes abruptly back to the original far-away surface at the edge of the experiment. The prototype has not been benchmarked on a phone. Desktop keys and the existing touch controls both feed the real crab's controls.

The original separate sandbox remains available for comparison:

```sh
cargo run --release --bin underwater_spike
```

That sandbox has its own simpler crab and controls: WASD/arrows swim and push; Q reverses the current; R resets.
