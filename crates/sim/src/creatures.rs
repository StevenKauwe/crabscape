//! The ten bestiary creatures: one module each, so ten lanes never collide.
//!
//! Each module holds its creature's name, state, tuning, placement and
//! hooks into the generic enemy skeleton (`enemy::step`). This file only
//! dispatches by kind; it is the one place that lists them all.

use bevy_math::Vec2;

use crate::combat::Outcome;
use crate::enemy::{AnimalPrey, Enemy, Kind};
use crate::level::Level;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, Tuning};
use crate::world::{PlayerId, SimCrab};

pub mod amphipods;
pub mod bobbit;
pub mod grouper;
pub mod hagfish;
pub mod hermit;
pub mod isopod;
pub mod lobster;
pub mod squid;
pub mod turtle;
pub mod viperfish;

/// A new creature's own state, beside the generic enemy's.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Extra {
    #[default]
    None,
    Turtle(turtle::State),
    Squid(squid::State),
    Viperfish(viperfish::State),
    Grouper(grouper::State),
    Lobster(lobster::State),
    Hermit(hermit::State),
    Bobbit(bobbit::State),
    Isopod(isopod::State),
    Hagfish(hagfish::State),
    Amphipods(amphipods::State),
    Sixgill(crate::sixgill::State),
}

/// The starting [`Extra`] for a kind.
pub fn extra(kind: Kind) -> Extra {
    match kind {
        Kind::LoggerheadTurtle => Extra::Turtle(turtle::State::default()),
        Kind::HumboldtSquid => Extra::Squid(squid::State::default()),
        Kind::Viperfish => Extra::Viperfish(viperfish::State::default()),
        Kind::GoliathGrouper => Extra::Grouper(grouper::State::default()),
        Kind::Lobster => Extra::Lobster(lobster::State::default()),
        Kind::HermitCrab => Extra::Hermit(hermit::State::default()),
        Kind::BobbitWorm => Extra::Bobbit(bobbit::State::default()),
        Kind::GiantIsopod => Extra::Isopod(isopod::State::default()),
        Kind::Hagfish => Extra::Hagfish(hagfish::State::default()),
        Kind::AmphipodSwarm => Extra::Amphipods(amphipods::State::default()),
        Kind::Sixgill => Extra::Sixgill(crate::sixgill::State::default()),
        _ => Extra::None,
    }
}

/// Its tuning, for the new kinds.
pub fn tuning(kind: Kind) -> Option<&'static EnemyTuning> {
    match kind {
        Kind::LoggerheadTurtle => Some(turtle::tuning()),
        Kind::HumboldtSquid => Some(squid::tuning()),
        Kind::Viperfish => Some(viperfish::tuning()),
        Kind::GoliathGrouper => Some(grouper::tuning()),
        Kind::Lobster => Some(lobster::tuning()),
        Kind::HermitCrab => Some(hermit::tuning()),
        Kind::BobbitWorm => Some(bobbit::tuning()),
        Kind::GiantIsopod => Some(isopod::tuning()),
        Kind::Hagfish => Some(hagfish::tuning()),
        Kind::AmphipodSwarm => Some(amphipods::tuning()),
        _ => None,
    }
}

/// Its own step, if it has one (`None` = the generic skeleton).
#[allow(clippy::too_many_arguments)]
pub(crate) fn step(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
    animals: &[AnimalPrey],
    bounded_region: bool,
) -> Option<Vec<(PlayerId, Outcome)>> {
    match e.kind {
        Kind::LoggerheadTurtle => turtle::step(e, crabs, t, level, rng, animals, bounded_region),
        Kind::HumboldtSquid => squid::step(e, crabs, t, level, rng, animals, bounded_region),
        Kind::Viperfish => viperfish::step(e, crabs, t, level, rng, animals, bounded_region),
        Kind::GoliathGrouper => grouper::step(e, crabs, t, level, rng, animals, bounded_region),
        Kind::Lobster => lobster::step(e, crabs, t, level, rng, animals, bounded_region),
        Kind::HermitCrab => hermit::step(e, crabs, t, level, rng, animals, bounded_region),
        Kind::BobbitWorm => bobbit::step(e, crabs, t, level, rng, animals, bounded_region),
        Kind::GiantIsopod => isopod::step(e, crabs, t, level, rng, animals, bounded_region),
        Kind::Hagfish => hagfish::step(e, crabs, t, level, rng, animals, bounded_region),
        Kind::AmphipodSwarm => amphipods::step(e, crabs, t, level, rng, animals, bounded_region),
        _ => None,
    }
}

