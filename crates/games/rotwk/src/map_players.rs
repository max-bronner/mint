use std::collections::BTreeSet;

use mint_movement::Vec2;
use mint_sage_map::{Object, Side};
use mint_sage_modules::{ObjectRecord, TemplateRecords};
use mint_sim::PlayerId;

const OBJECT_NAME_KEY: &str = "objectName";

const OBJECT_INDESTRUCTIBLE_KEY: &str = "objectIndestructible";

#[must_use]
pub(crate) fn object_script_name(object: &Object) -> Option<&str> {
    match object.properties.get(OBJECT_NAME_KEY)? {
        mint_sage_map::Value::Str(name) | mint_sage_map::Value::WideStr(name) => Some(name),
        _ => None,
    }
}

fn object_indestructible(object: &Object) -> bool {
    matches!(
        object.properties.get(OBJECT_INDESTRUCTIBLE_KEY),
        Some(mint_sage_map::Value::Bool(true))
    )
}

fn object_team(object: &Object) -> Option<&str> {
    match object.properties.get(mint_sage_map::ORIGINAL_OWNER_KEY)? {
        mint_sage_map::Value::Str(owner) | mint_sage_map::Value::WideStr(owner) => {
            owner.contains('/').then_some(owner.as_str())
        }
        _ => None,
    }
}

const SOUGHT_KINDS: [&str; 4] = ["CAPTUREFLAG", "CAPTURABLE", "LINKED_TO_FLAG", "SALT_LICK"];

const INERT_KIND: &str = "INERT";

const SHRUB_KIND: &str = "SHRUB";
const CLEARED_BY_BUILD_KIND: &str = "CLEARED_BY_BUILD";

const CREEP_SIDE_NAME: &str = "PlyrCreeps";

