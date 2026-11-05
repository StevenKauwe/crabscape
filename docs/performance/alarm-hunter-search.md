# Alarm hunter selection

An alarm-jelly flash chooses the nearest eligible, reachable moray, with enemy ID breaking equal-distance ties. Previously it swept the hunter's body through terrain for every eligible animal before comparing distances. Candidates now use the same distance/ID order first; the first clear path wins, so farther terrain queries are unnecessary.

This changes neither alarm range, hunter leash, line-of-sight requirements, nor the chosen responder. Searches remain bounded by the existing enemy list. Candidate sorting costs O(n log n) and one temporary vector; the improvement is fewer expensive terrain queries, not a demonstrated whole-frame speedup.

Validation:

- All 32 visibility combinations for five hunters (including ties), in forward and reversed storage order, preserve the previous winner.
- Twelve reachable hunters require one clearance query instead of twelve. Two blocked nearer hunters require three queries.
- Existing alarm-jelly encounter tests cover committed attacks, hiding, selection, bounded movement, deterministic state transfer, and the optional traversal route.

Run `cargo test -p crabsim --lib alarm_jelly::search_tests` and `cargo test -p crabsim --test alarm_jelly`.
