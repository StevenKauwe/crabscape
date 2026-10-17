# CrabScape — Gameplay Loop Proposal

> owner: **@molt-keeper (#6)** — sole drafter | red team: #1–#5 | last: 2026-10-17
> status: **FINAL-PENDING-OWNER** (v1.8 — + RF-1b decay asymmetry)
>
> Citations: **C#** = principle (`bible.md`) · **Cmd #** =
> project call (`commandments.md`) · game docs: `vision` /
> `world` / `how-it-plays` / `universe` / `ocean-shaped` / bestiary
> (`creatures/`). `[GAP: …]` = owner decision, never invented. §7 carries all
> red-team dispositions. Passes `tools/verbatim-check.py`.

---

## 1. The pitch

CrabScape's loop is a **two-book engine**: every dive is a run whose inputs
(oxygen margin, strokes, cargo, lantern fuel) live in the *dive-book* and die
with the dive; everything worth having (skills, build-outs, recipes, standing,
knowledge) lives in the *home-book* and grows only across dives — and the two
books never co-mingle, because **nothing converts from dive-book to home-book
except by surviving the way home** [C2; Cmd 1–2]. The player's whole career is
learning where that conversion is worth risking.

## 2. The loop stack

### 2.1 Minute loop — the verb cycle

- **States** (`how-it-plays`): grounded (floor jumps are free) → stroking
  (each swim stroke costs stamina) → winded (reserve empty; legs tired until
  let go) → gripping (kelp/coral climbing is free and slow) → resting
  (solid footing; ~30 s reserve regen) → carrying (one item on the back).
- **Verbs:** walk/scuttle, floor jump, stroke, dodge-burst/sprint,
  dive-and-flip, grip/climb/hang/kick-off, rest, carry, **shed cargo** [LEN-F2:
  the winded-over-water moment has two outcomes — drop weight and make the
  climb, or keep the load and pay in strokes; what's dropped stays dive-book
  litter in the world, Cmd 10]. Combat verbs (cut/hammer/rip, parry) are
  sketched but unbuilt [GAP-1].
- **THE decision this tier exists to create: spend or hold.** The water
  column is bought meter by meter from a finite reserve; the player is always
  choosing between burning strokes to cross now, climbing free but slow, or
  committing to a footing — exposed ledge or safe crevice, the choice *is*
  the rest verb [C1; LEN-F3; `how-it-plays`]. Once the lantern exists, the
  same choice repeats as light: *see vs be seen*, set per dive [C4, C10].
- **Costs:** strokes, reserve, sprint drain, the single carry slot, lantern
  setting. No clocks underwater [C4; Cmd 6].
- **Two-books wiring:** minute-level overreach (winded mid-water) is the raw
  material of dive-book losses. Eggs are eaten in place (`how-it-plays`) —
  **R-3 ruling, new Cmd 12:** an egg eaten in place is CLAIMED, NOT BANKED;
  the stroke converts only when the crab comes home; die with it in your
  belly and the sea keeps it. Zero content change — the canon exception
  becomes the law's strongest proof [Cmd 2, Cmd 12]. **GAP-5 RULED:**
  claim-not-bank applies to all in-place verb-grants, all zones — no further
  owner call needed.
- **Telemetry that proves fun:** strokes-per-depth-band, time-winded, rest
  rate, shed-cargo rate, death-position vs last-solid-footing [C9; Cmd 8].

### 2.2 The dive — the run

- **States:** PLAN (home) → DESCEND → EXPLORE → *turn-around* → **THE CLIMB**
  → BANK [Cmd 1; NAR-N4 accepted: the turn-around is **not an engine state**
  — it is the player's deepest point, defined post-hoc by telemetry [C9];
  RETURN renamed THE CLIMB].
- **Verbs:** pack (one loadout), forage, set lantern brightness, open gates,
  carry/shed, rescue-dive [Cmd 10].
- **THE decision this tier exists to create: the turn-around** — push one
  encounter deeper, or convert this cargo now? It has two priced sides
  [ARCH-F4]: conversion (head home with what you hold) vs **curiosity** (the
  passage past the turn-around — currently the unpriced side; the plan-vs-
  actual telemetry below is its price tag) [C1; Cmd 2]. The PLAN state is its
  sibling: the loadout gamble, warm at home, never nagged underwater
  [C10; Cmd 7].
- **Costs:** oxygen margin, lantern fuel, food-as-dive-duration (consumption
  is a player action under pressure, never a clock) [C4; Cmd 6, 7].
  **EXPLORE variance** [ARCH-F6]: forage spawns and predator positions ride
  the ecological layer — two explores of one passage differ because the water
  differs [C5; Cmd 4].
- **THE CLIMB — and the mid-dive re-roll dial [LEN-F1, named]:** do passage
  states roll between turn-around and bank? Default: **schedule-rolled, not
  random** — the route table re-rolls on the tidal tick (bible Q3 layer 1),
  so a dive that fits inside one tick climbs uniform (C11's mundane rhythm —
  the norm), and a dive that spans a tick climbs through changed water:
  tension spikes are earned by time-and-depth, never dealt at random, and
  the tide table makes them knowable in advance [C5, C6; Cmd 4]. Between
  ticks the climb is deliberately mundane verbs — strokes, grips, rests:
  peaks are earned by the ordinary rhythm around them [C11].
- **BANK:** the conversion made **diegetic and visible** — cargo laid on the
  shelf, the shell visibly grown; never a menu [ARCH-F1; C10]. Followed
  immediately by named mundane verbs: stow, feed the shell, tend the stew —
  no celebration screen; the ordinary rhythm is the reward [C11; **Cmd 11**;
  R-2].
- **Two-books wiring:** this tier IS the conversion gate [Cmd 2]. Death wakes
  the crab at home or last rest stop with worn gear; carried cargo stays in
  the cast-off moult where it fell — and **the moult is a place, not a
  mission: the rescue is never surfaced by UI** [ARCH-F2; `world`; Cmd 10].
- **Telemetry:** dive-length distribution, max-depth vs turn-around-depth
  (defining the turn-around post-hoc), cargo-carried-home rate,
  cause-of-death histogram [C9; Cmd 8].

### 2.3 The multi-dive arc — the week

- **States:** home rhythm between dives; rung-pursuit; rest-stop web
  expansion; zone needs-ladder stage (safety → food → capability →
  curiosity) [C7; Q4 application].
- **Verbs:** build (shell-home rooms, rest stops), batch-cook rations,
  culture lantern, train skills by use (Climbing, +8%/level [`world`] —
  fluency polish on an owned verb, never a gate; gates are verbs only
  [C3; ARCH-F3]), rescue-dive.
- **THE decision this tier exists to create: what the next dive is FOR.**
  Capability rung, rescue, provisioning, or curiosity — the arc is where the
  player authors their own goals, and rest-stop placement is its physical
  form (building = exploration; a camp moves the reach boundary) [C7; Q4
  application].
- **Costs:** home-book resources only; upkeep batched and warm [C4; Cmd 7].
  Batch composition is itself a decision [AUD-F12]: which risk-packs do the
  week's dives get — dense stews for deep pushes, light rations for
  hauling runs, culture split between brightness now and spare fuel later?
- **Two-books wiring:** the arc is home-book growth [Cmd 1]; rescue-dives
  are the only bridge — retrieval attempts under the same conversion rule
  [Cmd 2].
- **Telemetry:** dives-per-rung, rest-stop usage, rescue success rate,
  sessions between rung unlocks [C9; Cmd 8].

### 2.4 The campaign — the zone formula

- **States per zone:** a zone is *one gate, one new verb, one boss exam*
  [Cmd 9; C8], staged on a needs ladder [C7; Q4 application], mapped by a
  **route table**: ~a dozen named passages, each with 2–3 states driven by
  four rotation layers — tidal, ecological, catastrophic, player-authored
  [C5, C6; Cmd 4].
- **Verbs granted per zone:** one persistent new verb via a found-thing
  (egg → stroke; culture → lantern reach; tooth → cut) [C3, C7; Cmd 5].
  **Found-thing approach routes are route-table-gated** [ARCH-F5]: the egg
  reached through a different passage state is a different dive — the paths
  rotate even where the prizes sit [C5, C6].
- **THE decision this tier exists to create: which gate, with what.** More
  gates than keys; the campaign decision is sequencing. Post-mastery: the
  dive contract (dim lantern, hungrier water, shorter slack — priced,
  reward-coupled) [C5; Cmd 4].
- **Costs:** at most ONE maintenance system per zone [C4; Cmd 6]; hazards
  published on the zone-pager's hazard calendar [Q4 application].
- **Legibility boundary** [ARCH-F7]: authored marks render above rotational
  state — the player can always tell "the water moved" from "I
  misremembered" [C6; Cmd 4].
- **Co-op note** (per core adjudication on AUD-F2): each player keeps a
  dive-book; the home-book and bay are shared per group [`world` one-world-
  per-group; `vision` solo-first].
- **Telemetry:** passage-state distribution per dive, zone-exit death
  causes, contract adoption rate [C9; Cmd 8].

### 2.5 Skills — the RuneScape layer (pointer)

**The skills web lives in `skills-web.md`** (red-teamed, @01a140a7
draft). Its interlocks with this loop, verified this cycle:

- **Three currencies, three laws** [Cmd 12; C7; 02 §3–4]: new verbs (eggs,
  found-things) are *keys* — claim-then-bank, they ride the back and obey
  the survival law; fluency (levels) banks live by doing; perception
  (sensing) banks live by doing too — C7 is its own progression kind, not a
  verb grant. Gates check the perceiving or the doing, never the claiming
  [C3]. The egg and the climb-length were never the same currency.
- **The slice gains only Climbing** [Cmd 3, 9]; every other skill arrives
  with its verb, per the zone formula (02 §6).
- **C12 enforcement** [02 §5]: every zone best-trains a skill; endgame sinks
  consume low-zone resources (veterans return as suppliers, not tourists);
  re-entry secrets re-key old rock with new light; skills × route-table
  states = the same dozen passages play differently for months. Gate: the
  revisit test (≥1 reason per older zone, SYS-F7 standard) + the corridor
  gauge (old-zone visit rate; decay to zero = C12 failing in practice).
- **Proposed Cmd 13** (curves are feel; gates are verbs or perception) and
  **Cmd 14** (web keeps old water load-bearing, ≥1 standard) await core
  promotion; **Cmd 12 ruling R-3 and Cmd 11** are already load-bearing here.

### 2.6 Biomass, feats, and the molt ceremony (pointer)

**Cmd 16's system lives in `skills-web.md` §8–10** (owner-amended:
three-tier molt ladder; @01a140a7 draft). Interlocks with this loop:

