//! One rock batch for the lower canyon and the future strata bodies.
//! Four upper frames hold strata; the deeper water lanes remain sealed.
use super::strata::PocketKind;
use super::{Brittle, Climb, Growth, Level, WATER_SURFACE_Y};
use bevy_math::{Rect, Vec2};

pub(super) const DAM: Rect = Rect::new(17600.0, -10000.0, 17781.0, -4520.0);
pub(super) const NARROWS: Rect = Rect::new(18000.0, -10000.0, 18080.0, -9800.0);
pub(super) const VENTS: Rect = Rect::new(18300.0, -10000.0, 20800.0, -9000.0);

/// One opening in a frame's bedrock wall. Walls are twenty units thick, so
/// two frames forty units apart share a doorway when their gaps line up.
#[derive(Clone, Copy, Debug)]
pub(super) struct Gap {
    pub rect: Rect,
    /// Open gaps stay water or sediment; closed gaps are plugged with rock.
    pub open: bool,
}

/// An authored sealed frame and the cavities inside it.
#[derive(Clone, Debug)]
pub(super) struct Frame {
    pub name: &'static str,
    /// The interior; bedrock walls stand just outside it.
    pub rect: Rect,
    /// `Some` for a pocket crabs can use, with its fear register.
    pub kind: Option<PocketKind>,
    pub gaps: Vec<Gap>,
    /// The pocket's doors when they are not simply its open gaps (the home
    /// burrow's doors are the tops of its shafts, at the floor).
    pub doors: Vec<Rect>,
    /// Silt shafts from a gap up to the ground (diggable sand).
    pub shafts: Vec<Rect>,
    /// Water cavities inside the frame, and the gaps between cavities.
    pub voids: Vec<Rect>,
    /// Where the chamber's door and prize are (chambers only).
    pub chamber: Option<(Rect, Vec2)>,
}

const W: f32 = 120.0;

fn top(r: Rect, x: f32, w: f32, open: bool) -> Gap {
    Gap {
        rect: Rect::new(x - w * 0.5, r.max.y, x + w * 0.5, r.max.y + 20.0),
        open,
    }
}
fn bottom(r: Rect, x: f32, open: bool) -> Gap {
    Gap {
        rect: Rect::new(x - W * 0.5, r.min.y - 20.0, x + W * 0.5, r.min.y),
        open,
    }
}
fn left(r: Rect, y: f32, open: bool) -> Gap {
    Gap {
        rect: Rect::new(r.min.x - 20.0, y - W * 0.5, r.min.x, y + W * 0.5),
        open,
    }
}
fn right(r: Rect, y: f32, open: bool) -> Gap {
    Gap {
        rect: Rect::new(r.max.x, y - W * 0.5, r.max.x + 20.0, y + W * 0.5),
        open,
    }
}

// The pocket chain under the basin floor (the filled pit's old place).
pub(super) const BONE_PRIZE: Vec2 = Vec2::new(3600.0, -2420.0);
pub(super) const BONE_TOOTH: Vec2 = Vec2::new(3680.0, -2530.0);
/// The sponge that was the eastern gallery's second find lies here now.
pub(super) const CATHEDRAL_PRIZE: Vec2 = Vec2::new(16100.0, -5750.0);
pub(super) const SEEP_GARDEN_PRIZE: Vec2 = Vec2::new(19160.0, -10470.0);
pub(super) const WARM_SEAM_PRIZE: Vec2 = Vec2::new(20420.0, -11410.0);
/// The home burrow's first-dig clam and the two in its gallery floor.
pub(super) const FIRST_DIG_CLAM: Vec2 = Vec2::new(172.0, -45.0);
pub(super) const GALLERY_CLAMS: [Vec2; 2] = [Vec2::new(330.0, -528.0), Vec2::new(440.0, -528.0)];
/// The crust between the crust vault and the warm seam (needs a full charge).
pub(super) const WARM_SEAM_CRUST: Rect = Rect::new(20480.0, -10800.0, 20600.0, -10760.0);
/// The hanging garden: coral on the dam's east face, at the seep's outlet.
pub(super) const HANGING_GARDEN: Climb = Climb {
    x: 17800.0,
    bottom: -5900.0,
    top: -5700.0,
    kind: Growth::Coral,
};

