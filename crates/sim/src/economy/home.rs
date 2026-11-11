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
    KelpCurtain, "kelp-curtain", "Kelp curtain", 3,
        "A swaying curtain across the doorway.", [(Kelp, 6), (KelpCord, 2)];
    GlassLamp, "glass-lamp", "Glass lamp", 8,
        "A glass sponge lamp: a warm glow at the home.", [(Glass, 4), (KelpCord, 1)];
    ShellMosaic, "shell-mosaic", "Shell mosaic", 15,
        "Clam valves and grit set in a pattern on the wall.", [(Valve, 8), (Grit, 4), (Pearl, 1)];
    WhelkDoor, "whelk-door", "Whelk door", 20,
        "A whelk shell set in clay, a door for the doorway.", [(WhelkShell, 1), (Clay, 2)];
    BoneArch, "bone-arch", "Bone arch", 35,
        "Whale bone ribs over the home, clay-set.", [(WhaleBone, 2), (Clay, 3), (MantisClub, 1)];
    PhotophoreString, "photophore-string", "Photophore string", 40,
        "Photophores on slime thread: a cold light along the roof.", [(Photophore, 3), (SlimeThread, 2)];
    CoralGarden, "coral-garden", "Coral garden", 55,
        "Black and bamboo coral set in clay beside the door.", [(BlackCoral, 2), (BambooCoral, 1), (Clay, 4)];
    NoduleHearth, "nodule-hearth", "Nodule hearth", 70,
        "Warm vent-stone nodules, no fire: the shell mends sooner at home.", [(Nodule, 4), (ScalePlate, 1), (Clay, 4)];
}

/// Building experience for building a piece of Building `level`:
/// `40 x level / 5`, at least 40.
pub fn piece_xp(level: u32) -> u32 {
    (40 * level / 5).max(40)
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
        self.earn(id, piece.skill(), piece.xp() as f32);
        self.sound(crate::world::cues::Cue::Crafted { crab: id });
    }
}
