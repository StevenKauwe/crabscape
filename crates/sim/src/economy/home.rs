//! Home pieces: the things a crab builds into the shared home with its
//! Building skill (`docs/design/skills-and-resources.md`, "Home pieces").
//!
//! A piece is built once per home *world* (the host's in co-op, its own
//! solo) and everyone in that world sees it. The materials come out of the
//! builder's own bank; the piece is shared. Where they are kept:
//! `Shared::home_pieces`, host-authoritative like a taken clam
//! ([`Thing::Piece`] through `Msg::Take` and `Msg::Taken`), and saved with
//! the world's part of `Progress` (`Progress::home_pieces`, next to the
//! clam beds): in solo and on the host the crab's own world is its home.

use serde::{Deserialize, Serialize};

use super::Item;
use crate::skills::Skill;

macro_rules! pieces {
    ($( $v:ident, $word:literal, $name:literal, $level:literal, $does:literal, [$( ($i:ident, $n:literal) ),*]; )*) => {
        /// One piece that can be built into the shared home.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum Piece {
            $( $v, )*
        }

        /// Every piece's row, in order of Building level.
        pub const PIECES: &[PieceDef] = &[
            $( PieceDef {
                piece: Piece::$v,
                word: $word,
                name: $name,
                level: $level,
                needs: &[$( (Item::$i, $n) ),*],
                does: $does,
            }, )*
        ];
    };
}

/// One row of the home pieces table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PieceDef {
    pub piece: Piece,
    /// The word a replay writes (`craft=glass-lamp`).
    pub word: &'static str,
    pub name: &'static str,
    /// The Building level needed.
    pub level: u32,
    pub needs: &'static [(Item, u32)],
    /// What it does, in a few words.
    pub does: &'static str,
}

pieces! {
    ValveStep, "valve-step", "Valve step", 2,
        "Clam valves laid like paving at the doorway.", [(Valve, 6), (Grit, 2)];
    KelpCurtain, "kelp-curtain", "Kelp curtain", 3,
        "A swaying curtain across the doorway.", [(Kelp, 6), (KelpCord, 2)];
    SnailCairn, "snail-cairn", "Snail-shell cairn", 4,
        "Empty snail shells stacked beside the door.", [(Snail, 4), (Grit, 3)];
    GritPath, "grit-path", "Grit path", 6,
        "A path of grit edged with polished valves, raked to the door.", [(PolishedValve, 4), (Grit, 8)];
    GlowJar, "glow-jar", "Glow jar", 7,
        "A valve jar of shore-glow bacteria: a faint light by the door.", [(ShoreStrain, 2), (Valve, 2), (KelpCord, 1)];
    ValveChime, "valve-chime", "Valve chime", 8,
        "Clam valves on kelp cord, clicking in the current.", [(Valve, 6), (KelpCord, 2), (Grit, 2)];
    GlassLamp, "glass-lamp", "Glass lamp", 8,
        "A glass sponge lamp: a warm glow at the home.", [(Glass, 4), (KelpCord, 1)];
    RopeRail, "rope-rail", "Rope rail", 9,
        "A kelp-rope rail along the doorstep.", [(KelpRope, 2), (Kelp, 4)];
    KelpGarden, "kelp-garden", "Kelp garden", 10,
        "Kelp tufts rooted in grit, with shore-glow in the shade.", [(Kelp, 8), (ShoreStrain, 2), (Grit, 3)];
    ClawRack, "claw-rack", "Claw rack", 11,
        "Snapper claws hung on kelp rope by the door.", [(SnapperClaw, 2), (KelpRope, 1), (Grit, 2)];
    ValveShelf, "valve-shelf", "Valve shelf", 12,
        "A shelf of clam valves, rope-hung against the wall.", [(Valve, 8), (KelpRope, 2), (Grit, 4)];
    SandSill, "sand-sill", "Sand-dollar sill", 13,
        "Sand dollars set in shell cement along the sill.", [(SandDollar, 4), (ShellCement, 1), (KelpCord, 2)];
    EelgrassThatch, "eelgrass-thatch", "Eelgrass thatch", 14,
        "Eelgrass laid over kelp mats, a soft roof over the door.", [(Eelgrass, 8), (KelpMat, 2), (Grit, 2)];
    BarnacleWall, "barnacle-wall", "Barnacle wall", 15,
        "Barnacles set in shell cement, a rough crust on the wall.", [(Barnacle, 6), (ShellCement, 2), (Grit, 2)];
    ShellMosaic, "shell-mosaic", "Shell mosaic", 17,
        "Clam valves, grit and shed teeth set in a pattern on the wall.", [(Valve, 8), (Grit, 4), (Pearl, 1), (Tooth, 4)];
    WhelkDoor, "whelk-door", "Whelk door", 20,
        "A whelk shell set in clay and edged with seep mussel shells, a door for the doorway.", [(WhelkShell, 1), (Clay, 2), (SeepMussel, 2)];
    BoneArch, "bone-arch", "Bone arch", 35,
        "Whale bone ribs over the home, clay-set.", [(WhaleBone, 2), (Clay, 3), (MantisClub, 1)];
    PhotophoreString, "photophore-string", "Photophore string", 40,
        "Photophores on slime thread: a cold light along the roof.", [(Photophore, 3), (SlimeThread, 2)];
    CoralGarden, "coral-garden", "Coral garden", 55,
        "Black coral and a mantis telson set in clay beside the door.", [(BlackCoral, 2), (MantisTelson, 1), (Clay, 4)];
    NoduleHearth, "nodule-hearth", "Nodule hearth", 70,
        "Warm vent-stone nodules, no fire: the shell mends sooner at home.", [(Nodule, 4), (ScalePlate, 1), (Clay, 4)];
}