/// What a blow did to it, if it has its own rules (`None` = generic).
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    match e.kind {
        Kind::LoggerheadTurtle => turtle::feels(e, crab, outcome, t),
        Kind::HumboldtSquid => squid::feels(e, crab, outcome, t),
        Kind::Viperfish => viperfish::feels(e, crab, outcome, t),
        Kind::GoliathGrouper => grouper::feels(e, crab, outcome, t),
        Kind::Lobster => lobster::feels(e, crab, outcome, t),
        Kind::HermitCrab => hermit::feels(e, crab, outcome, t),
        Kind::BobbitWorm => bobbit::feels(e, crab, outcome, t),
        Kind::GiantIsopod => isopod::feels(e, crab, outcome, t),
        Kind::Hagfish => hagfish::feels(e, crab, outcome, t),
        Kind::AmphipodSwarm => amphipods::feels(e, crab, outcome, t),
        _ => None,
    }
}

/// Its finer state, if it names one (`None` = generic).
pub fn phase(e: &Enemy, _t: &Tuning) -> Option<&'static str> {
    match e.kind {
        Kind::LoggerheadTurtle => turtle::phase(e),
        Kind::HumboldtSquid => squid::phase(e),
        Kind::Viperfish => viperfish::phase(e),
        Kind::GoliathGrouper => grouper::phase(e),
        Kind::Lobster => lobster::phase(e),
        Kind::HermitCrab => hermit::phase(e),
        Kind::BobbitWorm => bobbit::phase(e),
        Kind::GiantIsopod => isopod::phase(e),
        Kind::Hagfish => hagfish::phase(e),
        Kind::AmphipodSwarm => amphipods::phase(e),
        _ => None,
    }
}

/// Where every new creature lives, in table order.
pub fn spots(level: &Level) -> Vec<(Kind, Vec2)> {
    let mut all = Vec::new();
    all.extend(
        turtle::spots(level)
            .into_iter()
            .map(|p| (Kind::LoggerheadTurtle, p)),
    );
    all.extend(
        squid::spots(level)
            .into_iter()
            .map(|p| (Kind::HumboldtSquid, p)),
    );
    all.extend(
        viperfish::spots(level)
            .into_iter()
            .map(|p| (Kind::Viperfish, p)),
    );
    all.extend(
        grouper::spots(level)
            .into_iter()
            .map(|p| (Kind::GoliathGrouper, p)),
    );
    all.extend(
        lobster::spots(level)
            .into_iter()
            .map(|p| (Kind::Lobster, p)),
    );
    all.extend(
        hermit::spots(level)
            .into_iter()
            .map(|p| (Kind::HermitCrab, p)),
    );
    all.extend(
        bobbit::spots(level)
            .into_iter()
            .map(|p| (Kind::BobbitWorm, p)),
    );
    all.extend(
        isopod::spots(level)
            .into_iter()
            .map(|p| (Kind::GiantIsopod, p)),
    );
    all.extend(
        hagfish::spots(level)
            .into_iter()
            .map(|p| (Kind::Hagfish, p)),
    );
    all.extend(
        amphipods::spots(level)
            .into_iter()
            .map(|p| (Kind::AmphipodSwarm, p)),
    );
    all
}

/// Swims in open water and keeps its authored depth.
pub fn midwater(kind: Kind) -> bool {
    match kind {
        Kind::LoggerheadTurtle => turtle::MIDWATER,
        Kind::HumboldtSquid => squid::MIDWATER,
        Kind::Viperfish => viperfish::MIDWATER,
        Kind::GoliathGrouper => grouper::MIDWATER,
        Kind::Lobster => lobster::MIDWATER,
        Kind::HermitCrab => hermit::MIDWATER,
        Kind::BobbitWorm => bobbit::MIDWATER,
        Kind::GiantIsopod => isopod::MIDWATER,
        Kind::Hagfish => hagfish::MIDWATER,
        Kind::AmphipodSwarm => amphipods::MIDWATER,
        _ => false,
    }
}

/// What its body leaves.
pub fn drops(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::LoggerheadTurtle => turtle::DROPS,
        Kind::HumboldtSquid => squid::DROPS,
        Kind::Viperfish => viperfish::DROPS,
        Kind::GoliathGrouper => grouper::DROPS,
        Kind::Lobster => lobster::DROPS,
        Kind::HermitCrab => hermit::DROPS,
        Kind::BobbitWorm => bobbit::DROPS,
        Kind::GiantIsopod => isopod::DROPS,
        Kind::Hagfish => hagfish::DROPS,
        Kind::AmphipodSwarm => amphipods::DROPS,
        _ => &[],
    }
}
