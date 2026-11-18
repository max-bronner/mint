use std::sync::{Arc, OnceLock};

use mint_movement::{Facing, Vec2};
use mint_sage_economy::Alignment;
use mint_sage_ini::PlayerTemplateDef;
use mint_sage_map::{BaseLayout, Object};
use mint_sage_passability::{Obstacle, Passability};
use mint_sim::{FrameHash, Pcg32, PlayerId, Simulation};

use crate::combat::{AiBaseDef, GameData};
use crate::horde::HordeSpawn;
use crate::rules::RotwkRules;
use crate::templates::{rotate, KindTable};
use crate::unit::{Kind, Spawn};

const MAIN_BUILDING_KIND: &str = "COMMANDCENTER";

pub(crate) const DEFAULT_FACTION: &str = "FactionMen";
pub(crate) const DEFAULT_OPPONENT: &str = "FactionMordor";

pub(crate) const DEFAULT_MAP: &str = "map mp fords of isen ii";

pub(crate) const SKIRMISH_SEED: u64 = 0x005E_EDC8;

const ECONOMY_TASK_PRIORITY: i32 = 45;

const ECONOMY_TASK_STEP: f32 = 250.0;

const ECONOMY_TASK_MARGIN: f32 = 100.0;

const DEFAULT_PORTER_TARGET: i64 = 1;

#[derive(Debug, Clone)]
pub(crate) struct PlayerStart {
    pub(crate) player: PlayerId,
    pub(crate) faction: PlayerTemplateDef,
    pub(crate) at: Vec2,
    pub(crate) castle: Option<(String, BaseLayout)>,
    pub(crate) ai_base_candidates: Vec<(String, BaseLayout)>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Inputs {
    pub(crate) players: Vec<PlayerStart>,
    pub(crate) first_obstacle_id: u32,
    pub(crate) route_markers: Vec<crate::skirmish_ai::RouteMarker>,
    pub(crate) lines: Vec<String>,
    pub(crate) teams: Vec<Option<u8>>,
    pub(crate) handicaps: Vec<crate::handicap::Handicap>,
    pub(crate) heroes: Vec<(PlayerId, Arc<mint_sage_cah::HeroRecord>)>,
    pub(crate) starting_money: Option<i64>,
    pub(crate) command_point_factor: Option<u32>,
    pub(crate) allow_ring_heroes: bool,
    pub(crate) test_seeds: Vec<String>,
    pub(crate) map_cast: crate::map_players::MapCast,
    pub(crate) trees: Vec<crate::forest::TreeSeed>,
    pub(crate) ai_difficulty: Vec<(PlayerId, crate::lobby::Difficulty)>,
    pub(crate) difficulty: crate::rules::difficulty::DifficultySetup,
    pub(crate) map_scripts: Option<crate::rules::map_scripts::MapScriptEngine>,
    pub(crate) starts: Vec<(u8, PlayerId)>,
    pub(crate) castle_layouts: crate::castle::CastleLayouts,
}

pub(crate) struct Start {
    pub(crate) simulation: Simulation<RotwkRules>,
    pub(crate) kinds: Arc<KindTable>,
    pub(crate) starts: Vec<Vec2>,
    pub(crate) lines: Vec<String>,
}

pub(crate) fn frame_hash() -> FrameHash {
    static MODE: OnceLock<FrameHash> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("MINT_ROTWK_HASH").as_deref() {
        Ok("incremental") => FrameHash::Incremental,
        Ok("verify") => FrameHash::Verified,
        Ok("legacy") => FrameHash::Legacy,
        _ => FrameHash::default_for_build(),
    })
}

pub(crate) const fn frame_hash_label(mode: FrameHash) -> &'static str {
    match mode {
        FrameHash::Incremental => "incremental",
        FrameHash::Verified => "verify",
        FrameHash::Legacy => "legacy",
    }
}

