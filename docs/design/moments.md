# Game Moments — playable doctrine snippets

Four small, playable beats built on the existing engine so the owner can
*feel* the doctrine before committing to the slice. Each moment is a
self-contained scenario: fixed setup, one objective, one feeling target.

Framework (built first):
- A moment picker (M key from the title / current level; Esc returns;
  R restarts the moment).
- `Moment { name, blurb, setup, objective_text, done/fail }` registry;
  each moment in its own file under `src/moments/`, registered in one place.
- Shared molt state machine (Hard → Committed → Ecdysis → Soft → Hardened):
  Committed locks eating (premolt anorexia); Ecdysis is immobile; Soft is
  defense ≈ 0 and slow; Hardened unlocks the feat + eating.
- Shared biomass counter (units): eat raw = stamina now; banked at home or a
  rest stop = molt fuel. All numbers live in `tuning.rs`.
- Death-moult entity: holds cargo + prepaid biomass; wild moults carry a
  shrinking decay ring (carrion clock); home moults persist.

The moments:

1. **THE TURN-AROUND** (`turn_around.rs`) — a pit with food at three depths;
   reserve that cannot cover deepest + return; one carry slot. Objective:
   come home with two items. Feeling: the push-or-bank pang. C1/Cmd 2.
2. **FIRST MOLT** (`first_molt.rs`) — home scene; a biomass pile; the bill
   posted; eat some raw on the way (spend the savings?) then commit at the
   shell: anorexia locks, the ceremony plays warm and short, the shell
   hardens, and the moment closes on the FIRST MEAL after. Feeling: the
   ceremony is cozy; growth is anatomy. Cmd 16/C11.
3. **FORWARD-MOLT SIEGE** (`forward_siege.rs`) — a rest stop beside a gate;
   biomass carried from home; commit in the wild; a patrol converges during
   the soft window; hide, or autotomize a claw to delay ecdysis; when
   hardened, swim through the gate wearing the new verb. Feeling: chosen
   terror; risk bought with reach. Cmd 16 tier 2 + autotomy-delay.
4. **THE RESCUE** (`rescue.rs`) — you died soft: a card says so; your moult
   (biomass + the tooth) sits in the pit with a decay ring; scavengers
   gather as it shrinks. Reach it, take your future, come home. Feeling:
   the bay eating your future. Cmd 15/RF-1b.

Build rules: `cargo check` green before any commit; numbers in `tuning.rs`;
deterministic (seeded) per repo rules; no new assets — ink/palette only
from existing style; local branch only, NO push, NO PR.

Feel targets are in the doctrine docs; if a moment doesn't produce its
feeling, the doc or the tuning is wrong — note it, don't paper over it.
