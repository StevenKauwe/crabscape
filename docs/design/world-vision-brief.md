# World vision brief (the taste bible)

Read this whole page before every world task. It is the owner's vision for the world, written so that someone with
no lead designer in the loop still makes the right calls. The plan that builds it is `world-plan.md`; the rules that
never bend are in `CLAUDE.md` and repeated at the end. When this page and a task disagree, this page wins; when this
page is silent, pick the choice that makes the crab's body, light and sound carry the feeling, record it in
`decisions.md`, and move on.

## The game in one breath
You are a heavy, lantern-carrying crab, 12 cm across, on the floor of an enormous ocean. **Cozy** (home, food and
friends are a warm refuge), **brutal** (the deep is indifferent and bodies visibly break), **surreal** (the real
ocean is stranger than anything made up). Every dive is a debt repaid on the way back up. Every art, sound and
design choice is checked against all three pillars.

**The crab feels heavy, never sluggish.** Weight is in anticipation, landings and the rhythm of strokes, not in slow
response. Nothing on this page may add input lag, slow the crab's controls, or lower a movement number. Pressure,
fear and wounds are shown by the body (cracks, tremor, sound), not by worse handling. If a feel test in
`crates/sim/tests/scenarios.rs` changes, you did it wrong.

## The world's shape
The world is a great **submarine canyon** cut into the shelf (the overworld) with **sediment strata** buried in its
soft banks (the underworld). The two halves are different worlds that touch at doors:

| | Overworld: the canyon | Underworld: the strata |
|---|---|---|
| Senses | Sight, lantern, sound | Smell, touch, the lantern's small circle |
| Verbs | Swim, climb, glide, fight | Dig, bury, shore, follow scent |
| Feel | Wide, exposed, a long fall | Close, careful, cozy then brutal |
| Danger | Open-water hunters, currents | Ambushers at mouths, guardians in chambers, cave-ins with tells |
| Built from | Authored level data (bedrock, kelp, coral, currents) | Bounded cellular windows inside bedrock frames |

- **Burrow mouths** in the soft banks are the only doors between the two. You choose to go in.
- **Seep chimneys** carry the crab up for free and come out somewhere else. **Dug tunnels** (shored with shell grit)
  join pockets you opened. Together they are **wormholes**: the ground is connected in ways the water is not.
- **Bedrock frames hold every gate.** Nothing dug, ridden or glided ever skips an ability gate.
- **Every pocket has two or more exits.** Nobody is ever sealed in.

## The three fear registers (non-negotiable)
Every place belongs to exactly one register. Know which before you build it.