pub(crate) fn start_waypoints(objects: &[Object]) -> Vec<[f32; 2]> {
    let mut out = Vec::new();
    for slot in 0..u8::try_from(crate::lobby::MAX_SLOTS).unwrap_or(8) {
        let name = mint_sage_map::start_waypoint_name(slot);
        match mint_sage_map::named_waypoint(objects, &name) {
            Some(at) => out.push([at.x, at.y]),
            None => break,
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve(
    data: &GameData,
    objects: &[Object],
    read: impl Fn(&str) -> Option<Vec<u8>>,
    players: &[crate::lobby::ResolvedPlayer],
    map_label: &str,
    map_scripts: Option<crate::rules::map_scripts::MapScriptEngine>,
    script_lines: Vec<String>,
    allow_ring_heroes: bool,
) -> Result<Inputs, String> {
    let route_markers = crate::skirmish_ai::route_markers(objects);
    let mut inputs = Inputs {
        first_obstacle_id: u32::try_from(objects.len()).unwrap_or(u32::MAX),
        route_markers,
        allow_ring_heroes,
        trees: crate::forest::tree_seeds(&data.templates, objects),
        ..Inputs::default()
    };
    inputs.map_scripts = map_scripts;
    inputs.lines.extend(script_lines);
    inputs.castle_layouts = castle_layouts(data, &read, &mut inputs.lines);
    if !inputs.route_markers.is_empty() {
        let mut counts: Vec<String> = Vec::new();
        for category in crate::skirmish_ai::ROUTE_CATEGORIES {
            let count = inputs
                .route_markers
                .iter()
                .filter(|marker| marker.category == category)
                .count();
            if count > 0 {
                counts.push(format!("{category:?} {count}"));
            }
        }
        inputs.lines.push(format!(
            "skirmish-ai: this map carries {} route marker(s) ({})",
            inputs.route_markers.len(),
            counts.join(", ")
        ));
    }
    for resolved in players {
        let name = &resolved.faction;
        let slot = resolved.start;
        let Some(faction) = data.players.get(name) else {
            let known: Vec<&str> = data
                .players
                .iter()
                .filter(|template| template.playable == Some(true))
                .map(|template| template.name.as_str())
                .collect();
            return Err(format!(
                "no PlayerTemplate named {name}; playable ones are {}",
                known.join(", ")
            ));
        };
        if faction.playable != Some(true) {
            inputs.lines.push(format!(
                "skirmish: {name} is not a PlayableSide - starting it anyway"
            ));
        }
        let waypoint = mint_sage_map::start_waypoint_name(slot);
        let Some(start) = mint_sage_map::named_waypoint(objects, &waypoint) else {
            let found: Vec<&str> = objects
                .iter()
                .filter_map(Object::waypoint_name)
                .filter(|name| name.starts_with("Player_") && name.ends_with("_Start"))
                .collect();
            return Err(format!(
                "this map has no {waypoint} waypoint for player {slot} (it has: {}); \
                 pick a map with one, or pass --demo",
                if found.is_empty() {
                    String::from("none")
                } else {
                    found.join(", ")
                }
            ));
        };
        let castle = castle_for(data, faction, &inputs.castle_layouts, &mut inputs.lines);
        let player = resolved.player;
        let ai_base_candidates = if resolved.ai.is_some() {
            ai_base_candidates_for(data, faction, &read, map_label, &mut inputs.lines)
        } else {
            Vec::new()
        };
        inputs.starts.push((slot, player));
        inputs.players.push(PlayerStart {
            player,
            faction: faction.clone(),
            at: Vec2::new(start.x, start.y),
            castle,
            ai_base_candidates,
        });
    }
    Ok(inputs)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_map_scripts(
    objects: &[Object],
    sides: &[mint_sage_map::Side],
    map_tree: Option<&mint_sage_map::ScriptTree>,
    map_teams: &[mint_sage_map::TeamDef],
    library_lists: &[Vec<String>],
    trigger_areas: &[mint_sage_map::TriggerArea],
    named_cameras: &[mint_sage_map::NamedCamera],
    allow_ring_heroes: bool,
    read: &impl Fn(&str) -> Option<Vec<u8>>,
) -> (
    Option<crate::rules::map_scripts::MapScriptEngine>,
    Vec<mint_sage_map::TeamDef>,
    Vec<String>,
) {
    let mut lines = Vec::new();
    let mut library_teams = Vec::new();
    let side_names: Vec<String> = sides
        .iter()
        .map(|side| side.name().unwrap_or_default().to_owned())
        .collect();
    let mut tree = map_tree.cloned().unwrap_or_default();
    let mut joined = 0usize;
    if library_lists.iter().any(|list| !list.is_empty()) {
        let read_library = |path: &str| {
            let path = path.replace('\\', "/").to_ascii_lowercase();
            read(&path).and_then(|bytes| mint_sage_map::read_map_library(&bytes).ok())
        };
        let mut team_names: std::collections::BTreeSet<String> =
            map_teams.iter().map(|team| team.name.clone()).collect();
        for (index, list) in library_lists.iter().enumerate() {
            let Some(side) = side_names.get(index) else {
                break;
            };
            if tree.player_slots.len() <= index {
                tree.player_slots
                    .resize_with(index.saturating_add(1), Vec::new);
            }
            let Some(dest) = tree.player_slots.get_mut(index) else {
                continue;
            };
            let mut link = LibraryLink {
                side,
                read: &read_library,
                linked: std::collections::BTreeSet::new(),
                team_names: &mut team_names,
                teams: &mut library_teams,
                lines: &mut lines,
            };
            let mut linked = Vec::new();
            link.link(list, &mut linked, 0);
            joined = joined.saturating_add(link.linked.len());
            *dest = take_over(linked, dest);
        }
    }
    let trees: Vec<mint_sage_map::ScriptTree> = if map_tree.is_some() || joined > 0 {
        vec![tree]
    } else {
        Vec::new()
    };
    let tree_sides = vec![side_names];
    if trees.is_empty() {
        return (None, library_teams, lines);
    }
    let waypoints: std::collections::BTreeMap<String, (Vec2, f32)> = objects
        .iter()
        .filter_map(|object| {
            let name = object.waypoint_name()?;
            Some((
                name.to_owned(),
                (Vec2::new(object.x, object.y), object.angle),
            ))
        })
        .collect();
    let areas: std::collections::BTreeMap<String, Vec<(f32, f32)>> = trigger_areas
        .iter()
        .map(|area| (area.name.clone(), area.points.clone()))
        .collect();
    let cameras: std::collections::BTreeMap<String, mint_sage_map::NamedCamera> = named_cameras
        .iter()
        .map(|camera| (camera.name.clone(), camera.clone()))
        .collect();
    let engine = crate::rules::map_scripts::MapScriptEngine::build_with_cameras(
        &trees,
        &tree_sides,
        &waypoints,
        &areas,
        allow_ring_heroes,
        &cameras,
    );
    lines.push(format!(
        "script: {joined} librar(ies) linked into their sides, {} team(s) copied",
        library_teams.len()
    ));
    (Some(engine), library_teams, lines)
}

const MAX_LIBRARY_DEPTH: usize = 16;

struct LibraryLink<'a, R> {
    side: &'a str,
    read: &'a R,
    linked: std::collections::BTreeSet<String>,
    team_names: &'a mut std::collections::BTreeSet<String>,
    teams: &'a mut Vec<mint_sage_map::TeamDef>,
    lines: &'a mut Vec<String>,
}

impl<R: Fn(&str) -> Option<mint_sage_map::MapLibrary>> LibraryLink<'_, R> {
    fn link(&mut self, list: &[String], acc: &mut Vec<mint_sage_map::ScriptNode>, depth: usize) {
        if depth >= MAX_LIBRARY_DEPTH {
            return;
        }
        for name in list.iter().rev() {
            let key = name.to_ascii_lowercase();
            if !self.linked.insert(key) {
                continue;
            }
            let Some(library) = (self.read)(name) else {
                self.lines.push(format!(
                    "script: library {name} did not load - side {:?} does not link it",
                    self.side
                ));
                continue;
            };
            if let Some(nested) = library.library_map_lists.get(1) {
                self.link(nested, acc, depth.saturating_add(1));
            }
            if let Some(slot) = library
                .scripts
                .as_ref()
                .and_then(|tree| tree.player_slots.get(1))
            {
                *acc = take_over(std::mem::take(acc), slot);
            }
            for team in &library.teams {
                if team.owner.is_empty() || !self.team_names.insert(team.name.clone()) {
                    continue;
                }
                let mut team = team.clone();
                team.owner = self.side.to_owned();
                self.teams.push(team);
            }
            self.lines
                .push(format!("script: side {:?} links library {name}", self.side));
        }
    }
}

#[derive(Default)]
struct Taken {
    scripts: std::collections::BTreeSet<String>,
    groups: std::collections::BTreeSet<String>,
}

fn take_over(
    existing: Vec<mint_sage_map::ScriptNode>,
    incoming: &[mint_sage_map::ScriptNode],
) -> Vec<mint_sage_map::ScriptNode> {
    let mut taken = Taken::default();
    let mut merged = take_level(incoming, &mut taken);
    merged.extend(without_taken(existing, &taken));
    merged
}

fn take_level(
    nodes: &[mint_sage_map::ScriptNode],
    taken: &mut Taken,
) -> Vec<mint_sage_map::ScriptNode> {
    use mint_sage_map::ScriptNode;
    let mut kept: Vec<Option<ScriptNode>> = vec![None; nodes.len()];
    for (node, slot) in nodes.iter().zip(kept.iter_mut()).rev() {
        if let ScriptNode::Group(group) = node {
            if taken.groups.insert(group.name.clone()) {
                let children = take_level(&group.children, taken);
                *slot = Some(ScriptNode::Group(mint_sage_map::ScriptGroup {
                    name: group.name.clone(),
                    active: group.active,
                    is_subroutine: group.is_subroutine,
                    children,
                }));
            }
        }
    }
    for (node, slot) in nodes.iter().zip(kept.iter_mut()).rev() {
        if let ScriptNode::Script(script) = node {
            if taken.scripts.insert(script.name.clone()) {
                *slot = Some(node.clone());
            }
        }
    }
    kept.into_iter().flatten().collect()
}

fn without_taken(
    nodes: Vec<mint_sage_map::ScriptNode>,
    taken: &Taken,
) -> Vec<mint_sage_map::ScriptNode> {
    use mint_sage_map::ScriptNode;
    nodes
        .into_iter()
        .filter_map(|node| match node {
            ScriptNode::Script(script) => {
                (!taken.scripts.contains(&script.name)).then_some(ScriptNode::Script(script))
            }
            ScriptNode::Group(mut group) => {
                if taken.groups.contains(&group.name) {
                    return None;
                }
                group.children = without_taken(std::mem::take(&mut group.children), taken);
                Some(ScriptNode::Group(group))
            }
        })
        .collect()
}

fn ai_base_candidates_for(
    data: &GameData,
    faction: &PlayerTemplateDef,
    read: &impl Fn(&str) -> Option<Vec<u8>>,
    map_label: &str,
    lines: &mut Vec<String>,
) -> Vec<(String, BaseLayout)> {
    let side = faction.side.as_deref().unwrap_or_default();
    let map_leaf = map_label
        .rsplit('/')
        .next()
        .unwrap_or(map_label)
        .to_ascii_lowercase();
    let for_side = |base: &&AiBaseDef| base.side.eq_ignore_ascii_case(side);
    let matching: Vec<&AiBaseDef> = data
        .ai_bases
        .iter()
        .filter(for_side)
        .filter(|base| base.game_map_to_use_on.to_ascii_lowercase() == map_leaf)
        .collect();
    let any: Vec<&AiBaseDef> = data
        .ai_bases
        .iter()
        .filter(for_side)
        .filter(|base| base.game_map_to_use_on.eq_ignore_ascii_case("<ANY>"))
        .collect();
    let pool = if matching.is_empty() { any } else { matching };

    let mut candidates = Vec::new();
    for base in pool {
        let path = mint_sage_map::base_path(&base.map);
        let Some(bytes) = read(&path) else {
            lines.push(format!(
                "skirmish-ai: AIBase \"{}\" ({path}) not in any mounted archive - skipped",
                base.map
            ));
            continue;
        };
        let parsed = mint_sage_map::MapFile::parse(&bytes)
            .map_err(|e| e.to_string())
            .and_then(|map| {
                BaseLayout::from_objects(&map.objects)
                    .ok_or_else(|| String::from("no objectIsABase anchor"))
            });
        match parsed {
            Ok(layout) => candidates.push((base.map.clone(), layout)),
            Err(why) => lines.push(format!(
                "skirmish-ai: AIBase \"{}\" ({path}) did not load - {why} - skipped",
                base.map
            )),
        }
    }
    candidates
}

fn castle_layouts(
    data: &GameData,
    read: &impl Fn(&str) -> Option<Vec<u8>>,
    lines: &mut Vec<String>,
) -> crate::castle::CastleLayouts {
    let names: std::collections::BTreeSet<&str> = data
        .templates
        .iter()
        .filter_map(|record| record.castle.as_ref())
        .flat_map(|castle| castle.unpack.iter().map(|(_, name)| name.as_str()))
        .collect();
    let mut layouts = crate::castle::CastleLayouts::new();
    for name in names {
        let path = mint_sage_map::base_path(name);
        let parsed = read(&path)
            .ok_or_else(|| String::from("not in any mounted archive"))
            .and_then(|bytes| mint_sage_map::MapFile::parse(&bytes).map_err(|e| e.to_string()))
            .and_then(|map| {
                BaseLayout::from_objects(&map.objects)
                    .ok_or_else(|| String::from("no objectIsABase anchor"))
            });
        match parsed {
            Ok(layout) => {
                layouts.insert(name.to_ascii_lowercase(), layout);
            }
            Err(why) => lines.push(format!(
                "castle: layout {name} ({path}) did not load - {why}; a castle naming it unpacks nothing"
            )),
        }
    }
    layouts
}

fn castle_for(
    data: &GameData,
    faction: &PlayerTemplateDef,
    layouts: &crate::castle::CastleLayouts,
    lines: &mut Vec<String>,
) -> Option<(String, BaseLayout)> {
    let building = faction.starting_building.as_deref()?;
    let castle = data.templates.get(building)?.castle.as_ref()?;
    let side = faction.side.as_deref().unwrap_or_default();
    if castle.for_side(side).is_none() {
        lines.push(format!(
            "skirmish: {building} declares no CastleToUnpackForFaction for side {side} - nothing unpacked"
        ));
        return None;
    }
    let found = crate::castle::layout_for(layouts, castle, side)
        .map(|(name, layout)| (name.to_owned(), layout.clone()));
    if found.is_none() {
        lines.push(format!(
            "skirmish: {building}'s castle did not load; the site stands alone and nothing produces"
        ));
    }
    found
}

fn passenger_perches(
    data: &GameData,
    kinds: &KindTable,
    perches: &dyn Fn(&str, &str) -> Vec<crate::perches::Perch>,
) -> Vec<Vec<crate::perches::Perch>> {
    kinds
        .iter()
        .map(|entry| {
            entry
                .record
                .declared_modules
                .iter()
                .filter(|module| module.field("PassengerBonePrefix").is_some())
                .find_map(|module| {
                    mint_sage_modules::contain::open_contain_from_fields(
                        &module.source(&data.upgrade_bits),
                    )
                    .ok()?
                    .passenger_bone_prefix()
                    .map(str::to_owned)
                })
                .map(|prefix| perches(&entry.record.name, &prefix))
                .unwrap_or_default()
        })
        .collect()
}

fn siege_docks(
    kinds: &KindTable,
    perches: &dyn Fn(&str, &str) -> Vec<crate::perches::Perch>,
) -> Vec<Vec<crate::rules::siege::DockBone>> {
    kinds
        .iter()
        .map(|entry| {
            if !entry.record.siege.docking {
                return Vec::new();
            }
            [("SIEGETOWER", false), ("SIEGELADDER", true)]
                .into_iter()
                .flat_map(|(prefix, ladder)| {
                    perches(&entry.record.name, prefix)
                        .into_iter()
                        .take(crate::rules::siege::MAX_SLOTS)
                        .map(move |perch| crate::rules::siege::DockBone {
                            ladder,
                            at: mint_movement::Vec2::new(perch.position[0], perch.position[1]),
                            axis: perch.facing,
                        })
                })
                .collect()
        })
        .collect()
}

fn siege_ladder_exits(
    kinds: &KindTable,
    perches: &dyn Fn(&str, &str) -> Vec<crate::perches::Perch>,
) -> Vec<Option<crate::perches::Perch>> {
    kinds
        .iter()
        .map(|entry| {
            let (kind_of, _) = mint_sage_asset::KindOf::parse(&entry.record.kind_of.join(" "));
            if !kind_of.contains(mint_sage_asset::KindOf::SIEGE_TOWER) {
                return None;
            }
            perches(&entry.record.name, "Ladder").into_iter().nth(3)
        })
        .collect()
}

fn siege_above_wall(
    data: &GameData,
    kinds: &KindTable,
    perches: &dyn Fn(&str, &str) -> Vec<crate::perches::Perch>,
) -> Vec<Option<mint_movement::Vec2>> {
    kinds
        .iter()
        .map(|entry| {
            entry.record.siege.deploy.as_ref()?;
            let portal = entry
                .record
                .declared_modules
                .iter()
                .filter(|module| module.name.eq_ignore_ascii_case("DynamicPortalBehaviour"))
                .find_map(|module| {
                    mint_sage_modules::dynamic_portal_behaviour_from_fields(
                        &module.source(&data.upgrade_bits),
                        mint_sim::Frame::ZERO,
                    )
                    .ok()
                })?;
            let waypoint = portal
                .waypoints()
                .get(usize::try_from(portal.above_wall()?).ok()?)?
                .index;
            let bones = perches(&entry.record.name, portal.bone_prefix()?);
            let perch = bones.get(usize::try_from(waypoint).ok()?)?;
            Some(mint_movement::Vec2::new(
                perch.position[0],
                perch.position[1],
            ))
        })
        .collect()
}

fn spawn_points(
    data: &GameData,
    kinds: &KindTable,
    perches: &dyn Fn(&str, &str) -> Vec<crate::perches::Perch>,
) -> Vec<Vec<crate::perches::Perch>> {
    kinds
        .iter()
        .map(|entry| {
            entry
                .record
                .declared_modules
                .iter()
                .filter(|module| {
                    module
                        .name
                        .eq_ignore_ascii_case("SpawnPointProductionExitUpdate")
                })
                .find_map(|module| {
                    mint_sage_modules::spawn_point_production_exit_update_from_fields(
                        &module.source(&data.upgrade_bits),
                        mint_sim::Frame::ZERO,
                    )
                    .ok()
                })
                .filter(|exit| !exit.bone_name().is_empty())
                .map(|exit| {
                    let mut bones = perches(&entry.record.name, exit.bone_name());
                    bones.truncate(mint_sage_modules::MAX_SPAWN_POINTS);
                    bones
                })
                .unwrap_or_default()
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    data: &GameData,
    object_index: Option<&mint_sage_model::ObjectIndex>,
    inputs: &Inputs,
    blend_tile: &mint_sage_map::BlendTileData,
    grid: impl Fn(
        &[Obstacle],
    ) -> Result<(Passability, Vec<mint_sage_passability::WallGeometry>), String>,
    perches: &dyn Fn(&str, &str) -> Vec<crate::perches::Perch>,
    launches: &dyn Fn(&str) -> Vec<crate::perches::SlotLaunch>,
    wall_bounds_for: &dyn Fn(&str) -> Option<crate::wall_meshes::WallBounds>,
    ramp_meshes_for: &dyn Fn(&str) -> Vec<crate::wall_meshes::RampMesh>,
    has_raised_wall_mesh_for: &dyn Fn(&str) -> bool,
    seed: u64,
    ai_players: &[PlayerId],
) -> Result<Start, String> {
    let mut lines = inputs.lines.clone();
    let mut variation_rng = Pcg32::new(seed);

    let mut seeds: Vec<String> = Vec::new();
    for start in &inputs.players {
        seeds.extend(start.faction.starting_building.iter().cloned());
        if let Some((_, layout)) = &start.castle {
            seeds.extend(
                layout
                    .entries
                    .iter()
                    .map(|entry| entry.template_name.clone()),
            );
        }
        seeds.extend(
            start
                .faction
                .starting_units
                .iter()
                .map(|unit| unit.template.clone()),
        );
        seeds.extend(
            hero_roster(start, inputs.allow_ring_heroes)
                .into_iter()
                .filter(|name| data.templates.get(name).is_some()),
        );
        seeds.extend(start.faction.spell_book_mp.iter().cloned());
    }
    seeds.extend(
        inputs
            .map_cast
            .players
            .iter()
            .filter_map(|side| data.players.get(&side.faction)?.spell_book_mp.clone()),
    );
    seeds.extend(inputs.test_seeds.iter().cloned());
    seeds.extend(
        inputs
            .map_scripts
            .as_ref()
            .map(crate::rules::map_scripts::MapScriptEngine::create_templates)
            .unwrap_or_default(),
    );
    seeds.extend(
        inputs
            .map_cast
            .entities
            .iter()
            .map(|entity| entity.template.clone()),
    );
    seeds.extend(
        crate::combat::projectile_template_names(&data.combat)
            .into_iter()
            .filter(|name| data.templates.get(name).is_some()),
    );
    seeds.extend(inputs.trees.iter().map(|seed| seed.template.clone()));
    let (mut kinds, mut missing) = KindTable::closure(data, &seeds);
    let match_sides: Vec<&str> = inputs
        .players
        .iter()
        .filter_map(|start| start.faction.side.as_deref())
        .chain(inputs.map_cast.players.iter().filter_map(|side| {
            data.players
                .get(&side.faction)
                .and_then(|template| template.side.as_deref())
        }))
        .collect();
    loop {
        let mut extra: Vec<String> = kinds
            .iter()
            .filter_map(|entry| entry.record.castle.as_ref())
            .flat_map(|castle| {
                match_sides.iter().filter_map(|side| {
                    crate::castle::layout_for(&inputs.castle_layouts, castle, side)
                })
            })
            .flat_map(|(_, layout)| layout.entries.iter())
            .map(|entry| entry.template_name.clone())
            .filter(|name| {
                kinds.kind(name).is_none()
                    && data.templates.get(name).is_some()
                    && !seeds.contains(name)
            })
            .collect();
        extra.sort();
        extra.dedup();
        if extra.is_empty() {
            break;
        }
        lines.push(format!(
            "castle: layouts reachable in play add {} template(s): {}",
            extra.len(),
            extra.join(", ")
        ));
        seeds.extend(extra);
        (kinds, missing) = KindTable::closure(data, &seeds);
    }
    for name in &missing {
        lines.push(format!(
            "skirmish: MISSING template {name} - reached from a start and not declared; not spawned"
        ));
    }
    lines.push(crate::templates::speed_census_line(&kinds));
    lines.push(crate::templates::burning_run_census_line(&kinds));
    let production = kinds.production(data);
    lines.extend(production.lines.iter().cloned());
    let hero_names: Vec<Vec<String>> = inputs
        .players
        .iter()
        .map(|start| hero_roster(start, inputs.allow_ring_heroes))
        .collect();
    let (hero_rosters, hero_buildables, hero_lines) = kinds.hero_setup(data, &hero_names);
    lines.extend(hero_lines);
    lines.push(format!(
        "heroes: {} hero page(s), {} priced hero(es)",
        hero_rosters.len(),
        hero_buildables.len()
    ));
    lines.push(format!(
        "victory: unit filter {:?} (parsed, not consulted for defeat)",
        data.victory_unit_filter
    ));
    lines.push(format!(
        "command points: {} of 8 solo/campaign field(s) read ({})",
        data.command_points_solo.fields(),
        if inputs.difficulty.multiplayer {
            "unconsumed by skirmish"
        } else {
            "consumed by this campaign mission"
        }
    ));
    let (cah_plans, cah_lines) =
        crate::create_a_hero::plan_heroes(data, &mut kinds, &inputs.heroes);
    lines.extend(cah_lines);
    let cah_can_build = data
        .create_a_hero
        .system
        .can_build_upgrade
        .as_deref()
        .and_then(|name| data.upgrade_bits.bit(name));
    let cah_revival_discount = crate::create_a_hero::revival_discount(&data.create_a_hero.system);
    let kinds = Arc::new(kinds);

    let mut spawns: Vec<Spawn> = Vec::new();
    let mut obstacles: Vec<Obstacle> = Vec::new();
    let mut structure_footprints: Vec<(usize, Vec<mint_sage_passability::FootprintRecord>)> =
        Vec::new();
    let mut facings: Vec<Facing> = Vec::new();
    let mut castle_members: Vec<(usize, usize)> = Vec::new();
    let mut ai_seeds: Vec<crate::skirmish_ai::AiSeed> = Vec::new();
    let mut ai_army_plans: Vec<crate::skirmish_ai::ArmyPlan> = Vec::new();
    let neutral = inputs
        .map_cast
        .players
        .iter()
        .position(|side| side.name.is_empty())
        .and_then(|at| u8::try_from(at.saturating_add(inputs.players.len().max(1))).ok())
        .map(PlayerId::new);
    for start in &inputs.players {
        let building = start
            .faction
            .starting_building
            .as_deref()
            .and_then(|name| kinds.kind(name));
        let facing = building.map_or(Facing::EAST, |kind| placement_facing(&kinds, kind));
        facings.push(facing);
        let mut already_built: Vec<Kind> = building.into_iter().collect();
        let mut place_for =
            |kind: Kind, at: Vec2, facing: Facing, owner: PlayerId| -> Option<usize> {
                let mut spawn = kinds.spawn(&data.combat, kind, at, facing, owner)?;
                spawn.producer = kinds.producer(&production, kind, at, facing);
                let name = kinds
                    .get(kind)
                    .map_or("", |entry| entry.record.name.as_str());
                if let Some(records) = data.footprints.lookup(name) {
                    let id = inputs
                        .first_obstacle_id
                        .saturating_add(u32::try_from(obstacles.len()).unwrap_or(u32::MAX));
                    obstacles.push(Obstacle {
                        id,
                        centre: at,
                        angle: facing.radians(),
                        records: records.clone(),
                    });
                    structure_footprints.push((spawns.len(), records));
                }
                spawns.push(spawn);
                Some(spawns.len().saturating_sub(1))
            };
        let map_main = inputs.map_cast.entities.iter().find(|entity| {
            entity.owner == crate::map_players::Relation::Contestant(start.player)
                && data
                    .templates
                    .get(&entity.template)
                    .is_some_and(|record| record.has_kind(MAIN_BUILDING_KIND))
        });
        let building = match map_main {
            Some(main) => {
                lines.push(format!(
                    "skirmish: player {} holds the map's {} at its start - no StartingBuilding",
                    start.player.get(),
                    main.template
                ));
                None
            }
            None => building,
        };
        let site = match building {
            Some(kind) => place_for(kind, start.at, facing, start.player),
            None if map_main.is_some() => None,
            None => {
                lines.push(format!(
                    "skirmish: {} has no usable StartingBuilding - player {} starts without one",
                    start.faction.name,
                    start.player.get()
                ));
                None
            }
        };
        if let Some((castle_name, layout)) = start.castle.as_ref().filter(|_| map_main.is_none()) {
            let castle = building
                .and_then(|kind| kinds.get(kind))
                .and_then(|entry| entry.record.castle.clone())
                .unwrap_or_default();
            let mut dropped = 0usize;
            let plan = crate::castle::unpack_plan(&kinds, &castle, layout, start.at, facing);
            let placed = plan.len();
            for entry in plan {
                let owner = if entry.owned {
                    start.player
                } else if let Some(neutral) = neutral {
                    neutral
                } else {
                    dropped = dropped.saturating_add(1);
                    continue;
                };
                already_built.push(entry.kind);
                let member = place_for(entry.kind, entry.at, entry.facing, owner);
                if let (Some(member), Some(site)) = (member, site) {
                    castle_members.push((member, site));
                }
            }
            lines.push(format!(
                "skirmish: player {} unpacked {castle_name}: {} object(s) placed, {dropped} outside \
                 FilterValidOwnedEntries left out for want of a neutral player, {} skipped in the file",
                start.player.get(),
                placed.saturating_sub(dropped),
                layout.skipped
            ));
        }

        if ai_players.contains(&start.player) {
            let side = start.faction.side.as_deref().unwrap_or_default();
            let army_def = data.army_definitions.by_side(side);
            let farm_kind = army_def
                .and_then(|def| def.economy_template.as_deref())
                .and_then(|name| kinds.kind(name));
            let farm_target = army_def
                .and_then(|def| def.economy_builder_min_farms_owned)
                .unwrap_or(0)
                .max(0);
            match (army_def, farm_kind) {
                (Some(_), None) if farm_target > 0 => lines.push(format!(
                    "skirmish-ai: player {} wants {farm_target} economy structure(s) but \
                     AIEconomyAssigment's TemplateName ({:?}) did not resolve to a kind",
                    start.player.get(),
                    army_def.and_then(|def| def.economy_template.as_deref())
                )),
                (None, _) => lines.push(format!(
                    "skirmish-ai: player {} (Side {side:?}) has no ArmyDefinition - no \
                     production, no economy builder, no porter target",
                    start.player.get()
                )),
                _ => {}
            }

            let mut candidates = Vec::new();
            for (name, layout) in &start.ai_base_candidates {
                let mut tasks = Vec::new();
                let mut unresolved = 0usize;
                let mut already = 0usize;
                for entry in &layout.entries {
                    let Some((priority, phase)) = entry.build_order else {
                        continue;
                    };
                    let Some(kind) = kinds.kind(&entry.template_name) else {
                        unresolved = unresolved.saturating_add(1);
                        continue;
                    };
                    if already_built.contains(&kind) {
                        already = already.saturating_add(1);
                        continue;
                    }
                    tasks.push(crate::skirmish_ai::AiTask {
                        kind,
                        position: rotate(start.at, facing, entry.offset),
                        angle: facing.radians() + entry.angle,
                        priority,
                        phase,
                    });
                }

                let base_extent = tasks
                    .iter()
                    .map(|task| {
                        let dx = task.position.x - start.at.x;
                        let dy = task.position.y - start.at.y;
                        (dx * dx + dy * dy).sqrt()
                    })
                    .fold(0.0f32, f32::max);
                let farm_step = (base_extent + ECONOMY_TASK_MARGIN).max(ECONOMY_TASK_STEP);
                let anchors = crate::skirmish_ai::retry_spiral(farm_step);
                let mut farm_tasks_added = 0usize;
                if let Some(kind) = farm_kind {
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let count = farm_target.min(i64::from(u16::MAX)) as usize;
                    for index in 0..count {
                        let anchor = anchors
                            .get(index.saturating_add(1))
                            .or_else(|| anchors.last())
                            .copied()
                            .unwrap_or(Vec2::ZERO);
                        tasks.push(crate::skirmish_ai::AiTask {
                            kind,
                            position: rotate(start.at, facing, [anchor.x, anchor.y]),
                            angle: facing.radians(),
                            priority: ECONOMY_TASK_PRIORITY,
                            phase: mint_sage_map::BasePhase::One,
                        });
                        farm_tasks_added = farm_tasks_added.saturating_add(1);
                    }
                }

                lines.push(format!(
                    "skirmish-ai: candidate \"{name}\": {} task(s), {unresolved} unresolved \
                     template(s), {already} already standing, {} skipped/refused in the file, \
                     {farm_tasks_added} economy task(s) added at radius {farm_step:.0}",
                    tasks.len().saturating_sub(farm_tasks_added),
                    layout.skipped.saturating_add(layout.refused),
                ));
                candidates.push(crate::skirmish_ai::AiCandidate {
                    name: name.clone(),
                    tasks: crate::skirmish_ai::sorted_tasks(tasks),
                });
            }
            ai_seeds.push(crate::skirmish_ai::AiSeed {
                player: start.player,
                candidates,
            });

            if let Some(def) = army_def {
                let members: Vec<crate::skirmish_ai::ArmyMemberPlan> = def
                    .members
                    .iter()
                    .map(|member| {
                        let kind = member.unit.as_deref().and_then(|name| kinds.kind(name));
                        crate::skirmish_ai::ArmyMemberPlan {
                            kind,
                            phase: [
                                member.phase[0].unwrap_or(0.0),
                                member.phase[1].unwrap_or(0.0),
                                member.phase[2].unwrap_or(0.0),
                            ],
                        }
                    })
                    .collect();
                let resolved_members = members.iter().filter(|m| m.kind.is_some()).count();
                let trainable_members = members
                    .iter()
                    .filter(|m| m.kind.is_some_and(|kind| kinds.trains(kind)))
                    .count();
                let unresolved_names: Vec<&str> = def
                    .members
                    .iter()
                    .zip(&members)
                    .filter(|(_, resolved)| resolved.kind.is_none())
                    .filter_map(|(member, _)| member.unit.as_deref())
                    .collect();

                let hero_build_order: Vec<Option<Kind>> = def
                    .hero_build_order
                    .iter()
                    .map(|name| kinds.kind(name))
                    .collect();
                let resolved_heroes = hero_build_order.iter().filter(|k| k.is_some()).count();

                let porter_name = start
                    .faction
                    .starting_units
                    .first()
                    .map(|unit| unit.template.as_str());
                let porter_kind = porter_name.and_then(|name| kinds.kind(name));
                let porter_target = data
                    .porter_targets
                    .iter()
                    .find(|(key, _)| *key == side.to_ascii_lowercase())
                    .map_or(DEFAULT_PORTER_TARGET, |(_, target)| *target);

                lines.push(format!(
                    "skirmish-ai: player {} \"{}\": {resolved_members} of {} \
                     ArmyMemberDefinition(s) resolved ({trainable_members} trainable by some \
                     structure in this match), {resolved_heroes} of {} HeroBuildOrder entr(ies) \
                     resolved, porter target {porter_target} ({}), unresolved member(s): {}",
                    start.player.get(),
                    def.name,
                    members.len(),
                    def.hero_build_order.len(),
                    porter_kind.map_or_else(
                        || String::from("no porter kind resolved"),
                        |k| { format!("kind {}", k.0) }
                    ),
                    if unresolved_names.is_empty() {
                        "none".to_owned()
                    } else {
                        unresolved_names.join(", ")
                    },
                ));

                ai_army_plans.push(crate::skirmish_ai::ArmyPlan {
                    player: start.player,
                    members,
                    must_use_command_point_percentage: [
                        def.must_use_command_point_percentage[0].unwrap_or(0.0),
                        def.must_use_command_point_percentage[1].unwrap_or(0.0),
                        def.must_use_command_point_percentage[2].unwrap_or(0.0),
                    ],
                    percent_to_save: [
                        def.percent_to_save[0].unwrap_or(0.0),
                        def.percent_to_save[1].unwrap_or(0.0),
                        def.percent_to_save[2].unwrap_or(0.0),
                    ],
                    phase_duration_rush: def.phase_duration_rush.unwrap_or(0.0),
                    phase_duration_mid_game: def.phase_duration_mid_game.unwrap_or(0.0),
                    hero_build_order,
                    porter_kind,
                    porter_target,
                });
            }
        }
    }

    obstacles.extend(inputs.map_cast.entities.iter().filter_map(|entity| {
        Some(Obstacle {
            id: u32::try_from(entity.object).ok()?,
            centre: entity.at,
            angle: entity.angle,
            records: map_entity_footprint(data, entity)?,
        })
    }));
    let (placement_grid, _) = grid(&obstacles)?;

    let mut hordes: Vec<HordeSpawn> = Vec::new();
    for (start, facing) in inputs.players.iter().zip(facings.iter().copied()) {
        for unit in &start.faction.starting_units {
            let Some(kind) = kinds.kind(&unit.template) else {
                continue;
            };
            let [x, y, _] = unit.offset.unwrap_or([0.0, 0.0, 0.0]);
            let wanted = rotate(start.at, facing, [y, x]);
            let at = standable(&placement_grid, wanted);
            if at != wanted {
                lines.push(format!(
                    "skirmish: {} for player {} moved from ({:.0}, {:.0}) to ({:.0}, {:.0}) - \
                     its offset lands on a blocked cell",
                    unit.template,
                    start.player.get(),
                    wanted.x,
                    wanted.y,
                    at.x,
                    at.y
                ));
            }
            let Some(entry) = kinds.get(kind) else {
                continue;
            };
            match (entry.formation, entry.member) {
                (Some(formation), Some(member)) => {
                    let member = draw_build_variation(&kinds, &mut variation_rng, member);
                    let Some(member) = kinds.spawn(&data.combat, member, at, facing, start.player)
                    else {
                        continue;
                    };
                    hordes.push(HordeSpawn {
                        formation,
                        origin: at,
                        facing,
                        member,
                        count: data
                            .formations
                            .tables()
                            .get(usize::from(formation))
                            .map_or(0, |table| u32::try_from(table.len()).unwrap_or(0)),
                        vision: entry
                            .record
                            .shroud_clearing_range
                            .or(entry.record.vision_range)
                            .filter(|range| (0.0..=2_000.0).contains(range))
                            .unwrap_or(crate::shroud::HORDE_SHROUD_RANGE),
                    });
                }
                _ => {
                    let kind = draw_build_variation(&kinds, &mut variation_rng, kind);
                    if let Some(spawn) = kinds.spawn(&data.combat, kind, at, facing, start.player) {
                        spawns.push(Spawn {
                            upkeep: kinds.upkeep(kind),
                            ..spawn
                        });
                    }
                }
            }
        }
    }

    let contestants = u8::try_from(inputs.players.len())
        .unwrap_or(u8::MAX)
        .clamp(1, crate::shroud::SHROUD_PLAYERS_U8);
    let mut map_placements: Vec<(usize, u32)> = Vec::new();
    let mut script_object_names: Vec<(usize, String)> = Vec::new();
    let mut indestructible: Vec<usize> = Vec::new();
    let mut placed_keys: Vec<(usize, crate::map_keys::ObjectKeys)> = Vec::new();
    let mut script_teams: Vec<(usize, String)> = Vec::new();
    let mut placed_hordes: Vec<crate::rules::PlacedHorde> = Vec::new();
    let map_setup = place_map_entities(
        data,
        inputs,
        contestants,
        &kinds,
        &production,
        &mut spawns,
        &mut structure_footprints,
        &mut map_placements,
        &mut script_object_names,
        &mut indestructible,
        &mut placed_keys,
        &mut script_teams,
        &mut placed_hordes,
        &mut lines,
        &mut variation_rng,
    );

    let computer_sides: Vec<crate::economy::ComputerSide> = inputs
        .map_cast
        .players
        .iter()
        .zip(0u8..)
        .filter_map(|(side, offset)| {
            if side.human || side.name.is_empty() || side.universal_hostile {
                return None;
            }
            let template = data.players.get(&side.faction);
            Some(crate::economy::ComputerSide {
                player: PlayerId::new(contestants.checked_add(offset)?),
                evil: template.and_then(|t| t.evil) == Some(true),
                cash: template.and_then(|t| t.start_money).unwrap_or(0),
            })
        })
        .collect();

    let alignments: Vec<Alignment> = inputs
        .players
        .iter()
        .map(|start| {
            if start.faction.evil == Some(true) {
                Alignment::Evil
            } else {
                Alignment::Good
            }
        })
        .collect();
    let starting_cash_overrides: Vec<Option<i64>> = if inputs.starting_money.is_some() {
        Vec::new()
    } else {
        inputs
            .players
            .iter()
            .map(|start| start.faction.start_money.filter(|money| *money != 0))
            .collect()
    };
    let initial_upgrades: Vec<mint_sim::UpgradeMask> = inputs
        .players
        .iter()
        .map(|start| data.upgrade_bits.mask(&start.faction.initial_upgrades))
        .collect();
    let production_cost_change: Vec<f32> = inputs
        .players
        .iter()
        .map(|start| start.faction.production_cost_change.unwrap_or(0.0))
        .collect();
    let player_sides: Vec<(PlayerId, String)> = inputs
        .players
        .iter()
        .filter_map(|start| start.faction.side.clone().map(|side| (start.player, side)))
        .chain(
            inputs
                .map_cast
                .players
                .iter()
                .enumerate()
                .filter_map(|(at, side)| {
                    let id = u8::try_from(at)
                        .ok()
                        .and_then(|at| contestants.checked_add(at))?;
                    let faction_side = data.players.get(&side.faction)?.side.clone()?;
                    Some((PlayerId::new(id), faction_side))
                }),
        )
        .collect();
    let (passability, wall_geometry) = grid(&[])?;
    let border = u32::from(passability.border);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fire_nx =
        (passability.playable_high.x / crate::rules::terrain_fire::CELL_SIZE).floor() as usize + 1;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fire_ny =
        (passability.playable_high.y / crate::rules::terrain_fire::CELL_SIZE).floor() as usize + 1;
    let fire_grid = crate::rules::terrain_fire::FireGrid::build(
        fire_nx,
        fire_ny,
        |x, y| {
            let byte = blend_tile.flammability(
                u32::try_from(x).unwrap_or(u32::MAX).saturating_add(border),
                u32::try_from(y).unwrap_or(u32::MAX).saturating_add(border),
            )?;
            Some(match byte {
                mint_sage_map::Flammability::FireResistant => 0,
                mint_sage_map::Flammability::Grass => 1,
                mint_sage_map::Flammability::HighlyFlammable => 2,
                mint_sage_map::Flammability::Undefined => 3,
            })
        },
        &data.fire_logic_system,
    );
    lines.push(format!(
        "skirmish: {} template(s) in the match's closure, {} producer menu(s), {} buildable(s), \
         {} object(s) and {} horde(s) at start ({} placed on the map, {} member(s) wanted), {} \
         structure footprint(s) will stamp the dynamic layer at setup",
        kinds.len(),
        production.menus.len(),
        production.buildables.len(),
        spawns.len(),
        hordes.len().saturating_add(placed_hordes.len()),
        placed_hordes.len(),
        placed_hordes
            .iter()
            .map(|placed| u64::from(placed.spawn.count))
            .sum::<u64>(),
        obstacles.len()
    ));
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let (sciences, ranks, mut spell_plans) = spell_setup(data, &kinds, &inputs.players);
    if spell_plans.len() < usize::from(contestants) {
        spell_plans.resize(
            usize::from(contestants),
            crate::sciences::SpellPlan::default(),
        );
    }
    spell_plans.extend(inputs.map_cast.players.iter().map(|side| {
        crate::sciences::SpellPlan {
            spell_book: data
                .players
                .get(&side.faction)
                .and_then(|faction| faction.spell_book_mp.as_deref())
                .and_then(|name| kinds.kind(name)),
            ..crate::sciences::SpellPlan::default()
        }
    }));
    lines.push(sciences.describe());
    let power_sciences = power_names.required_sciences(&data.special_powers, &sciences);
    let launch_bones: Vec<Vec<crate::perches::SlotLaunch>> = kinds
        .iter()
        .map(|entry| launches(&entry.record.name))
        .collect();
    lines.push(format!(
        "launch bones: {} of {} template(s) fire from bones on their model, {} slot(s) and {} barrel(s) in all, posed by {} firing state(s)",
        launch_bones
            .iter()
            .filter(|slots| slots.iter().any(|slot| !slot.rest.is_empty()))
            .count(),
        launch_bones.len(),
        launch_bones
            .iter()
            .flatten()
            .filter(|slot| !slot.rest.is_empty())
            .count(),
        launch_bones
            .iter()
            .flatten()
            .map(|slot| slot.rest.len())
            .sum::<usize>(),
        launch_bones
            .iter()
            .flatten()
            .map(|slot| slot.poses.len())
            .sum::<usize>()
    ));
    let wall_bounds: Vec<Option<crate::wall_meshes::WallBounds>> = kinds
        .iter()
        .map(|entry| wall_bounds_for(&entry.record.name))
        .collect();
    lines.push(format!(
        "wall bounds: {} of {} template(s) carry a WallBoundsMesh",
        wall_bounds.iter().filter(|bounds| bounds.is_some()).count(),
        wall_bounds.len()
    ));
    let ramp_meshes: Vec<Vec<crate::wall_meshes::RampMesh>> = kinds
        .iter()
        .map(|entry| ramp_meshes_for(&entry.record.name))
        .collect();
    lines.push(format!(
        "ramp meshes: {} of {} template(s) carry a ramp",
        ramp_meshes.iter().filter(|ramps| !ramps.is_empty()).count(),
        ramp_meshes.len()
    ));
    let raised_wall_meshes: Vec<bool> = kinds
        .iter()
        .map(|entry| has_raised_wall_mesh_for(&entry.record.name))
        .collect();
    let tree_templates = crate::forest::TreeTemplates {
        supply_per_tree: data.supply_boxes_per_tree,
        tainted: kinds
            .iter()
            .map(|entry| {
                crate::forest::is_tree(&entry.record)
                    && object_index.is_some_and(|index| index.tainted_tree(&entry.record.name))
            })
            .collect(),
        motion: kinds
            .iter()
            .map(|entry| {
                crate::forest::is_tree(&entry.record)
                    .then(|| object_index?.tree_topple(&entry.record.name))
                    .flatten()
                    .map(|draw| crate::forest::ToppleMotion::from_draw(&draw))
            })
            .collect(),
        default_motion: crate::forest::ToppleMotion::default(),
    };
    lines.push(format!(
        "forest: {} tree kind(s) fall by their own W3DTreeDraw, {} tainted",
        tree_templates.motion.iter().flatten().count(),
        tree_templates
            .tainted
            .iter()
            .filter(|tainted| **tainted)
            .count()
    ));
    let mut forest_missing = 0usize;
    let forest_seeds: Vec<crate::forest::ForestTreeInit> = inputs
        .trees
        .iter()
        .filter_map(|seed| {
            let Some(kind) = kinds.kind(&seed.template) else {
                forest_missing = forest_missing.saturating_add(1);
                return None;
            };
            Some(crate::forest::ForestTreeInit {
                pos: seed.pos,
                kind,
                grabbable: seed.grabbable,
                harvestable: seed.harvestable,
                tainted: tree_templates
                    .tainted
                    .get(usize::from(kind.0))
                    .copied()
                    .unwrap_or(false),
                source_index: seed.source_index,
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                supply: seed
                    .supply_override
                    .unwrap_or(data.supply_boxes_per_tree)
                    .max(0.0) as u32,
            })
        })
        .collect();
    lines.push(format!(
        "forest: {} tree seed(s) resolved, {forest_missing} skipped for want of a Kind",
        forest_seeds.len()
    ));
    lines.push(format!(
        "order queue: ORDERMODE_WAYPOINT key {}",
        data.order_mode_key
            .map_or("unresolved (second decode)".to_owned(), |key| format!(
                "{key:?}"
            ))
    ));
    let local_player = inputs
        .players
        .iter()
        .map(|start| start.player)
        .find(|player| !ai_seeds.iter().any(|seed| seed.player == *player));
    let mut simulation = Simulation::new(
        seed,
        RotwkRules::new(passability, spawns)
            .with_wall_geometry(wall_geometry)
            .with_structure_footprints(structure_footprints)
            .with_map_placements(map_placements)
            .with_script_object_names(script_object_names)
            .with_indestructible(indestructible)
            .with_placed_keys(placed_keys)
            .with_script_teams(script_teams)
            .with_placed_hordes(placed_hordes)
            .with_fire_grid(fire_grid)
            .with_forest_seeds(forest_seeds)
            .with_tree_templates(tree_templates)
            .with_castles(castle_members, Arc::new(inputs.castle_layouts.clone()))
            .with_combat(data.combat.clone())
            .with_emotions(Arc::clone(&data.emotions))
            .with_stances(Arc::clone(&data.stances))
            .with_ai_power_buttons(Arc::clone(&data.command_buttons))
            .with_command_sets(Arc::clone(&data.command_sets))
            .with_ai_power_tuning(
                data.ai_power_activation_tuning,
                inputs.ai_difficulty.clone(),
            )
            .with_difficulty(inputs.difficulty.clone())
            .with_combat_chains((*data.combat_chains).clone())
            .with_hordes(data.formations.tables().to_vec(), hordes)
            .with_melee_hordes(data.formations.melee().to_vec())
            .with_horde_rangefinders(data.formations.rangefinders().to_vec())
            .with_banners(crate::horde::banner_plans(&data.formations, &kinds))
            .with_construction(Arc::clone(&kinds), production.clone())
            .with_cancel_refund_percent(data.construction_cancel_refund_percent)
            .with_max_cells_adjust_horde_melee(data.max_cells_adjust_horde_melee)
            .with_max_cells_adjust_destination(data.max_cells_adjust_destination)
            .with_max_cells_adjust_to_possible_destination(
                data.max_cells_adjust_to_possible_destination,
            )
            .with_max_tunnel_capacity(data.max_tunnel_capacity)
            .with_melee_placement(data.melee_placement)
            .with_group_formation(data.group_formation)
            .with_drag_formation(Arc::clone(&data.drag_formation))
            .with_castle_siege_stand_back_distance(data.castle_siege_stand_back_distance)
            .with_victory(Arc::clone(&kinds), data.victory_structure_filter.clone())
            .with_passenger_perches(passenger_perches(data, &kinds, perches))
            .with_spawn_points(spawn_points(data, &kinds, perches))
            .with_launch_bones(launch_bones)
            .with_wall_bounds(wall_bounds)
            .with_ramp_meshes(ramp_meshes)
            .with_raised_wall_meshes(raised_wall_meshes)
            .with_siege_bones(
                siege_docks(&kinds, perches),
                siege_above_wall(data, &kinds, perches),
                siege_ladder_exits(&kinds, perches),
            )
            .with_economy(
                production.buildables,
                inputs
                    .starting_money
                    .unwrap_or(mint_sage_economy::DEFAULT_STARTING_CASH),
            )
            .with_starting_cash_overrides(starting_cash_overrides)
            .with_initial_upgrades(initial_upgrades)
            .with_production_cost_change(production_cost_change)
            .with_menus(production.menus)
            .with_menu_needs(production.menu_needs)
            .with_heroes(hero_rosters, hero_buildables)
            .with_create_a_hero(cah_plans, cah_can_build, cah_revival_discount)
            .with_upgrades(kinds.upgrades(data), Arc::clone(&data.upgrade_bits))
            .with_powers(
                power_names.clone(),
                power_names.reload_table(&data.special_powers),
            )
            .with_power_cast_rules(power_names.cast_rules(&data.special_powers))
            .with_experience(Arc::clone(&data.experience), data.armor_bonus_cap)
            .with_experience_scalars(Arc::clone(&data.experience_scalars))
            .with_damage_thresholds(data.damage_thresholds)
            .with_engaged_defaults(data.engaged_defaults)
            .with_spell_books(sciences, ranks, spell_plans, power_sciences)
            .with_creation_lists(Arc::clone(&data.ocls))
            .with_sell_percentage(data.sell_percentage)
            .with_base_check_frames(data.base_check_frames)
            .with_value_per_supply_box(data.value_per_supply_box)
            .with_gravity(data.gravity)
            .with_ground_stiffness(data.ground_stiffness)
            .with_claim_cell_size(data.terrain_resource_cell_size)
            .with_guard_ranges(data.guard_ranges)
            .with_attack_priorities(data.attack_priorities.clone())
            .with_alignments(alignments)
            .with_skirmish_ai(ai_seeds, data.ai_timing, ai_army_plans)
            .with_object_scripts(data.object_scripts.clone())
            .with_attack_ai(
                data.attack_urgency_values,
                inputs.route_markers.clone(),
                player_sides,
            )
            .with_player_starts(
                inputs
                    .players
                    .iter()
                    .map(|start| (start.player, start.at))
                    .collect(),
            )
            .with_shroud(false)
            .with_players(contestants)
            .with_teams(inputs.teams.clone())
            .with_map_players(contestants, map_setup.names, map_setup.relations)
            .with_contestant_sides(map_setup.contestant_sides)
            .with_computer_sides(computer_sides)
            .with_map_scripts(inputs.map_scripts.clone(), inputs.starts.clone())
            .with_script_named_sides(inputs.map_cast.named.clone())
            .with_script_local_player(local_player)
            .with_handicaps(inputs.handicaps.clone())
            .with_command_point_factor(inputs.command_point_factor.unwrap_or(100))
            .with_command_point_table(data.command_points)
            .with_campaign_command_points(
                (!inputs.difficulty.multiplayer).then_some(data.command_points_solo),
            )
            .with_multiplay_multipliers(data.multiplay)
            .with_score_weights(data.score_weights)
            .with_objects_that_score(data.objects_that_score.clone()),
    )
    .map_err(|e| e.to_string())?;
    simulation
        .set_frame_hash(frame_hash())
        .map_err(|e| e.to_string())?;
    Ok(Start {
        simulation,
        kinds,
        starts: inputs.players.iter().map(|start| start.at).collect(),
        lines,
    })
}

fn draw_build_variation(kinds: &KindTable, rng: &mut Pcg32, kind: Kind) -> Kind {
    let count = kinds.variant_count(kind);
    let Ok(count) = u32::try_from(count) else {
        return kind;
    };
    if count == 0 {
        return kind;
    }
    kinds.resolve_build_variation(kind, rng.below(count))
}

fn map_entity_footprint(
    data: &GameData,
    entity: &crate::map_players::MapEntity,
) -> Option<Vec<mint_sage_passability::FootprintRecord>> {
    let grounded = entity.z.is_finite() && entity.z <= mint_sage_map::MAP_XY_FACTOR;
    grounded
        .then(|| data.footprints.lookup(&entity.template))
        .flatten()
}

#[allow(clippy::too_many_arguments)]
fn place_map_entities(
    data: &GameData,
    inputs: &Inputs,
    first: u8,
    kinds: &KindTable,
    production: &crate::templates::Production,
    spawns: &mut Vec<Spawn>,
    structure_footprints: &mut Vec<(usize, Vec<mint_sage_passability::FootprintRecord>)>,
    map_placements: &mut Vec<(usize, u32)>,
    script_object_names: &mut Vec<(usize, String)>,
    indestructible: &mut Vec<usize>,
    placed_keys: &mut Vec<(usize, crate::map_keys::ObjectKeys)>,
    script_teams: &mut Vec<(usize, String)>,
    placed_hordes: &mut Vec<crate::rules::PlacedHorde>,
    lines: &mut Vec<String>,
    rng: &mut Pcg32,
) -> MapPlayerSetup {
    let cast = &inputs.map_cast;
    lines.extend(cast.lines.iter().cloned());
    let id_of = |owner: usize| {
        u8::try_from(owner)
            .ok()
            .and_then(|offset| first.checked_add(offset))
            .map(PlayerId::new)
    };
    let mut counts: Vec<usize> = vec![0; cast.players.len()];
    let mut held = 0usize;
    for entity in &cast.entities {
        let (Some(owner), Some(kind)) = (entity.owner.resolve(id_of), kinds.kind(&entity.template))
        else {
            lines.push(format!(
                "map players: {} (object {}) not placed - no player id or no kind",
                entity.template, entity.object
            ));
            continue;
        };
        let facing = Facing::from_radians(entity.angle);
        let map_player = match entity.owner {
            crate::map_players::Relation::MapPlayer(at) => Some(at),
            crate::map_players::Relation::Contestant(_) => None,
        };
        if let Some(placed) = placed_horde(data, kinds, entity, kind, facing, owner, map_player) {
            match map_player {
                Some(at) => {
                    if let Some(count) = counts.get_mut(at) {
                        *count = count.saturating_add(1);
                    }
                }
                None => held = held.saturating_add(1),
            }
            placed_hordes.push(placed);
            continue;
        }
        let kind = draw_build_variation(kinds, rng, kind);
        let Some(mut spawn) = kinds.spawn(&data.combat, kind, entity.at, facing, owner) else {
            continue;
        };
        spawn.producer = kinds.producer(production, kind, entity.at, facing);
        spawn.ground_offset = (entity.z != 0.0).then_some(entity.z);
        match entity.owner {
            crate::map_players::Relation::MapPlayer(at) => {
                spawn.vision = None;
                if let Some(count) = counts.get_mut(at) {
                    *count = count.saturating_add(1);
                }
            }
            crate::map_players::Relation::Contestant(_) => {
                held = held.saturating_add(1);
            }
        }
        if let Some(records) = map_entity_footprint(data, entity) {
            structure_footprints.push((spawns.len(), records));
        }
        if let Ok(object) = u32::try_from(entity.object) {
            map_placements.push((spawns.len(), object));
        }
        if let Some(name) = &entity.script_name {
            script_object_names.push((spawns.len(), name.clone()));
        }
        if entity.indestructible {
            indestructible.push(spawns.len());
        }
        if !entity.keys.is_empty() {
            placed_keys.push((spawns.len(), entity.keys.clone()));
        }
        if let Some(team) = &entity.team {
            script_teams.push((spawns.len(), team.clone()));
        }
        spawns.push(spawn);
    }
    if held > 0 {
        lines.push(format!(
            "map players: {held} entit(ies) owned by the contestants holding their Player_N starts"
        ));
    }
    let names: Vec<String> = cast.players.iter().map(|p| p.name.clone()).collect();
    let relations: Vec<crate::alliance::MapRelations> = cast
        .players
        .iter()
        .map(|player| crate::alliance::MapRelations {
            allies: player
                .allies
                .iter()
                .filter_map(|r| r.resolve(id_of))
                .collect(),
            enemies: player
                .enemies
                .iter()
                .filter_map(|r| r.resolve(id_of))
                .collect(),
            universal_hostile: player.universal_hostile,
        })
        .collect();
    let ids = |list: &[PlayerId]| list.iter().map(|id| id.get()).collect::<Vec<_>>();
    for (((player, count), relation), offset) in cast
        .players
        .iter()
        .zip(&counts)
        .zip(&relations)
        .zip(0usize..)
    {
        lines.push(format!(
            "map players: player {} is {}: {count} entit(ies), allies {:?}, enemies {:?}",
            id_of(offset).map_or(u8::MAX, PlayerId::get),
            player.label(),
            ids(&relation.allies),
            ids(&relation.enemies)
        ));
    }
    let contestant_sides: Vec<(PlayerId, crate::alliance::MapRelations)> = cast
        .contestants
        .iter()
        .map(|side| {
            let own = crate::alliance::MapRelations {
                allies: side
                    .allies
                    .iter()
                    .filter_map(|r| r.resolve(id_of))
                    .collect(),
                enemies: side
                    .enemies
                    .iter()
                    .filter_map(|r| r.resolve(id_of))
                    .collect(),
                universal_hostile: false,
            };
            lines.push(format!(
                "map players: contestant {} reads its own side's lists: allies {:?}, enemies {:?}",
                side.player.get(),
                ids(&own.allies),
                ids(&own.enemies)
            ));
            (side.player, own)
        })
        .collect();
    MapPlayerSetup {
        names,
        relations,
        contestant_sides,
    }
}

struct MapPlayerSetup {
    names: Vec<String>,
    relations: Vec<crate::alliance::MapRelations>,
    contestant_sides: Vec<(PlayerId, crate::alliance::MapRelations)>,
}

pub(crate) fn placed_horde(
    data: &GameData,
    kinds: &KindTable,
    entity: &crate::map_players::MapEntity,
    kind: Kind,
    facing: Facing,
    owner: PlayerId,
    map_player: Option<usize>,
) -> Option<crate::rules::PlacedHorde> {
    let entry = kinds.get(kind)?;
    let (Some(formation), Some(member)) = (entry.formation, entry.member) else {
        return None;
    };
    let mut member = kinds.spawn(&data.combat, member, entity.at, facing, owner)?;
    let vision = if map_player.is_some() {
        member.vision = None;
        0.0
    } else {
        entry
            .record
            .shroud_clearing_range
            .or(entry.record.vision_range)
            .filter(|range| (0.0..=2_000.0).contains(range))
            .unwrap_or(crate::shroud::HORDE_SHROUD_RANGE)
    };
    Some(crate::rules::PlacedHorde {
        spawn: HordeSpawn {
            formation,
            origin: entity.at,
            facing,
            member,
            count: data
                .formations
                .tables()
                .get(usize::from(formation))
                .map_or(0, |table| u32::try_from(table.len()).unwrap_or(0)),
            vision,
        },
        kind,
        placement: u32::try_from(entity.object).ok(),
        name: entity.script_name.clone(),
        team: entity.team.clone(),
        indestructible: entity.indestructible,
        keys: entity.keys.clone(),
    })
}

pub(crate) fn spell_setup(
    data: &GameData,
    kinds: &KindTable,
    players: &[PlayerStart],
) -> (
    Arc<crate::sciences::ScienceTable>,
    Arc<crate::sciences::Ranks>,
    Vec<crate::sciences::SpellPlan>,
) {
    let sciences = crate::sciences::ScienceTable::build(&data.sciences);
    let ranks = crate::sciences::Ranks::build(&data.ranks, &sciences);
    let plans = players
        .iter()
        .map(|start| {
            let faction = &start.faction;
            crate::sciences::SpellPlan {
                spell_book: faction
                    .spell_book_mp
                    .as_deref()
                    .and_then(|name| kinds.kind(name)),
                store: faction
                    .purchase_science_command_set_mp
                    .as_deref()
                    .map(|set| {
                        mint_sage_modules::purchase_science_buttons(
                            set,
                            &data.command_sets,
                            &data.command_buttons,
                        )
                        .into_iter()
                        .filter_map(|(button, science)| Some((button, sciences.id(&science)?)))
                        .collect()
                    })
                    .unwrap_or_default(),
                intrinsic: faction
                    .intrinsic_sciences_mp
                    .iter()
                    .filter_map(|name| sciences.id(name))
                    .collect(),
                rank_cap: faction
                    .max_level_mp
                    .as_deref()
                    .and_then(|raw| {
                        mint_sage_ini::value::parse_float_or_expression(raw, &data.macros)
                    })
                    .filter(|cap| cap.is_finite() && *cap >= 1.0 && *cap <= 100_000.0)
                    .map(|cap| {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let whole = cap as u32;
                        whole
                    }),
                intrinsic_purchase_points: faction
                    .intrinsic_science_purchase_points
                    .as_deref()
                    .and_then(|raw| {
                        mint_sage_ini::value::parse_float_or_expression(raw, &data.macros)
                    })
                    .filter(|points| points.is_finite() && *points >= 0.0 && *points <= 100_000.0)
                    .map_or(0, |points| {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let whole = points as u32;
                        whole
                    }),
            }
        })
        .collect();
    (Arc::new(sciences), Arc::new(ranks), plans)
}

fn hero_roster(start: &PlayerStart, allow_ring_heroes: bool) -> Vec<String> {
    crate::setup_rules::hero_list(
        allow_ring_heroes,
        &start.faction.ring_heroes,
        &start.faction.heroes,
    )
}

fn placement_facing(kinds: &KindTable, kind: Kind) -> Facing {
    let degrees = kinds
        .get(kind)
        .and_then(|entry| entry.record.placement_view_angle)
        .filter(|degrees| degrees.is_finite())
        .unwrap_or(0.0);
    Facing::from_radians(degrees.to_radians())
}

pub(crate) fn standable(passability: &Passability, wanted: Vec2) -> Vec2 {
    let Some(cell) = passability.cell_at(wanted) else {
        return wanted;
    };
    if mint_movement::Passability::is_passable(
        &passability.grid,
        cell,
        mint_sage_passability::GROUND,
    ) {
        return wanted;
    }
    passability
        .nearest_passable(cell, mint_sage_passability::GROUND)
        .map_or(wanted, |cell| passability.geometry.centre_of(cell))
}

#[cfg(test)]
mod tests;