fn full(name: &'static str, rect: Rect, kind: Option<PocketKind>, gaps: Vec<Gap>) -> Frame {
    Frame {
        name,
        rect,
        kind,
        gaps,
        doors: Vec::new(),
        shafts: Vec::new(),
        voids: vec![rect],
        chamber: None,
    }
}

/// Every sealed frame, derived from constants (never stored in `Level`).
pub(super) fn specs() -> &'static [Frame] {
    static SPECS: std::sync::OnceLock<Vec<Frame>> = std::sync::OnceLock::new();
    SPECS.get_or_init(build_specs)
}

fn build_specs() -> Vec<Frame> {
    use PocketKind::{Chamber, Tunnel};
    let mut out = Vec::new();

    // s0, under home: two silt shafts from the ground to a clay gallery.
    let r = Rect::new(80.0, -760.0, 680.0, -460.0);
    out.push(Frame {
        name: "home burrow",
        rect: r,
        kind: Some(Tunnel),
        gaps: vec![top(r, 172.0, 96.0, true), top(r, 588.0, 96.0, true)],
        doors: vec![
            Rect::new(124.0, -24.0, 220.0, -4.0),
            Rect::new(540.0, -24.0, 636.0, -4.0),
        ],
        // Door one is a silt shaft (the first dig, resting on the frame's
        // clay: nothing hangs over open water, unsupported sediment falls);
        // door two is a cased water shaft: a crab cannot dig up through a
        // long sealed column (grains need water within claw reach), so the
        // way out is a swim. It stands east of the shallow shortcut's sand.
        shafts: vec![Rect::new(124.0, -440.0, 220.0, 0.0)],
        // The gallery hangs from the frame's own bedrock roof.
        voids: vec![
            Rect::new(240.0, -532.0, 636.0, -460.0),
            Rect::new(540.0, -460.0, 636.0, 40.0),
        ],
        chamber: None,
    });

    // s1, in the filled pit: mantis mouth, clay tunnel, bone hall, seep shaft.
    let mantis = Rect::new(3720.0, -900.0, 3880.0, -720.0);
    out.push(full(
        "mantis mouth",
        mantis,
        Some(Tunnel),
        vec![top(mantis, 3800.0, W, true), bottom(mantis, 3800.0, true)],
    ));
    let clay = Rect::new(3720.0, -2100.0, 3880.0, -940.0);
    out.push(full(
        "clay tunnel",
        clay,
        Some(Tunnel),
        vec![top(clay, 3800.0, W, true), bottom(clay, 3800.0, true)],
    ));
    let hall = Rect::new(3380.0, -2640.0, 4180.0, -2140.0);
    out.push(Frame {
        name: "bone hall",
        rect: hall,
        kind: Some(Chamber),
        gaps: vec![top(hall, 3800.0, W, true), top(hall, 4110.0, W, true)],
        doors: Vec::new(),
        shafts: Vec::new(),
        // The hall holds its bones in clay: floor and walls stay clay.
        voids: vec![Rect::new(3440.0, -2540.0, 4170.0, -2140.0)],
        chamber: Some((Rect::new(3740.0, -2180.0, 3860.0, -2140.0), BONE_PRIZE)),
    });
    let seep = Rect::new(4040.0, -2100.0, 4180.0, -720.0);
    out.push(full(
        "seep shaft",
        seep,
        Some(Tunnel),
        vec![bottom(seep, 4110.0, true), top(seep, 4110.0, W, true)],
    ));

    // s2, under the eastern step (floor y -5000).
    let cath = Rect::new(15920.0, -5800.0, 16320.0, -5020.0);
    out.push(Frame {
        name: "clay cathedral",
        rect: cath,
        kind: Some(Chamber),
        gaps: vec![
            top(cath, 16120.0, W, true),
            right(cath, -5300.0, true),
            right(cath, -5700.0, true),
        ],
        doors: Vec::new(),
        shafts: Vec::new(),
        voids: vec![Rect::new(15960.0, -5760.0, 16320.0, -5020.0)],
        chamber: Some((
            Rect::new(16060.0, -5060.0, 16180.0, -5020.0),
            CATHEDRAL_PRIZE,
        )),
    });
    let warren = Rect::new(16360.0, -5500.0, 16560.0, -5020.0);
    out.push(full(
        "pebble warren",
        warren,
        Some(Tunnel),
        vec![top(warren, 16460.0, W, true), left(warren, -5300.0, true)],
    ));
    let cseep = Rect::new(16360.0, -5800.0, 17781.0, -5600.0);
    out.push(full(
        "cathedral seep",
        cseep,
        Some(Tunnel),
        vec![left(cseep, -5700.0, true), right(cseep, -5700.0, true)],
    ));

    // s3, under the vent field.
    let lc = Rect::new(720.0, -12800.0, 880.0, -10.0);
    out.push(full(
        "long chimney",
        lc,
        Some(Tunnel),
        vec![top(lc, 800.0, W, true), right(lc, -12700.0, true)],
    ));
    let ret = Rect::new(920.0, -12800.0, 18360.0, -12600.0);
    out.push(full(
        "chimney return",
        ret,
        Some(Tunnel),
        vec![left(ret, -12700.0, true), right(ret, -12700.0, true)],
    ));
    let ch = Rect::new(18400.0, -12800.0, 19000.0, -12400.0);
    out.push(full(
        "chimney hall",
        ch,
        Some(Tunnel),
        vec![
            left(ch, -12700.0, true),
            top(ch, 18920.0, W, true),
            bottom(ch, 18800.0, true),
        ],
    ));
    let cm = Rect::new(18840.0, -12360.0, 19000.0, -10020.0);
    out.push(full(
        "chimney mouth",
        cm,
        Some(Tunnel),
        vec![
            top(cm, 18920.0, W, true),
            bottom(cm, 18920.0, true),
            right(cm, -10300.0, true),
        ],
    ));
    let garden = Rect::new(19040.0, -10560.0, 19640.0, -10020.0);
    out.push(Frame {
        name: "seep garden",
        rect: garden,
        kind: Some(Chamber),
        gaps: vec![top(garden, 19350.0, W, true), left(garden, -10300.0, true)],
        doors: Vec::new(),
        shafts: Vec::new(),
        voids: vec![Rect::new(19040.0, -10520.0, 19620.0, -10020.0)],
        chamber: Some((
            Rect::new(19290.0, -10060.0, 19410.0, -10020.0),
            SEEP_GARDEN_PRIZE,
        )),
    });

    // s4, under the vent field's east end.
    let vault = Rect::new(20440.0, -10760.0, 20640.0, -10020.0);
    out.push(full(
        "crust vault",
        vault,
        Some(Tunnel),
        vec![top(vault, 20540.0, W, true), bottom(vault, 20540.0, true)],
    ));
    let seam = Rect::new(20300.0, -11500.0, 20960.0, -10800.0);
    out.push(Frame {
        name: "warm seam",
        rect: seam,
        kind: Some(Chamber),
        gaps: vec![top(seam, 20540.0, W, true)],
        doors: Vec::new(),
        shafts: Vec::new(),
        voids: vec![Rect::new(20340.0, -11460.0, 20920.0, -10800.0)],
        chamber: Some((
            Rect::new(20480.0, -10840.0, 20600.0, -10800.0),
            WARM_SEAM_PRIZE,
        )),
    });

    // The Plunge: water inside, sealed from everything except the odd
    // tunnel's dead end (the Plunge lane scripts the rest).
    let odd = Rect::new(18700.0, -13000.0, 18900.0, -12840.0);
    out.push(full(
        "odd tunnel",
        odd,
        None,
        vec![top(odd, 18800.0, W, true), bottom(odd, 18800.0, false)],
    ));
    let shaft = Rect::new(18700.0, -14100.0, 18900.0, -13040.0);
    out.push(full(
        "Plunge shaft",
        shaft,
        None,
        vec![top(shaft, 18800.0, W, false), right(shaft, -14025.0, false)],
    ));
    let crack = Rect::new(18940.0, -14100.0, 19240.0, -13950.0);
    out.push(full(
        "Plunge crack",
        crack,
        None,
        vec![left(crack, -14025.0, false), top(crack, 19090.0, W, false)],
    ));
    let stage = Rect::new(19340.0, -14200.0, 20040.0, -13800.0);
    out.push(full(
        "angler stage",
        stage,
        None,
        vec![top(stage, 19690.0, W, false), bottom(stage, 19690.0, false)],
    ));
    out
}