- **Two moments, two sources** [Cmd 12; Cmd 16; 02 §9]: keys (found-thing
  verbs) convert at *homecoming*; earned feats (skill milestones) deploy at
  *the molt*. The pit teaches claiming; the bay teaches molting.
- **The molt is prepaid** (owner amendment; premolt anorexia, documented):
  committing locks eating until the post-hardening meal — which is the C11
  resumption beat, structurally guaranteed.
- **Dual-mode biomass** [ML-3 RULED]: raw is edible mid-dive, no artificial
  penalty — raw = savings spent on the present; rations = efficient dive
  fuel; Cooking improves the conversion. The conversion choice survives
  [C1].
- **The three-tier molt ladder** (owner amendment): **HOME** = the safe
  ceremony (Cmd 16 intact) · **REST STOP** = forward-deploy siege, gain the
  feat in place (molt at the net with the tooth; co-op guardable) ·
  **ANYWHERE** = paper (defense ≈ 0, discovered = dead; soft-window
  lethality). **Vulnerability is opted into — risk bought with reach**
  [C1].
- **Death during a forward molt — RF-1 doctrine (core ruling):**
  eligibility survives automatically [Cmd 15 — the sea never keeps what you
  became; re-molt needs only re-payment], and the **prepaid biomass rides
  the body**: committed biomass is IN the crab, so a soft-window death puts
  it in the death-moult with the cargo — rescue-recoverable [Cmd 10]. A
  failed forward molt is not a wipe: *your old shell holds your cargo, your
  soft body holds your future, and you go back for both.* That is what
  makes the REST-STOP tier honest — risk priced in recoverable futures,
  not erased progress — and it gives co-op guarding its exact stakes:
  protect the molting friend, or their body becomes the objective.
