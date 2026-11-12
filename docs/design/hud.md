# The HUD and menus

How CrabScape's screens look and where each kind of information lives. The owner (2026-11-12): "our current
HUDs/UI are atrocious." The game's focus is a hella fun levels 1 to 15 in every skill, so the HUD's job is to stay out
of the way while you play and to make every bit of progress feel good. Code: `src/ui_theme.rs` (the look),
`src/hud.rs` (the reward line); each panel keeps its own file.

## What was wrong (audit, 2026-11-12, crabscape.com at 1280 x 800, 390 x 844 and 844 x 390)
- **A debug font.** Every word was Bevy's built-in monospace: it read as a developer overlay, not a painted game.
- **No reward for earning.** Experience drops were 11-pixel words over the crab that vanished in a second; a gather
  often seemed to give nothing. A level-up was a few lines of floating world text, easy to miss in a fight.
- **Clutter where the eye rests.** The top-left held the game's name, a key hint, the reserve and shell bars side by
  side, and a growing column of every skill touched (13 rows by mid-game), with the goal line at the bottom of it.
- **Things in odd places.** The "Inventory (0) [B]" box sat top-centre on a wide screen and mid-left on a phone; the
  well-fed sign floated under the shell bar.
- **Five button styles.** Corner buttons, panels, the workbench, the inventory and the touch buttons each had their
  own fills, rims, corners and picked colours (teal here, brown there), all low-contrast over the painted rock.
- **Phone.** The touch buttons were labels in plain circles; the hint ran under the corner buttons.

## What good games do, and what we take
- **RuneScape (the skill loop):** every action that earns drops a number ("+25") that flies into a tracker showing
  the skill, its level and a bar to the next. A level-up is a moment: a jingle, a firework on the character, a big
  message naming the level and **what it unlocks**. Taken: the experience tracker with drops, the level card with
  its unlock lines, the claws' cheer and ring that were already on the crab.
- **Hollow Knight and Silksong (the HUD while playing):** almost nothing on screen. Masks and soul sit small in the
  top-left corner, drawn in the world's own ink; everything else waits in menus. Taken: one top-left corner for the
  crab's state (shell, reserve, cargo, goal) and nothing else while you play; rewards appear only when earned and
  fade.
- **Valheim and Stardew (inventory and crafting):** the inventory is one key away, a grid of what you carry; crafting
  lives at the station, lists recipes with have/need counts, and greys what you can't make yet. A pinned
  recipe or quest stays on screen in a small tracker. Taken: the cargo chip that opens the shell's list, the
  workbench's have/need lines, the pinned goal as a small pill with a diamond.
- **Mobile touch layouts:** thumbs own the bottom corners (move left, jump right, actions in an arc above jump);
  buttons at least 44 points; contextual buttons appear only when they can act; nothing the eye needs sits under a
  thumb; the top edge is for state. Taken: the existing thumb layout kept, the walks and jump ringed in amber as the
  thumbs' home, GATHER only near something to gather (already), the reward line placed between the top buttons and
  the thumbs on a portrait phone.
- **Juice (game feel for rewards):** an anticipation-impact-settle curve (a quick overshoot then a settle), a
  layered reward (sound, character, screen), sizes that scale with importance, and short holds so play isn't
  blocked. Taken: the card pops in past full size and settles, a gold line draws under the title, drops swell and
  float off; the crab's ring, cheer and chime still fire; nothing full-screen (art direction: never a full-frame
  overlay).

## The system
**Type:** Fira Sans (SIL Open Font License, cut down to Latin text, about 40 KB a weight, baked into the game: no
download, no decode before join). Regular for words, Bold for headings and numbers. Five sizes: 11 caption, 13 body,
15 label, 18 title, 26 display. Every word over the world gets a dark ink shadow so it reads over bright rock.

**Palette** (the art direction's colour key): deep-sea ink panels; a warm ink rim like the world's outlines; parchment
words with a quieter grey-teal second line; **amber** for the crab, its lantern and home (reserve, cargo, the goal's
diamond, the thumbs' buttons); **gold** for what is earned (experience, levels, a goal in hand); **coral** for the
shell; pale cyan for poise. A picked tab or option is warm amber everywhere.

**Panels:** ink fill, 1.5 px warm rim, 12 px corners, a soft shadow under them. **Buttons:** the same ink a little
see-through, a fainter rim, fully rounded. The workbench at home keeps a warmer, browner ink: it is by the hearth.

**Icon language:** few icons, all shapes: a diamond is a goal; bars are amounts (coral shell, cyan poise, amber
reserve segments, gold experience); arrows are the walks.

## One place for each kind of information
| What | Where | When |
|---|---|---|
| Shell, poise, reserve, well fed | Top-left corner, one left edge | Always while playing |
| Cargo ("Cargo 3", B) | Under them; opens into the shell's list | Always |
| The pinned goal (diamond, "Goal: kelp cord, 1/2 kelp") | Under the cargo; gold when all in hand | Away from home, when pinned |
| Experience drops and the skill being trained | The reward line, top centre (a portrait phone: under the corner buttons) | A few seconds after earning |
| A level gained, a goal in hand | The same line, as a card that pops in, holds and fades | When it happens; one at a time |
| Corner buttons (Mic, Invite, Skills, Home, Settings) | One row of icons, top-right (`src/corner.rs`) | Always (Home fades when it can't go; Mic in a room) |
| Every skill, its level, its unlocks | The Skills panel (K) | On demand |
| Recipes, wants, home pieces | The workbench, at home | Tucked in at home |
| Keys | The "? Keys" card, bottom-left: two columns of keycaps | On demand |
| Short notes about the crab ("Ate it", "Too soft") | Floating over the crab | When they happen |

## Follow-up (2026-11-12)

- **Pixel density.** The HUD is laid out in logical pixels: physical size over the scale factor. Under Chrome's
  device emulation the canvas's `devicePixelContentBoxSize` comes back in CSS pixels, so a 390-wide phone at 3x was
  390 physical at scale 3 and the HUD was drawn for a 130-wide screen, three times too big. `src/screen_fit.rs`
  checks the page box (`#game`) every frame and, when the physical size is not CSS size x scale, sets it to that.
  An honest browser is left alone. `hud_scale` (CI job "HUD size at 1x, 2x and 3x pixel density") opens a phone at
  each density, upright and turned, checks `data-window` against `innerWidth` / `innerHeight` and clicks Settings.
- **Workbench: three tabs.** Goals (task board, next unlocks), Craft (the first recipes, then cooking, shellcraft,
  weaving and mending), Home (pieces, culture beds, building and culturing). Each lists the recipes the crab has the
  level for, by level, then one locked recipe per skill. Repairs show only when something needs mending. What is
  open is the sim's rule (skill level against the recipe's, and `can_make_econ`'s nothing-to-mend), so the systems
  lane's unlock ladder and task-board changes show without UI changes.
- **Corner row.** Settings, Home, Skills, Invite and Mic (right to left; the Mic, only in a room, last so the row has
  no gap) are 40 px icon squares, 8 apart, in one row. The icons are
  painted at startup from a few shapes (5 x 64 px; nothing downloaded). A press counts 3.5 px round a button, under
  half the gap, so it never hits two. Invite has the amber ring; Mic shows its state by colour.
- **Keys card.** Two columns of keycaps (arrows drawn as arrows, from the font subset), about 630 x 260 instead of
  390 x 300.

