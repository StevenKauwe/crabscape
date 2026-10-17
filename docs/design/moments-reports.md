# Moments QA — the owner's pre-play reports

> QA: @z-ai-agent-0#2 | branch: design-bible-moments | gate: cargo check 0 ·
> tests 79/0 · clippy 0 (verified on this worktree) | each report: one
> paragraph, what to feel for, and whether the mechanism delivers it.

**Verdict up front: all four moments are mechanically honest to their
feeling targets. Play them in picker order.** Two QA notes follow the
reports; neither blocks play.

---

## 1 · THE TURN-AROUND — *the push-or-bank pang*

Three piles at three depths: one at your feet, one a patient climb down the
shelf, one hanging in the black at the shaft floor — and a reserve that
covers the dive but not the prize and the return together. Bringing two
home means the free grab plus one real choice, and the moment's genius is
that both routes cross the same pang in different currencies: the shelf
costs minutes of slow climbing, the prize costs more strokes than you hold,
and when the rings die mid-water the carried pile becomes your only fuel —
eat it (spend the savings on the present) or cling and climb with the
future on your back. The status logic has teeth: eat too much in the field
and the objective becomes arithmetically impossible — the moment fails you
for spending your own savings, which is exactly the doctrine. Feel for: the
instant you look at the prize, then at your reserve, and understand the
arithmetic. That pause is the whole game in miniature.

## 2 · FIRST MOLT — *the ceremony is cozy; growth is anatomy*

The cleanest doctrine snippet of the four: a biomass pile in the field, an
empty larder with the bill posted (six units — one feat), and the shell
doorway. Eat raw on the way and the savings shrink; haul it home and E at
the shell commits you — and committing locks eating (the anorexia is real,
engine-side: the sim refuses food from commit to hardening). The close is
the doctrine's best beat: no fanfare, the shell hardens, and the moment
ends on the first meal after — the ordinary rhythm as the reward, exactly
as C11 demands. This file adds zero numbers of its own; every rule comes
from tuning, which is the discipline every other moment should envy. Feel
for: the moment of hesitation at the shell door with the bill covered —
the ceremony is warm precisely because the wild isn't.

## 3 · FORWARD-MOLT SIEGE — *chosen terror; risk bought with reach*

The tier-2 ladder made playable: you carry the bill from home, commit at a
rest stop in the wild, and the Watcher — one deterministic finder sweeping
three waypoints — is the entire threat model, faithful to the soft-window
doctrine's "one finder is the end." While soft you cannot stroke or jump
(engine law, regression-tested), you move at ×0.45 walking and ×0.6
climbing, and the only shelter is gripping the weed, blind and blind to.
The siege's distinctive verb is fear priced in body: shedding a claw
mid-premolt delays the ecdysis three seconds — autotomy as a composure
tool, not an escape. Win by hardening and climbing out through the gate
wearing the new verb. Feel for: the committed silence after you press
commit at the rest stop — the point where the terror was your idea.

## 4 · THE RESCUE — *the bay eating your future*

You wake soft at home with an empty wallet: your biomass and your tooth
are in your old body at the pit's floor, and the moult carries a decay
ring on the carrion clock — 180 seconds, visibly shrinking. The soft
window outlasts the clock, so the entire rescue is paper-shell: no
strokes, no jumps, climbing at ×0.6 under a countdown, with the moment
done only when the tooth is *fitted at home* (the full conversion law —
surviving the way home, not merely touching the prize). The fail state is
the doctrine's grimmest sentence made literal: if the ring empties before
you reclaim it, the bay ate your future. The presentation note (scavenger
density as the visible progress bar, harrying not killing) is staged for
the art pass — the clock already carries the fail. Feel for: the moment
you're halfway down the wall, ring shrinking, and realize the climb back
up is the same length as the climb down.

---

## QA notes (filed to the synthesis workspace ledger)

1. **RESCUE numbers not yet migrated to tuning** (the one discipline gap):
   the decay/soft constants live in-file with "proposed" comments, and the
   `wild_decay_seconds = 420` assignment is dead — overridden immediately
   by the moult's own 180-second `decay_left`. Effective numbers: ring
   180 s, soft 480 s, bill 6. Migrate before the slice; not a play-blocker.
2. **Status-logic tests**: THE TURN-AROUND ships three (ongoing / done /
   over-eating-fails); the other three ship none. The 79-test suite covers
   the movement laws and physics beautifully — the moments' own win/fail
   contracts are the remaining blind spot. Copy the turn-around pattern.

**Strengths on record:** the soft-shell law is real physics with three
regression tests by name; the Watcher is doctrine-shaped (one finder, no
generic enemy); autotomy is gated to the Committed window exactly per the
crab-ocean canon; FIRST MOLT's zero-numbers discipline is the file to
imitate. The moments are ready for the owner's hands.
