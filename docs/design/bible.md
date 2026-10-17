# The Design Bible — Pure Principles

> owner: **core** | status: v1 PURE — no project-specific decisions herein | last: 2026-10-17
>
> This document is the religion: what the studied sources teach, separated
> from any one game's application of it. Every principle is evidence-tagged
> to its source lane (`courses/`, `talks/`, `books/`, `channels/`).
> Project-specific calls live in `commandments.md`
> (Applications) and downstream design docs — never here.
> All lane docs pass `tools/verbatim-check.py`; this bible lifts no prose.

---

## The Five Questions

### 1 · Chore vs fun
**Agreed across sources:** friction is fun exactly when it forces a choice
and a chore exactly when it doesn't. Actions should be textured verbs with
discernible, integrated outcomes — actions without both are filler to
automate or cut (Schell, books/). Paper-prototype the economy before coding:
a boring resource loop on a table is still boring in engine (Fullerton,
books/). Balance in playtests by asking whether the player just made a
decision or just did a task (MSU, courses/). Harsh friction earns its place
when the friction *is* the game (Dead Cells, talks/). Upkeep lands best when
it is diegetic — the reason the session has a shape, not a bar to fill
(Subnautica, talks/). Presentation matters as much as mechanics: the same
task reads as errand or gamble depending on framing (Extra Credits,
channels/).
**Divergence:** keep-friction (Dead Cells) vs hide-friction (diegetic camp)
— reconciled by asking what decision the friction carries.

### 2 · Run-progression vs meta-progression
**Agreed:** the admired roguelites split progression into two ledgers that
never co-mingle — temporary run power vs persistent advancement (Hades,
talks/). The conversion rule between the books is the game's moral center:
what survives a failed run, and how, defines what the game is about.
Meta-progression kept deliberately light keeps runs honest (Dead Cells,
talks/). Tuning that split is done from logged play distributions, not
intuition (Slay the Spire, talks/). Persistent unlocks work best as new
verbs, not number multipliers (GMTK, channels/).

### 3 · Fixed map, dynamic world
**Agreed:** near-infinite replay comes from rotating conditions on small
fixed spaces, not from more geometry (Into the Breach, talks/). Replay layers
are modifiers on known rooms (Hades, talks/). Post-mastery longevity comes
from player-set difficulty contracts — specific, priced, reward-coupled
(Hades, talks/). Session-reset-with-persistent-world is accepted when
persistence is legible — the player can always point at what their last run
changed (Against the Storm, talks/). Live-game cadence is new rules on old
rocks (Dead Cells, talks/). Same-space replay survives when the *solution
set* rotates — which route, which threat, which rule (Extra Credits,
channels/). Condition variance beats loot variance (Slay the Spire, GMTK).

### 4 · Progression inside a space
**Agreed:** one contiguous space becomes distinct zones through gated
visibility, pressure, and fear — perception distance is progression
(Subnautica, talks/). A zone deserves its own page: identity, resources,
hazards, story (CalArts, courses/). Internal staging works as a needs ladder
— safety, food, capability, curiosity (Extra Credits, channels/). One new
dimension of complexity per shipping cycle (MSU, courses/). Ability-gates
inside zones create re-entry loops that keep old spaces alive (GMTK on
Hollow Knight grammar, channels/).

### 5 · Slice discipline
**Agreed:** the unit of proof is the first playable prototype (MSU,
courses/). An artifact is half credit without documented iteration and an
honest postmortem (MIT OCW, courses/). Prototype the riskiest thing first —
if the core verb isn't a toy yet, nothing built on it matters (Schell,
books/; GMTK, channels/). Prototype on paper what can be (Fullerton,
books/). Cut years of work to perfect the tiny loop rather than ship the
large unfinished one (Into the Breach, talks/). Ship one region fully
loop-complete; let the roadmap be depth (Valheim, talks/).

---

## The Catechism (pure form)

1. Friction that creates a decision is fun; friction that creates a task is
   a chore. When in doubt, remove the element and ask what decision was lost.
2. Run-progression and meta-progression are separate books. The conversion
   rule between them — what survives failure, and how — is the game's
   meaning. Keep it legible.
3. A persistent unlock that grants a new verb outlives one that grants a
   number.
4. At most one maintenance system per content unit; consumption happens by
   player choice under pressure, never by clock.
5. Fixed geometry plus rotating conditions sustains replay longer than new
   geometry, and costs less.
6. Persistence must be legible: after every session the player can point at
   what changed.
7. Changing what the player can do or perceive re-skins the same space;
   perception gates are progression.
8. Prove the core loop in a slice before adding content. Done = playable +
   documented iteration + postmortem.
9. Instrument before tuning: log sessions, balance from distributions.
10. Presentation is half the design: a decision framed as an errand plays as
    one.
11. After every emotional peak, one mundane verb within a breath. Peaks are
    earned by the ordinary rhythm around them; lingering after a peak
    converts earned feeling into sentimentality. *(Provenance: internal —
    the crab-ocean world bible's resumption rule; kin to Schell's interest
    curves, books/. Not corpus-derived; labeled honestly.)*
12. A progression-by-use skill web is the fabric that keeps every space
    load-bearing: if advancement only lives in the newest space, the world
    is a corridor; if skills are trained *by doing* and every space feeds
    some skill or sink, the world is a home the player never outgrows.
    Moving on must never mean the old places stop mattering.
    *(Provenance: internal — the project's RuneScape/Valheim touchstones,
    vision.md; kin to C7 perception-gating and Q4 ability-gate re-entry
    loops, channels/.)*

## Source map
courses/ (CS50G, MSU, CalArts, MIT OCW) · talks/ (GDC: ItB, Dead Cells ×2,
StS ×2, Hades, Against the Storm, Valheim, Subnautica) · books/ (Schell,
Fullerton) · channels/ (GMTK, Extra Credits). Verified negative: no Kasavin
design talk in the free vault at fetch time (talks/).
