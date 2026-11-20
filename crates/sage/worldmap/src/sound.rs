use std::collections::BTreeMap;

use mint_sage_ini::living_world::presentation::{
    ArmyIconDef, MapInfoDef, SoundFlag, WorldSoundDef,
};
use mint_sage_ini::living_world::Coord3;
use mint_sage_livingworld::{ArmyId, PlayerId};

use crate::layout::ArmyMark;

#[derive(Debug, Clone, PartialEq)]
pub struct MapSound {
    pub event: String,
    pub position: Option<[f32; 3]>,
}

impl MapSound {
    pub(crate) fn global(event: &str) -> Self {
        Self {
            event: event.to_owned(),
            position: None,
        }
    }
}

fn inside(point: [f32; 2], low: Option<(f32, f32)>, high: Option<(f32, f32)>) -> bool {
    match (low, high) {
        (Some(low), Some(high)) => {
            point[0] >= low.0 && point[0] <= high.0 && point[1] >= low.1 && point[1] <= high.1
        }
        _ => true,
    }
}

pub(crate) fn opening(
    info: &MapInfoDef,
    sounds: &[WorldSoundDef],
    focus: [f32; 2],
) -> Vec<MapSound> {
    let mut out = Vec::new();
    if let Some(event) = &info.enter_map_sound {
        out.push(MapSound::global(event));
    }
    for def in sounds {
        let Some(event) = &def.sound else { continue };
        if def.flags.0.contains(&SoundFlag::ZoomingIn)
            && !inside(
                focus,
                def.zoom_region_low.map(|c| (c.x, c.y)),
                def.zoom_region_high.map(|c| (c.x, c.y)),
            )
        {
            continue;
        }
        let position = def.position.map(|Coord3 { x, y, z }| [x, y, z]);
        out.push(MapSound {
            event: event.clone(),
            position,
        });
    }
    out
}

pub(crate) fn conquered_regions(
    before: &[Option<PlayerId>],
    after: &[Option<PlayerId>],
) -> Vec<usize> {
    after
        .iter()
        .enumerate()
        .filter(|(index, owner)| owner.is_some() && before.get(*index) != Some(owner))
        .map(|(index, _)| index)
        .collect()
}

pub(crate) fn fresh<T: PartialEq>(before: &[T], now: &[T]) -> usize {
    now.iter().filter(|item| !before.contains(item)).count()
}

pub(crate) fn army_sounds(
    icons: &BTreeMap<String, ArmyIconDef>,
    before: &[ArmyMark],
    after: &[ArmyMark],
    moved: &[ArmyId],
) -> Vec<MapSound> {
    let mut out = Vec::new();
    for mark in after {
        let Some(icon) = icons.get(&mark.icon.to_ascii_lowercase()) else {
            continue;
        };
        let planned = mark.order.is_some()
            && before
                .iter()
                .find(|b| b.army == mark.army)
                .is_none_or(|b| b.order != mark.order);
        if planned {
            out.extend(icon.on_move_planned_sound.as_deref().map(MapSound::global));
        }
        if moved.contains(&mark.army) {
            out.extend(icon.on_move_started_sound.as_deref().map(MapSound::global));
        }
    }
    out
}

pub(crate) fn selected_sound(
    icons: &BTreeMap<String, ArmyIconDef>,
    mark: &ArmyMark,
) -> Option<MapSound> {
    icons
        .get(&mark.icon.to_ascii_lowercase())?
        .on_selected_sound
        .as_deref()
        .map(MapSound::global)
}

#[cfg(test)]
mod tests {
    use mint_sage_ini::living_world::presentation::ArmySize;
    use mint_sage_ini::living_world::Bits;
    use mint_sage_livingworld::RegionId;

    use super::*;

    fn mark(army: u32, region: u16, order: Option<u16>) -> ArmyMark {
        ArmyMark {
            army: ArmyId(army),
            player: PlayerId(0),
            region: RegionId(region),
            spot: [0.0; 2],
            size: ArmySize::Small,
            icon: "Icon".to_owned(),
            order: order.map(RegionId),
            move_speed: 1.0,
        }
    }

    fn icons() -> BTreeMap<String, ArmyIconDef> {
        let icon = ArmyIconDef {
            on_move_planned_sound: Some("planned".to_owned()),
            on_move_started_sound: Some("started".to_owned()),
            on_selected_sound: Some("selected".to_owned()),
            ..ArmyIconDef::default()
        };
        BTreeMap::from([("icon".to_owned(), icon)])
    }

    #[test]
    fn a_new_order_plans_and_a_walk_starts_each_with_the_icons_sound() {
        let before = [mark(1, 0, None), mark(2, 0, Some(3))];
        let after = [mark(1, 0, Some(2)), mark(2, 3, None)];
        let sounds = army_sounds(&icons(), &before, &after, &[ArmyId(2)]);
        let names: Vec<&str> = sounds.iter().map(|s| s.event.as_str()).collect();
        assert_eq!(names, ["planned", "started"]);
    }

    #[test]
    fn an_unchanged_order_is_not_planned_again() {
        let before = [mark(1, 0, Some(2))];
        assert!(army_sounds(&icons(), &before, &before, &[]).is_empty());
        let idle = [mark(3, 0, None)];
        assert!(army_sounds(&icons(), &[], &idle, &[]).is_empty());
        let idle = [mark(3, 0, None)];
        assert!(army_sounds(&icons(), &[], &idle, &[]).is_empty());
    }

    #[test]
    fn a_region_that_changes_hands_or_is_taken_from_neutral_is_conquered() {
        let (a, b) = (Some(PlayerId(0)), Some(PlayerId(1)));
        let before = [a, a, None, b];
        let after = [a, b, a, None];
        assert_eq!(conquered_regions(&before, &after), [1, 2]);
    }

    #[test]
    fn a_zooming_in_sound_plays_only_when_the_focus_is_in_its_region() {
        let mut zoom = WorldSoundDef {
            sound: Some("zoom".to_owned()),
            flags: Bits(vec![SoundFlag::ZoomingIn]),
            ..WorldSoundDef::default()
        };
        zoom.zoom_region_low = Some(mint_sage_ini::living_world::Coord2 { x: 0.0, y: 0.0 });
        zoom.zoom_region_high = Some(mint_sage_ini::living_world::Coord2 { x: 10.0, y: 10.0 });
        let info = MapInfoDef {
            enter_map_sound: Some("enter".to_owned()),
            ..MapInfoDef::default()
        };
        let at = |p| {
            opening(&info, std::slice::from_ref(&zoom), p)
                .into_iter()
                .map(|s| s.event)
                .collect::<Vec<_>>()
        };
        assert_eq!(at([5.0, 5.0]), ["enter", "zoom"]);
        assert_eq!(at([50.0, 5.0]), ["enter"]);
    }
}
