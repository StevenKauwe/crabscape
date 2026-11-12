//! Every thing a crab can collect, carry, bank and make from (the spec's
//! "Items": `docs/design/skills-and-resources.md`).
//!
//! Tools, gear and food portions are not items: they are equipped state
//! (`Economy::tools`, `Economy::gear`) or portions (`PreparedFood`).

use serde::{Deserialize, Serialize};

macro_rules! items {
    ($( $v:ident, $word:literal, $one:literal, $many:literal, $tier:literal, $big:literal; )*) => {
        /// One kind of thing in the bank or on the shell. `Ord` so it can key
        /// a `BTreeMap` (the bank is deterministic to iterate).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum Item {
            $( $v, )*
        }

        impl Item {
            /// Every item, in the order of the spec: raw, by-products, processed.
            pub const ALL: &'static [Item] = &[ $( Item::$v, )* ];

            /// What the menu calls one, and several.
            pub fn names(self) -> (&'static str, &'static str) {
                match self { $( Item::$v => ($one, $many), )* }
            }

            /// Does it ride on the back, one at a time? (Everything else
            /// clings to the shell.)
            pub fn big(self) -> bool {
                match self { $( Item::$v => $big, )* }
            }

            /// The tier of the nodes it comes from (0: not from a node).
            pub fn tier(self) -> u8 {
                match self { $( Item::$v => $tier, )* }
            }

            /// The word replays and typed codes use (kebab-case).
            pub fn word(self) -> &'static str {
                match self { $( Item::$v => $word, )* }
            }
        }
    };
}

items! {
    // Raw, from nodes (tier 1 to 5).
    Mussel, "mussel", "mussel", "mussels", 1, false;
    Clam, "clam", "clam", "clams", 2, false;
    Scallop, "scallop", "glass scallop", "glass scallops", 3, false;
    SeepMussel, "seep-mussel", "seep mussel", "seep mussels", 4, false;
    VentClam, "vent-clam", "vent clam", "vent clams", 5, false;
    Kelp, "kelp", "kelp frond", "kelp fronds", 1, false;
    Quill, "quill", "sea-pen quill", "sea-pen quills", 2, false;
    Glass, "glass", "glass sponge", "glass sponges", 3, false;
    BlackCoral, "black-coral", "black coral", "black coral", 4, false;
    BambooCoral, "bamboo-coral", "bamboo coral", "bamboo coral", 5, false;
    Grit, "grit", "shell grit", "shell grit", 1, false;
    Clay, "clay", "clay lump", "clay lumps", 2, false;
    Tooth, "tooth", "shed tooth", "shed teeth", 3, false;
    WhaleBone, "whale-bone", "whale bone", "whale bones", 4, true;
    Nodule, "nodule", "nodule", "nodules", 5, false;
    Snail, "snail", "snail", "snails", 1, false;
    Worm, "worm", "mud worm", "mud worms", 2, false;
    Lanternfish, "lanternfish", "lanternfish", "lanternfish", 3, false;
    Grenadier, "grenadier", "grenadier", "grenadiers", 4, true;
    ScalyFoot, "scaly-foot", "scaly-foot snail", "scaly-foot snails", 5, false;
    ShoreStrain, "shore-strain", "shore strain", "shore strains", 1, false;
    Culture, "culture", "glowing culture", "glowing cultures", 2, false;
    TwilightStrain, "twilight-strain", "twilight strain", "twilight strains", 3, false;
    SulfurMat, "sulfur-mat", "sulfur mat", "sulfur mats", 4, false;
    VentMat, "vent-mat", "vent mat", "vent mats", 5, false;
    // By-products.
    Valve, "valve", "clam valve", "clam valves", 0, false;
    Pearl, "pearl", "pearl", "pearls", 0, false;
    Scale, "scale", "scale", "scales", 0, false;
    Spoiled, "spoiled", "spoiled batch", "spoiled batches", 0, false;
    // Creature drops (the tier is the zone the creature lives in: basin 2,
    // twilight 3, midnight 4).
    SquidBeak, "squid-beak", "squid beak", "squid beaks", 3, false;
    ViperfishFang, "viperfish-fang", "viperfish fang", "viperfish fangs", 4, false;
    Photophore, "photophore", "photophore", "photophores", 4, false;
    WhelkShell, "whelk-shell", "whelk shell", "whelk shells", 2, false;
    Anemone, "anemone", "anemone", "anemones", 2, false;
    IsopodPlate, "isopod-plate", "isopod plate", "isopod plates", 4, false;
    BobbitJaw, "bobbit-jaw", "bobbit jaw", "bobbit jaws", 2, false;
    LobsterFragment, "lobster-fragment", "crusher fragment", "crusher fragments", 3, false;
    SlimeThread, "slime-thread", "slime thread", "slime threads", 4, false;
    Amphipods, "amphipods", "amphipod", "amphipods", 4, false;
    // Drops of the original creatures (the tier is the zone they live in).
    SnapperClaw, "snapper-claw", "snapper claw", "snapper claws", 1, false;
    ShrimpMeat, "shrimp-meat", "shrimp tail meat", "shrimp tail meat", 1, false;
    MantisClub, "mantis-club", "mantis club", "mantis clubs", 1, false;
    MorayTooth, "moray-tooth", "moray tooth", "moray teeth", 2, false;
    TriggerPlate, "trigger-plate", "triggerfish plate", "triggerfish plates", 2, false;
    JellyMesoglea, "jelly-mesoglea", "jelly mesoglea", "jelly mesoglea", 2, false;
    OctopusInk, "octopus-ink", "octopus ink", "octopus ink", 3, false;
    // Processed.
    KelpCord, "kelp-cord", "kelp cord", "kelp cord", 0, false;
    BraidedCord, "braided-cord", "braided cord", "braided cord", 0, false;
    ShellBlank, "shell-blank", "shell blank", "shell blanks", 0, false;
    QuillRod, "quill-rod", "quill rod", "quill rods", 0, false;
    SpiculeBundle, "spicule-bundle", "spicule bundle", "spicule bundles", 0, false;
    BoneSplint, "bone-splint", "bone splint", "bone splints", 0, false;
    NodulePlate, "nodule-plate", "nodule plate", "nodule plates", 0, false;
    ScalePlate, "scale-plate", "scale plate", "scale plates", 0, false;
    // From the Plunge's crack: a wild blue-green light, cultured at home
    // into a lantern of that colour (appended: item order is saved order).
    WildLight, "wild-light", "wild blue-green light", "wild blue-green lights", 0, false;
}

