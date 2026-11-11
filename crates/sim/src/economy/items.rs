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
    // Processed.
    KelpCord, "kelp-cord", "kelp cord", "kelp cord", 0, false;
    BraidedCord, "braided-cord", "braided cord", "braided cord", 0, false;
    ShellBlank, "shell-blank", "shell blank", "shell blanks", 0, false;
    QuillRod, "quill-rod", "quill rod", "quill rods", 0, false;
    SpiculeBundle, "spicule-bundle", "spicule bundle", "spicule bundles", 0, false;
    BoneSplint, "bone-splint", "bone splint", "bone splints", 0, false;
    NodulePlate, "nodule-plate", "nodule plate", "nodule plates", 0, false;
    ScalePlate, "scale-plate", "scale plate", "scale plates", 0, false;
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