pub(super) fn shape(level: &mut Level) {
    // A zero-area event marker inside sealed bedrock, never a collision
    // surface. Its cut bit uses the existing world snapshot/event transport.
    level.nets.push(super::Net {
        rect: TETHER_SWITCH,
        cut: false,
        from: super::CutFrom::default(),
    });
    level.clams.push(DEEP_FOOD);
    level.max_x = 21000.0;
    level.floor.extend([
        Vec2::new(17700.0, -4520.0),
        Vec2::new(17780.0, -4520.0),
        Vec2::new(17781.0, -10000.0),
        Vec2::new(21000.0, -10000.0),
    ]);
    // A sediment heap rises eighty units beyond Coral Rise, then collapses
    // into a sheer face. The old heightmap and its outer bank stay intact.
    // The lock is a slot in a buttress rising to the water surface, like the
    // shallow sea arch: no high glide or excavated sill bypasses the crusher.
    level.rocks.extend([
        Rect::new(17960.0, -10300.0, 18120.0, -10000.0),
        Rect::new(17960.0, -9800.0, 18120.0, WATER_SURFACE_Y),
    ]);
    level.brittle.push(Brittle {
        rect: NARROWS,
        cracked: false,
    });
    for x in [18380.0, 19550.0, 20720.0] {
        level.climbs.push(Climb {
            x,
            bottom: -10000.0,
            top: -9800.0,
            kind: Growth::Coral,
        });
    }
    for spec in specs() {
        frame(level, spec);
    }
    // A rock casing around the home burrow's water shaft, so the floor's
    // sand cannot slump down it into the gallery.
    level.rocks.extend([
        Rect::new(524.0, -100.0, 540.0, 34.0),
        Rect::new(636.0, -100.0, 652.0, 50.0),
    ]);
    // The warm seam sits behind a crust that only a full crusher charge
    // cracks; the long chimney's crust plug caps its top (see `frame`).
    level.brittle.push(Brittle {
        rect: WARM_SEAM_CRUST,
        cracked: false,
    });
    // The hanging garden: a short coral ledge on the dam's east face, at the
    // cathedral seep's outlet (a refuge, rooted on the dam; never a ladder:
    // it stands a full rise below the dam's top).
    level.climbs.push(HANGING_GARDEN);
    // The crack has a 100-unit thick viewing wall, with a twelve-unit slit
    // into a separately sealed dark stage. A crab cannot fit through it.
    for (x0, x1, y0, y1) in [
        (19240.0, 19260.0, -14120.0, -13930.0),
        (19320.0, 19340.0, -14220.0, -13780.0),
    ] {
        level.rocks.retain(|r| *r != Rect::new(x0, y0, x1, y1));
        level.rocks.extend([
            Rect::new(x0, y0, 19340.0, -14031.0),
            Rect::new(x0, -14019.0, 19340.0, y1),
        ]);
    }
}