/// What a beaten creature of `kind` leaves, as items: the bestiary's drops
/// by name, and the original creatures' (the snapping shrimp's claw and
/// tail meat, the mantis's club, the moray's teeth, the triggerfish's
/// plates, the jelly's mesoglea, the octopus's ink; not verified as
/// harvestable products).
pub fn drops_of(kind: crate::enemy::Kind) -> Vec<Item> {
    use crate::enemy::Kind;
    let own: &[Item] = match kind {
        Kind::Snapper => &[Item::SnapperClaw, Item::ShrimpMeat],
        Kind::Mantis => &[Item::MantisClub],
        Kind::Moray => &[Item::MorayTooth],
        Kind::Triggerfish => &[Item::TriggerPlate],
        Kind::AlarmJelly => &[Item::JellyMesoglea],
        Kind::Octopus => &[Item::OctopusInk],
        _ => &[],
    };
    let mut out: Vec<Item> = crate::creatures::drops(kind)
        .iter()
        .filter_map(|name| from_drop(name))
        .collect();
    out.extend_from_slice(own);
    out
}

/// The item a creature's drop name (`creatures::drops`) stands for.
pub fn from_drop(name: &str) -> Option<Item> {
    Some(match name {
        "squid beak" => Item::SquidBeak,
        "viperfish fang" => Item::ViperfishFang,
        "photophore" => Item::Photophore,
        "whelk shell" => Item::WhelkShell,
        "anemone" => Item::Anemone,
        "isopod plate" => Item::IsopodPlate,
        "bobbit jaw" => Item::BobbitJaw,
        "lobster crusher fragment" => Item::LobsterFragment,
        "slime thread" => Item::SlimeThread,
        "amphipods" => Item::Amphipods,
        _ => return None,
    })
}

impl Item {
    /// The item a replay or code word names.
    pub fn parse(word: &str) -> Option<Item> {
        let word = word.trim();
        Item::ALL.iter().copied().find(|i| i.word() == word)
    }

    /// What a thing carried home is kept as at the workbench (a clam goes
    /// into the larder instead).
    pub fn kept(thing: crate::physics::Carry) -> Option<Item> {
        use crate::physics::Carry;
        match thing {
            Carry::Culture => Some(Item::Culture),
            Carry::Tooth => Some(Item::Tooth),
            Carry::Grit => Some(Item::Grit),
            Carry::Valve => Some(Item::Valve),
            Carry::Glass => Some(Item::Glass),
            Carry::Item(Item::Clam) | Carry::Clam => None,
            Carry::Item(item) => Some(item),
        }
    }

    /// Placeholder look for `src/`: a tint per tier (0: by-products and
    /// processed things, the workbench's own).
    pub fn tint(self) -> [f32; 3] {
        match self.tier() {
            1 => [0.62, 0.78, 0.55],
            2 => [0.55, 0.72, 0.78],
            3 => [0.55, 0.62, 0.86],
            4 => [0.66, 0.5, 0.82],
            5 => [0.9, 0.52, 0.44],
            _ => [0.82, 0.76, 0.62],
        }
    }
}
