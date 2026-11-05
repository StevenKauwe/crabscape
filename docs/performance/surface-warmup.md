# Prepare the surface shader before traversal

The three sea-surface meshes use the same vertex-coloured ColorMaterial layout.
Their first visibility previously queued that shader variant during ascent.
A hardware Chrome profile captured a 233 ms frame gap with about 221 ms sampled
inside WebGL getProgramParameter; a later shader inspection identified this
variant. This is evidence for a first-use program hitch, not proof that every
transition stall has this cause.

The existing sky mesh now bypasses CPU frustum culling for three observed visible
frames at startup. Its real geometry remains outside the camera and is clipped
by the GPU. This queues the shared pipeline without adding a dummy overlay or
changing surface appearance. Both temporary components are then removed so
normal culling resumes. Simulation, terrain, saves and co-op are unchanged.

The unit test checks that hidden updates do not consume the warmup and that
normal culling returns after three visible updates. The existing shared-edge
surface test also passes, as does native all-target game clippy.

## Browser comparison

November 5, hardware Chrome on the same Mac, optimized wasm using Binaryen 133
-Oz, isolated fresh browser profiles. A pre-navigation WebGL LINK_STATUS hook
recorded only ColorMaterial shaders using vertex colour. The route held right,
repeatedly paddled upward for 15 seconds and dived for 15 seconds with the hulk
cheat. No player save or browser profile was accessed.

- Baseline 6dbbbfc: first surface program at 12,397.9 ms, during traversal,
  crab (1212, 4788); call took 4.7 ms.
- Candidate based on 83a8822: first surface program at 877.5 ms, during startup,
  crab (160, 12); call took 3.8 ms. No new surface program during traversal.
- Both reached y=4951 and returned below y=-190; maximum recorded frame gap was
  16.8 ms in both runs. The timing difference is placement of preparation, not
  a measured reduction of a repeated 233 ms hitch: the long stall did not recur
  in this paired test, and GPU caches can persist across browser profiles.

Local captures: /tmp/crab-surface-program-6630/capture.json (baseline) and
/tmp/crab-surface-program-8364/capture.json (candidate). The separate committed
frame_probe remains available for whole-frame ascent/descent regression checks.
Guest co-op transfer pauses and Claude's unfinished cellular-compute work remain
separate. Three visible frames give pipeline queueing opportunities; this is not
a guarantee that every driver finishes all compilation before play begins.
