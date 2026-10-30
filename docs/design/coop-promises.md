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
| Words you type in the box (Enter) reach your friends and show in their top left corner; a link in them (https only) opens only after a yes that shows the whole address; a friend can't flood your screen, speak as someone else, or hide text with invisible characters | 5 s | `join` | `a_line_of_chat_crosses_the_wire_and_a_forged_one_is_dropped`, `cleaning_removes_what_hides_or_reverses_text`, `only_plain_https_links_are_offered`, `a_friend_cannot_flood_the_screen` |
| A friend who reloads is back with you, even on a network where the browser takes 40 s to finish gathering its addresses (a VPN, a virtual machine's network, IPv6 with no way out) | 10 s | `guest-reload` (slow gathering pretended in both browsers) | `a_friend_who_reloads_is_back_within_10_seconds` |
| You reload: your lobby opens again by itself and your friend is back | 10 s | `host-reload` | `a_friend_who_reloads_is_back_within_10_seconds` |
| A friend who presses Start over is back, with no error | 10 s | `guest-start-over` | (Start over is a reload: as above) |
| You press Start over: same link, your friend is back, no error | 10 s | `host-start-over` | `start_over_wipes_only_this_builds_keys` (`save.rs`: the lobby is kept) |

## The lobby

The link is a game world with a main host (you, its owner), and anyone on it can meet up there. Each device has one
lobby code, made the first time it presses Invite and kept for good with the save (each build has its own). The link is
that code. While you're away, the friends on your link play together in one friend's world: that friend stands in as
host until you come (its line says "You host until the main host is here", the others' "Crab 07 hosts until the main
host is here"). When you come, it's your world again.

| Promise | Limit | Scenario | Fast test |
|---|---|---|---|
| Your link is the same every day | always | `own-link`, `host-reload`, `host-start-over` | `start_over_wipes_only_this_builds_keys` |
| Once you've shared it, your lobby opens whenever you play, with "Lobby open: waiting for a friend..." under the button | at start | `own-link` | `your_own_link_makes_you_host_and_your_lobby_opens_whenever_you_play` |
| Opening your own link makes you the host | at start | `own-link` | same |
| A second tab on the same device plays alone instead of opening the lobby twice | at start | (one device, two tabs: fast test only) | `a_tab_knows_when_another_tab_has_the_lobby_open` |
| A friend alone on your link while you're away plays their own world ("Waiting for your friend..."), then joins you when you start | 10 s after you start | `waiting-friend` | `a_friend_waiting_in_an_empty_lobby_stays_put_and_a_dead_connection_is_redone`, `a_waiting_friend_takes_the_hosts_world_from_its_first_word` |
| Two friends on your link while you're away play together (one stands in as host, in its own world); when you start, both join you | together 10 s after the second arrives; with you 10 s after you start | `two-waiting` | `friends_on_a_link_agree_who_hosts_with_no_asking_and_it_sticks`, `a_waiting_friend_takes_the_hosts_world_from_its_first_word` (only the host a friend goes with is taken at its word) |
| The stand-in reloads: the friend who stayed carries on as stand-in, and the one who reloaded is back with it | 10 s after its game started again | `two-waiting` | `friends_on_a_link_agree_who_hosts_with_no_asking_and_it_sticks` |
| The stand-in leaves: the friend who stayed carries on as stand-in, alone (no error, its own world), and a friend who comes later joins it | 10 s each | `stand-in-leaves` | same |
| Moving to a new host (you came, or the stand-in changed) never shows a frozen half-world or an error: a friend plays its own whole world until the new host's first word | always | `two-waiting`, `stand-in-leaves` | `a_waiting_friend_takes_the_hosts_world_from_its_first_word` |
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

Voice chat is written down only on the host's computer (`src/scribe.rs`): the device running the world (the host,
or a friend standing in for it) writes down everyone, its own player from its mic and each friend from their call as
it arrives, and sends every line to everyone. With `--voice on`, every scenario also checks that each player in a
guest world says "not writing (Guest)" and the one running the world (the host or a stand-in) doesn't, so a guest
never listens for speech or starts the model. The rest are
checked by fast tests (the browser runs can't, without downloading the 110 MB model):

| Promise | Fast test |
|---|---|
| Only the host writes anything down; a guest writes nothing | `only_the_host_writes` |
| A phone or tablet never starts the model, host or guest; with a phone hosting nothing is written down, and everyone is told why | `a_phone_never_writes_anything_down_host_or_guest` |
| A friend's line written by the host carries that friend's crab name, and lands in everyone's transcript | `a_friends_line_written_by_the_host_carries_the_friends_name`, `a_line_sent_by_a_friends_device_lands_in_my_transcript_with_their_name` |
| "Write down my voice" off: the host skips that player's voice (talk already at the worker too); friends see "(not written down)" | `a_friends_opt_out_is_honoured_by_the_host`, `a_friend_who_doesnt_write_shows_as_not_written_down` |
| Battery saver, two cores or fewer, a slow frame rate or a worker falling behind on the host: nothing written, everyone told | `why_not_write_battery_saver_weak_busy_behind` |
| Lines older than 15 minutes drop, on every device, also after a reload | `lines_older_than_15_minutes_drop` |
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
- Who hosts (`who_hosts`, `src/coop.rs`): every device works it out alone from the hellos it already swaps (each says
  its crab, whether it hosts, and whether it has played here yet), so all agree with no asking. Whoever hosts keeps it;
  among hosts the lowest crab wins, and the main host's crab is 0, so it always wins. With friends on the link and
  nobody hosting for 2 s, one stands in: a friend who has played here before a newcomer, then the lowest crab, then
  the connection's number (no ties). A friend who has never met anyone and is alone just waits.
- A guest goes into the host's world only when it hears the host it goes with (`take_in`), and back to its own when
  the host goes or changes; nobody else's word on the world is taken.
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