- **RF-1b — the decay asymmetry (core ruling):** forward death-moults
  **decay on the carrion-convergence clock** — the soft corpse + moult is a
  plume, and the web eats it — while **home death-moults persist
  indefinitely**. What dies at home waits for you; what dies in the wild
  feeds the web. This closes the die-on-purpose soft-teleport exploit with
  canon that already exists (no new rule), and makes rescue urgency
  diegetic: the bay is eating your future, not a UI timer. Supersedes the earlier no-wild-molts note (GAP-9 narrowed: co-op
  raiding remains GAP-8).
- **The molt bill** [02 §10]: ecdysis deploys everything queued — feats and
  autotomy-regrowth, one bill against the biomass wallet, super-linear
  throttle. Claw-back-or-new-feat when short [C1]. [LIBERTY-tagged as
  before.]
- **The open dial [GAP-10]:** can a COMMITTED molt be delayed (resorbed at
  condition cost — real biology on both sides)? Owner-undecided. Seat
  recommendation: default OFF (committed fires on its clock; planning =
  being home), dial ON as post-mastery contract space [Cmd 4 spirit] — and
  a discovered-mechanic candidate grounded in documented decapod biology:
  autotomy during premolt DELAYS ecdysis — sacrifice a claw to postpone the
  molt = the rest-stop siege verb.
