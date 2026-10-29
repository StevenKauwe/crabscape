# Co-op promises

What playing together promises, in plain words, each with its time limit and the check that proves it. Two kinds of
check stand behind every promise:

- **A scenario** (`cargo cooptest`, `tools/cooptest`): real Chrome browsers, one per player (each its own device,
  with its own saved game and lobby), the real web build and a local introducer. It does what a player does and reads
  the game's own words (`data-coop` on the canvas: "Playing with 1 friend", "Waiting for your friend...") and whose
  world each browser plays in (`data-world`: solo, host or guest). Every scenario also fails if any browser shows the
  "couldn't start" screen or has an uncaught error.
- **A fast test** (`cargo test`, in `src/coop.rs`): the same rule with a pretend clock and no browser, in
  milliseconds.

Times count from when the game that came back is running again (its page loaded): loading the game is its own
promise (`cargo webtest`). "Together" means every game says it plays with the others, and they stay so for 6 s
(longer than a game takes to notice a friend went silent, so a game that hasn't noticed it was cut off yet doesn't
pass).

## Joining

| Promise | Limit | Scenario | Fast test |
|---|---|---|---|
| A friend who opens your link plays with you | 10 s | `join` | `your_own_link_makes_you_host_and_your_lobby_opens_whenever_you_play` |
| A friend who reloads is back with you, even on a network where the browser takes 40 s to finish gathering its addresses (a VPN, a virtual machine's network, IPv6 with no way out) | 10 s | `guest-reload` (slow gathering pretended in both browsers) | `a_friend_who_reloads_is_back_within_10_seconds` |
| You reload: your lobby opens again by itself and your friend is back | 10 s | `host-reload` | `a_friend_who_reloads_is_back_within_10_seconds` |
| A friend who presses Start over is back, with no error | 10 s | `guest-start-over` | (Start over is a reload: as above) |
| You press Start over: same link, your friend is back, no error | 10 s | `host-start-over` | `start_over_wipes_only_this_builds_keys` (`save.rs`: the lobby is kept) |

## The lobby

Each device has one lobby code, made the first time it presses Invite and kept for good with the save (each build has
its own). The link is that code.

| Promise | Limit | Scenario | Fast test |
|---|---|---|---|
| Your link is the same every day | always | `own-link`, `host-reload`, `host-start-over` | `start_over_wipes_only_this_builds_keys` |
| Once you've shared it, your lobby opens whenever you play, with "Lobby open: waiting for a friend..." under the button | at start | `own-link` | `your_own_link_makes_you_host_and_your_lobby_opens_whenever_you_play` |
| Opening your own link makes you the host | at start | `own-link` | same |
| A second tab on the same device plays alone instead of opening the lobby twice | at start | (one device, two tabs: fast test only) | `a_tab_knows_when_another_tab_has_the_lobby_open` |
| A friend on your link while you're away plays alone ("Waiting for your friend..."), then joins you when you start | 10 s after you start | `waiting-friend` | `a_friend_waiting_in_an_empty_lobby_stays_put_and_a_dead_connection_is_redone`, `a_waiting_friend_takes_the_hosts_world_from_its_first_word` |
| Two friends waiting each play alone: never one frozen half of the other's world; both join you when you start | 10 s after you start | `two-waiting` | `a_waiting_friend_takes_the_hosts_world_from_its_first_word` (another waiting friend is not the host) |
| After "New link" (Settings, Co-op, asked twice), the old link no longer reaches you and the new one does | at once | `new-link` (a friend on the old link is alone for 20 s) | (the code is random: no pure rule) |

## When things go wrong

| Promise | Limit | Scenario | Fast test |
|---|---|---|---|
| A game frozen or cut off for a few seconds (a dropped connection, a phone that stalled) is back with its friend | 10 s after | `stall` (each side frozen 6 s in turn, past the 5 s after which a friend counts as gone) | `a_friend_waiting_in_an_empty_lobby_stays_put_and_a_dead_connection_is_redone` (a dead connection); taking a silent device back lives in the browser-only part of `coop.rs`, so the scenario is its check |
| The introducer restarts or sleeps mid-game: both games carry on alone (nothing freezes or errors), then are back together | 15 s after it's back | `introducer-restart` (local introducer only) | `a_friend_waiting_in_an_empty_lobby_stays_put_and_a_dead_connection_is_redone` |
| A friend lost with the connection still up (they closed the tab) is looked for again | every 15 s | (part of `new-link`) | `lost_company_is_looked_for_every_15_seconds` |
| Anything of the above, in any order, any number of times: always back together, never an error | as above | `cargo cooptest --stress` (random steps from a printed seed) | |

## Voice

Every scenario runs with voice off, or with `--voice on` (both players' mics on, a fake beeping microphone) where it
also checks each player hears the other within **15 s** of being together, after every reload or drop. A reload
turns your mic off (the browser asks again); turning it on brings voice back within the same 15 s.

## Running them

| What | Command | Where |
|---|---|---|
| Every scenario once, voice off (CI, every PR) | `cargo cooptest --repeat 1 --shard I/3` | the `co-op promises, part I of 3` jobs, at once (behind the required `browser` check) |
| Every scenario 3 times, voice off and on, and a stress run | `cargo cooptest --voice both`, `cargo cooptest --stress --steps 12` | `coop.yml`, after each deploy to main and daily, local introducer |
| The live site and the real introducer | `cargo cooptest --url https://stevenkauwe.github.io/crabscape/` | `coop.yml`, same times |
| A failed stress seed again | the `replay:` line it printed | anywhere |

Failures on `main` (the after-deploy and daily runs) open or add to the `main-broken` issue.

## How the game keeps them

- The lobby code lives in the save (`-lobby`, `src/coop.rs`); Start over keeps it (`save.rs`); "New link" replaces it
  and moves the host to the new room.
- A guest goes into the host's world only when it hears the host (`take_in`), and back to its own when the host goes:
  friends waiting in an empty lobby play alone.
- A device silent for 5 s (frozen, its network stalled, or a friend playing alone while waiting) is no longer played
  with, but its connection and voice call stay up, and it is met again the moment its crab speaks. Before, it was
  dropped from the connection list: after a stall both sides waited 15 s for a fresh join, and two friends who waited
  together never met (found by the `stall` and `two-waiting` scenarios).
- A device whose connection is dead joins its room afresh after 5 s; one that lost its friends but is still connected
  looks for them every 15 s. A browser's address gathering gets at most 3 s
  (`vendor/matchbox_socket`, see `decisions.md`).
- An introducer restart ends every game's connection (it was the connection's other end): each joins afresh 5 s
  later and finds the other. The introducer stores nothing, so a restart forgets nothing worth keeping.

