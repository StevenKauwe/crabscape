# Next goals: the pull after the guide

The first-hour guide ends at about minute 15. From then until level 15 in every skill (about three hours), the
crab needs a steady stream of near-term wants, or the player drifts. Next goals are that stream: RuneScape's "one
more level", not a chore list. The rules are in `crates/sim/src/economy/goals.rs`.

## What the board offers

Once the guide is done or skipped, the board keeps up to **three** short goals open at a time, picked from what is
close:

| Goal | Picked when | Done when |
|---|---|---|
| **The pinned want** ("Make kelp mat") | A want is pinned | It is made or built |
| **One thing away** ("Make mussel and snail hash") | A recipe or home piece the crab has the level for is short of one kind of thing, and the crab can already gather or make that thing | It is made or built |
| **The next level** ("Prying 10") | Its unlock line says what it opens. One is for the skill that lags (the lowest level), one for the skill trained most lately ("one more level"). A level that opens a node the crab has seen comes first in both ("Opens the barnacle cluster you saw"); the one nearest its next level breaks ties. Only skills within 3 levels of the lowest are offered, so following the board keeps every skill moving | The level is reached |

There is one goal per skill and one per recipe, and at most one "one thing away" goal besides the pinned want.

## What a goal pays

A tenth of a level in its skill, in experience: 20 Prying XP for reaching Prying 10. The next goal is offered at
once. Nothing costs anything or slows anything.

## The board follows the crab

A goal the crab has not moved on for four minutes is quietly swapped for one nearer what it is doing: it went off
to do something else, and a quest log full of stale lines is a chore. A goal moves on when the skill earns
experience, or when the crab holds more of what the recipe needs than it has before. The swapped goal rests (it is
not offered again while four newer goals have been swapped). A goal open ten minutes is swapped too, unless it is nine tenths done: these are near-term wants, not long hauls. A pinned want is never swapped: the player chose it.

## Where it shows

- **The corner pill:** one line, the goal nearest done ("Next: Prying 10, 80%", "Next: kelp mat, 2/4 kelp"). A
  pinned want away from home, or the guide while it runs, takes the pill first, as before.
- **The board** (the workbench's Goals tab): every open goal at the top, with how far, what it opens or what is
  short, and what it pays.
- **A card** when a goal is done ("GOAL DONE / Made kelp mat / +12 Weaving XP / Next: Prying 10"). A level goal's
  bonus rides on that level's own card, so there is one card, not two.

## Measured

`crates/sim/tests/first_hours.rs` logs every goal in two three-hour runs of mixed play:

| Run | Met | Swapped | Longest open |
|---|---|---|---|
| The bot plays its usual way and ignores the board | 40 | 59 | 9.8 min |
| The bot follows the board: work for an open goal comes before the lowest skill, at home and out | 100 | 40 | 6.8 min |

`first_hours_targets` holds the follower to about 10 minutes open (10.5), and the bot that ignores the board to 13 (a goal nine tenths done is left to finish). The run that ignores the board must meet at
least 25 goals. The run that follows it must meet more goals than it swaps.

Both runs bring every skill to 15 within three hours (since the row shore crab and Fighting's 500% at levels 10 to 14): the last at 151.5 minutes ignoring the board and 163.4 following it (Fighting 161.1).

## Co-op

Goals are personal: they live in the crab's own economy, change only on its own device, and are never touched for a
remote crab. A home piece goal is done when the shared home has the piece, whoever built it.