- **Ceremony telemetry** [C9; Cmd 8]: the bill, the deployment count, the
  post-molt feeding surge, ladder-tier distribution, forward-molt outcomes
  (completed / died / rescued), and the **decay-rescue race** (rescue
  attempts vs decay completions) — how often players buy reach with risk,
  and what the bay takes for it.

## 3. The fun proofs

| Tier | Telemetry (C9, Cmd 8) | Playtest question (C1 remove-test) |
|---|---|---|
| Minute | strokes/depth-band, winded-time, rest rate, shed-cargo rate | "Did you choose to rest — and what did you leave behind?" [LEN-F5] |
| Dive | dive-length dist, turn-around vs death depth, cargo-home rate | "Where did you turn around — and why there?" · "What did you do in the ten seconds after the big moment?" [NAR-N5] |
| Arc | dives-per-rung, rescue success, rest-stop usage | "What is your next dive for?" (unanswerable = arc failing) |
| Campaign | passage-state distribution, contract adoption, **plan-vs-actual divergence; passage-state-at-commit vs believed** [ARCH-F9] | "Point at what changed since last dive" (C6; Cmd 4) |

**Baselines** [AUD-F6]: the slice's playtests ARE the first distributions —
collection starts at dive #1; no baseline, no tuning call.

## 4. Death & the two books

Death sits at the END of a dive and the START of an arc — never inside the
minute loop. What banks: knowledge (a clue, half a recipe), skills already
grown, the world as the player authored it [C2; Cmd 10]. What is lost: the
dive-book — oxygen spent, strokes burned, cargo carried [`world`]. What
persists in the world: the moult, holding the cargo, as a destination —
**a place, not a mission; the rescue is never surfaced by UI**
[`world`; Cmd 10; ARCH-F2]. Why that's fun: the rescue dive is a *chosen*
re-entry into the exact risk that killed you, carrying better knowledge —
the only currency death pays [Cmd 10; C2]. **Co-op delta** (core
adjudication): dive-books are per-player; the home-book and bay are shared
per group; solo-first stands [`world`; `vision`]. The conversion rule is the
prominent law of every tier: **no line item crosses from dive-book to
home-book without surviving the way home** [Cmd 2; C2].

## 5. Cozy/brutal/surreal audit

| Tier | Cozy | Brutal | Surreal |
|---|---|---|---|
| Minute | resting on a ledge, rings fading | winded over open water | **the carry slot itself** — one absurd found-thing riding your back (sponge crabs really do) [NAR-N2] |
| Dive | the warm kit-up ritual at home | full cargo, long climb, dimming light | what the lantern re-skins at new radius |
| Arc | the shell grows; stew batched | rescue-diving your own bones | the village that lives on a whale-fall |
| Campaign | a named passage you made safe | granite that does not care | lights far below, unexplained |

All twelve cells are mechanics or canon places, not labels [`vision`;
`world`; `how-it-plays`].

## 6. First-biome go-criteria — the Sunlit Shallows

IF the owner says go, "detailed Sunlit Shallows" contains exactly the slice
canon names [C8; Cmd 3; `world` first loop]:

