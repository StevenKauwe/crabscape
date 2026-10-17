# CrabScape Applications — the Commandments

> owner: **core** | status: v1 | last: 2026-10-17
>
> THE PURE BIBLE lives at `bible.md` — principles only, no
> project decisions. THIS doc applies those principles to CrabScape. Every
> commandment cites its bible principle (C#) and source lane. Design docs
> downstream cite: `bible.md` for principles, this file for
> project calls, and the catechism numbers below.

---

## Q1 application · Upkeep without chores (from Bible Q1; C1, C4, C10)
1. Friction lives on the dive, never at home. Home rituals are batch and warm
   (one stew = several dives' rations, chosen like a loadout).
2. The lantern isn't fed on a timer; it's cultured big at home, and how
   bright to run it is a dive-long choice between seeing and being seen.
3. One maintenance system per zone, maximum.
4. No hunger nag. Food is dive-duration currency, spent by player choice.
5. Kit-framing at home (gambles), never to-do framing underwater.

## Q2 application · The two books (from Bible Q2; C2, C3)
- **Dive-book** (dies or is spent on the dive): oxygen margin, stamina
  reserve, carried cargo, this trip's lantern fuel.
- **Home-book** (persists): skills-by-use, shell-home build-outs, recipes,
  village standing, authored world changes, and the player's own knowledge.
- **The conversion rule — the game's meaning:** nothing moves dive-book →
  home-book except by surviving the way home. Death may bank knowledge, never
  cargo.
- Telemetry: log depth reached, death cause, cargo carried home, dive length
  — every dive. The death-cause histogram is the balance instrument (C9).

## Q3 application · The rotating bay (from Bible Q3; C5, C6)
Four layers, by cadence: tidal (hours) → ecological (days/seasons) →
catastrophic (rare) → player-authored (permanent, always legible). Build
artifact: the **route table** — ~a dozen named passages, 2–3 states each.
Post-mastery: player-set dive contracts (dim lantern, hungrier water,
shorter slack) — priced and reward-coupled, never shame-coupled.

## Q4 application · Zones as ladders (from Bible Q4; C3, C7)
- Visibility is the first rung: every lantern improvement re-skins the same
  pit. Generalize per zone.
- One page per zone: identity, resources, hazards, story beat, and its 3–5
  capability rungs (each opened by something found in that zone).
- Stage each zone as a needs ladder: safety → food → capability → curiosity.
- Rest stops are built reach: constructing a camp extends the zone. Building
  = exploration.
- Rungs are verbs (grip, burrow, cut) — never percentage stats.

## Q5 application · The slice (from Bible Q5; C8)
- The Sunlit Shallows vertical slice is the whole pre-production goal:
  home → dive → eggs → lantern → scissor claws → ghost net → octopus exam →
  bigger shell. No zone two until a stranger wants one more dive.
- Definition of done: playable end-to-end + documented iteration + postmortem.
- Paper-prototype the dive loadout decision (cards) before coding it.
- Each zone ships as one gate / one verb / one boss exam — never more.

## The commandments, short form
1. The dive is the run; the bay is the meta; the books never mix.
2. Nothing converts dive-book → home-book except surviving the way home.
3. The pit is the slice — prove the loop before adding depth.
4. Fixed rocks, rotating water; the player always sees what changed.
5. Rungs are verbs; visibility is the first rung.
6. One maintenance system per zone; spend by choice, never by clock.
7. Kit up at home; never nag underwater.
8. Instrument every dive; tune from histograms.
9. One gate, one verb, one exam per zone.
10. Death banks knowledge, never cargo.
11. Peaks end into a mundane verb — after the bank, after the exam, after
    the rescue: cook, repair, build. No celebration screen; the ordinary
    rhythm is the reward. *(from C11; NAR-N5 promoted 2026-10-17)*
12. An egg eaten in place is *claimed, not banked*: the stroke converts
    only when the crab comes home. Die with it in your belly and the sea
    keeps it. *(from C2/Cmd 2; the exception becomes the law's proof —
    GAP-5 ruling 2026-10-17. T-4 ruled same day: eggs remain goods-class*
    *under Cmd 12 — they do not draw the molt wallet; the wallet is reserved*
    *for the regrow-vs-feat decision and autotomy-delay. The pit teaches*
    *claiming; the bay teaches molting.)*
13. Curves are feel; gates are verbs or perception. No gate may check a
    level number. The one capacity exception is depth-adaptation, and it is
    sealed by lethality: a gate is capacity-class only if failing it kills
    (pressure, cold, oxygen); light merely stops you, so light-radius gates
    are Sensing-class. *(from C3/C6/C7 + skills-web §4; L-2 lethality seal;
    promoted 2026-10-17)*
14. The web keeps old water load-bearing: every new zone ships with a sink,
    a secret, and a training ground for every older zone. If any old zone
    has none, the new zone is a leaver — fix it before ship. *(from C12;
    skills-web §5 revisit test; promoted 2026-10-17)*
15. Two currencies, two laws: keys and goods are claimed-then-banked (the
    sea keeps cargo); competence banks live (the sea never keeps what you
    became). *(from Cmd 12 + skills-web §3/§3a; the two-currency principle,
    2026-10-17)*
16. Growth deploys at the molt. Skill earns eligibility; biomass pays the
    body's cost; the molt — rested at home, fed and safe — is the ceremony
    where a feat becomes anatomy. Feats never unlock mid-dive. Home is the
    only place a crab can afford to be soft. *(owner concept, 2026-10-17;
    C2/C11/C12; lineage: the crab-ocean world bible's molt clock)*
17. Home never punishes absence. The garden does not wither; the stew does
    not spoil; the shell waits warm. The lantern culture remains the home's
    single maintenance system — home is where upkeep goes to rest, and the
    cozy pillar is law, not decor. *(from C4/Cmd 7; skills-web v2.0 proposal;
    ML-5 support; promoted 2026-10-17)*

## Source map
See `bible.md` — every application above traces to a
principle there and a source lane (`courses/`, `talks/`, `books/`,
`channels/`). All lane docs pass `tools/verbatim-check.py`.
