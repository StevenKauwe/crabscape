# Per-search swimmer clearance cache

Moray A* routing revisits the same 41×41 grid cells during neighbor expansion and
when testing diagonal corners. Each clearance query checks static solids, cellular
terrain and the floor. The level, body size and regional leash remain fixed for the
duration of a search.

Cache each grid cell's blocked/clear result within that search. The cache is a
bounded 1,681-entry stack array and is discarded immediately afterward, so digging,
falling terrain and changing goals are visible to the next search. The final swept
line check still samples its exact original points. Search order, cost, tie breaks,
visit limit and returned waypoints remain unchanged.

## Evidence

`cargo test -p crabsim --lib navigation -- --nocapture` compares cached and uncached
routes across 24 combinations of start, goal and obstacle geometry, including
fractional origins, diagonal corners, a wall with a route around it and a sealed
wall. Existing routing assertions also pass.

The sealed-wall regression counts clearance calls rather than measuring a noisy
wall-clock threshold: **13,683 → 943 queries (93.1% fewer)**, with the same no-route
result. It guards the per-search grid bound and requires at least a fourfold
reduction against the uncached reference.

This is a reduction in routing collision work, not a measurement of whole-game
frame time or a claim that cell-transition stutter is fixed. Morays only perform
this search when an obstacle requires routing and their retry interval permits it.