1. **Scope list:** home shell (full rest) · the pit · three eggs (three
   strokes) · lantern culture + first lantern · the light-gated alcove
   (tooth + clue) · workbench → scissor claws · ghost-net gate · the crack
   with trench lights · **the octopus exam** — canon-grounded:
   `creatures/README.md` (common octopus, boss, "Rocky shallows, its den";
   tell: rises and spreads its web) + `universe.md` Sunlit Shallows row.
   **Disambiguation (core ruling):** the shallows boss ≠ the whale-fall
   octopus *trader* (Midnight-Zone NPC, `world`).
2. **Capability rungs (verbs, not numbers)** [C3; Cmd 5]: double jump →
   swim strokes ×2 → lantern reach (+⅓ per culture lump, per `world`)
   [AUD-F10] → cut.
3. **The exam** proves the new verb under pressure [Cmd 9]. The tested verb
   is an owner decision: **[GAP-4]** (a) cut-only exam — teach-coherent
   today, combat feel stays unproven (pairs with GAP-1's named risk); or
   (b) tell-read exam + snapping-shrimp practice encounter — **core
   RECOMMENDATION: (b) primary** (the bestiary already specs the snapping
   shrimp as the shallows turret tutorial, tick numbers included —
   implementation, not invention); (a) documented as the fallback if combat
   misses the slice window. [PRED-F3]
   **[GAP-3] rides here (owner-visible; core RECOMMENDATION attached):**
   option (a) — power via three canon-adjacent beats: (i) the ghost-net cut
   (gear-as-verb win, canon), (ii) a parity duel vs a same-size rival crab
   (`rivals.md` peers — the only fight canon lets you win), (iii) surviving
   the octopus's first assault via block/parry (survival as victory against
   a boss). Option (b) — utility-only power — remains the honest fallback
   named in the postmortem. [PRED-F2]
4. **Definition of done** [C8]: playable end to end · iteration documented
   per playtest · postmortem names proved/unproven · **named risk in the
   postmortem template:** "combat feel is unproven in the slice" if the
   cut-only path is taken [PRED-F1] · **Gate 0 — the toy gate:** movement
   with all progression stripped must be fun alone, before any slice DoD
   item [LEN-F6] · **gate: a stranger plays it and wants one more dive.**
5. **Paper first** [C8; Cmd 3]: the loadout/turn-around decision as cards
   this week — it is the same decision seen from both ends [LEN-RT-3
   accepted].
6. **Persistence:** [GAP-2 **RULED (R-1):** save-on-bank; persistence banks
   at the shell only. Quitting mid-dive ABANDONS the dive — the crab comes
   home empty, cargo lost, no death, no moult. The conversion law is
   enforced mechanically, rage is minimized, nothing was invented
   (ARCH-F8 framing accepted).]
7. **Explicitly out of scope:** co-op mechanics (per-player dive-books noted
   in §4), zones below the crack, trade economy, dive contracts
   (post-mastery [Cmd 4]).

## 7. Red-team ledger

*Dispositions by findings file; escalations go to core. AUD = #2, ARCH = #1,
LEN = #4, NAR = #5, PRED = #3.*

