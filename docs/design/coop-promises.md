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
| Each of you sees the other's name over their crab (a friend on a game from before names: no name, and still plays with you) | 5 s after meeting | `join` | `a_name_comes_back_whole_from_its_bytes_whatever_it_spells`, `a_message_from_a_newer_game_is_dropped_unread` |
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

## Versions

Friends play only on the same version of the game (the commit its web build was made from). Nothing in co-op is
kept readable by an older game: the older one reloads into the new one. Nothing is lost by it: the save is on the
device and the link stays the same, so it rejoins as usual. (Saves and character codes are another matter: when a
change breaks them they get a migration, never a lost crab.)

| Promise | Limit | Scenario | Fast test |
|---|---|---|---|
| A game never reads a message from another version as game data, and sends it none | always | `new-version` | `a_hello_says_the_version_and_is_never_a_game_message` |
| The older game (the one that isn't what the site serves now) shows "A new version of CrabScape is out — updating…" and reloads; then the two play together | 10 s once it's running again | `new-version` (the host on the build before a pretend deploy, its friend on the new one) | `friends_on_different_versions_never_read_each_others_messages_and_the_old_game_reloads` |
| No reload loop: a game that reloaded and is still on the same version (a stale cache) says "Your friend is on a different version" instead; so does one that can't read the site's version | at once | | same |

## Voice

Every scenario runs with voice off, or with `--voice on` (both players' mics on, a fake beeping microphone) where it
also checks each player hears the other within **15 s** of being together, after every reload or drop. A reload
turns your mic off (the browser asks again); turning it on brings voice back within the same 15 s.

Voice chat is written down (`src/scribe.rs`); these are promises too, checked by fast tests (the browser runs can't,
without downloading the 110 MB model):

| Promise | Fast test |
|---|---|
| Each device writes down only its own player's voice; a friend's line lands in everyone's transcript with their crab name | `a_line_sent_by_a_friends_device_lands_in_my_transcript_with_their_name` |
| Lines older than 15 minutes drop, on every device, also after a reload | `lines_older_than_15_minutes_drop` |
| "Write down my voice" off: the device writes nothing and sends no lines; friends see "(not written down)" | `the_write_down_setting_stops_writing_and_sending`, `a_friend_who_doesnt_write_shows_as_not_written_down` |
| A test microphone's beeps are never speech, so the browser runs never start the model | `silence_clicks_and_a_test_microphones_beeps_are_not_speech` |
| A report carries voice chat only when ticked; the introducer caps it and nothing in it pings people or escapes its fold | `a_report_carries_voice_chat_only_when_ticked`, `voice_chat_cannot_ping_people_or_break_out_of_its_fold` |

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
- Every connection starts with a hello naming the game's version (`CRABSCAPE_VERSION`, set by `tools/build-web.sh`);
  until a device's hello says this game's own version, nothing it sends is read and nothing is sent to it (a device
  that sends no hello in 5 s is a game from before hellos). On a mismatch each game fetches `pkg/commit.txt` (the
  site's version, past any cache): the one that doesn't match it reloads, noting the version it left in the tab's
  own storage so it never reloads for it twice.
- An introducer restart ends every game's connection (it was the connection's other end): each joins afresh 5 s
  later and finds the other. The introducer stores nothing, so a restart forgets nothing worth keeping.
