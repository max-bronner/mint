use mint_movement::Vec2;
use mint_sim::{Component, Frame, PlayerId, StateHasher};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MiscSound {
    RallyPointSet,
    UnableToSetRallyPoint,
    BuildingPlacement,
    WallPlacement,
    DefectorTimerTick,
}

impl MiscSound {
    pub(crate) const fn field(self) -> &'static str {
        match self {
            Self::RallyPointSet => "RallyPointSet",
            Self::UnableToSetRallyPoint => "UnableToSetRallyPoint",
            Self::BuildingPlacement => "BuildingPlacementSound",
            Self::WallPlacement => "WallPlacementSound",
            Self::DefectorTimerTick => "DefectorTimerTickSound",
        }
    }

    const fn code(self) -> u8 {
        match self {
            Self::RallyPointSet => 0,
            Self::UnableToSetRallyPoint => 1,
            Self::BuildingPlacement => 2,
            Self::WallPlacement => 3,
            Self::DefectorTimerTick => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MiscSoundEvent {
    pub(crate) sound: MiscSound,
    pub(crate) player: PlayerId,
    pub(crate) at: Vec2,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct MiscSounds {
    frame: Frame,
    events: Vec<MiscSoundEvent>,
}

impl MiscSounds {
    fn push(&mut self, now: Frame, event: MiscSoundEvent) {
        if self.frame != now {
            self.events.clear();
            self.frame = now;
        }
        self.events.push(event);
    }

    fn of(&self, now: Frame) -> &[MiscSoundEvent] {
        if self.frame == now {
            &self.events
        } else {
            &[]
        }
    }
}

impl Component for MiscSounds {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        hasher.write_usize(self.events.len());
        for event in &self.events {
            hasher.write_u8(event.sound.code());
            hasher.write_u8(event.player.get());
            hasher.write_f32(event.at.x);
            hasher.write_f32(event.at.y);
        }
    }
}

impl RotwkRules {
    pub(super) fn push_misc_sound(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        sound: MiscSound,
        player: PlayerId,
        at: Vec2,
    ) {
        let now = tick.frame();
        let Some(store) = tick.state_mut().store_mut(ids.misc_sounds) else {
            return;
        };
        if let Some((_, sounds)) = store.iter_mut().next() {
            sounds.push(now, MiscSoundEvent { sound, player, at });
        }
    }

    pub(crate) fn misc_sounds<'w>(&self, world: &'w World, frame: Frame) -> &'w [MiscSoundEvent] {
        let Some(ids) = self.ids else {
            return &[];
        };
        world
            .store(ids.misc_sounds)
            .and_then(|store| store.iter().next())
            .map_or(&[], |(_, sounds)| sounds.of(frame))
    }
}