const TETHER_SWITCH: Rect = Rect::new(18040.0, -10150.0, 18040.0, -10150.0);
const TETHER: Rect = Rect::new(17781.0, -10000.0, 17960.0, -4300.0);
const DEEP_FOOD: Vec2 = Vec2::new(18450.0, -9995.0);

impl Level {
    pub fn tether_open(&self) -> bool {
        self.nets.iter().any(|n| n.rect == TETHER_SWITCH && n.cut)
    }

    pub(crate) fn visit_vents(&mut self, pos: Vec2) -> Option<usize> {
        if !VENTS.contains(pos) {
            return None;
        }
        let (i, net) = self
            .nets
            .iter_mut()
            .enumerate()
            .find(|(_, n)| n.rect == TETHER_SWITCH && !n.cut)?;
        net.cut = true;
        Some(i)
    }

    /// A clam-shaped placeholder for an unnamed deep food (species not verified).
    pub fn deep_food(&self) -> Option<Vec2> {
        self.clams.iter().copied().find(|p| *p == DEEP_FOOD)
    }

    /// Reuse the ordinary current's drag, collision and control rules. Only
    /// this local crab's physics step samples the steady lane; restore the
    /// world's tide immediately so other crabs and regions keep their tide.
    pub(crate) fn step_with_tether(
        &mut self,
        mover: &mut crate::Mover,
        controls: &crate::Controls,
        tuning: &crate::Tuning,
    ) -> crate::physics::StepEvents {
        let in_lane =
            self.tether_open() && TETHER.contains(mover.pos + Vec2::Y * mover.height(tuning) * 0.5);
        let tide = self.tide;
        if in_lane {
            self.currents.push(TETHER);
            self.tide = tuning.tide.upwelling_speed / tuning.tide.up_speed.max(1.0);
        }
        let events = crate::physics::step(mover, controls, tuning, self, crate::STEP);
        if in_lane {
            self.currents.pop();
            self.tide = tide;
        }
        events
    }
}