| # | Finding | Disposition |
|---|---|---|
| AUD-F1 | rotation layers homeless (skeleton-era) | resolved-in-v1 (core-confirmed) |
| AUD-F2 | co-op silence | **accepted** — §4 + §2.4 one-liners per core ruling |
| AUD-F3 | octopus exam uncited/contradictory | **denied as stated** (core: bestiary + universe.md canonize the shallows boss); process point accepted — §6.1 cites + disambiguation added |
| AUD-F4 | death omits rescue dive | resolved-in-v1 |
| AUD-F5 | coverage map | accepted — this doc cites C#/Cmd per element; map implicit in Sources key |
| AUD-F6 | telemetry baselines | **accepted** — §3 baselines line added |
| AUD-F7/F-13 | universe/ocean-shaped drift | **accepted** — both cited in Sources; core confirms both exist |
| AUD-F8 | citation scheme stale | resolved-in-v1.1 (re-anchored pre-review; crossing noted) |
| AUD-F9 | egg mechanic vs how-it-plays | **accepted → RULED** — §2.1 corrected; **Cmd 12 issued (claim-not-bank); GAP-5 closed** |
| AUD-F10 | lantern ×2 vs +⅓ | **accepted** — §6.2 now canon +⅓/lump |
| AUD-F11 | C11 uncited at peaks | **accepted** — BANK mundane verbs named (§2.2); post-exam beat folded into GAP-1 needs |
| AUD-F12 | batch-cook has no decision | **accepted** — risk-pack composition added (§2.3) |
| ARCH-F1 | bank unspecified/menu risk | **accepted** — diegetic bank spec (§2.2) |
| ARCH-F2 | rescue must be world-fact | **accepted** — "place, not mission" (§4) |
| ARCH-F3 | +8%/level vs verbs-doctrine | **accepted-as-noted** — fluency-polish framing (§2.3); owner exception flag if ever a gate |
| ARCH-F4 | dive-internal needs ladder; curiosity unpriced | **accepted** — two-sided turn-around (§2.2); plan-vs-actual telemetry prices curiosity |
| ARCH-F5 | found-things collapse variance | **accepted** — approach-routes-gated line (§2.4) |
| ARCH-F6 | EXPLORE variance unstated | **accepted** — ecological-layer line (§2.2) |
| ARCH-F7 | authored-vs-rotational boundary | **accepted** — authored-renders-above rule (§2.4) |
| ARCH-F8 | GAP-2 is the enforcement point | **escalated → RULED (R-1)** — save-on-bank; quit-mid-dive abandons the dive (§6.6) |
| ARCH-F9 | legibility has no gauge | **accepted** — two gauges added (§3) |
| LEN-F1 | return leg undesigned | **accepted — dial named:** mid-dive re-roll; default schedule-rolled on the tidal tick (§2.2) |
| LEN-F2/F5 | drop-cargo verb + playtest Q | **accepted** — shed-cargo verb (§2.1); question in §3 |
| LEN-F3 | rest is a state, footing is the verb | **accepted** — THE decision reworded (§2.1) |
| LEN-F4 | fallback exam unproven as fun | **accepted** — paper-sketch requirement folded into GAP-1/GAP-4 |
| LEN-F6 | toy test as named gate | **accepted** — Gate 0 (§6.4) |
| NAR-N1 | citation layer stale | resolved-in-v1.1 |
| NAR-N2 | pillar audit labels-not-loop | **accepted** — carry-slot surreal cell (§5) |
| NAR-N3 | bank homecoming; C6 dive-tier home | **partially accepted** (bank presentation §2.2); remainder superseded by R-1/R-2 — bank is now specified + load-bearing |
| NAR-N4 | turn-around is not an engine state | **accepted** — THE CLIMB rename + post-hoc definition (§2.2) |
| NAR-N5 | Cmd 11 promotion + ten-seconds hook | **RULED (R-2): Cmd 11 LIVE** — cited §2.2/§3; hook in §3; **promotion accepted** |
| PRED-F1 | combat feel unsliced under fallback | **accepted** — named-risk line in §6.4 |
| PRED-F2 | GAP-3 power-fantasy evidence | **owner-visible [GAP-3]** — core RECOMMENDATION attached: option (a) via three canon-adjacent beats (§6.3); (b) fallback in postmortem |
| PRED-F3 | GAP-4 exam tested-verb | **owner-visible [GAP-4]** — core RECOMMENDATION attached: option (b) primary (snapping-shrimp turret, bestiary-specced); (a) cut-only fallback (§6.3) |
| PRED-F4 | GAP-1 spec-needs incomplete | **accepted** — GAP-1 extended: commit-budget (ms per attack class + dodge rule) + post-peak mundane verb (C11) |

## Sources

- **C1–C11** — `bible.md` §catechism (principles).
- **Cmd 1–10** — `commandments.md` §commandments (applications).
- `vision.md`, `world.md`, `how-it-plays.md`, `universe.md` (ecology/food
  web), `ocean-shaped.md` (grounding rulebook), `creatures/README.md`
  (bestiary) — `~/Work/crabscape/docs/design/`.
- Red-team files: `loop-findings/findings-{auditor,architecture,lenses,
  narrative,predator}.md`; skills-web red team: `findings-{systems-skills,
  skills-auditor,predator-skills}.md`.
- `skills-web.md` — the skills addendum (§2.5 pointer).
- `[GAP-1]` combat/exam spec (+ commit-budget, post-peak verb) · `[GAP-2]`
  **RULED (R-1)** save-on-bank · `[GAP-3]` power-fantasy beats
  (owner-visible, recommendation attached) · `[GAP-4]` exam tested-verb
  (owner-visible, recommendation attached) · `[GAP-5]` **closed by Cmd 12**.