pub(crate) fn start_named(name: &str) -> Option<u8> {
    let prefix = name.get(..7)?;
    if !prefix.eq_ignore_ascii_case("player_") {
        return None;
    }
    let digits = name.get(7..)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let slot: u32 = digits.parse().ok()?;
    let slot = slot.checked_sub(1)?;
    u8::try_from(slot).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Relation {
    MapPlayer(usize),
    Contestant(PlayerId),
}

impl Relation {
    pub(crate) fn resolve(self, id_of: impl Fn(usize) -> Option<PlayerId>) -> Option<PlayerId> {
        match self {
            Self::MapPlayer(at) => id_of(at),
            Self::Contestant(id) => Some(id),
        }
    }
}

pub(crate) fn is_entity(record: &ObjectRecord) -> bool {
    !decorates(record)
        && (SOUGHT_KINDS.iter().any(|kind| record.has_kind(kind))
            || acts(record)
            || (!record.has_kind(INERT_KIND) && (record.max_health.is_some() || record.produces)))
}

fn decorates(record: &ObjectRecord) -> bool {
    crate::forest::is_tree(record)
        || record.has_kind(SHRUB_KIND)
        || (record.has_kind(CLEARED_BY_BUILD_KIND)
            && record.fence_width.is_none_or(|width| width <= 0.0))
}

fn acts(record: &ObjectRecord) -> bool {
    record.declared_modules.iter().any(|module| {
        mint_sage_modules::is_ai_module(&module.name)
            || module.name.eq_ignore_ascii_case("SpawnBehavior")
    })
}

const ROAD_OR_BRIDGE_POINT_FLAGS: i32 = 0x36;

pub(crate) fn places_entity(
    templates: &TemplateRecords,
    object: &Object,
    referenced: &BTreeSet<String>,
) -> bool {
    if object_script_name(object).is_some_and(|name| referenced.contains(name)) {
        return true;
    }
    if object.flags & ROAD_OR_BRIDGE_POINT_FLAGS != 0 {
        return false;
    }
    let for_a_start = object.original_player().and_then(start_named).is_some();
    templates
        .get(&object.template_name)
        .is_some_and(|record| is_entity(record) || (for_a_start && record.max_health.is_some()))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MapPlayer {
    pub(crate) name: String,
    pub(crate) allies: Vec<Relation>,
    pub(crate) enemies: Vec<Relation>,
    pub(crate) universal_hostile: bool,
    pub(crate) faction: String,
    pub(crate) human: bool,
}

impl MapPlayer {
    pub(crate) fn label(&self) -> &str {
        if self.name.is_empty() {
            "(neutral side)"
        } else {
            &self.name
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MapEntity {
    pub(crate) object: usize,
    pub(crate) template: String,
    pub(crate) owner: Relation,
    pub(crate) at: Vec2,
    pub(crate) angle: f32,
    pub(crate) z: f32,
    pub(crate) script_name: Option<String>,
    pub(crate) team: Option<String>,
    pub(crate) indestructible: bool,
    pub(crate) keys: crate::map_keys::ObjectKeys,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MapCast {
    pub(crate) players: Vec<MapPlayer>,
    pub(crate) entities: Vec<MapEntity>,
    pub(crate) lines: Vec<String>,
    pub(crate) contestants: Vec<ContestantSide>,
    pub(crate) named: Vec<(String, PlayerId)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContestantSide {
    pub(crate) player: PlayerId,
    pub(crate) allies: Vec<Relation>,
    pub(crate) enemies: Vec<Relation>,
    pub(crate) faction_icon: Option<String>,
}

impl MapCast {
    pub(crate) fn rebuild_creep_relations(&mut self, contestants: &[PlayerId]) {
        for player in self.players.iter_mut().filter(|p| p.universal_hostile) {
            player.allies.clear();
            player.enemies = contestants
                .iter()
                .copied()
                .map(Relation::Contestant)
                .collect();
        }
    }
}

fn stands_at(templates: &TemplateRecords, object: &Object) -> Vec2 {
    let Some((x, y)) = templates
        .get(&object.template_name)
        .and_then(|record| record.rotation_anchor)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
    else {
        return Vec2::new(object.x, object.y);
    };
    let (sin, cos) = mint_math::f32::sin_cos(object.angle);
    Vec2::new(object.x + x * cos - y * sin, object.y + x * sin + y * cos)
}

pub(crate) fn cast(
    templates: &TemplateRecords,
    sides: &[Side],
    objects: &[Object],
    starts: &[(u8, PlayerId)],
    named: &[(String, PlayerId)],
    referenced: &BTreeSet<String>,
) -> MapCast {
    let contestant = |name: &str| {
        if let Some((_, player)) = named
            .iter()
            .find(|(side, _)| side.eq_ignore_ascii_case(name))
        {
            return Some(*player);
        }
        let start = start_named(name)?;
        starts
            .iter()
            .find(|(held, _)| *held == start)
            .map(|(_, player)| *player)
    };
    let side_of = |name: &str| {
        sides
            .iter()
            .position(|side| side.name().is_some_and(|n| n.eq_ignore_ascii_case(name)))
    };
    let neutral = side_of("");
    let mut lines = Vec::new();
    let mut owned: Vec<(usize, Option<usize>)> = Vec::new();
    let mut held: Vec<(usize, PlayerId)> = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        if !places_entity(templates, object, referenced) {
            continue;
        }
        let named = object.original_player().unwrap_or_default();
        if let Some(player) = contestant(named) {
            held.push((index, player));
            continue;
        }
        let side = side_of(named).or_else(|| {
            lines.push(format!(
                "map players: {} (object {index}) names owner {named:?}, which no side declares; \
                 given to the neutral side",
                object.template_name
            ));
            neutral
        });
        owned.push((index, side));
    }

    let mut order: Vec<Option<usize>> = owned.iter().map(|(_, side)| *side).collect();
    if let Some(at) = side_of(CREEP_SIDE_NAME) {
        order.push(Some(at));
    }
    if !named.is_empty() {
        order.extend(
            sides
                .iter()
                .enumerate()
                .filter(|(_, side)| {
                    let name = side.name().unwrap_or_default();
                    start_named(name).is_none() && contestant(name).is_none()
                })
                .map(|(at, _)| Some(at)),
        );
    }
    order.push(neutral);
    order.sort_unstable_by_key(|side| side.map_or(usize::MAX, |at| at));
    order.dedup();
    let player_of = |side: Option<usize>| order.iter().position(|held| *held == side);
    let resolve = |list: &str, side: &Side| -> Vec<Relation> {
        side.names_under(list)
            .filter_map(|name| {
                if start_named(name).is_some()
                    || named.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
                {
                    return contestant(name).map(Relation::Contestant);
                }
                side_of(name)
                    .and_then(|at| player_of(Some(at)))
                    .map(Relation::MapPlayer)
            })
            .collect()
    };
    let players = order
        .iter()
        .map(|side| match side.and_then(|at| sides.get(at)) {
            Some(side) => {
                let name = side.name().unwrap_or_default();
                MapPlayer {
                    name: name.to_owned(),
                    allies: resolve("playerAllies", side),
                    enemies: resolve("playerEnemies", side),
                    universal_hostile: name.eq_ignore_ascii_case(CREEP_SIDE_NAME),
                    faction: side.text("playerFaction").unwrap_or_default().to_owned(),
                    human: matches!(
                        side.properties.get("playerIsHuman"),
                        Some(mint_sage_map::Value::Bool(true))
                    ),
                }
            }
            None => MapPlayer::default(),
        })
        .collect();
    let mut placed: Vec<(usize, Relation)> = owned
        .iter()
        .filter_map(|(index, side)| Some((*index, Relation::MapPlayer(player_of(*side)?))))
        .chain(
            held.iter()
                .map(|(index, player)| (*index, Relation::Contestant(*player))),
        )
        .collect();
    placed.sort_unstable_by_key(|(index, _)| *index);
    let entities = placed
        .into_iter()
        .filter_map(|(index, owner)| {
            let object = objects.get(index)?;
            Some(MapEntity {
                object: index,
                template: object.template_name.clone(),
                owner,
                at: stands_at(templates, object),
                angle: object.angle,
                z: if object.z.is_finite() { object.z } else { 0.0 },
                script_name: object_script_name(object).map(str::to_owned),
                team: object_team(object).map(str::to_owned),
                indestructible: object_indestructible(object),
                keys: crate::map_keys::ObjectKeys::from_object(object),
            })
        })
        .collect();
    let contestants = named
        .iter()
        .filter_map(|(name, player)| {
            let side = sides.get(side_of(name)?)?;
            Some(ContestantSide {
                player: *player,
                allies: resolve("playerAllies", side),
                enemies: resolve("playerEnemies", side),
                faction_icon: side
                    .text("playerFactionIcon")
                    .filter(|icon| !icon.is_empty())
                    .map(str::to_owned),
            })
        })
        .collect();
    MapCast {
        players,
        entities,
        lines,
        contestants,
        named: named.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_map::{Dictionary, Object as MapObject, Value};

    const FLAG_INI: &str = "
Object ZFlag
  KindOf = CAPTURABLE CAPTUREFLAG IMMOBILE
End
";

    fn side(name: &str, allies: &str, enemies: &str) -> Side {
        Side {
            properties: Dictionary::from_pairs(vec![
                ("playerName".into(), Value::Str(name.into())),
                ("playerAllies".into(), Value::Str(allies.into())),
                ("playerEnemies".into(), Value::Str(enemies.into())),
            ]),
            build_list: Vec::new(),
        }
    }

    fn object(template: &str, owner: &str) -> MapObject {
        MapObject {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            angle: 0.0,
            flags: 0,
            template_name: template.into(),
            properties: Dictionary::from_pairs(vec![(
                mint_sage_map::ORIGINAL_OWNER_KEY.into(),
                Value::Str(owner.into()),
            )]),
        }
    }

    #[test]
    fn a_campaign_side_that_owns_nothing_is_still_a_player() {
        let data = crate::game::tests::data_from(FLAG_INI);
        let sides = [
            side("", "", ""),
            side("PlyrHero", "PlyrAlly", ""),
            side("PlyrAlly", "PlyrHero", ""),
            side("PlyrFoe", "", ""),
        ];
        let objects = [object("ZFlag", "PlyrFoe/team")];
        let names = |named: &[(String, PlayerId)]| -> Vec<String> {
            cast(
                &data.templates,
                &sides,
                &objects,
                &[],
                named,
                &BTreeSet::new(),
            )
            .players
            .into_iter()
            .map(|p| p.name)
            .collect()
        };
        assert_eq!(
            names(&[("PlyrHero".to_owned(), PlayerId::new(0))]),
            ["", "PlyrAlly", "PlyrFoe"]
        );
        assert_eq!(names(&[]), ["", "PlyrFoe"]);
    }

    #[test]
    fn player_n_resolves_to_the_contestant_and_the_creep_side_is_always_hostile() {
        let data = crate::game::tests::data_from(FLAG_INI);
        let sides = [
            side("", "", ""),
            side("Gaia", "", "Player_3 Player_4 Player_5"),
            side("PlyrCreeps", "", ""),
        ];
        let objects = [object("ZFlag", "Gaia/team")];
        let starts = [(2, PlayerId::new(1)), (3, PlayerId::new(0))];
        let cast = cast(
            &data.templates,
            &sides,
            &objects,
            &starts,
            &[],
            &BTreeSet::new(),
        );
        let gaia = cast
            .players
            .iter()
            .find(|p| p.name == "Gaia")
            .expect("Gaia owns the flag, so it is a map player");
        assert_eq!(
            gaia.enemies,
            vec![
                Relation::Contestant(PlayerId::new(1)),
                Relation::Contestant(PlayerId::new(0)),
            ]
        );
        assert!(!gaia.universal_hostile);
        let creeps = cast
            .players
            .iter()
            .find(|p| p.name == "PlyrCreeps")
            .expect("the reserved creep side gets a slot despite owning nothing");
        assert!(creeps.universal_hostile);
        assert!(
            creeps.enemies.is_empty(),
            "nothing declared - the hostility comes from the reserved name, not a list"
        );
    }

    #[test]
    fn rebuilt_creep_relations_are_the_occupied_slots_and_no_allies() {
        let data = crate::game::tests::data_from(FLAG_INI);
        let sides = [
            side("", "", ""),
            side("Gaia", "", ""),
            side("PlyrCreeps", "Gaia", "Gaia"),
        ];
        let objects = [object("ZFlag", "Gaia/team")];
        let mut cast = cast(
            &data.templates,
            &sides,
            &objects,
            &[],
            &[],
            &BTreeSet::new(),
        );
        let creeps_of = |cast: &MapCast| {
            cast.players
                .iter()
                .find(|p| p.universal_hostile)
                .cloned()
                .expect("creep slot")
        };
        assert!(!creeps_of(&cast).allies.is_empty());
        cast.rebuild_creep_relations(&[PlayerId::new(1), PlayerId::new(0)]);
        let creeps = creeps_of(&cast);
        assert!(creeps.allies.is_empty());
        assert_eq!(
            creeps.enemies,
            vec![
                Relation::Contestant(PlayerId::new(1)),
                Relation::Contestant(PlayerId::new(0))
            ]
        );
    }

    #[test]
    fn every_match_has_a_neutral_player_declared_or_not() {
        let data = crate::game::tests::data_from(FLAG_INI);
        let starts = [(0, PlayerId::new(0))];
        for sides in [
            vec![side("", "", ""), side("Gaia", "", "")],
            vec![side("Gaia", "", "")],
        ] {
            let cast = cast(&data.templates, &sides, &[], &starts, &[], &BTreeSet::new());
            assert_eq!(
                cast.players.iter().filter(|p| p.name.is_empty()).count(),
                1,
                "{} side(s)",
                sides.len()
            );
        }
    }

    #[test]
    fn a_placement_a_player_could_touch_is_an_entity_and_decoration_is_not() {
        let data = crate::game::tests::data_from(
            "
Object ZLair
  KindOf = STRUCTURE IMMOBILE SELECTABLE
  Body = ActiveBody ModuleTag_ZBody
    MaxHealth = 900
  End
  Behavior = SpawnBehavior ModuleTag_ZSpawn
    SpawnNumber = 3
    SpawnTemplateName = ZBeast
  End
End
Object ZBrigand
  KindOf = SELECTABLE INFANTRY
  Body = ActiveBody ModuleTag_ZBody
    MaxHealth = 90
  End
  Behavior = AIUpdateInterface ModuleTag_ZAI
  End
End
Object ZHeron
  KindOf = INERT
  Behavior = AIUpdateInterface ModuleTag_ZAI
  End
End
Object ZRuin
  KindOf = STRUCTURE IMMOBILE SELECTABLE
  Body = ActiveBody ModuleTag_ZBody
    MaxHealth = 500
  End
End
Object ZStable
  KindOf = STRUCTURE IMMOBILE SELECTABLE
  Body = InactiveBody ModuleTag_ZBody
  End
  Behavior = ProductionUpdate ModuleTag_ZProduce
  End
End
Object ZMarker
  KindOf = IMMOBILE INERT
  Body = ActiveBody ModuleTag_ZBody
    MaxHealth = 1
  End
End
Object ZLick
  KindOf = SALT_LICK NO_COLLIDE INERT
  Body = ImmortalBody ModuleTag_ZBody
    MaxHealth = 1
  End
End
Object ZFern
  KindOf = IMMOBILE OPTIMIZED_PROP
End
Object ZFluff
  KindOf = IMMOBILE CLEARED_BY_BUILD
  Body = ActiveBody ModuleTag_ZBody
    MaxHealth = 50
  End
End
Object ZFence
  KindOf = IMMOBILE CLEARED_BY_BUILD
  FenceWidth = 8
  Body = ActiveBody ModuleTag_ZBody
    MaxHealth = 50
  End
End
",
        );
        let sides = [
            side("", "", ""),
            side("Wildmen", "", ""),
            side("Kingdom", "", ""),
        ];
        let mut bridge_end = object("ZRuin", "Wildmen/teamWildmen");
        bridge_end.flags = 0x10;
        let objects = [
            object("ZLair", "Wildmen/teamWildmen"),
            object("ZBrigand", "Wildmen/teamWildmen"),
            object("ZHeron", "Wildmen/teamWildmen"),
            object("ZRuin", "Wildmen/teamWildmen"),
            object("ZStable", "Kingdom/teamKingdom"),
            object("ZMarker", "/team"),
            object("ZLick", "/team"),
            object("ZFern", "/team"),
            object("ZFluff", "/team"),
            object("ZFence", "/team"),
            bridge_end,
        ];
        let cast = cast(
            &data.templates,
            &sides,
            &objects,
            &[],
            &[],
            &BTreeSet::new(),
        );
        let placed: Vec<(&str, &str)> = cast
            .entities
            .iter()
            .map(|e| {
                let owner = match e.owner {
                    Relation::MapPlayer(at) => cast.players.get(at).map_or("?", MapPlayer::label),
                    Relation::Contestant(_) => "a contestant",
                };
                (e.template.as_str(), owner)
            })
            .collect();
        assert_eq!(
            placed,
            vec![
                ("ZLair", "Wildmen"),
                ("ZBrigand", "Wildmen"),
                ("ZHeron", "Wildmen"),
                ("ZRuin", "Wildmen"),
                ("ZStable", "Kingdom"),
                ("ZLick", "(neutral side)"),
                ("ZFence", "(neutral side)"),
            ]
        );
    }

    #[test]
    fn a_player_n_placement_belongs_to_the_contestant_at_that_start() {
        let data = crate::game::tests::data_from(
            "
Object ZKeep
  KindOf = STRUCTURE IMMOBILE SELECTABLE COMMANDCENTER
  Body = ActiveBody ModuleTag_ZBody
    MaxHealth = 900
  End
End
Object ZDecor
  KindOf = IMMOBILE
End
",
        );
        let sides = [side("", "", ""), side("Player_2", "", "")];
        let objects = [
            object("ZKeep", "Player_2/teamPlayer_2"),
            object("ZDecor", "Player_2/teamPlayer_2"),
        ];
        let held = cast(
            &data.templates,
            &sides,
            &objects,
            &[(0, PlayerId::new(1)), (1, PlayerId::new(0))],
            &[],
            &BTreeSet::new(),
        );
        let owners: Vec<(&str, Relation)> = held
            .entities
            .iter()
            .map(|e| (e.template.as_str(), e.owner))
            .collect();
        assert_eq!(
            owners,
            vec![("ZKeep", Relation::Contestant(PlayerId::new(0)))]
        );
        let empty = cast(
            &data.templates,
            &sides,
            &objects,
            &[(0, PlayerId::new(0))],
            &[],
            &BTreeSet::new(),
        );
        let side_named = |e: &MapEntity| match e.owner {
            Relation::MapPlayer(at) => empty.players.get(at).map(|p| p.name.clone()),
            Relation::Contestant(_) => None,
        };
        assert_eq!(
            empty.entities.iter().map(side_named).collect::<Vec<_>>(),
            vec![Some(String::from("Player_2"))]
        );
    }

    #[test]
    fn player_slot_parses_any_n_case_insensitively() {
        assert_eq!(start_named("Player_1"), Some(0));
        assert_eq!(start_named("player_12"), Some(11));
        assert_eq!(start_named("Player_0"), None, "1-based");
        assert_eq!(start_named("Player_"), None);
        assert_eq!(start_named("PlayerX_1"), None);
        assert_eq!(start_named("Player_257"), None, "slot does not fit a u8");
    }

    #[test]
    #[ignore = "needs a retail installation; set MINT_CORPUS"]
    fn every_map_mp_player_n_entry_resolves_to_its_slot() {
        let Some(root) = std::env::var_os("MINT_CORPUS").map(std::path::PathBuf::from) else {
            println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
            return;
        };
        let mut archives = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("big"))
                {
                    archives.push(path);
                }
            }
        }
        archives.sort();
        assert!(
            !archives.is_empty(),
            "no .big archives under {}",
            root.display()
        );

        let (mut maps, mut entries) = (0usize, 0usize);
        for archive_path in &archives {
            let Ok(archive) = mint_sage_big::BigArchive::open(archive_path) else {
                continue;
            };
            for map_entry in archive.entries() {
                let lower = map_entry.name.to_ascii_lowercase();
                if !lower.ends_with(".map") || !lower.contains("map mp") {
                    continue;
                }
                let Ok(raw) = archive.read_entry(map_entry) else {
                    continue;
                };
                let Ok(map) = mint_sage_map::MapFile::parse(&raw) else {
                    continue;
                };
                let Ok(sides) = map.sides() else {
                    continue;
                };
                maps += 1;
                for side in &sides {
                    for list in ["playerAllies", "playerEnemies"] {
                        for name in side.names_under(list) {
                            let Some(slot) = start_named(name) else {
                                continue;
                            };
                            let want = name
                                .get(7..)
                                .and_then(|digits| digits.parse::<u32>().ok())
                                .and_then(|n| n.checked_sub(1))
                                .and_then(|n| u8::try_from(n).ok());
                            assert_eq!(Some(slot), want, "{name:?} in {}", map_entry.name);
                            entries += 1;
                        }
                    }
                }
            }
        }
        println!("{maps} map mp map(s), {entries} Player_N entr(ies) resolved");
        assert!(maps > 0, "no map mp maps found under {}", root.display());
    }
}
