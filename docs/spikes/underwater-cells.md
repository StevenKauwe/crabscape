# Underwater cells: disposable draft

This is an opt-in, single-player experiment using the actual CrabScape game and crab. On a computer:

```sh
cargo run --release --bin crabscape -- --cells
```

The [draft’s browser preview](https://stevenkauwe.github.io/crabscape/pr-345/) starts the experiment automatically. Other browser builds can opt in with `?cells`; the normal game remains their default. The opening section (x = 0–2560 world units) uses a 640 × 360 material grid at four world units per cell. Its initial floor and fixed rocks come from the existing level. Hold C and a direction to dig; C + Down digs into loose ground, and jump pushes back out. Shift remains dodge and sprint. On a touch screen, drag the DIG joystick in the direction you want to burrow. Drag up or hold JUMP to escape. The opening has bubbles and a current-driven stalk that can be gripped and climbed by jumping again beside it. It continues to CrabScape’s original water surface. The remaining level uses its original terrain and physics.

The current varies slowly by depth and time. A stateless world-wide background current is sampled by the crab and the upper kelp; the opening grid has its own local current, which moves the lower kelp, bubbles, sediment and crab. The two current models are not yet coupled at the boundary. The real crab moves loose material and creates wakes. Sand and mixed-density mud fall, slide, and can be carried by the current; light mud lifts more easily and neighboring mud grains cohere. Fine visual silt does not add to the material count. The large experimental stalk follows the current and supplies the crab's climbable grip; the smaller grid kelp is decorative. Water fills empty cells. The fluid field, lift, cohesion, reaction forces and crab collision are approximations, not a conservation-accurate fluid or rigid-body solver.

**Do not merge this as a complete conversion.** Terrain resets on reload, normal saves and replay recording are disabled in this mode, and co-op is disabled because terrain changes are not synchronized. The grid is a bounded opening-area test: the boundary, original level objects, lighting/memory, phone performance, asset style and larger crabs need further work. Ground-walking animals sample the same smoothed support surface and stir the water, but their full physics and all world objects are not yet coupled to the cell world. The upper stalk follows the sampled background current, while bubbles still recycle at the top of the cell field. The prototype has not been benchmarked on a phone. Desktop keys and the existing touch controls both feed the real crab's controls.

The original separate sandbox remains available for comparison:

```sh
cargo run --release --bin underwater_spike
```

That sandbox has its own simpler crab and controls: WASD/arrows swim and push; Q reverses the current; R resets.

## World-scale simulation and catch-up

The background current is a deterministic function of world height and simulation tick. It can be sampled at a future tick without running the intervening ticks, and the camera never changes it. This is implemented outside the opening cell grid; the grid continues to update normally even when the crab is elsewhere. The present grid and the background do not yet exchange disturbances at their boundary.

A full conversion needs persisted spatial chunks, deterministic actor-driven activation (including on-map AI), and a consistent ordering for transfers of water, grains and forces between neighboring chunks. A chunk can sleep exactly only while nothing outside can affect it, or if all such boundary events are retained and replayed. An arbitrary disturbed cellular simulation cannot in general jump to its exact later state for free: exact catch-up has to replay its history, possibly including neighboring chunks. A cheaper coarse model can be advanced directly, but its result is an approximation and must be the same regardless of camera zoom or the order in which regions are visited. Decide those rules before replacing the existing single grid; do not silently discard or regenerate disturbed terrain.

Still needed: connect the fine grid to the background current without changing walking or sediment behavior; implement and persist chunks and boundary events; move creatures and material across chunk borders; render and move bubbles beyond the opening grid; and measure catch-up latency, memory and frame time on a phone. Add tests comparing continuous execution with sleep/wake under the chosen rules, including border crossings and different camera views. The existing native/WebAssembly determinism job does not exercise this experimental mode, so cross-target bit-identical behavior for the complete cellular simulation is not yet established.