### 1. Open ocean is terrifying (fair terror)
Made with: **exposure** (wide water, nothing to hold, the lantern showing you to everything), **long-reach hunters
with long tells** (something big that strikes from far, after a warning you can read), and **a visible refuge always
in reach** (a crevice, a kelp stand, a burrow mouth, a lee behind rock).
- The hunter's tell starts at least 3 s before its strike, and can be read from at least 1,200 units away. The tell
  is silence first (the world's small sounds stop), then a shape, then the strike.
- From every point of an open-water crossing, a refuge is visible and within 900 units (about 1.4 m at crab
  scale): what the crab can swim during the shortest tell. Refuges in open water are small and local (a crevice at
  a wall's foot, a short clump of coral on a wall, a ledge in a lee), never a ladder: a line of refuges must not
  become a climbing route that replaces the tide or a gate. The middle of a crossing stays open.
- A hit in open water costs something real (a leg, the carried find, reserve) but is never an instant death.
- Good: crossing the canyon from the doorstep to the far rim, the reef sounds die away, a long pale shape turns far
  off, and the crab has four seconds to reach the coral clump it can see on the wall.
- Bad: a shark that appears on screen and bites in the same second. Bad: an open crossing with nothing in sight to
  run to. Bad: a hunter that kills in one hit.

### 2. Digging and tunnels feel safer
Made with: **no wandering hunters inside dug or authored tunnels**, close walls, the crab's own sounds, a small warm
circle of lantern. Tension lives at the **burrow mouths**, where an ambusher may wait, never inside.
- Zero hunters spawn in, path into, or roam inside a tunnel. A hunter may lunge into a mouth up to its own body's
  reach, never further.
- A cave-in always has a tell (grains trickle, a rumble) about a second before; the crab can always dig up.
- Good: a mantis sits buried just outside a burrow mouth; a slightly wrong mound and a trickle of grains give it
  away; once inside, the crab hears only its own legs. Bad: a mantis patrols a tunnel. Bad: a moray that follows the
  crab down a dug tunnel. Bad: a collapse with no warning.

### 3. Chambers and micro-zones are threatening but alluring
Made with: **a treasure visible from the doorway**, and **one threat**: a guardian, a hazard or a clock. **Retreat
to the tunnel is always possible.**
- From the chamber's door, the prize is in clear sight (no rock between the door and the prize) and within 800
  units. From the prize, the way back to a tunnel or refuge takes at most 6 s at walking speed.
- One threat per chamber, with a readable tell. The threat never follows the crab into the tunnel.
- Good: a mantis guards a chamber with a tooth glinting at the door; you can see the tooth, you can see the mantis
  breathe. Good: a brine pool between you and the clam, its surface flat and dead. Bad: a treasure only visible
  once you are past the guardian. Bad: a chamber whose door seals behind you. Bad: three hazards stacked in one room.

### Fairness rules (all registers)
Always a visible refuge. A tell before every hit. No cheap deaths: nothing kills in one hit, nothing hurts without
a warning a careful player can read, and dying returns you to the last place you rested with your finds in your
cast-off shell (the existing rule). Fear comes from not knowing and from being exposed, never from being cheated.

## Brutal, not bigger
- **True sizes.** Every creature keeps its real size at crab scale (75 units = 12 cm, 625 units = 1 m). Depth changes
  who lives there and how they behave, never how big they are. An octopus arm is 1 m (625 units); a moray is several
  crabs long. Any size you use must be sourced in `ocean-shaped.md` or marked "not verified".
- **Pressure is shown on the crab's body.** Below its adapted depth, fine cracks creep over the carapace; they stay
  as scars until the next moult. Limbs tremble and claws twitch (animation only). Adapted depth grows by moulting and
  eating deep food (body chemistry, `ocean-shaped.md` row 13), never by gear. Good: the crab rests in a crevice at
  the canyon floor and its shell ticks as cracks spread. Bad: a red "PRESSURE 85%" bar. Bad: slower walking.
- **Sound.** The crab's own body gets louder as the world gets quieter: shell creak, grit under the legs, claw
  ticks. Sounds with no visible source: bone and rock groaning, a vent you find by ear, slow calls from far off.
  **Silence is a signal**: small sounds stop before something arrives. Little music; one low tone that shifts when
  the crab is watched. **No bubbles, no air, no breathing sounds: crabs breathe water.**
- **Anxiety.** Not knowing (the map shows only the crab's memory; the lantern is a small circle; a smell wisp points
  at something unseen). Being watched (an eye that follows and never attacks). Waiting (long quiet stretches, then
  one sudden thing). The lantern is a liability (bright shows you, dim blinds you). Wounds carry scent (a hurt crab
  is smelled from further).
- **No full-frame filters or overlays, ever.** No vignette, no screen tint, no screen shake for fear, no darkening
  layer. Depth darkens because the lantern's own light shrinks and changes colour and the world's shapes sink into
  the water. Everything is expressed through the crab, its light, sound and shapes.
- **Everything runs from sim state.** Strain, scent, watched-ness and cues are numbers in `crates/sim`, so every
  player sees the same world and a replay reproduces it. The game only presents them. A remote crab's body is drawn
  from what its own device sent; nothing on your device changes a friend's crab.

## Each bed feels deeper
Each new bed (layer) must feel deeper than the last, in five ways at once, and hint at how far down it goes.

| Bed | Light | Sound | Grain (digs like) | Smell | Who lives there | Hint of depth |
|---|---|---|---|---|---|---|
| Shallows and home burrow | Daylight from above, lantern warm and wide | Busy: snapping shrimp, surge, many small clicks | Silt: fast, soft, flows back | Weed | Snapping shrimp, clams | A faint glow far below a crack in the canyon floor |
| Basin and mudbanks | Daylight fading, blue-grey | Fewer clicks; the crab's legs start to be heard | Silt over clay | Weed and clay | Mantis at burrow mouths, moray in crevices | The pit's darkness below the lip |
| Canyon walls and the bone hall (twilight) | Last blue; the lantern starts to matter | Water running, far calls, the tide | Clay: slow, holds a tunnel | Clay | Lanternfish shoals, octopus, alarm jelly | Sea pens glowing far below |
| Canyon floor and lower canyon (midnight) | No daylight; only living light and the lantern, which reaches less and turns bluer | Near silence; the crab's shell creaks; bone groans | Pebbles: heavy, effortful | Sulfur | Whale-fall scavengers (a crowd, not a monster), the sixgill passing | A carcass drifting up a seep |
| Seep garden, vent field, crust vault (deepest reachable) | Chemical glow only | One low hum; silence before arrivals | Crust: barely diggable | Metallic | Seep life, guardians | A cold draught in an odd tunnel (the Plunge) |

Use the crab's lantern (reach and colour), the world's ambient sounds, the stroke speed and sound of digging, the
smell wisp's tint and the residents to make each step felt. The depth gauge has a long tail that runs off the bottom:
there is always more below. (Smell tints and the residents' depth ranges are design choices; biology "not verified"
unless sourced in `ocean-shaped.md`.)

## The Plunge (a secret set piece)
A secret found by exploring, never by being told. It tells the player how deep the game goes without opening any of
it. **You cannot die in the Plunge: the terror is real, the risk is not.** The terror is atmosphere, not bigger
monsters. It is found after act 3 (the long chimney home is open; `world-plan.md`).

| # | Beat | Time | What happens | What the player feels |
|---|---|---|---|---|
| 1 | An odd tunnel | free | A quiet dead end; the water tastes cold (smell wisp turns metallic) and moves down | Curiosity |
| 2 | Dig around | ~30 s | Silt trickles into a gap; a low roar | Something is behind this |
| 3 | The hole | ~5 s | The current already pulls; the crab can still back out | I could go back |
| 4 | The grab | ~2 s | The current takes the crab; a little steering, no damage | Taken |
| 5 | Marine snow | ~15 s | The depth counter spins; the lantern's colour drains; the sound muffles; the shell creaks | Falling forever |
| 6 | A carcass falls past | ~5 s | A dead whale tumbling past, bones and all, at its real size, enormous to a 12 cm crab | Scale |
| 7 | The lights go out | ~10 s | The last blue is gone; only the lantern | Alone |
| 8 | The crack | ~3 s | Spat into a tight crack. Silence, then the shell ticking. Fine cracks appear and stay until the moult | Safe, and marked |
| 9 | The flash and dinner | ~20 s | The wall lights up from behind; something pale comes into a lure's glow; the lure flares; gone | Hold still. Watcher, not meal |
| 10 | Treasure | free | A small find in the crack: a wild blue-green light | Wonder |
| 11 | Something answers | ~5 s | Far below, a light blinks twice. Nothing else | There is more |
| 12 | The vent | ~20 s | A hot updraft carries the crab all the way home, to the tunnel it started in | Relief, and a question |

- The crack is a tunnel (register 2): nothing can reach the crab there. The angler scene is a script seen through the
  crack. In co-op, whoever enters the hole takes the ride (a friend digging beside comes too), the scene plays for
  those present, the treasure is per crab, and the vent takes each crab home.
- The depth counter's numbers are a design choice, not a measurement (the game already says it shows compressed
  distances). The colour drain is the lantern's own light, never a screen filter.
- Biology to source before it becomes a rule: anglerfish lures, the blue-green light, hydrothermal updrafts, carcass
  sizes ("not verified" until then).

## Anti-patterns (each one betrays the vision)
- Bigger monsters as the scare; any creature scaled up for drama.
- Damage numbers, health bars or a HUD number standing in for a body cue ("pressure 80%" instead of cracks).
- Instant deaths, one-hit kills, or hits with no tell.
- Mazes with no refuge; open water with nothing in sight to run to.
- Wandering enemies inside tunnels; anything that follows the crab into a tunnel.
- Treasure behind pure grind (repeat X times) instead of nerve, reading and skill.
- A treasure you cannot see from the chamber's door; a chamber that traps.
- Full-frame filters, vignettes, screen tints, fear screen-shake, overlays.
- Bubbles, air, breathing sounds, helmets, air tanks, goggles.
- Fear made by worse controls: slower movement, input lag, sluggish tremor.
- Gates that a dig, a chimney, a glide or a ride can skip.
- Rules in Bevy systems instead of `crates/sim`; anything that changes a remote crab.
- Made-up biology presented as fact.

## Rules that never bend (from `CLAUDE.md`)
- No licence. All rights reserved. Never add a LICENSE file or licence fields.
- Ocean-shaped: everything is biologically real; self-consistency never bends. Crabs breathe water: no air, air
  tanks, goggles or pressure helmets. Facts need real, opened sources; otherwise mark them "not verified".
- Art varies inside each shape; never full-frame filters or overlays.
- Never commit generated junk: `target/`, `web/pkg/`, `web/assets/`, renders, scratch, throwaway art, videos.
- Changing any of these is a direction call for the owner (StevenKauwe).
