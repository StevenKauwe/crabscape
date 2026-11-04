# Stationary paging churn (#431)

The live game could render normally while simulation time barely advanced. A five-second browser observation counted 882 IndexedDB reads, repeatedly revisiting the same terrain coordinates. Required offscreen sediment pages were evicted by speculative lookahead after each operation cleared its temporary protection.

The runtime cache now remembers recent demand for eight operations (two material steps). Speculation cannot displace that working set; urgent reads still can. Dirty and pinned pages remain protected, capacity stays at 256, and old demand expires. This changes residency only, not terrain, saves, inputs, or baked physics.

In the reconstructed #431 full-window capture, a model with 60 rendered frames per second and page replies after two frames produced these results over 1,800 frames:

| Metric | Before | After |
|---|---:|---:|
| Simulation ticks advanced | 1,572 | 3,332 |
| Frames blocked on terrain | 1,284 | 151 |
| Terrain page reads | 1,692 | 367 |
| Maximum reads of one coordinate | 129 | 2 |

These are deterministic scheduling measurements, not measured browser FPS. Fresh-world kernel timings alone did not expose the problem; archived falling-grain history and delayed reads are essential.

`cargo test -p crabsim --test paging_progress` reconstructs the mechanism without embedding a player's save: a full canyon window, eight valid offscreen sediment packets, delayed replies, and durable dirty-page acknowledgements. Both normal idle and fully grown jump/rotation/dive cases must advance 3,300 ticks within 1,800 frames, block fewer than 150 frames, read any coordinate at most three times, and stay within 256 resident pages. Their movers and complete active terrain checkpoints must equal a resident reference at the same simulation tick, including flow, grain identities and pending falls. Existing cold-revisit/restart and memory-age tests separately guard stored edits.

Setting `data-cell-performance-probe` on the canvas opts into the lightweight `data-cell-performance` value (`simulation_tick:real_frame_delta_seconds`). It avoids allocating/hashing the complete world merely to measure simulation progress.