fn frame(level: &mut Level, spec: &Frame) {
    let r = spec.rect;
    let gaps: Vec<Rect> = spec.gaps.iter().map(|g| g.rect).collect();
    for (wall, horizontal) in [
        (
            Rect::new(r.min.x - 20.0, r.min.y - 20.0, r.min.x, r.max.y + 20.0),
            false,
        ),
        (
            Rect::new(r.max.x, r.min.y - 20.0, r.max.x + 20.0, r.max.y + 20.0),
            false,
        ),
        (Rect::new(r.min.x, r.min.y - 20.0, r.max.x, r.min.y), true),
        (Rect::new(r.min.x, r.max.y, r.max.x, r.max.y + 20.0), true),
    ] {
        let mut pieces = vec![wall];
        for gap in &gaps {
            pieces = pieces
                .into_iter()
                .flat_map(|piece| {
                    let cut = piece.intersect(*gap);
                    if cut.is_empty() {
                        return vec![piece];
                    }
                    let mut before = piece;
                    let mut after = piece;
                    if horizontal {
                        before.max.x = cut.min.x;
                        after.min.x = cut.max.x;
                    } else {
                        before.max.y = cut.min.y;
                        after.min.y = cut.max.y;
                    }
                    [before, after]
                        .into_iter()
                        .filter(|r| !r.is_empty())
                        .collect()
                })
                .collect();
        }
        level.rocks.extend(pieces);
    }
    for (i, gap) in spec.gaps.iter().enumerate() {
        if !gap.open {
            level.rocks.push(gap.rect);
        } else if spec.name == "long chimney" && i == 0 {
            // Cracked from below by the heavy crusher, with nothing solid
            // behind it: the sand above stays diggable.
            level.brittle.push(Brittle {
                rect: gap.rect,
                cracked: false,
            });
        }
    }
}