/// Building experience for building a piece of Building `level`.
///
/// From level 2 to 15 a piece (and an early station) pays a level and a
/// fifth of Building: the experience from its own level to the next, plus 20%, so a new crab
/// that can build one piece at each level climbs without a stall, even if
/// it never meets clay, quill or glass (the first hours, `ladder.rs`). Above
/// 15 it is `40 x level / 5`, at least 40.
pub fn piece_xp(level: u32) -> u32 {
    if (2..=crate::economy::ladder::LAST_EARLY).contains(&level) {
        super::xp::level_step(level) * 6 / 5
    } else {
        (40 * level / 5).max(40)
    }
}

impl Piece {
    /// Its row.
    pub fn def(self) -> &'static PieceDef {
        PIECES
            .iter()
            .find(|d| d.piece == self)
            .expect("every piece has a row")
    }

    /// The piece a replay word names, if it is one.
    pub fn parse(word: &str) -> Option<Piece> {
        let word = word.trim();
        PIECES.iter().find(|d| d.word == word).map(|d| d.piece)
    }

    pub fn word(self) -> &'static str {
        self.def().word
    }

    pub fn name(self) -> &'static str {
        self.def().name
    }

    /// The Building level needed.
    pub fn level(self) -> u32 {
        self.def().level
    }

    pub fn needs(self) -> &'static [(Item, u32)] {
        self.def().needs
    }

    /// The experience the builder earns.
    pub fn xp(self) -> u32 {
        piece_xp(self.level())
    }

    /// The skill that builds it.
    pub fn skill(self) -> Skill {
        Skill::Building
    }
}

use std::collections::BTreeSet;

use crate::craft::CantMakeEcon;
use crate::net::{Msg, To};
use crate::shared::Thing;
use crate::world::{PlayerId, Sim};

impl Sim {
    /// The pieces built into the home of the world this device shows (the
    /// host's, for a guest), for drawing.
    pub fn home_pieces(&self) -> &BTreeSet<Piece> {
        &self.shared.home_pieces
    }

    /// Could crab `id` build `piece` right now? It must be tucked in at
    /// home, with the Building level and the materials in its own bank, and
    /// the home must not have the piece yet (or a request for it pending).
    pub fn can_build_piece(&self, id: PlayerId, piece: Piece) -> Result<(), CantMakeEcon> {
        let crab = self.at_home(id)?;
        let def = piece.def();
        if crab.level(Skill::Building, &self.tuning) < def.level {
            return Err(CantMakeEcon::Level(Skill::Building, def.level));
        }
        let thing = Thing::Piece(piece);
        let asked = self
            .net_ref()
            .is_some_and(|n| n.asked.iter().any(|a| a.1 == thing));
        if !self.shared.there(thing, &self.tuning) || asked {
            return Err(CantMakeEcon::Made);
        }
        for &(item, n) in def.needs {
            let have = crab.stash.count(item);
            if have < n {
                return Err(CantMakeEcon::Missing(item, n - have));
            }
        }
        Ok(())
    }

    /// Crab `id` builds `piece` into the shared home. Solo and the host
    /// build at once; a guest asks the host and pays, and earns, only when
    /// the host says the piece is built and its own ([`Sim::finish_piece`]).
    /// A running recording notes it (`craft=<piece word>`).
    pub fn build_piece(&mut self, id: PlayerId, piece: Piece) -> Result<(), CantMakeEcon> {
        self.can_build_piece(id, piece)?;
        let thing = Thing::Piece(piece);
        match self.net_mut() {
            Some(n) if !n.host => {
                n.asked.push((id, thing));
                n.send(To::Host, Msg::Take { by: id, thing });
            }
            _ => {
                self.shared.take(thing, &self.tuning);
                if let Some(n) = self.net_mut() {
                    n.send(
                        To::Everyone,
                        Msg::Taken {
                            by: Some(id),
                            thing,
                        },
                    );
                }
                self.finish_piece(id, piece);
            }
        }
        self.record_craft(id, crate::craft::Craft::Piece(piece));
        Ok(())
    }

    /// The home has `piece` and crab `id` built it: its materials go from
    /// its own bank and Building earns the experience. (A crab that no
    /// longer holds the materials pays and earns nothing: the piece stays.)
    pub(crate) fn finish_piece(&mut self, id: PlayerId, piece: Piece) {
        let Some(crab) = self.crab_mut(id).filter(|c| !c.remote) else {
            return;
        };
        if !crab.stash.has(piece.needs()) {
            return;
        }
        crab.stash.take(piece.needs());
        crab.econ.crafts_done += 1;
        if crab
            .econ
            .pinned
            .as_ref()
            .is_some_and(|p| p.word == piece.word())
        {
            crab.econ.pinned = None;
        }
        self.earn(id, piece.skill(), piece.xp() as f32);
        self.sound(crate::world::cues::Cue::Crafted { crab: id });
    }
}
