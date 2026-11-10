//! Kills by buildings, which the game counts nowhere.
//!
//! The game counts a unit's kills on the unit itself, but a building's kill count stays at zero
//! however many units it kills. So the damage the game deals is watched (the `DamageUnit` hook) for
//! blows a building strikes that kill an enemy, and those are kept here by frame, the way the rest
//! of the history is: what a step killed is stamped with the frame the step ended on, a frame a
//! replay seek steps again comes out as it did and is left as recorded, and a frame a rollback
//! steps again is forgotten from on before what it kills this time is recorded.
//!
//! A building is named by its unique id, which it keeps for as long as it stands.

use std::sync::atomic::{AtomicBool, Ordering};

use super::Resimulation;

/// Whether the game's damage is being watched. Without that every building would read as having
/// killed nothing, whatever it has killed.
static COUNTED: AtomicBool = AtomicBool::new(false);

/// Says that the game's damage is being watched for buildings' kills.
pub fn set_counted() {
    COUNTED.store(true, Ordering::Relaxed);
}

/// Whether buildings' kills are being counted at all.
pub fn counted() -> bool {
    COUNTED.load(Ordering::Relaxed)
}

/// See the module docs.
#[derive(Default)]
pub struct BuildingKills {
    /// Sorted by frame.
    kills: Vec<BuildingKill>,
    /// The building behind each kill made in the step under way, by unique id.
    pending: Vec<u32>,
    /// The newest frame whose kills are recorded.
    through: Option<u32>,
}

#[derive(Copy, Clone, Debug)]
pub struct BuildingKill {
    frame: u32,
    /// The building's unique id.
    building: u32,
}

impl BuildingKills {
    /// Notes a kill by the building with unique id `building` in the step under way.
    pub fn note(&mut self, building: u32) {
        self.pending.push(building);
    }

    /// Records the kills of the step that just ended on `frame`.
    pub fn finish_step(&mut self, frame: u32, resimulation: Resimulation) {
        if self.through.is_some_and(|through| frame <= through) {
            match resimulation {
                Resimulation::Identical => {
                    self.pending.clear();
                    return;
                }
                Resimulation::MayDiffer => self.forget_from(frame),
            }
        }
        self.kills.extend(
            self.pending
                .drain(..)
                .map(|building| BuildingKill { frame, building }),
        );
        self.through = Some(frame);
    }

    /// Forgets every kill from `frame` on, for frames about to be simulated again differently.
    fn forget_from(&mut self, frame: u32) {
        self.kills
            .truncate(self.kills.partition_point(|kill| kill.frame < frame));
    }

    /// The kills made up to `frame`, which is the frame on screen: a replay that seeked back keeps
    /// what it recorded past it, and none of that has happened yet as far as the watcher is
    /// concerned.
    pub fn up_to(&self, frame: u32) -> &[BuildingKill] {
        &self.kills[..self.kills.partition_point(|kill| kill.frame <= frame)]
    }
}

/// How many of `kills` the building with unique id `building` made.
pub fn count(kills: &[BuildingKill], building: u32) -> u32 {
    kills
        .iter()
        .filter(|kill| kill.building == building)
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANNON: u32 = 0x2001;
    const SUNKEN: u32 = 0x4002;

    #[test]
    fn a_steps_kills_are_counted_from_the_frame_it_ended_on() {
        let mut kills = BuildingKills::default();
        kills.note(CANNON);
        kills.note(SUNKEN);
        kills.note(CANNON);
        kills.finish_step(10, Resimulation::Identical);

        assert_eq!(count(kills.up_to(9), CANNON), 0);
        assert_eq!(count(kills.up_to(10), CANNON), 2);
        assert_eq!(count(kills.up_to(10), SUNKEN), 1);
    }

    #[test]
    fn a_replay_stepping_frames_again_leaves_their_kills_as_recorded() {
        let mut kills = BuildingKills::default();
        kills.note(CANNON);
        kills.finish_step(10, Resimulation::Identical);
        kills.finish_step(11, Resimulation::Identical);
        // A seek back to before frame 10, which steps it again with the same kill.
        kills.note(CANNON);
        kills.finish_step(10, Resimulation::Identical);
        kills.finish_step(11, Resimulation::Identical);
        kills.finish_step(12, Resimulation::Identical);

        assert_eq!(count(kills.up_to(12), CANNON), 1);
    }

    #[test]
    fn a_rollback_forgets_the_kills_of_the_frames_it_steps_again() {
        let mut kills = BuildingKills::default();
        kills.note(CANNON);
        kills.finish_step(10, Resimulation::Identical);
        kills.note(CANNON);
        kills.finish_step(12, Resimulation::Identical);
        // The rollback steps frame 11 again, and the second kill now comes a frame later.
        kills.finish_step(11, Resimulation::MayDiffer);
        assert_eq!(count(kills.up_to(12), CANNON), 1);
        kills.finish_step(12, Resimulation::MayDiffer);
        kills.note(CANNON);
        kills.finish_step(13, Resimulation::MayDiffer);

        assert_eq!(count(kills.up_to(12), CANNON), 1);
        assert_eq!(count(kills.up_to(13), CANNON), 2);
    }

    #[test]
    fn kills_past_the_frame_on_screen_have_not_happened_yet() {
        let mut kills = BuildingKills::default();
        kills.note(CANNON);
        kills.finish_step(10, Resimulation::Identical);
        kills.note(CANNON);
        kills.finish_step(20, Resimulation::Identical);

        assert_eq!(count(kills.up_to(15), CANNON), 1);
        assert_eq!(count(kills.up_to(20), CANNON), 2);
    }
}
