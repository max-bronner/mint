use mint_app::{App, InputEvent, Key, RtsCamera, DEFAULT_EDGE_SCROLL_MARGIN};
use mint_movement::{AvoidanceMode, SearchMode, SmoothingMode, Vec2};
use mint_render::{Ray, Renderer, ScreenDraw, UiGeometry, Vec3};
use mint_sim::{Frame, PlayerId, Simulation};
use mint_ui::{HitTarget, InputRouter, PointerButton as UiPointerButton, PointerRouting};

use crate::apt_overlay::{
    minimap_uv_of, minimap_world_at, route_pointer_button, route_pointer_moved, slot_widget_id,
    AptOverlay,
};
use crate::command_bar::SlotAction;
use crate::mode::{transition, Event, Mode};
use crate::pick::{
    box_select, pick_force_target, pick_target, pick_unit, BoxMode, Declared, ScreenRect,
};
use crate::pointer::{radar_click, Intent, Pointer, RadarAction, RadarArmed};
use crate::render_state::{PoseTrack, UnitView};
use crate::report::{FrameTimes, SecondReport};
use crate::rules::{DebugCommand, RotwkCommand, RotwkRules};
use crate::scene::MatchScene;
use crate::unit::{Kind, Modes};
use crate::visuals::UnitVisuals;
use mint_sage_asset::KindOf;
use std::path::PathBuf;
use std::time::Duration;

use mint_sage_scene::{ShroudFade, ShroudView, UnitPose, UnitScene, CLEAR_ALPHA, SHROUD_ALPHA};
use mint_sage_terrain::TerrainShroud;
use mint_vfs::Vfs;
use std::sync::Arc;

use crate::game_data::GameData;
use crate::horde::FormationMode;
use crate::match_values::{MatchStart, StartAt};
use crate::templates::KindTable;
use mint_sage_passability::{Obstacle, Passability};

mod cah;
mod cinematic;
pub(crate) mod claim_outline;
mod crash_report;
pub(crate) mod decals;
mod keys;
mod projectile_art;
mod script;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) use projectile_art::load_projectile_art;
use projectile_art::ProjectileArt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ControlSet {
    All,
    Listed(Vec<PlayerId>),
}

impl ControlSet {
    #[must_use]
    pub(crate) fn default_local() -> Self {
        Self::Listed(vec![LOCAL_PLAYER])
    }

    pub(crate) fn parse(spec: &str) -> Result<Self, String> {
        if spec.eq_ignore_ascii_case("all") {
            return Ok(Self::All);
        }
        let mut players = Vec::new();
        for part in spec.split(',') {
            let part = part.trim();
            let index: u8 = part.parse().map_err(|_| {
                format!("--control wants \"all\" or a comma list of indices, got {spec}")
            })?;
            players.push(PlayerId::new(index));
        }
        if players.is_empty() {
            return Err(format!("--control's list is empty: {spec}"));
        }
        Ok(Self::Listed(players))
    }

    #[must_use]
    pub(crate) fn next(&self, current: PlayerId, player_count: usize) -> PlayerId {
        match self {
            Self::All => {
                let Ok(count) = u8::try_from(player_count) else {
                    return current;
                };
                if count == 0 {
                    return current;
                }
                PlayerId::new((current.get().saturating_add(1)) % count)
            }
            Self::Listed(players) => {
                if players.is_empty() {
                    return current;
                }
                let at = players
                    .iter()
                    .position(|player| *player == current)
                    .unwrap_or(0);
                let next = at.saturating_add(1) % players.len();
                players.get(next).copied().unwrap_or(current)
            }
        }
    }

    #[must_use]
    pub(crate) fn label(&self) -> String {
        match self {
            Self::All => "all".to_owned(),
            Self::Listed(players) => players
                .iter()
                .map(|player| player.get().to_string())
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum MatchConfig {
    Skirmish(crate::lobby::SkirmishSetup),
    ShellGame(crate::shell_game::ShellGameKind),
    Campaign(crate::campaign::CampaignMatch),
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self::Skirmish(crate::lobby::SkirmishSetup::two_player(
            String::from(crate::skirmish::DEFAULT_FACTION),
            String::from(crate::skirmish::DEFAULT_OPPONENT),
            String::new(),
            [false, true],
            None,
        ))
    }
}

impl MatchConfig {
    pub(crate) fn scene_filter(&self) -> (bool, bool) {
        match self {
            Self::Skirmish(setup) => (true, setup.rules.allow_ring_heroes),
            Self::ShellGame(_) => (true, false),
            Self::Campaign(_) => (true, crate::lobby::MatchRules::default().allow_ring_heroes),
        }
    }

    pub(crate) fn game_mode(&self) -> crate::game_mode::GameMode {
        match self {
            Self::Skirmish(_) => crate::game_mode::GameMode::Skirmish,
            Self::ShellGame(_) => crate::game_mode::GameMode::Shell,
            Self::Campaign(campaign) => campaign.mode,
        }
    }

    pub(crate) fn shell_game_kind(&self) -> Option<crate::shell_game::ShellGameKind> {
        match self {
            Self::Skirmish(_) | Self::Campaign(_) => None,
            Self::ShellGame(kind) => Some(*kind),
        }
    }
}

pub(crate) struct MatchSetup {
    pub(crate) simulation: Simulation<RotwkRules>,
    pub(crate) kinds: Arc<KindTable>,
    pub(crate) focus: Vec3,
    pub(crate) starts: Vec<Vec3>,
    pub(crate) local_evil: bool,
    pub(crate) local_faction: Option<String>,
    pub(crate) local_heroes: Vec<String>,
    pub(crate) local_side: Option<String>,
    pub(crate) local_template: Option<String>,
    pub(crate) colours: Vec<u8>,
}

pub(crate) fn faction_icon_image(side: &str) -> String {
    format!("AptIcon{side}")
}

pub(crate) fn palantir_faction(
    map_icon: Option<String>,
    template: &mint_sage_ini::PlayerTemplateDef,
) -> String {
    map_icon
        .filter(|icon| !icon.is_empty())
        .or_else(|| {
            template
                .playable
                .unwrap_or(false)
                .then(|| template.side.clone())
                .flatten()
        })
        .unwrap_or_default()
}

pub(crate) fn build_match(
    scene: &MatchScene,
    data: &GameData,
    vfs: &Vfs,
    config: &MatchConfig,
    sources: PassabilitySources,
) -> Result<MatchSetup, String> {
    let start = match config {
        MatchConfig::Skirmish(setup) => match &setup.wotr {
            Some(spec) => MatchStart::for_wotr_battle(&spec.0, setup, data)?,
            None => MatchStart::from_lobby(setup, &scene.objects, data, vfs)?,
        },
        MatchConfig::Campaign(mission) => MatchStart::for_mission(mission, &scene.sides, data)?,
        MatchConfig::ShellGame(_) => MatchStart::scenery(),
    };
    build_from_start(scene, data, vfs, &start, sources)
}

pub(crate) fn build_from_start(
    scene: &MatchScene,
    data: &GameData,
    vfs: &Vfs,
    start_value: &MatchStart,
    sources: PassabilitySources,
) -> Result<MatchSetup, String> {
    for seat in &start_value.players {
        println!(
            "skirmish: player {}: {}, {}, team {}, colour {}, handicap {}%, start {}",
            seat.player.get(),
            seat.template,
            seat.ai
                .map_or_else(|| "human".to_owned(), |d| format!("{d:?} AI")),
            seat.team
                .map_or_else(|| "-".to_owned(), |t| (t + 1).to_string()),
            seat.colour,
            seat.handicap,
            match &seat.at {
                StartAt::Waypoint(slot) => slot.saturating_add(1).to_string(),
                StartAt::MapSide(name) => name.clone(),
            },
        );
    }
    let ai_players: Vec<PlayerId> = start_value
        .players
        .iter()
        .filter(|p| p.ai.is_some())
        .map(|p| p.player)
        .collect();
    let inputs = crate::match_values::inputs(
        start_value,
        data,
        crate::match_values::MapFacts {
            objects: &scene.objects,
            sides: &scene.sides,
            scripts: scene.map_scripts.clone(),
            script_lines: scene.map_script_lines.clone(),
            referenced_unit_names: &scene.referenced_unit_names,
            snowy: scene.snowy,
        },
        |path| vfs.read(path).ok(),
    )?;
    let mut start = crate::skirmish::build(
        data,
        scene.object_index.as_ref(),
        &inputs,
        &scene.blend_tile,
        |extra| map_grid(scene, data, sources, extra, &inputs.map_cast.entities),
        &|template, prefix| {
            scene.object_index.as_ref().map_or_else(Vec::new, |index| {
                crate::perches::passenger_bones(vfs, &scene.resolver, index, template, prefix)
            })
        },
        &|template| {
            scene.object_index.as_ref().map_or_else(Vec::new, |index| {
                crate::perches::launch_bones(vfs, &scene.resolver, index, template)
            })
        },
        &|template| {
            scene.object_index.as_ref().and_then(|index| {
                crate::wall_meshes::wall_bounds(vfs, &scene.resolver, index, template)
            })
        },
        &|template| {
            scene.object_index.as_ref().map_or_else(Vec::new, |index| {
                [
                    crate::wall_meshes::ramp_mesh_1(vfs, &scene.resolver, index, template),
                    crate::wall_meshes::ramp_mesh_2(vfs, &scene.resolver, index, template),
                ]
                .into_iter()
                .flatten()
                .collect()
            })
        },
        &|template| {
            scene.object_index.as_ref().is_some_and(|index| {
                crate::wall_meshes::has_raised_wall_mesh(vfs, &scene.resolver, index, template)
            })
        },
        start_value
            .wotr
            .as_ref()
            .map_or(crate::skirmish::SKIRMISH_SEED, |battle| battle.seed),
        &ai_players,
    )?;
    for line in &start.lines {
        println!("{line}");
    }
    for at in 0..start.kinds.len() {
        let kind = Kind(u16::try_from(at).unwrap_or(u16::MAX));
        crate::diag::diag!("templates: {}", start.kinds.describe(kind));
    }
    if let Some(per_side) = start_value.deploy.army {
        let sides: Vec<crate::army::SideStart> = inputs
            .players
            .iter()
            .map(|player_start| crate::army::SideStart {
                player: player_start.player,
                at: player_start.at,
                side: player_start.faction.side.clone(),
            })
            .collect();
        for line in crate::army::deploy(&mut start.simulation, &sides, per_side)? {
            println!("{line}");
        }
    }
    if let (Some(mine), Some(theirs)) = (inputs.players.first(), inputs.players.get(1)) {
        let line = crate::army::Line::towards(mine.at, theirs.at);
        let mut far_depth = 0.0f32;
        if let Some(spec) = &start_value.deploy.spawn {
            let deployed = crate::spawn::deploy_units(
                &mut start.simulation,
                mine.player,
                &line,
                crate::spawn::SPAWN_START_DEPTH,
                line.facing(),
                spec,
                crate::rules::SpawnRole::Normal,
            )?;
            far_depth = far_depth.max(deployed.far_depth);
            for line in deployed.lines {
                println!("{line}");
            }
        }
        if let Some(spec) = &start_value.deploy.spawn_buildings {
            let deployed = crate::spawn::deploy_buildings(
                &mut start.simulation,
                mine.player,
                mine.at,
                theirs.at,
                spec,
            )?;
            far_depth = far_depth.max(deployed.far_depth);
            for line in deployed.lines {
                println!("{line}");
            }
        }
        if let Some(spec) = &start_value.deploy.dummies {
            let facing = crate::army::Line::towards(theirs.at, mine.at).facing();
            let deployed = crate::spawn::deploy_units(
                &mut start.simulation,
                theirs.player,
                &line,
                crate::spawn::dummy_start_depth(far_depth),
                facing,
                spec,
                if start_value.deploy.dummies_attack {
                    crate::rules::SpawnRole::FightingDummy
                } else {
                    crate::rules::SpawnRole::Dummy
                },
            )?;
            for line in deployed.lines {
                println!("{line}");
            }
        }
    }
    let starts: Vec<Vec3> = start
        .starts
        .iter()
        .map(|at| Vec3::new(at.x, at.y, ground_at(scene, at.x, at.y)))
        .collect();
    let local_player_start = inputs
        .players
        .iter()
        .find(|player_start| player_start.player == LOCAL_PLAYER);
    let local_evil = local_player_start
        .and_then(|player_start| player_start.faction.evil)
        .unwrap_or(false);
    let local_faction = local_player_start.map(|player_start| {
        let map_icon = inputs
            .map_cast
            .contestants
            .iter()
            .find(|side| side.player == LOCAL_PLAYER)
            .and_then(|side| side.faction_icon.clone());
        palantir_faction(map_icon, &player_start.faction)
    });
    let local_heroes = local_player_start
        .map(|player_start| {
            let faction = &player_start.faction;
            crate::setup_rules::heroes_offered(
                &start_value.rules,
                &faction.ring_heroes,
                &faction.heroes,
            )
        })
        .unwrap_or_default();
    let local_side = local_player_start.and_then(|p| p.faction.side.clone());
    let local_template = local_player_start.map(|p| p.faction.name.clone());
    Ok(MatchSetup {
        simulation: start.simulation,
        kinds: start.kinds,
        focus: starts.first().copied().unwrap_or(Vec3::ZERO),
        starts,
        local_evil,
        local_faction,
        local_heroes,
        local_side,
        local_template,
        colours: start_value.players.iter().map(|p| p.colour).collect(),
    })
}

pub(crate) fn map_grid(
    scene: &MatchScene,
    data: &GameData,
    sources: PassabilitySources,
    extra: &[Obstacle],
    entities: &[crate::map_players::MapEntity],
) -> Result<(Passability, Vec<mint_sage_passability::WallGeometry>), String> {
    let mut obstacles = if sources.objects() {
        mint_sage_passability::footprints::resolve(&scene.objects, &data.footprints)
    } else {
        Vec::new()
    };
    let scaleable = if sources.objects() {
        mint_sage_passability::scaleable_ids(&scene.objects, &data.footprints)
    } else {
        Vec::new()
    };
    let wall_geometry = if sources.objects() {
        mint_sage_passability::scaleable_walls(&scene.objects, &data.footprints)
    } else {
        Vec::new()
    };
    obstacles.retain(|obstacle| {
        !entities
            .iter()
            .any(|entity| u32::try_from(entity.object).is_ok_and(|id| id == obstacle.id))
    });
    let map_obstacles = obstacles.len();
    if sources.objects() {
        obstacles.extend_from_slice(extra);
    }
    let mut builder = Passability::builder(&scene.height_map, scene.terrain.vertical_scale)
        .cliff_source(&scene.blend_tile)
        .obstacles(&obstacles)
        .scaleable_obstacles(&scaleable)
        .playable_border(sources.boundary());
    if sources.water() {
        builder = builder.water(&scene.water, data.water_depths);
    }
    let passability = builder.build()?;
    println!("passability sources: {}", sources.label());
    println!(
        "{} ({map_obstacles} from the map, {} placed by this match)",
        passability.report(),
        obstacles.len().saturating_sub(map_obstacles)
    );
    Ok((passability, wall_geometry))
}

pub(crate) const LOCAL_PLAYER: PlayerId = PlayerId::new(0);

const PLAN_CONDITION: &str = "PHANTOM_STRUCTURE";

const ROTATE_SENSITIVITY: f32 = 0.5;

const PLACEMENT_DRAG_THRESHOLD_PIXELS: f32 = 5.0;

const GHOST_RECHECK_CELL: f32 = 10.0;

pub(crate) const EVIL_HORDE: &str = "MordorFighterHorde";

pub(crate) const ENEMY_PLAYER: PlayerId = PlayerId::new(1);

const TACTICAL_HFOV_DEGREES: f32 = 50.0;

pub(crate) fn apply_tactical_lens(
    camera: &mut RtsCamera,
    defaults: &crate::combat::CameraDefaults,
    viewport: (u32, u32),
) {
    camera.set_pitch(defaults.pitch_degrees.to_radians());
    if let Some(fov_y) = tactical_fov_y(viewport) {
        camera.set_fov_y(fov_y);
    }
}

fn viewport_aspect(viewport: (u32, u32)) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (viewport.0 as f32, viewport.1 as f32);
    if h > 0.0 {
        w / h
    } else {
        1.0
    }
}

fn tactical_fov_y(viewport: (u32, u32)) -> Option<f32> {
    fov_y_for(TACTICAL_HFOV_DEGREES.to_radians(), viewport)
}

fn fov_y_for(hfov: f32, viewport: (u32, u32)) -> Option<f32> {
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (viewport.0 as f32, viewport.1 as f32);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let half_width = mint_math::f32::tan(hfov * 0.5);
    Some(2.0 * mint_math::f32::atan(half_width * h / w))
}

pub(crate) fn opening_distance(defaults: &crate::combat::CameraDefaults) -> f32 {
    let sine = mint_math::f32::sin(defaults.pitch_degrees.to_radians());
    if sine > 1e-3 {
        defaults.max_height / sine
    } else {
        defaults.max_height
    }
}

pub(crate) fn attach_shroud(
    renderer: &mut Renderer,
    scene: &mut MatchScene,
    simulation: &Simulation<RotwkRules>,
) -> Option<ShroudView> {
    let ids = simulation.rules().ids()?;
    let (cells_x, cells_y, extent) =
        crate::shroud::grid_shape(simulation.state(), RotwkRules::shroud_ids(ids))?;
    let view = match ShroudView::new(renderer, cells_x, cells_y) {
        Ok(view) => view,
        Err(error) => {
            println!("shroud: texture upload failed ({error}) - the ground draws unshrouded");
            return None;
        }
    };
    let padded_extent = crate::shroud::padded_shroud_extent(extent, cells_x, cells_y);
    if let Err(error) =
        scene
            .terrain_shading
            .bind_shroud(renderer, &scene.tuning, view.texture(), padded_extent)
    {
        println!("shroud: scene binding failed ({error}) - the ground draws unshrouded");
        return None;
    }
    scene.shroud_extent = padded_extent;
    scene.shroud_texture = Some(view.texture());
    if let Err(error) = scene
        .fx_water
        .bind_shroud(renderer, view.texture(), padded_extent)
    {
        println!("shroud: fx water binding failed ({error})");
    }
    let strength = crate::scene::shroud_strength(scene);
    let mut bound = 1usize;
    for (what, shader) in [
        ("roads", scene.road_scene.shader),
        ("water", scene.water_scene.shader),
        ("rivers", scene.river_scene.shader),
        ("waves", scene.wave_scene.shader),
    ] {
        match mint_sage_scene::bind_shroud_scene(
            renderer,
            shader,
            Some(view.texture()),
            padded_extent,
            strength,
        ) {
            Ok(()) => {
                if shader.is_some() {
                    bound = bound.saturating_add(1);
                }
            }
            Err(error) => {
                println!("shroud: {what} scene binding failed ({error}) - they draw unshrouded");
            }
        }
    }
    println!(
        "shroud: {cells_x}x{cells_y} cell texture ({}x{} with its border ring, held at {SHROUD_ALPHA}) \
         bound over ({:.0}, {:.0}) + {:.0}x{:.0} world units, into {bound} shader(s) of \
         terrain/roads/water/rivers/waves",
        cells_x.saturating_add(2),
        cells_y.saturating_add(2),
        extent[0],
        extent[1],
        extent[2],
        extent[3]
    );
    Some(view)
}

fn shroud_object_tints(state: &mut MatchState) {
    let Some(ids) = state.simulation.rules().ids() else {
        return;
    };
    let revealed = state.scene.terrain_shading.modulation.shroud == TerrainShroud::Revealed;
    let viewer = state.shroud_viewer;
    let scene = &mut state.scene;
    let read = crate::shroud::with_grid(
        state.simulation.state(),
        RotwkRules::shroud_ids(ids),
        |grid| write_object_tints(scene, Some(grid), viewer, revealed),
    );
    if read.is_none() {
        write_object_tints(scene, None, viewer, revealed);
    }
}

fn write_object_tints(
    scene: &mut MatchScene,
    grid: Option<&mint_sage_shroud::VisibilityGrid>,
    viewer: PlayerId,
    revealed: bool,
) {
    let placed = scene
        .drawn
        .get(scene.object_span.clone())
        .map_or(0, <[mint_render::Instance]>::len);
    for (offset, base) in scene.base_tints.iter().enumerate() {
        let Some(index) = scene.object_span.start.checked_add(offset) else {
            break;
        };
        let Some(instance) = scene.drawn.get_mut(index) else {
            break;
        };
        let (masked, on_grid) = if offset < placed {
            let slot = scene.object_proxy_slots.get(offset).copied().flatten();
            let point = shroud_point(instance.transform, slot, &scene.proxies);
            match grid {
                Some(grid) => {
                    let (x, y) = grid.world_to_cell(point.x, point.y);
                    let on_grid = grid.contains(point.x, point.y);
                    (
                        object_shroud(grid.cell_state(viewer, x, y), on_grid, revealed),
                        on_grid,
                    )
                }
                None => (Some(false), true),
            }
        } else {
            (Some(false), false)
        };
        let tree = scene.object_trees.get(offset).copied().unwrap_or(false);
        let (visible, mask) = tree_shroud(masked, tree, on_grid, revealed);
        instance.visible = visible;
        instance.visibility_mask = mask;
        instance.visibility_mask_alpha = mask && tree;
        instance.tint = *base;
    }
}

fn tree_shroud(masked: Option<bool>, tree: bool, on_grid: bool, revealed: bool) -> (bool, bool) {
    if tree && on_grid && !revealed {
        return (true, true);
    }
    (masked.is_some(), masked.unwrap_or(false))
}

fn object_shroud(
    cell: mint_sage_shroud::CellVisibility,
    on_grid: bool,
    revealed: bool,
) -> Option<bool> {
    if !on_grid {
        return None;
    }
    match cell {
        _ if revealed => Some(false),
        mint_sage_shroud::CellVisibility::Unexplored => None,
        mint_sage_shroud::CellVisibility::Explored => Some(true),
        mint_sage_shroud::CellVisibility::Visible => Some(false),
    }
}

fn shroud_point(
    part: mint_render::Mat4,
    slot: Option<usize>,
    proxies: &[mint_sage_terrain::ObjectProxy],
) -> Vec2 {
    let placement = slot
        .and_then(|slot| proxies.get(slot))
        .map_or(part, |proxy| proxy.placement);
    let columns = placement.to_cols();
    columns.get(3).map_or(Vec2::ZERO, |column| {
        Vec2::new(
            column.first().copied().unwrap_or(0.0),
            column.get(1).copied().unwrap_or(0.0),
        )
    })
}

fn shroud_obscured(
    grid: &mint_sage_shroud::VisibilityGrid,
    viewer: PlayerId,
    revealed: bool,
    point: Vec3,
) -> bool {
    if revealed && grid.contains(point.x, point.y) {
        return false;
    }
    let (x, y) = grid.world_to_cell(point.x, point.y);
    grid.cell_state(viewer, x, y) != mint_sage_shroud::CellVisibility::Visible
}

fn fx_play_shrouded(
    grid: &mint_sage_shroud::VisibilityGrid,
    viewer: PlayerId,
    revealed: bool,
    play_even_if_shrouded: Option<bool>,
    object: Option<mint_sage_shroud::ObjectVisibility>,
    at: Vec3,
) -> bool {
    if play_even_if_shrouded == Some(true) {
        return false;
    }
    match object {
        Some(status) if !(revealed && grid.contains(at.x, at.y)) => {
            status <= mint_sage_shroud::ObjectVisibility::Fogged
        }
        _ => shroud_obscured(grid, viewer, revealed, at),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FxShroud {
    viewer: PlayerId,
    revealed: bool,
}

impl FxShroud {
    pub(crate) fn of(state: &MatchState) -> Self {
        Self {
            viewer: state.shroud_viewer,
            revealed: state.scene.terrain_shading.modulation.shroud == TerrainShroud::Revealed,
        }
    }

    pub(crate) fn skips(
        self,
        simulation: &Simulation<RotwkRules>,
        particles: &mint_sage_scene::ParticleScene,
        name: &str,
        object: Option<mint_sim::Entity>,
        at: Vec3,
    ) -> bool {
        let Some(list) = particles.fx_list(name) else {
            return false;
        };
        simulation
            .rules()
            .ids()
            .and_then(|ids| {
                let world = simulation.state();
                let status = object
                    .and_then(|entity| world.get(ids.seen, entity))
                    .map(|seen| seen.visibility(self.viewer));
                crate::shroud::with_grid(world, RotwkRules::shroud_ids(ids), |grid| {
                    fx_play_shrouded(
                        grid,
                        self.viewer,
                        self.revealed,
                        list.play_even_if_shrouded,
                        status,
                        at,
                    )
                })
            })
            .unwrap_or(false)
    }
}

pub(crate) fn producing_structures(
    simulation: &Simulation<RotwkRules>,
    player: PlayerId,
) -> Vec<mint_sim::Entity> {
    let Some(ids) = simulation.rules().ids() else {
        return Vec::new();
    };
    let world = simulation.state();
    world
        .store(ids.producer)
        .map(|store| {
            store
                .entities()
                .iter()
                .copied()
                .filter(|entity| {
                    world
                        .get(ids.owner, *entity)
                        .is_some_and(|owner| owner.0 == player)
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn porters_of(
    simulation: &Simulation<RotwkRules>,
    player: PlayerId,
) -> Vec<mint_sim::Entity> {
    let Some(ids) = simulation.rules().ids() else {
        return Vec::new();
    };
    let world = simulation.state();
    world
        .store(ids.shape)
        .map(|store| {
            store
                .iter()
                .filter(|(entity, shape)| {
                    shape.kind_of.contains(mint_sage_asset::KindOf::DOZER)
                        && world
                            .get(ids.owner, *entity)
                            .is_some_and(|owner| owner.0 == player)
                })
                .map(|(entity, _)| entity)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn hordes_of(
    simulation: &Simulation<RotwkRules>,
    player: PlayerId,
) -> Vec<mint_sim::Entity> {
    let Some(ids) = simulation.rules().ids() else {
        return Vec::new();
    };
    let world = simulation.state();
    world
        .store(ids.member)
        .map(|store| {
            store
                .entities()
                .iter()
                .copied()
                .filter(|entity| {
                    world
                        .get(ids.owner, *entity)
                        .is_some_and(|owner| owner.0 == player)
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn match_census(simulation: &Simulation<RotwkRules>) -> Vec<String> {
    let Some(ids) = simulation.rules().ids() else {
        return Vec::new();
    };
    let mut lines = crate::economy::census(simulation.state(), RotwkRules::economy_ids(ids));
    lines.extend(crate::shroud::census(
        simulation.state(),
        RotwkRules::shroud_ids(ids),
    ));
    lines
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KindArt {
    Model(usize),
    Nothing,
    Prism,
}

impl KindArt {
    pub(crate) fn of(art: &[Self], kind: Option<u16>) -> Self {
        kind.and_then(|kind| art.get(usize::from(kind)).copied())
            .unwrap_or(Self::Prism)
    }
}

fn drop_undrawn(units: &mut Vec<UnitView>, art: &[KindArt]) {
    units.retain(|unit| KindArt::of(art, unit.art_kind) != KindArt::Nothing);
}

fn attack_move_cursor(buttons: &mint_sage_ini::CommandButtonIndex) -> CommandCursor {
    buttons
        .iter()
        .find(|(_, button)| {
            button
                .command
                .as_deref()
                .is_some_and(|command| command.eq_ignore_ascii_case("ATTACK_MOVE"))
        })
        .map_or_else(CommandCursor::default, |(name, _)| {
            CommandCursor::of(buttons, name)
        })
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShroudSource {
    frame: u64,
    viewer: mint_sim::PlayerId,
    census: Option<(u32, u32, u32)>,
}

pub(crate) struct MatchState {
    pub(crate) scene: MatchScene,
    pub(crate) camera: RtsCamera,
    pub(crate) view_box_z: f32,
    pub(crate) simulation: Simulation<RotwkRules>,
    pub(crate) visuals: UnitVisuals,
    pub(crate) units: UnitScene,
    pub(crate) unit_art: Vec<KindArt>,
    pub(crate) placed_scales: std::collections::BTreeMap<mint_sim::Entity, f32>,
    pub(crate) projectile_art: ProjectileArt,
    pub(crate) player_colors: PlayerColors,
    pub(crate) vfs: Arc<Vfs>,
    pub(crate) kinds: Arc<KindTable>,
    pub(crate) local_evil: bool,
    pub(crate) local_faction: Option<String>,
    pub(crate) local_heroes: Vec<String>,
    pub(crate) local_side: Option<String>,
    pub(crate) local_template: Option<String>,
    pub(crate) cinematic: crate::cinematic::Cinematic,
    pub(crate) goal: Option<(Vec3, Frame)>,
    pub(crate) stance_flyout: Option<u8>,
    pub(crate) ui_panels_sent: [bool; 3],
    pub(crate) scroll_pending: f32,
    pub(crate) script_camera_clock: (u64, f32),
    pub(crate) previous: PoseTrack,
    pub(crate) snapshot: crate::snapshot::FrameSnapshot,
    pub(crate) shroud_view: Option<ShroudView>,
    pub(crate) shroud_viewer: PlayerId,
    pub(crate) damage_thresholds: crate::render_state::DamageThresholds,
    pub(crate) tdfx: crate::tdfx::TdfxPresenter,
    pub(crate) shroud_fade: ShroudFade,
    pub(crate) shroud_clock: Option<f32>,
    pub(crate) shroud_source: Option<ShroudSource>,
    pub(crate) fx: crate::fx_present::FxPresenter,
    pub(crate) bone_fx: BoneFxTracker,
    pub(crate) level_ups: crate::level_up::LevelWatch,
    pub(crate) selection_flash: crate::selection_flash::SelectionFlash,
    pub(crate) floating: crate::floating_text::FloatingTexts,
    pub(crate) caption_points: f32,
    pub(crate) named_timer_style: crate::named_timer::TimerStyle,
    pub(crate) downwind_angle: f32,
    pub(crate) fonts: Arc<std::collections::HashMap<String, mint_ui::Font>>,
    pub(crate) strings: Arc<mint_sage_str::StringTable>,
}

#[derive(Debug, Clone)]
struct Placement {
    kind: crate::unit::Kind,
    label: String,
    angle: f32,
    anchor: Option<(f32, f32)>,
    checked: Option<(i32, i32, i32, u8)>,
}

#[derive(Debug, Clone)]
struct PowerTarget {
    order: TargetedOrder,
    kind: crate::templates::TargetKind,
    cursor: Option<crate::visuals::RadiusCursor>,
    look: CommandCursor,
    relation: crate::defeat::ObjectFilter,
}

#[derive(Debug, Clone, Default)]
struct CommandCursor {
    valid: Option<String>,
    invalid: Option<String>,
}

impl CommandCursor {
    fn of(buttons: &mint_sage_ini::CommandButtonIndex, button: &str) -> Self {
        buttons.get(button).map_or_else(Self::default, |def| Self {
            valid: def.cursor_name.clone(),
            invalid: def.invalid_cursor_name.clone(),
        })
    }

    fn want(&self, context: bool, valid: bool, decal_shown: bool) -> crate::cursors::Want {
        if decal_shown {
            crate::cursors::Want::Hidden
        } else if !context || valid {
            crate::cursors::Want::Command(self.valid.clone())
        } else {
            crate::cursors::Want::Command(self.invalid.clone())
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TargetedOrder {
    Power(u8),
    Spell(u8),
    Weapon(u8),
    Sacrifice,
}

impl PowerTarget {
    fn power_button(&self) -> Option<crate::rules::PowerButton> {
        match self.order {
            TargetedOrder::Power(button) => Some(crate::rules::PowerButton::Selection(button)),
            TargetedOrder::Spell(button) => Some(crate::rules::PowerButton::Spell(button)),
            TargetedOrder::Weapon(_) | TargetedOrder::Sacrifice => None,
        }
    }

    fn command(
        &self,
        target: Option<mint_sim::Entity>,
        at: Option<(f32, f32)>,
    ) -> Option<RotwkCommand> {
        Some(match self.order {
            TargetedOrder::Spell(button) => RotwkCommand::CastSpell { button, target, at },
            TargetedOrder::Power(button) => RotwkCommand::FireSpecialPower { button, target, at },
            TargetedOrder::Weapon(slot) => {
                let (x, y) = at?;
                RotwkCommand::AttackGround { slot, x, y }
            }
            TargetedOrder::Sacrifice => RotwkCommand::Sacrifice { target: target? },
        })
    }
}

#[allow(dead_code)]
pub(crate) struct MatchRemains {
    units: UnitScene,
    data: GameData,
    scene: MatchScene,
    kinds: Arc<KindTable>,
    unit_art: Vec<KindArt>,
    projectile_art: ProjectileArt,
    visuals: UnitVisuals,
}

impl MatchRemains {
    pub(crate) fn free_elsewhere(self) -> Option<std::thread::JoinHandle<()>> {
        std::thread::Builder::new()
            .name("match teardown".to_owned())
            .spawn(move || {
                let began = std::time::Instant::now();
                drop(self);
                println!(
                    "teardown: the last match's art and data freed in {:.0} ms",
                    began.elapsed().as_secs_f64() * 1000.0
                );
            })
            .ok()
    }
}

pub(crate) struct Game {
    mode: Mode,
    state: MatchState,
    viewport: (u32, u32),
    pointer: Pointer,
    last_pointer: Option<(f32, f32)>,
    pointer_hover_target: Option<String>,
    box_mode: BoxMode,
    preview_decals: crate::rules::PreviewDecals,
    modes: Modes,
    data: GameData,
    config: MatchConfig,
    shell_game: Option<crate::shell_game::ShellGameKind>,
    obstacle_overlay: bool,
    placement: Option<Placement>,
    span: Option<u8>,
    power_target: Option<PowerTarget>,
    last_blend: f32,
    passability_sources: PassabilitySources,
    audio: crate::audio::GameAudio,
    fade_trace: crate::render_state::FadeTrace,
    pan_forward: bool,
    pan_back: bool,
    pan_left: bool,
    pan_right: bool,
    ctrl_held: bool,
    shift_held: bool,
    order_mode_key: Option<mint_app::Key>,
    view_last_radar_event_key: Option<mint_app::Key>,
    diplomacy_key: Option<crate::game_data::KeyBinding>,
    spell_store_key: Option<crate::game_data::KeyBinding>,
    order_mode_waypoint: bool,
    attack_move_armed: bool,
    script_input_seen: u32,
    preferences: crate::options::OptionsPreferences,
    radar_events: crate::radar_events::RadarEventRing,
    report: SecondReport,
    mode_frames: FrameTimes,
    apt_overlay: Option<AptOverlay>,
    overlay_enabled: bool,
    ui_router: InputRouter,
    wants_shell: bool,
    cover_alpha: f32,
    forced_end: Option<mint_sage_campaign::MissionEnd>,
    wants_score: bool,
    quit_local: bool,
    wants_restart: bool,
    load_request: Option<std::path::PathBuf>,
    menu_was_open: bool,
    end_game: crate::end_game::EndGame,
    hero_stats: Option<crate::hero_stats::HeroMatchStats>,
    victory_screen_seen: u32,
    screen_shake_seen: u32,
    control: ControlSet,
    acting_player: PlayerId,
    test_controls: crate::test_controls::TestControls,
    builder_cycle: crate::hud::BuilderCycle,
    ui_clock: Duration,
    last_hero_press: Option<(crate::hud::HeroButton, Duration)>,
    actions: u64,
    pointer_over_ui: bool,
    move_hint_model: Option<Arc<mint_sage_model::CachedModel>>,
    script_anchor: Vec2,
    state_trace: Option<(u16, std::collections::BTreeMap<u32, String>)>,
    failure: Option<crate::crash::Failure>,
    tick_failed: bool,
    forced_crash: Option<crash_report::ForcedCrash>,
    live_objects: usize,
}

pub(crate) struct GameBuild {
    scene: MatchScene,
    data: GameData,
    viewport: (u32, u32),
    vfs: Arc<Vfs>,
    passability_sources: PassabilitySources,
    config: MatchConfig,
    install_roots: Vec<PathBuf>,
    control: ControlSet,
    setup: MatchSetup,
    shroud_view: Option<ShroudView>,
    visuals: UnitVisuals,
    art: UnitArtLoad,
    ui: Option<crate::ui_resources::UiResources>,
}

impl GameBuild {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin(
        renderer: &mut Renderer,
        mut scene: MatchScene,
        data: GameData,
        viewport: (u32, u32),
        vfs: Arc<Vfs>,
        passability_sources: PassabilitySources,
        config: MatchConfig,
        install_roots: Vec<PathBuf>,
        control: ControlSet,
        ui: Option<crate::ui_resources::UiResources>,
    ) -> Result<Box<Self>, String> {
        let setup = build_match(&scene, &data, &vfs, &config, passability_sources)?;
        let shroud_view = if config.shell_game_kind().is_some() {
            scene.terrain_shading.modulation.shroud = TerrainShroud::Revealed;
            None
        } else {
            attach_shroud(renderer, &mut scene, &setup.simulation)
        };
        let visuals = UnitVisuals::upload(renderer)?;
        let art = UnitArtLoad::begin(
            &vfs,
            &scene,
            setup.kinds.names(),
            setup.kinds.infantry(),
            setup.kinds.move_only(),
            &setup.colours,
        );
        Ok(Box::new(Self {
            scene,
            data,
            viewport,
            vfs,
            passability_sources,
            config,
            install_roots,
            control,
            setup,
            shroud_view,
            visuals,
            art,
            ui,
        }))
    }

    pub(crate) fn art_progress(&self) -> (usize, usize) {
        self.art.progress()
    }

    pub(crate) fn art_done(&self) -> bool {
        self.art.is_done()
    }

    pub(crate) fn step_art(&mut self, renderer: &mut Renderer) {
        self.art.step(renderer, &self.vfs, &self.scene);
    }

    pub(crate) fn state_hash(&self) -> u64 {
        self.setup.simulation.state_hash()
    }

    pub(crate) fn finish(self: Box<Self>, renderer: &mut Renderer) -> Result<Box<Game>, String> {
        Game::from_build(renderer, *self)
    }
}

impl Game {
    fn from_build(renderer: &mut Renderer, build: GameBuild) -> Result<Box<Self>, String> {
        let GameBuild {
            scene,
            data,
            viewport,
            vfs,
            passability_sources,
            config,
            install_roots,
            control,
            setup,
            shroud_view,
            visuals,
            art,
            ui,
        } = build;
        let simulation = setup.simulation;
        let squad_centre = setup.focus;
        let names = setup.kinds.names();
        let order_mode_key = data.order_mode_key;
        let view_last_radar_event_key = data.view_last_radar_event_key;
        let diplomacy_key = data.diplomacy_key;
        let spell_store_key = data.spell_store_key;
        let charge_default = data.default_voice_attack_charge_timeout_ms;
        let charge_timeouts: Vec<u64> = setup
            .kinds
            .iter()
            .map(|entry| {
                let ms = entry
                    .record
                    .voice_attack_charge_timeout_ms
                    .or(charge_default)
                    .filter(|ms| ms.is_finite() && *ms > 0.0)
                    .unwrap_or(0.0);
                u64::from(crate::game_data::retail_duration(ms).get())
            })
            .collect();
        let (mut units, unit_art, player_colors) = art.finish();
        attach_frame_events(&mut units, &unit_art, simulation.rules());
        let projectile_art = load_projectile_art(
            renderer,
            &vfs,
            &scene,
            &mut units,
            &simulation.rules().projectile_templates(),
        );
        units.release_load_caches();
        let mut camera = RtsCamera::new(Vec3::Z);
        frame_camera(&mut camera, &scene);
        apply_tactical_lens(&mut camera, &data.camera, viewport);
        if config.shell_game_kind().is_none() {
            focus_on(&mut camera, squad_centre, opening_distance(&data.camera));
        }
        let mode = [Event::Booted, Event::MapChosen, Event::SceneReady]
            .into_iter()
            .try_fold(Mode::Boot, transition)
            .unwrap_or(Mode::Match);
        println!(
            "hotkeys: F1 search, F2 smoothing, F3 avoidance, f/g/d stance \
             Battle/Hold Ground/Aggressive (retail's own letters), \
             F4 interpolation (unit position, object animation/UV mapper, \
             bone-attached particles, particle stepping), \
             m whatever letter a selection's command bar claims - on a horde, \
             its own HORDE_TOGGLE_FORMATION button, \
             F12 box-select rule, \
             F5 shroud draw, F6 shroud viewer, \
             1-9 queue button N at a selected structure, or (on a \
             selected porter) enter placement for its Nth DOZER_CONSTRUCT \
             button - left click anchors the pending site, drag rotates it, \
             release places it, right click or Escape cancels it; Ctrl+Z \
             cancels a selected queue entry or an unstarted selected site, \
             F9 obstacle overlay (amber static, red dynamic), \
             F10 audio rolloff curve, \
             F11 water reflection (mirror-retail/mirror-low/mirror), \
             Shift+F11 mirror skip on/off, \
             \\ or # waves, ' health bar fill, [ particle flipbook, \
             . animation blending, , spell and weapon effects, Ctrl+O distance fog, \
             j passability sources (restarts the match); \
             a arm attack-move, s stop (retail's own letters); \
             space APT overlay (InGameSideCommandBar/Palantir) on/off; \
             - window fit: stretch/anchored; \
             left click/drag \
             selects, right click orders - on an enemy it attacks, on ground \
             it moves - right drag scrolls the map, middle drag rotates the \
             camera, a middle click resets it, left+right chorded within 5 px \
             and 500 ms drags a formation order; test only: Enter acts as \
             the next --control player, > levels up the selection"
        );
        println!(
            "{}",
            crate::fx_present::census_line(
                &scene.particle_scene,
                simulation.rules(),
                crate::fx_present::FxShown::default()
            )
        );
        let mut audio = crate::audio::GameAudio::open(
            Arc::clone(&vfs),
            scene.object_index.as_ref(),
            names,
            simulation.rules(),
            &data.macros,
        );
        let options_defaults = crate::options::OptionsPreferences {
            health_bars: crate::user_options::UserOptions::load().all_health_bars(),
            ..crate::options::OptionsPreferences::from_audio_settings(&audio.settings())
        };
        if let Some(index) = scene.object_index.as_ref() {
            let placed: Vec<_> = scene
                .objects
                .iter()
                .enumerate()
                .filter(|(at, _)| scene.proxy_of_object.get(*at).copied().flatten().is_some())
                .filter_map(|(at, object)| {
                    let template = index
                        .sound(&object.template_name, crate::ambient::SOUND_AMBIENT)
                        .filter(|event| crate::audio::names_a_sound(event));
                    let placement =
                        crate::ambient::placed(&object.properties, template, |e, o| {
                            audio.event_facts(e, o)
                        })?;
                    let z = ground_at(&scene, object.x, object.y) + object.z;
                    Some((u32::try_from(at).ok()?, [object.x, object.y, z], placement))
                })
                .collect();
            audio.place_ambients(placed);
            let placements = simulation.rules().map_placements(simulation.state());
            let objects = placements.len();
            let custom: Vec<_> = placements
                .into_iter()
                .filter_map(|(entity, at)| {
                    let object = scene.objects.get(usize::try_from(at).ok()?)?;
                    let template = index
                        .sound(&object.template_name, crate::ambient::SOUND_AMBIENT)
                        .filter(|event| crate::audio::names_a_sound(event));
                    let custom =
                        crate::ambient::customised(&object.properties, template, |e, o| {
                            audio.event_facts(e, o)
                        })?;
                    Some((entity, custom))
                })
                .collect();
            audio.bind_custom_ambients(custom, objects);
        }
        audio.set_charge_timeouts(charge_timeouts);
        let ui = ui.unwrap_or_else(|| crate::ui_resources::UiResources::load(&install_roots));
        println!(
            "strings: {} localisation label(s) from {} .str file(s) ({} failed)",
            ui.strings.len(),
            ui.string_files,
            ui.string_failures,
        );
        let fonts = Arc::clone(&ui.fonts);
        let strings = Arc::clone(&ui.strings);
        let shell_game = config.shell_game_kind();
        #[allow(clippy::cast_precision_loss)]
        let apt_overlay = match AptOverlay::load(
            &ui,
            &install_roots,
            (viewport.0 as f32, viewport.1 as f32),
            renderer,
            Arc::clone(&vfs),
            Arc::clone(&data.mapped_images),
            Arc::clone(&data.command_buttons),
        ) {
            Ok(mut overlay) => {
                overlay.set_options_defaults(options_defaults);
                overlay.set_load_enabled(matches!(config, MatchConfig::Campaign(_)));
                overlay.set_campaign(matches!(config, MatchConfig::Campaign(_)));
                overlay.set_skirmish_or_multiplayer(config.game_mode());
                Some(overlay)
            }
            Err(error) => {
                println!(
                    "apt overlay: {error} - running without the in-game command bar/palantir \
                     overlay (space would toggle it if it had loaded)"
                );
                None
            }
        }
        .filter(|_| shell_game.is_none());
        let mut move_hint_cache = mint_sage_model::ModelCache::new();
        let move_hint_model = (!data.move_hint_name.is_empty()).then(|| {
            move_hint_cache.get_or_load(renderer, &vfs, &scene.resolver, &data.move_hint_name)
        });
        let move_hint_model = move_hint_model.flatten();
        if move_hint_model.is_none() {
            if let Some((name, reason)) = move_hint_cache.failures().first() {
                println!(
                    "move hint: {name} did not load ({reason}) - the placeholder ring stands in"
                );
            }
        }
        let placed_scales = placed_scales(&simulation, &scene.objects);
        let hero_stats = data.cah_system.clone().and_then(|sys| {
            crate::hero_stats::HeroMatchStats::open(
                sys,
                Arc::clone(&setup.kinds),
                simulation.rules(),
                LOCAL_PLAYER,
                config.game_mode(),
                crate::setup_heroes::own_dir(),
            )
        });
        Ok(Box::new(Self {
            mode,
            state: MatchState {
                view_box_z: crate::apt_overlay::average_ground_z(
                    &scene.height_map,
                    scene.terrain.vertical_scale,
                ),
                scene,
                camera,
                simulation,
                visuals,
                units,
                unit_art,
                placed_scales,
                projectile_art,
                player_colors,
                vfs,
                goal: None,
                stance_flyout: None,
                ui_panels_sent: [false; 3],
                scroll_pending: 0.0,
                script_camera_clock: (0, 0.0),
                previous: PoseTrack::new(),
                snapshot: crate::snapshot::FrameSnapshot::default(),
                shroud_view,
                shroud_viewer: LOCAL_PLAYER,
                damage_thresholds: data.damage_thresholds,
                tdfx: crate::tdfx::TdfxPresenter::default(),
                shroud_fade: ShroudFade::default(),
                shroud_clock: None,
                shroud_source: None,
                kinds: Arc::clone(&setup.kinds),
                local_evil: setup.local_evil,
                local_faction: setup.local_faction.clone(),
                local_heroes: setup.local_heroes.clone(),
                local_side: setup.local_side.clone(),
                local_template: setup.local_template.clone(),
                cinematic: crate::cinematic::Cinematic::default(),
                fx: crate::fx_present::FxPresenter::default(),
                bone_fx: BoneFxTracker::new(),
                level_ups: crate::level_up::LevelWatch::default(),
                selection_flash: crate::selection_flash::SelectionFlash::new(
                    data.selection_flash_saturation,
                    data.selection_flash_house_color,
                ),
                floating: crate::floating_text::FloatingTexts::default(),
                caption_points: data.drawable_caption_points,
                named_timer_style: data.named_timer.clone(),
                downwind_angle: data.downwind_angle,
                fonts,
                strings,
            },
            viewport,
            pointer: Pointer::default(),
            last_pointer: None,
            pointer_hover_target: None,
            box_mode: BoxMode::default(),
            preview_decals: crate::rules::PreviewDecals::default(),
            modes: Modes::default(),
            data,
            config,
            shell_game,
            passability_sources,
            obstacle_overlay: false,
            placement: None,
            span: None,
            power_target: None,
            last_blend: 0.0,
            audio,
            fade_trace: crate::render_state::FadeTrace::default(),
            pan_forward: false,
            pan_back: false,
            pan_left: false,
            pan_right: false,
            ctrl_held: false,
            shift_held: false,
            order_mode_key,
            view_last_radar_event_key,
            diplomacy_key,
            spell_store_key,
            order_mode_waypoint: false,
            attack_move_armed: false,
            script_input_seen: 0,
            preferences: options_defaults,
            radar_events: crate::radar_events::RadarEventRing::default(),
            report: SecondReport::default(),
            mode_frames: FrameTimes::default(),
            apt_overlay,
            overlay_enabled: shell_game.is_none(),
            ui_router: InputRouter::new(),
            wants_shell: false,
            cover_alpha: 0.0,
            forced_end: None,
            wants_score: false,
            quit_local: false,
            wants_restart: false,
            load_request: None,
            menu_was_open: false,
            end_game: crate::end_game::EndGame::default(),
            hero_stats,
            victory_screen_seen: 0,
            screen_shake_seen: 0,
            control,
            acting_player: LOCAL_PLAYER,
            test_controls: crate::test_controls::TestControls::default(),
            builder_cycle: crate::hud::BuilderCycle::default(),
            ui_clock: Duration::ZERO,
            last_hero_press: None,
            actions: 0,
            pointer_over_ui: false,
            move_hint_model,
            script_anchor: Vec2::ZERO,
            state_trace: None,
            failure: None,
            tick_failed: false,
            forced_crash: None,
            live_objects: 0,
        }))
    }

    pub(crate) fn take_failure(&mut self) -> Option<crate::crash::Failure> {
        self.failure.take()
    }

    pub(crate) fn set_cover(&mut self, alpha: f32) {
        self.cover_alpha = alpha;
    }

    pub(crate) fn set_audio_pool_gain(&self, gain: f32) {
        self.audio.set_pool_gain(gain);
    }

    pub(crate) fn sim_frame(&self) -> u64 {
        self.state.simulation.frame().get()
    }

    pub(crate) fn state_hash(&self) -> u64 {
        self.state.simulation.state_hash()
    }

    pub(crate) fn tick_hashes(&self) -> &[u64] {
        self.state.simulation.recording().hashes()
    }

    pub(crate) fn mission_end(&self) -> Option<mint_sage_campaign::MissionEnd> {
        self.forced_end.or(self.state.scene.script_view.mission_end)
    }

    pub(crate) fn match_result(&self) -> crate::match_values::MatchResult {
        let simulation = &self.state.simulation;
        let mut result = simulation.rules().match_result(simulation.state());
        result.end = self.mission_end();
        result
    }

    pub(crate) fn wotr_battle_result(
        &self,
        spec: &mint_sage_livingworld::BattleSpec,
        strategic: &mint_sage_livingworld::Rules,
    ) -> mint_sage_livingworld::BattleResult {
        let simulation = &self.state.simulation;
        let rules = simulation.rules();
        let world = simulation.state();
        let standing = |strategic_player: mint_sage_livingworld::PlayerId| {
            spec.participants
                .iter()
                .position(|p| p.player == strategic_player)
                .and_then(|index| u8::try_from(index).ok())
                .is_some_and(|index| !rules.defeated(world, PlayerId::new(index)))
        };
        let end = match self.forced_end {
            Some(mint_sage_campaign::MissionEnd::Victory) => crate::wotr_battle::LocalEnd::Won,
            Some(mint_sage_campaign::MissionEnd::Defeat) => crate::wotr_battle::LocalEnd::Lost,
            None if self.quit_local => crate::wotr_battle::LocalEnd::Lost,
            None => crate::wotr_battle::LocalEnd::ByStanding,
        };
        let winner = crate::wotr_battle::winning_side(spec, end, &standing);
        crate::wotr_battle::battle_result(rules, world, spec, strategic, winner)
    }

    pub(crate) fn wants_shell(&self) -> bool {
        self.wants_shell
    }

    pub(crate) fn into_remains(self: Box<Self>) -> MatchRemains {
        let Game { state, data, .. } = *self;
        let MatchState {
            scene,
            units,
            unit_art,
            projectile_art,
            visuals,
            kinds,
            ..
        } = state;
        MatchRemains {
            units,
            data,
            scene,
            kinds,
            unit_art,
            projectile_art,
            visuals,
        }
    }

    pub(crate) fn score_figures(&self) -> Option<Vec<crate::match_stats::PlayerFigures>> {
        if !self.wants_score {
            return None;
        }
        let simulation = &self.state.simulation;
        let ids = simulation.rules().ids()?;
        let now = u32::try_from(simulation.frame().get()).unwrap_or(u32::MAX);
        let quit = self.quit_local.then_some(LOCAL_PLAYER);
        Some(
            simulation
                .rules()
                .match_figures(simulation.state(), ids, now, quit),
        )
    }

    pub(crate) fn take_load_request(&mut self) -> Option<std::path::PathBuf> {
        self.load_request.take()
    }

    pub(crate) fn wants_restart(&self) -> bool {
        self.wants_restart
    }

    pub(crate) fn viewport(&self) -> (u32, u32) {
        self.viewport
    }

    fn print_state_trace(&mut self) {
        let Some((kind, last)) = self.state_trace.as_mut() else {
            return;
        };
        let Some(ids) = self.state.simulation.rules().ids() else {
            return;
        };
        let world = self.state.simulation.state();
        let tick = self.state.simulation.frame().get();
        let Some(kinds) = world.store(ids.kind) else {
            return;
        };
        let mut seen = Vec::new();
        for (entity, of) in kinds.iter() {
            if of.0 != *kind {
                continue;
            }
            let Some(line) = self.state.units.state_line(entity.index()) else {
                continue;
            };
            seen.push(entity.index());
            let health = world
                .get(ids.health, entity)
                .map_or(String::from("-"), |h| {
                    format!("{:.0}%", h.0.fraction() * 100.0)
                });
            let collapse = self.state.simulation.rules().collapse_view(world, entity);
            let key = format!(
                "health {health}{}; {line}",
                collapse.map_or(String::new(), |(state, _, _)| format!(
                    ", collapse {state:?}"
                ))
            );
            if last.get(&entity.index()) != Some(&key) {
                let sunk =
                    collapse.map_or(String::new(), |(_, sink, _)| format!(" (sunk {sink:.1})"));
                println!("trace: tick {tick} entity {}: {key}{sunk}", entity.index());
                last.insert(entity.index(), key);
            }
        }
        last.retain(|entity, _| {
            let live = seen.contains(entity);
            if !live {
                println!("trace: tick {tick} entity {entity}: removed");
            }
            live
        });
    }

    fn pointer_target_description(&self, x: f32, y: f32) -> String {
        if let Some(overlay) = self.apt_overlay.as_ref() {
            if let Some((movie, path)) = overlay.native_hover_target() {
                return format!("movie={movie} path={path}");
            }
        }
        if let Some(widget) = self.ui_router.hovered() {
            return format!("ui widget={widget:?}");
        }
        let Some(ray) = self.ray(x, y) else {
            return "none".to_owned();
        };
        let units = self.units();
        if let Some(entity) = pick_unit(&units, ray, self.acting_player, self.declared()) {
            let name = units
                .iter()
                .find(|unit| unit.entity == entity)
                .and_then(|unit| unit.kind)
                .and_then(|kind| self.state.kinds.get(crate::unit::Kind(kind)))
                .map_or("?", |entry| entry.record.name.as_str());
            return format!("entity={} template={name}", entity.index());
        }
        if let Some(point) = mint_sage_terrain::pick_surface(
            &self.state.scene.height_map,
            self.state.scene.terrain.vertical_scale,
            ray,
        ) {
            return format!("terrain=({:.1},{:.1})", point.x, point.y);
        }
        "none".to_owned()
    }

    fn capture_snapshot(&self) {
        let rules = self.state.simulation.rules();
        let world = self.state.simulation.state();
        for unit in self.units().iter().filter(|unit| unit.selected) {
            let entry = unit
                .kind
                .and_then(|kind| self.state.kinds.get(crate::unit::Kind(kind)));
            let template = entry.map_or("?", |entry| entry.record.name.as_str());
            let health = match (unit.health, entry.and_then(|entry| entry.record.max_health)) {
                (Some(fraction), Some(max)) => format!("{:.0}/{max:.0}", fraction * max),
                (Some(fraction), None) => format!("{:.2} of max", fraction),
                (None, _) => "none".to_owned(),
            };
            let weapon_slot = rules
                .ids()
                .and_then(|ids| world.get(ids.weapon_slots, unit.entity))
                .map_or(0, |slots| slots.current);
            let pose = self.state.units.drawable_by_id(unit.entity.index());
            let state = pose.and_then(mint_sage_scene::UnitDrawable::state);
            let phase = pose.and_then(mint_sage_scene::UnitDrawable::phase);
            let frame = self.state.units.clip_frame(unit.entity.index());
            crate::capture::snapshot_line(&format!(
                "snapshot: entity={} template={template} pos=({:.1},{:.1}) health={health} \
                 owner={} conditions={:?} state={state:?} phase={phase:?} frame={frame:?} \
                 weapon_slot={weapon_slot}",
                unit.entity.index(),
                unit.position.x,
                unit.position.y,
                unit.owner.get(),
                unit.conditions,
            ));
        }
    }

    fn ensure_snapshot(&mut self) {
        crate::snapshot::ensure(
            &mut self.state.snapshot,
            &self.state.simulation,
            &self.state.previous,
            self.state.shroud_viewer,
            self.acting_player,
        );
    }

    fn snapshot(&self) -> std::borrow::Cow<'_, crate::snapshot::FrameSnapshot> {
        let state = &self.state;
        if state.snapshot.is_for(
            state.simulation.frame(),
            state.shroud_viewer,
            self.acting_player,
        ) {
            return std::borrow::Cow::Borrowed(&state.snapshot);
        }
        crate::snapshot::note_fallback();
        std::borrow::Cow::Owned(crate::snapshot::FrameSnapshot::build(
            &state.simulation,
            &state.previous,
            state.shroud_viewer,
            self.acting_player,
        ))
    }

    fn units(&self) -> Vec<UnitView> {
        let snapshot = self.snapshot();
        let mut units = snapshot.units(Some(self.last_blend));
        crate::ghosts::drawn_visibility(
            &mut units,
            self.state.scene.terrain_shading.modulation.shroud == TerrainShroud::On,
            &|x, y| snapshot.playable(x, y),
        );
        drop_undrawn(&mut units, &self.state.unit_art);
        let drawing_nothing = self.state.units.ids_drawing_nothing();
        for unit in &mut units {
            if drawing_nothing.binary_search(&unit.entity.index()).is_ok() {
                unit.pick_type = mint_sage_asset::PickType::None;
            }
        }
        units
    }

    fn declared<'s>(&'s self) -> impl Fn(&UnitView) -> Declared<'s> + 's {
        let kinds = &self.state.kinds;
        let snapshot = &self.state.snapshot;
        let current = snapshot.is_for(
            self.state.simulation.frame(),
            self.state.shroud_viewer,
            self.acting_player,
        );
        let world = self.state.simulation.state();
        let upgrades = self.state.simulation.rules().upgrade_state_id();
        let ids = self.state.simulation.rules().ids();
        move |unit: &UnitView| {
            let (geometry, overrides) = if current {
                (
                    snapshot
                        .facts(unit.entity)
                        .and_then(|facts| facts.geometry_kind),
                    snapshot.geometry_overrides(unit.entity),
                )
            } else {
                (
                    ids.and_then(|ids| RotwkRules::geometry_kind(world, ids, unit.entity)),
                    upgrades
                        .and_then(|id| world.get(id, unit.entity))
                        .map_or(&[][..], |state| state.geometry.as_slice()),
                )
            };
            Declared {
                geometry: geometry
                    .or(unit.kind.map(Kind))
                    .and_then(|kind| kinds.get(kind))
                    .map(|entry| &entry.record.geometry),
                overrides,
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn pick_at(&self, x: f32, y: f32) -> Option<mint_sim::Entity> {
        let ray = self.ray(x, y)?;
        pick_unit(&self.units(), ray, self.acting_player, self.declared())
    }

    #[allow(dead_code)]
    pub(crate) fn camera_mut(&mut self) -> &mut RtsCamera {
        &mut self.state.camera
    }

    pub(crate) fn camera_target(&self) -> Vec3 {
        self.state.camera.camera().target
    }

    pub(crate) fn script_count(&self) -> usize {
        self.state
            .scene
            .map_scripts
            .as_ref()
            .map_or(0, crate::rules::map_scripts::MapScriptEngine::script_count)
    }

    pub(crate) fn object_count(&self) -> usize {
        self.state.simulation.state().iter_entities().count()
    }

    fn ray(&self, x: f32, y: f32) -> Option<Ray> {
        self.state
            .camera
            .camera()
            .ray_through_pixel(self.viewport, x, y)
    }

    fn world_hover(
        &self,
        units: &[UnitView],
    ) -> Option<(mint_sim::Entity, crate::help_box::ObjectHelp, Option<f32>)> {
        let (x, y) = self.last_pointer?;
        let ray = self.ray(x, y)?;
        let entity = pick_unit(units, ray, self.acting_player, self.declared())?;
        self.object_hover(units, entity)
    }

    fn object_hover(
        &self,
        units: &[UnitView],
        entity: mint_sim::Entity,
    ) -> Option<(mint_sim::Entity, crate::help_box::ObjectHelp, Option<f32>)> {
        let unit = units.iter().find(|u| u.entity == entity)?;
        let record = unit.kind.and_then(|k| self.state.kinds.get(Kind(k)));
        let hover_record = unit.hover_kind.and_then(|k| self.state.kinds.get(Kind(k)));
        let help = crate::help_box::ObjectHelp {
            display_name: hover_record.and_then(|e| e.record.display_name.clone()),
            literal_name: self
                .state
                .simulation
                .rules()
                .cah_display_name(self.state.simulation.state(), entity),
            description: hover_record.and_then(|e| e.record.description.clone()),
        };
        let max_health = record.and_then(|e| e.record.max_health);
        Some((entity, help, max_health))
    }

    pub(crate) fn cursor_want(&self) -> crate::cursors::Want {
        use crate::cursors::{CursorKind, Want};
        if self.mode != Mode::Match {
            return Want::Show(CursorKind::Arrow);
        }
        let (forward, right) = self.pan_axes();
        if forward != 0.0 || right != 0.0 {
            return Want::Scroll(right, -forward);
        }
        if self.placement.is_some() {
            return Want::Show(CursorKind::Arrow);
        }
        if self.pointer.drag_rect().is_some() {
            return Want::Keep;
        }
        if self.pointer_over_ui {
            return Want::Show(CursorKind::Arrow);
        }
        let Some(ray) = self.pointer.position().and_then(|(x, y)| self.ray(x, y)) else {
            return Want::Keep;
        };
        if let Some(option) = self.span {
            let legal = mint_sage_terrain::pick_surface(
                &self.state.scene.height_map,
                self.state.scene.terrain.vertical_scale,
                ray,
            )
            .is_some_and(|point| {
                let rules = self.state.simulation.rules();
                rules.ids().is_some_and(|ids| {
                    rules
                        .span_plan(
                            self.state.simulation.state(),
                            ids,
                            self.acting_player,
                            option,
                            point.x,
                            point.y,
                        )
                        .is_some()
                })
            });
            return Want::Show(if legal {
                CursorKind::Move
            } else {
                CursorKind::GenericInvalid
            });
        }
        if let Some(power_target) = &self.power_target {
            let valid = self.power_target_order(power_target, ray).is_ok();
            return power_target
                .look
                .want(true, valid, power_target.cursor.is_some());
        }
        if self.attack_move_armed {
            return attack_move_cursor(&self.data.command_buttons).want(false, true, false);
        }
        let units = self.units();
        let me = self.acting_player;
        let own_under = pick_unit(&units, ray, me, self.declared())
            .and_then(|entity| units.iter().find(|unit| unit.entity == entity))
            .is_some_and(|unit| unit.owner == me);
        let mut selected = units.iter().filter(|unit| unit.selected);
        let Some(first) = selected.next() else {
            return Want::Show(if own_under {
                CursorKind::Select
            } else {
                CursorKind::Arrow
            });
        };
        let single = selected.next().is_none();
        if single && first.owner != me {
            return Want::Show(CursorKind::Arrow);
        }
        if units.iter().any(|unit| {
            unit.selected && unit.owner == me && unit.kind_of.contains(KindOf::AUTO_RALLYPOINT)
        }) {
            let any_under = pick_unit(&units, ray, me, self.declared()).is_some();
            return Want::Show(if any_under {
                CursorKind::Select
            } else {
                CursorKind::SetRallyPoint
            });
        }
        let rules = self.state.simulation.rules();
        if let Some(target) = self.capture_under(&units, ray) {
            return self
                .capture_cursor(target)
                .map_or(Want::Show(CursorKind::AttackObject), Want::Named);
        }
        if self.ring_delivery_target(&units, ray).is_some() {
            return Want::Named(crate::cursors::DELIVER_RING_CURSOR.to_owned());
        }
        if self.container_under(&units, ray).is_some() {
            return Want::Named(crate::cursors::ENTER_CURSOR.to_owned());
        }
        if pick_target(&units, ray, self.declared(), |unit| {
            rules
                .view_relation(self.state.simulation.state(), me, unit.entity)
                .victim()
        })
        .is_some()
        {
            return Want::Show(CursorKind::AttackObject);
        }
        if own_under {
            return Want::Show(CursorKind::Select);
        }
        let Some(ground) = mint_sage_terrain::pick_surface(
            &self.state.scene.height_map,
            self.state.scene.terrain.vertical_scale,
            ray,
        ) else {
            return Want::Keep;
        };
        if single
            && first.kind_of.contains(KindOf::STRUCTURE)
            && pick_unit(&units, ray, me, self.declared()).is_none()
        {
            return Want::Show(CursorKind::GenericInvalid);
        }
        if !rules.selection_reaches(
            self.state.simulation.state(),
            me,
            mint_movement::Vec2::new(ground.x, ground.y),
        ) {
            return Want::Show(CursorKind::GenericInvalid);
        }
        Want::Show(CursorKind::Move)
    }

    fn power_target_order(
        &self,
        power_target: &PowerTarget,
        ray: Ray,
    ) -> Result<RotwkCommand, &'static str> {
        use crate::templates::TargetKind;
        let (target, at) = match power_target.kind {
            TargetKind::Confirm => (None, None),
            TargetKind::Object => {
                let units = self.units();
                match pick_unit(&units, ray, self.acting_player, self.declared()) {
                    Some(picked) => (Some(picked), None),
                    None => (None, Some(self.map_tree_click(ray)?)),
                }
            }
            TargetKind::Position | TargetKind::None => {
                let point = mint_sage_terrain::pick_surface(
                    &self.state.scene.height_map,
                    self.state.scene.terrain.vertical_scale,
                    ray,
                )
                .ok_or("no ground under the pointer")?;
                (None, Some((point.x, point.y)))
            }
        };
        self.power_target_resolve(power_target, target, at)
    }

    fn map_tree_click(&self, ray: Ray) -> Result<(f32, f32), &'static str> {
        const NO_UNIT: &str = "no unit under the pointer";
        let point = mint_sage_terrain::pick_surface(
            &self.state.scene.height_map,
            self.state.scene.terrain.vertical_scale,
            ray,
        )
        .ok_or(NO_UNIT)?;
        let world = self.state.simulation.state();
        let ids = self.state.simulation.rules().ids().ok_or(NO_UNIT)?;
        let (tree, _) =
            RotwkRules::map_tree_at(world, ids, mint_movement::Vec2::new(point.x, point.y))
                .ok_or(NO_UNIT)?;
        let revealed =
            self.state.scene.terrain_shading.modulation.shroud == TerrainShroud::Revealed;
        let shrouded = !revealed
            && crate::shroud::with_grid(world, RotwkRules::shroud_ids(ids), |grid| {
                let half = grid.extent().cell_size * 0.5;
                let (x, y) = grid.world_to_cell(tree.x - half, tree.y - half);
                [(0, 0), (1, 0), (0, 1), (1, 1)].iter().all(|(dx, dy)| {
                    grid.cell_state(self.acting_player, x + dx, y + dy)
                        == mint_sage_shroud::CellVisibility::Unexplored
                })
            })
            .unwrap_or(false);
        if shrouded {
            return Err(NO_UNIT);
        }
        Ok((point.x, point.y))
    }

    fn power_target_resolve(
        &self,
        power_target: &PowerTarget,
        target: Option<mint_sim::Entity>,
        at: Option<(f32, f32)>,
    ) -> Result<RotwkCommand, &'static str> {
        if let Some(button) = power_target.power_button() {
            if let Some(refusal) = self.state.simulation.rules().power_target_refusal(
                self.state.simulation.state(),
                self.acting_player,
                button,
                &power_target.relation,
                target,
                at,
            ) {
                return Err(refusal);
            }
        }
        power_target
            .command(target, at)
            .ok_or("no ground under the pointer")
    }

    fn power_target_order_at(
        &self,
        power_target: &PowerTarget,
        at: (f32, f32),
    ) -> Result<RotwkCommand, &'static str> {
        self.power_target_resolve(power_target, None, Some(at))
    }

    fn radar_armed(&self) -> RadarArmed {
        if let Some(power_target) = &self.power_target {
            if power_target.kind == crate::templates::TargetKind::Position {
                return RadarArmed::Cast;
            }
        }
        if self.attack_move_armed {
            return RadarArmed::AttackMove;
        }
        RadarArmed::None
    }

    fn capture_under(&self, units: &[UnitView], ray: Ray) -> Option<mint_sim::Entity> {
        let (rules, me) = (self.state.simulation.rules(), self.acting_player);
        let world = self.state.simulation.state();
        if !rules.selection_captures(world, me) {
            return None;
        }
        crate::pick::pick_capturable(units, ray, self.declared(), |entity| {
            rules.capture_target(world, me, entity).is_ok()
        })
    }

    fn container_under(&self, units: &[UnitView], ray: Ray) -> Option<mint_sim::Entity> {
        let (rules, me) = (self.state.simulation.rules(), self.acting_player);
        let world = self.state.simulation.state();
        let selection: Vec<mint_sim::Entity> = units
            .iter()
            .filter(|unit| unit.selected && unit.owner == me)
            .map(|unit| unit.entity)
            .collect();
        if selection.is_empty() {
            return None;
        }
        crate::pick::pick_capturable(units, ray, self.declared(), |entity| {
            selection
                .iter()
                .any(|object| rules.contain_order_admits(world, me, entity, *object))
        })
    }

    fn ring_delivery_target(&self, units: &[UnitView], ray: Ray) -> Option<mint_sim::Entity> {
        let (rules, me) = (self.state.simulation.rules(), self.acting_player);
        let world = self.state.simulation.state();
        let selection: Vec<mint_sim::Entity> = units
            .iter()
            .filter(|unit| unit.selected && unit.owner == me)
            .map(|unit| unit.entity)
            .collect();
        if selection.is_empty() {
            return None;
        }
        crate::pick::pick_capturable(units, ray, self.declared(), |entity| {
            rules.ring_delivery_target(world, entity, &selection)
        })
    }

    fn capture_cursor(&self, target: mint_sim::Entity) -> Option<String> {
        let rules = self.state.simulation.rules();
        let world = self.state.simulation.state();
        let power = rules.selection_capture_power(world, self.acting_player, target)?;
        self.data.command_buttons.iter().find_map(|(_, button)| {
            button
                .special_power
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(&power))
                .then(|| button.cursor_name.clone())
                .flatten()
        })
    }

    fn pan_axes(&self) -> (f32, f32) {
        if self.camera_scroll_locked() || self.script_input_disabled() {
            return (0.0, 0.0);
        }
        let forward = f32::from(self.pan_forward) - f32::from(self.pan_back);
        let right = f32::from(self.pan_right) - f32::from(self.pan_left);
        if forward != 0.0 || right != 0.0 {
            return (forward, right);
        }
        if let Some(axes) = self.pointer.scroll_axes() {
            return axes;
        }
        RtsCamera::edge_pan_axes(
            self.pointer.edge_position(),
            self.viewport,
            DEFAULT_EDGE_SCROLL_MARGIN,
        )
    }

    fn sync_ui_panels(&mut self) {
        let overlay = self.apt_overlay.as_ref();
        let now = [
            overlay.is_some_and(AptOverlay::spell_store_showing),
            overlay.is_some_and(AptOverlay::objectives_open),
            self.state.stance_flyout.is_some(),
        ];
        for panel in [
            crate::rules::UiPanel::SpellStore,
            crate::rules::UiPanel::Objectives,
            crate::rules::UiPanel::StanceSubMenu,
        ] {
            let at = usize::from(panel.code());
            let (Some(&open), Some(sent)) = (now.get(at), self.state.ui_panels_sent.get_mut(at))
            else {
                continue;
            };
            if open != *sent {
                *sent = open;
                self.submit(RotwkCommand::SetUiPanelOpen { panel, open });
            }
        }
    }

    fn submit(&mut self, command: RotwkCommand) {
        if self.shell_game.is_some() {
            return;
        }
        self.actions = self.actions.saturating_add(1);
        crate::diag::diag!("command: {} {command:?}", self.acting_player);
        if let Err(error) = self.state.simulation.submit(self.acting_player, command) {
            eprintln!("command refused: {error}");
        }
    }

    fn issue_targeted(&mut self, command: RotwkCommand) {
        if matches!(
            command,
            RotwkCommand::FireSpecialPower {
                target: Some(_),
                ..
            } | RotwkCommand::CastSpell {
                target: Some(_),
                ..
            }
        ) {
            self.audio
                .play_misc("TargetObjectWithSpecialPowerSound", None, None);
        }
        self.submit(command);
    }

    fn act(&mut self, intent: Intent) {
        if self.mode != Mode::Match {
            return;
        }
        if matches!(intent, Intent::CameraReset) {
            self.reset_camera();
            return;
        }
        if let Some(option) = self.span {
            match intent {
                Intent::Select { x, y } => {
                    let Some(ray) = self.ray(x, y) else { return };
                    let Some(point) = mint_sage_terrain::pick_surface(
                        &self.state.scene.height_map,
                        self.state.scene.terrain.vertical_scale,
                        ray,
                    ) else {
                        println!("wall: no ground under pixel ({x:.0}, {y:.0})");
                        return;
                    };
                    self.submit(RotwkCommand::WallSpan {
                        option,
                        x: point.x,
                        y: point.y,
                    });
                    self.span = None;
                }
                Intent::Order { .. }
                | Intent::SelectBox(_)
                | Intent::OrderDrag { .. }
                | Intent::FormationDrag { .. } => {
                    println!("wall: span cancelled");
                    self.span = None;
                }
                Intent::CameraReset => {}
            }
            return;
        }
        if let Some(placement) = self.placement.clone() {
            match intent {
                Intent::Select { x, y } => self.commit_placement(&placement, (x, y)),
                Intent::SelectBox(_) => {
                    let Some(anchor) = placement.anchor else {
                        println!("placement: cancelled");
                        self.placement = None;
                        return;
                    };
                    self.commit_placement(&placement, anchor);
                }
                Intent::Order { .. } | Intent::OrderDrag { .. } | Intent::FormationDrag { .. } => {
                    println!("placement: cancelled");
                    self.placement = None;
                }
                Intent::CameraReset => {}
            }
            return;
        }
        if let Some(power_target) = self.power_target.take() {
            match intent {
                Intent::Select { x, y } => {
                    let order = self
                        .ray(x, y)
                        .ok_or("no view ray")
                        .and_then(|ray| self.power_target_order(&power_target, ray));
                    match order {
                        Ok(command) => self.issue_targeted(command),
                        Err(reason) => {
                            println!(
                                "power: target at pixel ({x:.0}, {y:.0}) refused - {reason}; still armed"
                            );
                            self.power_target = Some(power_target);
                        }
                    }
                }
                Intent::Order { .. }
                | Intent::SelectBox(_)
                | Intent::OrderDrag { .. }
                | Intent::FormationDrag { .. } => {
                    println!("power: target cancelled");
                }
                Intent::CameraReset => {}
            }
            return;
        }
        match intent {
            Intent::Select { x, y } => {
                let Some(ray) = self.ray(x, y) else { return };
                let units = self.units();
                let picked = pick_unit(&units, ray, self.acting_player, self.declared());
                match picked {
                    Some(entity) => {
                        self.submit(RotwkCommand::SetSelection {
                            replace: true,
                            units: vec![entity],
                        });
                        println!("selection: 1 unit picked at pixel ({x:.0}, {y:.0})");
                    }
                    None => {
                        self.submit(RotwkCommand::ClearSelection);
                        println!("selection: cleared");
                    }
                }
            }
            Intent::SelectBox(rect) => {
                let units = self.units();
                let picked = box_select(
                    &units,
                    &self.state.camera.camera(),
                    self.viewport,
                    rect,
                    self.box_mode,
                    self.acting_player,
                );
                let count = picked.len();
                self.submit(RotwkCommand::SetSelection {
                    replace: true,
                    units: picked,
                });
                println!(
                    "selection: {count} unit(s) boxed, {}",
                    self.box_mode.label()
                );
            }
            Intent::Order { x, y } => {
                let Some(ray) = self.ray(x, y) else { return };
                let (rules, me) = (self.state.simulation.rules(), self.acting_player);
                let units = self.units();
                let rally_structures: Vec<mint_sim::Entity> = units
                    .iter()
                    .filter(|unit| {
                        unit.selected
                            && unit.owner == me
                            && unit.kind_of.contains(KindOf::AUTO_RALLYPOINT)
                    })
                    .map(|unit| unit.entity)
                    .collect();
                if !rally_structures.is_empty() {
                    let target =
                        crate::pick::pick_capturable(&units, ray, self.declared(), |_| true);
                    match target {
                        Some(target) => {
                            self.submit(RotwkCommand::SetRallyPoint {
                                structures: rally_structures,
                                x: 0.0,
                                y: 0.0,
                                global: false,
                                target: Some(target),
                            });
                            println!("order: rally point on entity {}", target.index());
                        }
                        None => {
                            let click_point = mint_sage_terrain::pick_surface(
                                &self.state.scene.height_map,
                                self.state.scene.terrain.vertical_scale,
                                ray,
                            );
                            let Some(point) = click_point else {
                                println!("order: rally - no ground under pixel ({x:.0}, {y:.0})");
                                return;
                            };
                            let global = self.shift_held;
                            self.submit(RotwkCommand::SetRallyPoint {
                                structures: rally_structures,
                                x: point.x,
                                y: point.y,
                                global,
                                target: None,
                            });
                            println!(
                                "order: rally point ({:.1}, {:.1}){}",
                                point.x,
                                point.y,
                                if global { " (global)" } else { "" }
                            );
                        }
                    }
                    return;
                }
                if self.attack_move_armed {
                    self.attack_move_armed = false;
                    let target = pick_target(&units, ray, self.declared(), |unit| {
                        rules
                            .view_relation(self.state.simulation.state(), me, unit.entity)
                            .victim()
                    });
                    if let Some(target) = target {
                        self.submit(RotwkCommand::AttackObject { target });
                        println!(
                            "order: attack-move armed, enemy under cursor - attack entity {}",
                            target.index()
                        );
                        return;
                    }
                    let click_point = mint_sage_terrain::pick_surface(
                        &self.state.scene.height_map,
                        self.state.scene.terrain.vertical_scale,
                        ray,
                    );
                    if let Some(point) = click_point {
                        let fogged = move_hint_point_fogged(&self.state, point.x, point.y);
                        if crate::visuals::shows_move_hint(&units, fogged) {
                            self.state.goal = Some((point, self.state.simulation.frame()));
                        }
                        self.submit(RotwkCommand::AttackMove {
                            x: point.x,
                            y: point.y,
                            queue: self.order_mode_waypoint,
                        });
                        println!("order: attack-move to ({:.1}, {:.1})", point.x, point.y);
                    } else {
                        println!("order: attack-move - no ground under pixel ({x:.0}, {y:.0})");
                    }
                    return;
                }
                if self.ctrl_held {
                    let target = pick_force_target(&units, ray, self.declared(), |unit| {
                        rules
                            .view_relation(self.state.simulation.state(), me, unit.entity)
                            .victim()
                    });
                    if let Some(target) = target {
                        self.submit(RotwkCommand::ForceAttackObject { target });
                        println!("order: force-attack entity {}", target.index());
                    } else {
                        println!("order: force-attack - nothing under pixel ({x:.0}, {y:.0})");
                    }
                    return;
                }
                if let Some(target) = self.capture_under(&units, ray) {
                    self.submit(RotwkCommand::CaptureObject { target });
                    println!("order: capture entity {}", target.index());
                    return;
                }
                if let Some(target) = self.ring_delivery_target(&units, ray) {
                    self.submit(RotwkCommand::Enter { target });
                    println!("order: deliver the ring, enter entity {}", target.index());
                    return;
                }
                if let Some(target) = self.container_under(&units, ray) {
                    self.submit(RotwkCommand::Enter { target });
                    println!("order: enter entity {}", target.index());
                    return;
                }
                let target = pick_target(&units, ray, self.declared(), |unit| {
                    rules
                        .view_relation(self.state.simulation.state(), me, unit.entity)
                        .victim()
                });
                let click_point = mint_sage_terrain::pick_surface(
                    &self.state.scene.height_map,
                    self.state.scene.terrain.vertical_scale,
                    ray,
                );
                if let Some(target) = target {
                    self.submit(RotwkCommand::AttackObject { target });
                    println!("order: attack entity {}", target.index());
                    return;
                }
                let Some(point) = click_point else {
                    println!("order: no ground under pixel ({x:.0}, {y:.0})");
                    return;
                };
                let fogged = move_hint_point_fogged(&self.state, point.x, point.y);
                if crate::visuals::shows_move_hint(&units, fogged) {
                    self.state.goal = Some((point, self.state.simulation.frame()));
                }
                self.submit(RotwkCommand::MoveTo {
                    x: point.x,
                    y: point.y,
                    queue: self.order_mode_waypoint,
                });
                println!("order: move to ({:.1}, {:.1})", point.x, point.y);
            }
            Intent::OrderDrag { anchor, at } => {
                if self.ctrl_held {
                    self.formation_drag(anchor, at);
                }
            }
            Intent::FormationDrag { anchor, at } => self.formation_drag(anchor, at),
            Intent::CameraReset => {}
        }
    }

    fn formation_drag(&mut self, anchor: (f32, f32), at: (f32, f32)) {
        let attack = self.attack_move_armed;
        self.attack_move_armed = false;
        let (Some(anchor_ray), Some(at_ray)) = (self.ray(anchor.0, anchor.1), self.ray(at.0, at.1))
        else {
            return;
        };
        let ground = |ray| {
            mint_sage_terrain::pick_surface(
                &self.state.scene.height_map,
                self.state.scene.terrain.vertical_scale,
                ray,
            )
        };
        let (Some(anchor_point), Some(at_point)) = (ground(anchor_ray), ground(at_ray)) else {
            println!("order: drag-to-face - no ground under the press or the release");
            return;
        };
        let (dx, dy) = (at_point.x - anchor_point.x, at_point.y - anchor_point.y);
        self.submit(RotwkCommand::MoveToFormation {
            x: anchor_point.x,
            y: anchor_point.y,
            dx,
            dy,
            attack,
        });
        println!(
            "order: drag-to-face at ({:.1}, {:.1}), facing ({dx:.1}, {dy:.1})",
            anchor_point.x, anchor_point.y
        );
    }

    fn commit_placement(&mut self, placement: &Placement, screen: (f32, f32)) {
        let Some(point) = self.ray(screen.0, screen.1).and_then(|ray| {
            mint_sage_terrain::pick_surface(
                &self.state.scene.height_map,
                self.state.scene.terrain.vertical_scale,
                ray,
            )
        }) else {
            println!(
                "placement: no ground under pixel ({:.0}, {:.0})",
                screen.0, screen.1
            );
            self.placement = None;
            return;
        };
        self.submit(RotwkCommand::Construct {
            kind: placement.kind.0,
            x: point.x,
            y: point.y,
            angle: placement.angle,
        });
        println!(
            "placement: {} built at ({:.1}, {:.1}), angle {:.0} degree(s)",
            placement.label,
            point.x,
            point.y,
            placement.angle.to_degrees()
        );
        self.placement = None;
    }

    fn update_placement_drag_angle(&mut self, at: (f32, f32)) {
        let Some(anchor) = self.placement.as_ref().and_then(|p| p.anchor) else {
            return;
        };
        let current = self.placement.as_ref().map_or(0.0, |p| p.angle);
        let anchor_point = self.ray(anchor.0, anchor.1).and_then(|ray| {
            mint_sage_terrain::pick_surface(
                &self.state.scene.height_map,
                self.state.scene.terrain.vertical_scale,
                ray,
            )
        });
        let now_point = self.ray(at.0, at.1).and_then(|ray| {
            mint_sage_terrain::pick_surface(
                &self.state.scene.height_map,
                self.state.scene.terrain.vertical_scale,
                ray,
            )
        });
        if let (Some(anchor_point), Some(now_point)) = (anchor_point, now_point) {
            let angle = placement_drag_angle(anchor, at, anchor_point, now_point, current);
            if let Some(placement) = self.placement.as_mut() {
                placement.angle = angle;
            }
        }
    }

    fn reset_camera(&mut self) {
        self.state.camera.reset_orientation();
        self.submit(RotwkCommand::CameraReset);
        println!(
            "camera: reset to yaw {:.0} deg, pitch {:.0} deg, distance {:.0}",
            self.state.camera.yaw().to_degrees(),
            self.state.camera.pitch().to_degrees(),
            self.state.camera.distance()
        );
    }

    fn cycle_modes(&mut self, next: Modes) {
        self.modes = next;
        self.submit(RotwkCommand::SetMovementModes(next));
        let rung = FormationMode::ALL
            .iter()
            .position(|mode| *mode == next.formation)
            .map_or(0, |index| index.saturating_add(1));
        println!(
            "movement: {}, {} [{rung} of {}]",
            next.config(),
            next.formation.label(),
            FormationMode::ALL.len()
        );
    }

    fn digit_pressed(&mut self, button: u8) {
        let target = self
            .selected_kind()
            .and_then(|kind| {
                self.state
                    .kinds
                    .get(kind)
                    .map(|entry| (kind, entry.clone()))
            })
            .filter(|(_, entry)| entry.record.dozer)
            .and_then(|(_, entry)| entry.constructs.get(usize::from(button)).copied());
        let Some(target) = target else {
            self.submit(RotwkCommand::QueueUnit { button });
            return;
        };
        let label = self
            .state
            .kinds
            .get(target)
            .map_or_else(|| String::from("?"), |entry| entry.record.name.clone());
        println!(
            "placement: {label} [button {}] - left-click to place, drag to rotate, right-click \
             or Escape to cancel",
            u16::from(button).saturating_add(1)
        );
        let angle = self
            .state
            .kinds
            .get(target)
            .and_then(|entry| entry.record.placement_view_angle)
            .filter(|degrees| degrees.is_finite())
            .map_or(0.0, f32::to_radians);
        self.placement = Some(Placement {
            kind: target,
            label,
            angle,
            anchor: None,
            checked: None,
        });
    }

    fn slot_action(&mut self, action: SlotAction) {
        if !matches!(action, SlotAction::ToggleStance(_)) {
            self.state.stance_flyout = None;
        }
        match action {
            SlotAction::Build(button) => self.digit_pressed(button),
            SlotAction::Unimplemented(what) => {
                println!("command bar: {what} has no simulation command yet - ignored");
            }
            SlotAction::RadialPush { .. } | SlotAction::RadialPop => {
                if let Some(overlay) = self.apt_overlay.as_mut() {
                    let _ = overlay.radial_action(&action);
                }
            }
            SlotAction::ObjectUpgrade(button) => {
                self.submit(RotwkCommand::ObjectUpgrade { button });
            }
            SlotAction::PlayerUpgrade(button) => {
                self.submit(RotwkCommand::PlayerUpgrade { button });
            }
            SlotAction::Revive(button) => {
                self.submit(RotwkCommand::RecruitHero { button });
            }
            SlotAction::FireSpecialPower {
                button,
                target_kind,
                name,
            } => {
                self.play_button_sound(&name);
                self.fire_power_button(button, target_kind, &name);
            }
            SlotAction::FireWeaponAt { slot, name } => {
                self.play_button_sound(&name);
                println!(
                    "weapon: {} needs a position - click one, right-click or Escape cancels",
                    mint_sage_combat::WeaponSlot::from_index(slot.into())
                        .map_or("?", mint_sage_combat::WeaponSlot::name)
                );
                self.power_target = Some(PowerTarget {
                    order: TargetedOrder::Weapon(slot),
                    kind: crate::templates::TargetKind::Position,
                    cursor: self.radius_cursor_of(&name),
                    look: CommandCursor::of(&self.data.command_buttons, &name),
                    relation: self.button_relation(&name),
                });
            }
            SlotAction::Sacrifice { name } => {
                self.play_button_sound(&name);
                println!("sacrifice: click an allied horde - right-click or Escape cancels");
                self.power_target = Some(PowerTarget {
                    order: TargetedOrder::Sacrifice,
                    kind: crate::templates::TargetKind::Object,
                    cursor: self.radius_cursor_of(&name),
                    look: CommandCursor::of(&self.data.command_buttons, &name),
                    relation: self.button_relation(&name),
                });
            }
            SlotAction::WakeAutoPickUp(option) => {
                self.submit(RotwkCommand::WakeAutoPickUp { option });
            }
            SlotAction::SetStance(stance) => self.set_stance(stance),
            SlotAction::ToggleWeaponSet(button) => {
                let lead = self
                    .units()
                    .into_iter()
                    .find(|u| u.selected)
                    .map(|u| u.entity);
                self.submit(RotwkCommand::ToggleWeaponSet { button, lead });
            }
            SlotAction::ToggleWeapon(toggles) => {
                let lead = self
                    .units()
                    .into_iter()
                    .find(|u| u.selected)
                    .map(|u| u.entity);
                self.submit(RotwkCommand::ToggleWeapon { toggles, lead });
            }
            SlotAction::ToggleHoldFire => {
                let lead = self
                    .units()
                    .into_iter()
                    .find(|u| u.selected)
                    .map(|u| u.entity);
                self.submit(RotwkCommand::ToggleHoldFire { lead });
            }
            SlotAction::Stop => self.submit(RotwkCommand::Stop),
            SlotAction::GateCommand(action) => {
                self.submit(RotwkCommand::GateCommand { action });
            }
            SlotAction::ToggleHordeFormation => {
                self.submit(RotwkCommand::ToggleHordeFormation);
            }
            SlotAction::ArmAttackMove => {
                self.attack_move_armed = !self.attack_move_armed;
                println!(
                    "order: attack-move {} - next click sends it",
                    if self.attack_move_armed {
                        "armed"
                    } else {
                        "disarmed"
                    }
                );
            }
            SlotAction::ToggleStance(ref cycle) => self.toggle_stance_flyout(cycle),
            SlotAction::Sell => self.submit(RotwkCommand::Sell),
            SlotAction::Exit(passenger) => self.submit(RotwkCommand::Exit { passenger }),
            SlotAction::EmptySlot | SlotAction::Passive => {}
            SlotAction::StartSelfRepair => self.submit(RotwkCommand::StartSelfRepair),
            SlotAction::CancelConstruction => self.submit(RotwkCommand::CancelConstruction),
            SlotAction::FoundationConstruct(button) => {
                self.submit(RotwkCommand::FoundationConstruct { button });
            }
            SlotAction::BeginWallSpan { option } => {
                println!(
                    "wall: span from the selected hub, module option {} - drag and release to                      place, right-click or Escape cancels",
                    u16::from(option).saturating_add(1)
                );
                self.span = Some(option);
            }
        }
    }

    fn command_hotkey_action(&self, key: char) -> Option<SlotAction> {
        let units = self.units();
        let hud = crate::hud::read_hud(&self.state, &self.data, &units, self.viewport);
        crate::command_bar::armed_hotkey_action(&hud.command_bar, &self.state.strings, key)
    }

    fn play_button_sound(&mut self, button: &str) {
        let Some(def) = self.data.command_buttons.get(button) else {
            return;
        };
        let units = self.units();
        let hud = crate::hud::read_hud(&self.state, &self.data, &units, self.viewport);
        let bar = &hud.command_bar;
        let enabled = bar
            .ring
            .iter()
            .flatten()
            .chain(&bar.bar)
            .chain(bar.radial.iter().map(|(_, slot)| slot))
            .any(|slot| slot.enabled && slot.command_button.eq_ignore_ascii_case(button));
        if !enabled {
            return;
        }
        let rules = self.state.simulation.rules();
        let world = self.state.simulation.state();
        let subject = units
            .iter()
            .find(|unit| unit.selected)
            .map(|unit| rules.command_subject(world, unit.entity));
        let held = subject
            .zip(rules.upgrade_state_id())
            .and_then(|(entity, id)| world.get(id, entity));
        let toggled = held.is_some_and(|held| {
            def.flags_used_for_toggle.iter().any(|flag| {
                held.model_conditions
                    .iter()
                    .chain(&held.weapon_set_flags)
                    .any(|raised| raised.eq_ignore_ascii_case(flag))
            })
        });
        let state = usize::from(toggled);
        let Some(sound) = def.unit_specific_sound(state).map(str::to_owned) else {
            return;
        };
        let outcome = self.audio.play(&sound, None, None);
        crate::diag::diag!(
            "button sound: {button} state {state} plays {sound} ({})",
            outcome.label()
        );
    }

    fn fire_power_button(
        &mut self,
        button: u8,
        target_kind: crate::templates::TargetKind,
        name: &str,
    ) {
        match target_kind {
            crate::templates::TargetKind::None => {
                self.submit(RotwkCommand::FireSpecialPower {
                    button,
                    target: None,
                    at: None,
                });
            }
            kind => {
                println!(
                    "power: button {} needs a {} - click one, right-click or Escape cancels",
                    u16::from(button).saturating_add(1),
                    match kind {
                        crate::templates::TargetKind::Position => "position",
                        crate::templates::TargetKind::Confirm => "click",
                        _ => "target",
                    }
                );
                self.power_target = Some(PowerTarget {
                    order: TargetedOrder::Power(button),
                    kind,
                    cursor: self.radius_cursor_of(name),
                    look: CommandCursor::of(&self.data.command_buttons, name),
                    relation: self.button_relation(name),
                });
            }
        }
    }

    fn set_stance(&mut self, stance: mint_sage_ini::Stance) {
        self.submit(RotwkCommand::SetStance {
            stance: stance.index(),
        });
        println!("stance: {} ordered on the selection", stance.as_str());
    }

    fn toggle_stance_flyout(&mut self, cycle: &[mint_sage_ini::Stance]) {
        let units = self.units();
        let hud = crate::hud::read_hud(&self.state, &self.data, &units, self.viewport);
        let position = hud
            .command_bar
            .ring
            .iter()
            .position(|slot| {
                slot.as_ref()
                    .is_some_and(|slot| matches!(slot.action, SlotAction::ToggleStance(_)))
            })
            .and_then(|position| u8::try_from(position).ok())
            .filter(|&position| usize::from(position) < crate::hud::FLYOUT_SLOTS)
            .filter(|_| !hud.command_bar.stance_flyout.is_empty());
        let Some(position) = position else {
            if let Some(stance) = self.next_stance(cycle) {
                self.set_stance(stance);
            }
            return;
        };
        self.state.stance_flyout = (self.state.stance_flyout != Some(position)).then_some(position);
        let buttons: Vec<&str> = hud
            .command_bar
            .stance_flyout
            .iter()
            .map(|slot| slot.command_button.as_str())
            .collect();
        println!(
            "stance: flyout {} at ring {position} - {}",
            if self.state.stance_flyout.is_some() {
                "opened"
            } else {
                "closed"
            },
            buttons.join(", ")
        );
    }

    fn next_stance(&self, cycle: &[mint_sage_ini::Stance]) -> Option<mint_sage_ini::Stance> {
        let rules = self.state.simulation.rules();
        rules.next_stance(
            self.state.simulation.state(),
            rules.ids()?,
            self.acting_player,
            cycle,
        )
    }

    fn slot_cancel(&mut self, action: &SlotAction) {
        match action {
            SlotAction::Build(button) => {
                self.submit(RotwkCommand::CancelBuild { button: *button });
            }
            SlotAction::Revive(button) => {
                self.submit(RotwkCommand::CancelHero { button: *button });
            }
            SlotAction::ObjectUpgrade(button) => {
                self.submit(RotwkCommand::CancelUpgrade {
                    button: *button,
                    player: false,
                });
            }
            SlotAction::PlayerUpgrade(button) => {
                self.submit(RotwkCommand::CancelUpgrade {
                    button: *button,
                    player: true,
                });
            }
            SlotAction::FireSpecialPower { button, .. } => {
                self.submit(RotwkCommand::ToggleAutoCast { button: *button });
            }
            SlotAction::RadialPush { .. }
            | SlotAction::RadialPop
            | SlotAction::Sell
            | SlotAction::Exit(_)
            | SlotAction::EmptySlot
            | SlotAction::Passive
            | SlotAction::StartSelfRepair
            | SlotAction::CancelConstruction
            | SlotAction::FoundationConstruct(_)
            | SlotAction::BeginWallSpan { .. }
            | SlotAction::SetStance(_)
            | SlotAction::ToggleWeaponSet(_)
            | SlotAction::ToggleHoldFire
            | SlotAction::Sacrifice { .. }
            | SlotAction::ToggleWeapon(_)
            | SlotAction::ToggleStance(_)
            | SlotAction::FireWeaponAt { .. }
            | SlotAction::WakeAutoPickUp(_)
            | SlotAction::Stop
            | SlotAction::ToggleHordeFormation
            | SlotAction::ArmAttackMove
            | SlotAction::GateCommand(_)
            | SlotAction::Unimplemented(_) => {}
        }
    }

    pub(crate) fn purchase_science(&mut self, science: &str) {
        if let Some(command) =
            crate::spell_book::purchase_clicked(self.state.simulation.rules(), science)
        {
            self.submit(command);
        }
    }

    pub(crate) fn cast_spell(&mut self, button: u8) {
        let target = self.state.simulation.rules().spell_target_kind(
            self.state.simulation.state(),
            self.acting_player,
            button,
        );
        let named = self
            .state
            .simulation
            .rules()
            .spell_bar_state(
                self.state.simulation.state(),
                self.acting_player,
                button,
                self.state.simulation.frame(),
            )
            .map(|(_, name)| name);
        match target {
            None => println!(
                "spell: no spell-book button {}",
                u16::from(button).saturating_add(1)
            ),
            Some(crate::templates::TargetKind::None) => self.submit(RotwkCommand::CastSpell {
                button,
                target: None,
                at: None,
            }),
            Some(kind) => {
                println!(
                    "spell: button {} needs a target - click one, right-click or Escape cancels",
                    u16::from(button).saturating_add(1)
                );
                let name = named.as_deref().unwrap_or_default();
                self.power_target = Some(PowerTarget {
                    order: TargetedOrder::Spell(button),
                    kind,
                    cursor: self.radius_cursor_of(name),
                    look: CommandCursor::of(&self.data.command_buttons, name),
                    relation: self.button_relation(name),
                });
            }
        }
    }

    fn select_idle_builder(&mut self) -> Option<mint_sim::Entity> {
        let ids = self.state.simulation.rules().ids()?;
        let available =
            crate::hud::available_builders(self.state.simulation.state(), ids, LOCAL_PLAYER);
        let units = self.units();
        let placed: Vec<(mint_sim::Entity, [f32; 2])> = units
            .iter()
            .filter(|unit| available.contains(&unit.entity))
            .map(|unit| (unit.entity, [unit.position.x, unit.position.y]))
            .collect();
        let focus = self.state.camera.focus();
        let timeout = Duration::from_millis(u64::from(self.data.builder_cycle_timeout_ms));
        let Some(builder) = self.builder_cycle.click(
            &placed,
            [focus.x, focus.y],
            self.ui_clock,
            self.actions,
            timeout,
        ) else {
            println!("builder button: no available builder");
            return None;
        };
        self.submit(RotwkCommand::SetSelection {
            replace: true,
            units: vec![builder],
        });
        self.builder_cycle.settle(self.actions);
        let mut moved = false;
        if let Some(unit) = units.iter().find(|unit| unit.entity == builder) {
            let (x, y) = (unit.position.x, unit.position.y);
            let z = ground_at(&self.state.scene, x, y);
            let at = Vec3::new(x, y, z);
            let pixel = crate::pick::project(&self.state.camera.camera(), self.viewport, at);
            if !crate::hud::on_screen(pixel, self.viewport) {
                self.state.camera.jump_to(at);
                moved = true;
            }
        }
        println!(
            "builder button: entity {} of {} available, camera {}",
            builder.index(),
            placed.len(),
            if moved {
                "moved to it"
            } else {
                "kept (on screen)"
            }
        );
        Some(builder)
    }

    fn jump_camera_to(&mut self, units: &[UnitView], entity: mint_sim::Entity) {
        if let Some(unit) = units.iter().find(|u| u.entity == entity) {
            let (x, y) = (unit.position.x, unit.position.y);
            let z = ground_at(&self.state.scene, x, y);
            self.state.camera.jump_to(Vec3::new(x, y, z));
        }
    }

    fn jump_camera_to_group(&mut self, units: &[UnitView], entities: &[mint_sim::Entity]) {
        let positions: Vec<(f32, f32)> = units
            .iter()
            .filter(|u| entities.contains(&u.entity))
            .map(|u| (u.position.x, u.position.y))
            .collect();
        if positions.is_empty() {
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let n = positions.len() as f32;
        let (sx, sy) = positions
            .iter()
            .fold((0.0, 0.0), |(ax, ay), (x, y)| (ax + x, ay + y));
        let (x, y) = (sx / n, sy / n);
        let z = ground_at(&self.state.scene, x, y);
        self.state.camera.jump_to(Vec3::new(x, y, z));
    }

    fn selected_kind(&self) -> Option<Kind> {
        selected_kind_of(&self.units())
    }

    fn placement_ghost(&mut self) -> Option<crate::visuals::PlacementGhost> {
        let placement = self.placement.clone()?;
        let (px, py) = placement.anchor.or(self.last_pointer)?;
        let ray = self.ray(px, py)?;
        let point = mint_sage_terrain::pick_surface(
            &self.state.scene.height_map,
            self.state.scene.terrain.vertical_scale,
            ray,
        )?;
        #[allow(clippy::cast_possible_truncation)]
        let key = (
            (point.x / GHOST_RECHECK_CELL).floor() as i32,
            (point.y / GHOST_RECHECK_CELL).floor() as i32,
            (placement.angle * 1000.0) as i32,
        );
        let code = match placement.checked {
            Some((x, y, angle, code)) if (x, y, angle) == key => code,
            _ => {
                let code = self.site_is_legal(placement.kind, point.x, point.y, placement.angle);
                if let Some(held) = self.placement.as_mut() {
                    held.checked = Some((key.0, key.1, key.2, code));
                }
                code
            }
        };
        Some(crate::visuals::PlacementGhost {
            kind: placement.kind.0,
            x: point.x,
            y: point.y,
            facing: placement.angle,
            code,
            span_code: None,
        })
    }

    fn span_ghosts(&mut self) -> Vec<crate::visuals::PlacementGhost> {
        let Some(option) = self.span else {
            return Vec::new();
        };
        let Some(point) = self
            .last_pointer
            .and_then(|(px, py)| self.ray(px, py))
            .and_then(|ray| {
                mint_sage_terrain::pick_surface(
                    &self.state.scene.height_map,
                    self.state.scene.terrain.vertical_scale,
                    ray,
                )
            })
        else {
            return Vec::new();
        };
        let rules = self.state.simulation.rules();
        let Some(ids) = rules.ids() else {
            return Vec::new();
        };
        rules
            .span_plan(
                self.state.simulation.state(),
                ids,
                self.acting_player,
                option,
                point.x,
                point.y,
            )
            .map(|plan| {
                let span = Some(plan.code());
                plan.drawn()
                    .iter()
                    .map(|piece| crate::visuals::PlacementGhost {
                        kind: piece.kind.0,
                        x: piece.centre.x,
                        y: piece.centre.y,
                        facing: piece.angle,
                        code: piece.code,
                        span_code: span,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn castle_ghosts(
        &self,
        ghost: &crate::visuals::PlacementGhost,
    ) -> Vec<crate::visuals::PlacementGhost> {
        let kinds = &self.state.kinds;
        let rules = self.state.simulation.rules();
        let Some(castle) = kinds
            .get(Kind(ghost.kind))
            .and_then(|entry| entry.record.castle.as_ref())
        else {
            return Vec::new();
        };
        let side = rules
            .player_sides()
            .iter()
            .find(|(player, _)| *player == self.acting_player)
            .map_or("", |(_, side)| side.as_str());
        let Some((_, layout)) = crate::castle::layout_for(rules.castle_layouts(), castle, side)
        else {
            return Vec::new();
        };
        let origin = Vec2::new(ghost.x, ghost.y);
        let facing = mint_movement::Facing::from_radians(ghost.facing);
        layout
            .entries
            .iter()
            .filter_map(|entry| {
                let kind = kinds.kind(&entry.template_name)?;
                let at = crate::templates::rotate(origin, facing, entry.offset);
                let code = if kinds
                    .get(kind)
                    .is_some_and(|piece| piece.record.has_kind("BASE_SITE"))
                {
                    self.site_is_legal(kind, at.x, at.y, ghost.facing)
                } else {
                    0
                };
                Some(crate::visuals::PlacementGhost {
                    kind: kind.0,
                    x: at.x,
                    y: at.y,
                    facing: ghost.facing,
                    code,
                    span_code: None,
                })
            })
            .collect()
    }

    fn formation_drag_anchor(&self) -> Option<(Vec2, Vec2)> {
        let anchor_px = self.pointer.chord_drag_anchor().or_else(|| {
            self.ctrl_held
                .then(|| self.pointer.secondary_drag_anchor())
                .flatten()
        });
        let (anchor_px, at_px) = (anchor_px?, self.pointer.position()?);
        let ground = |ray| {
            mint_sage_terrain::pick_surface(
                &self.state.scene.height_map,
                self.state.scene.terrain.vertical_scale,
                ray,
            )
        };
        let anchor_point = ground(self.ray(anchor_px.0, anchor_px.1)?)?;
        let at_point = ground(self.ray(at_px.0, at_px.1)?)?;
        let anchor = Vec2::new(anchor_point.x, anchor_point.y);
        let drag = Vec2::new(at_point.x - anchor_point.x, at_point.y - anchor_point.y);
        Some((anchor, drag))
    }

    fn formation_drag_preview(
        &self,
    ) -> (
        Vec<crate::rules::FormationPreviewItem>,
        Vec<crate::rules::ArrowPiece>,
    ) {
        let Some((anchor, drag)) = self.formation_drag_anchor() else {
            return (Vec::new(), Vec::new());
        };
        let rules = self.state.simulation.rules();
        let Some(ids) = rules.ids() else {
            return (Vec::new(), Vec::new());
        };
        let items = rules.formation_preview(
            self.state.simulation.state(),
            ids,
            self.acting_player,
            anchor,
            drag,
            self.preview_decals,
        );
        if items.is_empty() {
            return (items, Vec::new());
        }
        let arrow = rules.formation_arrow(anchor, drag, self.preview_decals);
        (items, arrow)
    }

    fn button_relation(&self, button: &str) -> crate::defeat::ObjectFilter {
        self.data
            .command_buttons
            .get(button)
            .map_or_else(crate::defeat::ObjectFilter::default, |def| {
                crate::templates::target_relation(&def.options)
            })
    }

    fn radius_cursor_of(&self, button: &str) -> Option<crate::visuals::RadiusCursor> {
        crate::visuals::RadiusCursor::of(
            button,
            &self.data.command_buttons,
            &self.data.special_powers,
            &self.data.radius_cursors,
        )
    }

    fn start_script_shake(&mut self, effects: crate::rules::map_scripts::ViewEffects) {
        if effects.shake_seq == self.screen_shake_seen {
            return;
        }
        self.screen_shake_seen = effects.shake_seq;
        let shake = self.data.shake;
        let Some(strength) = shake.strength(effects.shake_kind).filter(|s| *s > 0.0) else {
            return;
        };
        let (mut left, mut frames) = (strength, 0u16);
        while left > 0.01 && frames < 600 {
            left *= 0.75;
            frames = frames.saturating_add(1);
        }
        let seconds =
            f32::from(frames) / f32::from(u16::try_from(mint_app::SIMULATION_HZ).unwrap_or(30));
        let eye = self.state.camera.eye();
        self.state
            .camera
            .shake(eye, shake.max_range.max(1.0), strength, seconds);
        crate::diag::diag!(
            "script: SCREEN_SHAKE intensity {} -> strength {strength}, {frames} frame(s)",
            effects.shake_kind
        );
    }

    fn radius_cursor_draw(
        &mut self,
        renderer: &mut Renderer,
    ) -> Option<crate::visuals::RadiusCursorDraw> {
        let (px, py) = self.last_pointer?;
        let ray = self.ray(px, py)?;
        let power_target = self.power_target.as_ref()?;
        let rgb = if self.power_target_order(power_target, ray).is_ok() {
            crate::visuals::RADIUS_CURSOR_VALID_RGB
        } else {
            crate::visuals::RADIUS_CURSOR_INVALID_RGB
        };
        let cursor = power_target.cursor.as_ref()?;
        let point = mint_sage_terrain::pick_surface(
            &self.state.scene.height_map,
            self.state.scene.terrain.vertical_scale,
            ray,
        )?;
        let texture = self.state.visuals.ground_texture(
            renderer,
            &self.state.vfs,
            &self.state.scene.resolver,
            &cursor.texture,
        );
        let alpha = cursor.opacity(self.state.simulation.frame().get(), mint_sim::SIMULATION_HZ);
        Some(crate::visuals::RadiusCursorDraw {
            centre: Vec3::new(
                point.x,
                point.y,
                ground_at(&self.state.scene, point.x, point.y),
            ),
            radius: cursor.radius,
            texture,
            tint: [rgb[0], rgb[1], rgb[2], alpha],
        })
    }

    fn claim_ring(&mut self, renderer: &mut Renderer) -> Option<crate::visuals::RadiusCursorDraw> {
        let ghost = self.state.visuals.placement_ghost?;
        let record = &self.state.kinds.get(Kind(ghost.kind))?.record;
        let radius = placement_claim_radius(record)?;
        let name = self.data.radius_cursors.claimant_decal()?;
        let texture = match self.claim_fade() {
            Some(fade) => self.state.visuals.faded_claim_texture(
                renderer,
                &self.state.vfs,
                &self.state.scene.resolver,
                name,
                mint_movement::Vec2::new(ghost.x, ghost.y),
                radius,
                &fade,
            ),
            None => self.state.visuals.ground_texture(
                renderer,
                &self.state.vfs,
                &self.state.scene.resolver,
                name,
            ),
        };
        Some(crate::visuals::RadiusCursorDraw {
            centre: Vec3::new(
                ghost.x,
                ghost.y,
                ground_at(&self.state.scene, ghost.x, ghost.y),
            ),
            radius,
            texture,
            tint: [1.0; 4],
        })
    }

    fn claim_fade(&self) -> Option<mint_sage_passability::claims::Fade> {
        let ghost = self.state.visuals.placement_ghost?;
        let claimant = crate::templates::claimant(&self.state.kinds.get(Kind(ghost.kind))?.record)?;
        let rules = self.state.simulation.rules();
        let ids = rules.ids()?;
        let world = self.state.simulation.state();
        rules.claim_fade(
            world,
            ids,
            &claimant,
            mint_movement::Vec2::new(ghost.x, ghost.y),
            self.acting_player,
        )
    }

    fn claim_share(&self) -> Option<crate::visuals::ClaimShareLabel> {
        let ring = self.state.visuals.claim_ring.as_ref()?;
        let ghost = self.state.visuals.placement_ghost?;
        let claimant = crate::templates::claimant(&self.state.kinds.get(Kind(ghost.kind))?.record)?;
        let rules = self.state.simulation.rules();
        let ids = rules.ids()?;
        let world = self.state.simulation.state();
        let claim = rules.preview_claim(
            world,
            ids,
            &claimant,
            mint_movement::Vec2::new(ghost.x, ghost.y),
            self.acting_player,
        )?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let percent = (claim.share() * 100.0).floor() as u32;
        Some(crate::visuals::ClaimShareLabel {
            at: ring.centre,
            percent,
        })
    }

    fn site_is_legal(&self, kind: Kind, x: f32, y: f32, angle: f32) -> u8 {
        placement_is_legal(&self.state, &self.units(), kind, x, y, angle)
    }

    fn command_bar_targets(&mut self) -> Option<Vec<HitTarget>> {
        if !self.overlay_enabled {
            return None;
        }
        self.apt_overlay.as_mut().map(AptOverlay::hit_targets)
    }

    pub(crate) fn scale_mode(&self) -> mint_sage_apt_viewer::ScaleMode {
        self.apt_overlay.as_ref().map_or(
            mint_sage_apt_viewer::ScaleMode::default(),
            AptOverlay::scale_mode,
        )
    }

    fn toggle_scale_mode(&mut self) -> mint_sage_apt_viewer::ScaleMode {
        if let Some(overlay) = self.apt_overlay.as_mut() {
            overlay.toggle_scale_mode()
        } else {
            self.scale_mode()
        }
    }

    pub(crate) fn set_scale_mode(&mut self, mode: mint_sage_apt_viewer::ScaleMode) {
        if let Some(overlay) = self.apt_overlay.as_mut() {
            overlay.set_scale_mode(mode);
        }
    }

    fn toggle_image_modes(&mut self) -> mint_sage_apt_viewer::ImageModes {
        self.apt_overlay
            .as_mut()
            .map_or(mint_sage_apt_viewer::ImageModes::default(), |overlay| {
                overlay.toggle_image_modes()
            })
    }
}

const fn ui_pointer_button(button: mint_app::PointerButton) -> UiPointerButton {
    match button {
        mint_app::PointerButton::Secondary => UiPointerButton::Secondary,
        mint_app::PointerButton::Middle => UiPointerButton::Middle,
        _ => UiPointerButton::Primary,
    }
}

pub(crate) fn horde_report(simulation: &Simulation<RotwkRules>) -> Vec<String> {
    simulation.rules().ids().map_or_else(Vec::new, |ids| {
        crate::horde::report(simulation.state(), RotwkRules::horde_ids(ids))
    })
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PlayerColors {
    by_player: Vec<[f32; 4]>,
    pub(crate) house_off: bool,
}

impl PlayerColors {
    fn from_index(index: &mint_sage_ini::MultiplayerColorIndex, choices: &[u8]) -> Self {
        let by_player = choices
            .iter()
            .map(|choice| {
                index
                    .nth(usize::from(*choice))
                    .and_then(|(_, def)| def.rgb)
                    .map_or(NO_HOUSE_COLOUR, |rgb| {
                        let [r, g, b] = rgb.to_unit();
                        [r, g, b, 1.0]
                    })
            })
            .collect();
        Self {
            by_player,
            house_off: false,
        }
    }

    pub(crate) fn of(&self, player: PlayerId) -> [f32; 4] {
        self.by_player
            .get(usize::from(player.get()))
            .copied()
            .unwrap_or(NO_HOUSE_COLOUR)
    }

    pub(crate) fn of_drawn(&self, player: PlayerId) -> [f32; 4] {
        if self.house_off {
            NO_HOUSE_COLOUR
        } else {
            self.of(player)
        }
    }

    pub(crate) fn of_or_white(&self, player: PlayerId) -> [f32; 4] {
        Some(self.of(player))
            .filter(|colour| *colour != NO_HOUSE_COLOUR)
            .unwrap_or([1.0, 1.0, 1.0, 1.0])
    }

    pub(crate) fn len(&self) -> usize {
        self.by_player
            .iter()
            .filter(|colour| *colour != &NO_HOUSE_COLOUR)
            .count()
    }
}

const NO_HOUSE_COLOUR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub(crate) fn attach_frame_events(
    units: &mut UnitScene,
    art: &[KindArt],
    rules: &crate::rules::RotwkRules,
) {
    let (mut resolved, mut unresolved) = (0usize, 0usize);
    for (at, kind_art) in art.iter().enumerate() {
        let (KindArt::Model(slot), Ok(kind)) = (kind_art, u16::try_from(at)) else {
            continue;
        };
        for data in units.frame_event_data(*slot) {
            match rules.frame_event_parts(crate::unit::Kind(kind), &data) {
                Some(parts) => {
                    resolved = resolved.saturating_add(1);
                    units.set_frame_event_parts(*slot, &data, parts);
                }
                None => unresolved = unresolved.saturating_add(1),
            }
        }
    }
    if resolved > 0 || unresolved > 0 {
        println!(
            "units: {resolved} clip-frame LuaEvent data string(s) raise sub-object writes, \
             {unresolved} have no sub-object-only handler and are not raised (B519)"
        );
    }
}

pub(crate) struct UnitArtLoad {
    units: UnitScene,
    art: Vec<KindArt>,
    colours: PlayerColors,
    names: Vec<String>,
    infantry: Vec<bool>,
    move_only: Vec<bool>,
    next: usize,
    nothing: usize,
    started: std::time::Instant,
    palette_len: usize,
    house_entries: usize,
}

impl UnitArtLoad {
    pub(crate) fn begin(
        vfs: &Vfs,
        scene: &MatchScene,
        names: Vec<String>,
        infantry: Vec<bool>,
        move_only: Vec<bool>,
        colour_choices: &[u8],
    ) -> Self {
        let started = std::time::Instant::now();
        let mut units = UnitScene::new();
        let (palette, palette_error) = mint_sage_model::load_multiplayer_colors(vfs);
        if let Some(reason) = &palette_error {
            println!("house colour: {reason} - no player has a colour");
        }
        let colours = PlayerColors::from_index(&palette, colour_choices);
        let (house_colors, house_color_error) = mint_sage_model::load_house_colors(vfs);
        if let Some(reason) = &house_color_error {
            println!("house colour: {reason} - no unit is house-coloured");
        }
        let house_entries = house_colors.len();
        units.set_house_colors(std::sync::Arc::new(house_colors));
        let mut load = Self {
            units,
            art: Vec::with_capacity(names.len()),
            colours,
            names,
            infantry,
            move_only,
            next: 0,
            nothing: 0,
            started,
            palette_len: palette.len(),
            house_entries,
        };
        if scene.object_index.is_none() {
            println!("units: no object index - every unit draws the placeholder prism");
            load.art = vec![KindArt::Prism; load.names.len()];
            load.next = load.names.len();
        }
        load
    }

    pub(crate) fn progress(&self) -> (usize, usize) {
        (self.next, self.names.len())
    }

    pub(crate) fn is_done(&self) -> bool {
        self.next >= self.names.len()
    }

    pub(crate) fn step(&mut self, renderer: &mut Renderer, vfs: &Vfs, scene: &MatchScene) {
        let Some(index) = scene.object_index.as_ref() else {
            return;
        };
        let at = self.next;
        let Some(name) = self.names.get(at) else {
            return;
        };
        self.next = at.saturating_add(1);
        if let Some(slot) = self
            .units
            .template_for(renderer, vfs, &scene.resolver, index, name)
        {
            if self.infantry.get(at).copied().unwrap_or(false) {
                self.units
                    .set_light_set(slot, mint_sage_model::INFANTRY_LIGHT_SET);
            }
            self.units
                .set_swirl_host(slot, self.move_only.get(at).copied().unwrap_or(false));
            self.art.push(KindArt::Model(slot));
            return;
        }
        if let mint_sage_ini::Template::NoModel(reason) = index.lookup(name) {
            self.nothing = self.nothing.saturating_add(1);
            crate::diag::diag!("units: {name} draws nothing by its own template ({reason:?}) - not drawn, not pickable");
            self.art.push(KindArt::Nothing);
            return;
        }
        println!("units: {name} names a model that did not load - it draws the prism");
        self.art.push(KindArt::Prism);
    }

    pub(crate) fn finish(self) -> (UnitScene, Vec<KindArt>, PlayerColors) {
        let Self {
            units,
            art,
            colours,
            names,
            nothing,
            started,
            palette_len,
            house_entries,
            ..
        } = self;
        println!("{}", units.report_line());
        println!(
            "load: unit art for {} template(s) in {:.0} ms - {} with a model, {nothing} drawing nothing, {} prism(s)",
            names.len(),
            started.elapsed().as_secs_f64() * 1000.0,
            art.iter().filter(|a| matches!(a, KindArt::Model(_))).count(),
            art.iter().filter(|a| **a == KindArt::Prism).count(),
        );
        println!(
            "house colour: {house_entries} base texture(s) mapped by housecolor.ini, {} of {palette_len} multiplayer colour(s) assigned to a player",
            colours.len(),
        );
        let mut failed = 0usize;
        for template in &units.templates {
            for (asset, reason) in template.failures() {
                failed = failed.saturating_add(1);
                crate::diag::diag!("clips: NO CLIP {}: {asset}: {reason}", template.name);
            }
        }
        if failed > 0 {
            println!(
                "units: {failed} animation clip(s) failed to load (MINT_ROTWK_DIAG=clips lists them)"
            );
        }
        (units, art, colours)
    }
}

pub(crate) fn load_unit_art(
    renderer: &mut Renderer,
    vfs: &Vfs,
    scene: &MatchScene,
    names: &[String],
    infantry: &[bool],
    move_only: &[bool],
    colour_choices: &[u8],
) -> (UnitScene, Vec<KindArt>, PlayerColors) {
    let mut load = UnitArtLoad::begin(
        vfs,
        scene,
        names.to_vec(),
        infantry.to_vec(),
        move_only.to_vec(),
        colour_choices,
    );
    while !load.is_done() {
        load.step(renderer, vfs, scene);
    }
    load.finish()
}

pub(crate) fn placement_is_legal(
    state: &MatchState,
    units: &[UnitView],
    kind: Kind,
    x: f32,
    y: f32,
    angle: f32,
) -> u8 {
    let Some(ids) = state.simulation.rules().ids() else {
        return 1;
    };
    let Some(entry) = state.kinds.get(kind) else {
        return 1;
    };
    let porter = units.iter().find(|unit| {
        unit.selected && unit.kind_of.contains(KindOf::DOZER) && unit.owner == LOCAL_PLAYER
    });
    let Some(from) = porter.map(|unit| unit.position) else {
        return 1;
    };
    let footprint = crate::construction::footprint_of(&entry.record);
    match state.simulation.rules().validate_placement(
        state.simulation.state(),
        ids,
        LOCAL_PLAYER,
        &entry.record,
        &footprint,
        mint_movement::Vec2::new(x, y),
        angle,
        from,
    ) {
        Ok(()) => 0,
        Err(reason) => reason.code(),
    }
}

fn placement_claim_radius(record: &mint_sage_modules::ObjectRecord) -> Option<f32> {
    record
        .claim_radius
        .filter(|r| r.is_finite() && *r > 0.0 && record.claim_max_income.unwrap_or(0.0) > 0.0)
}

fn footprint_bounds(
    footprint: &[mint_sage_passability::FootprintRecord],
) -> Option<([f32; 2], [f32; 2])> {
    let mut records = footprint.iter().map(|record| {
        let (x, y) = match record.footprint {
            mint_sage_passability::Footprint::Box { major, minor } => (major, minor),
            mint_sage_passability::Footprint::Disc { radius } => (radius, radius),
        };
        let (cx, cy) = (record.offset.x, record.offset.y);
        [cx - x, cy - y, cx + x, cy + y]
    });
    let first = records.next()?;
    let [left, bottom, right, top] = records.fold(first, |[l, b, r, t], [l2, b2, r2, t2]| {
        [l.min(l2), b.min(b2), r.max(r2), t.max(t2)]
    });
    Some((
        [(left + right) * 0.5, (bottom + top) * 0.5],
        [(right - left) * 0.5, (top - bottom) * 0.5],
    ))
}

fn placement_at(x: f32, y: f32, z: f32, facing: f32) -> mint_render::Mat4 {
    let direction = mint_movement::Facing::from_radians(facing).to_vector();
    let (c, s) = (direction.x, direction.y);
    mint_render::Mat4::from_cols([
        [c, s, 0.0, 0.0],
        [-s, c, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [x, y, z, 1.0],
    ])
}

pub(crate) fn placed_scales(
    simulation: &Simulation<RotwkRules>,
    objects: &[mint_sage_map::Object],
) -> std::collections::BTreeMap<mint_sim::Entity, f32> {
    simulation
        .rules()
        .map_placements(simulation.state())
        .into_iter()
        .filter_map(|(entity, at)| {
            let object = objects.get(usize::try_from(at).ok()?)?;
            let scale = mint_sage_terrain::prototype_scale(object);
            (scale != 1.0).then_some((entity, scale))
        })
        .collect()
}

fn scaled_placement(placement: mint_render::Mat4, scale: f32) -> mint_render::Mat4 {
    if scale == 1.0 {
        return placement;
    }
    let [x, y, z, at] = placement.to_cols();
    let by = |column: [f32; 4]| {
        [
            column[0] * scale,
            column[1] * scale,
            column[2] * scale,
            column[3],
        ]
    };
    mint_render::Mat4::from_cols([by(x), by(y), by(z), at])
}

fn unit_placement(unit: &UnitView) -> mint_render::Mat4 {
    let direction = mint_movement::Facing::from_radians(unit.facing).to_vector();
    let (c, s) = (direction.x, direction.y);
    mint_render::Mat4::from_cols([
        [c, s, 0.0, 0.0],
        [-s, c, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [unit.position.x, unit.position.y, unit.ground, 1.0],
    ])
}

pub(crate) fn shroud_masked(visibility: mint_sage_shroud::ObjectVisibility) -> bool {
    visibility != mint_sage_shroud::ObjectVisibility::Clear
}

#[allow(clippy::too_many_arguments)]
fn unit_poses(
    units: &[UnitView],
    art: &[KindArt],
    colours: &PlayerColors,
    kinds: &crate::templates::KindTable,
    look: &mut dyn FnMut(&UnitView) -> mint_sage_scene::LookKey,
    opacity: &dyn Fn(mint_sim::Entity) -> f32,
    casts_shadow: &dyn Fn(mint_sim::Entity) -> bool,
    scale: &dyn Fn(mint_sim::Entity) -> f32,
    hero_colours: &dyn Fn(&UnitView) -> Option<[[f32; 4]; 3]>,
) -> Vec<UnitPose> {
    units
        .iter()
        .filter_map(|unit| {
            let KindArt::Model(template) = KindArt::of(art, unit.art_kind) else {
                return None;
            };
            Some(UnitPose {
                look: look(unit),
                id: unit.entity.index(),
                generation: unit.entity.generation(),
                template,
                placement: scaled_placement(unit_placement(unit), scale(unit.entity)),
                conditions: unit.conditions,
                attack_cycle: unit.attack_cycle,
                attack_slot: unit.attack_slot,
                speed: unit.speed,
                weapon_cycle: unit.weapon_cycle,
                house_color: colours.of_drawn(unit.owner),
                hero_colours: hero_colours(unit),
                shroud_masked: shroud_masked(unit.visibility),
                ghost: None,
                casts_shadow: casts_shadow(unit.entity),
                opacity: opacity(unit.entity),
                progress: unit.build_elapsed.and_then(|elapsed| {
                    let record = unit
                        .kind
                        .and_then(|kind| kinds.get(crate::unit::Kind(kind)))
                        .map(|entry| &entry.record)?;
                    crate::render_state::build_progress(elapsed, unit.build_total, record)
                }),
                ride: None,
            })
        })
        .collect()
}

pub(crate) fn report_second(seconds: f32) -> bool {
    if !seconds.is_finite() || seconds < 0.0 || !crate::diag::wants("report") {
        return false;
    }
    let fraction = seconds.fract();
    seconds >= 1.0
        && fraction < 1.0 / f32::from(u16::try_from(mint_app::SIMULATION_HZ).unwrap_or(30))
}

pub(crate) fn minimap_view_box(state: &MatchState, viewport: (u32, u32)) -> Option<[[f32; 2]; 4]> {
    let world =
        crate::apt_overlay::view_box_world(&state.camera.camera(), viewport, state.view_box_z)?;
    let mut uv = [[0.0; 2]; 4];
    for (slot, [x, y]) in uv.iter_mut().zip(world) {
        *slot = minimap_uv_of(&state.scene.terrain.active_boundary_bounds(), x, y)?;
    }
    Some(uv)
}

fn audience_of(
    rules: &RotwkRules,
    listener: PlayerId,
    owner: PlayerId,
) -> mint_sage_audio::Audience {
    if owner == listener {
        mint_sage_audio::Audience::Local
    } else if rules.allied(owner, listener) {
        mint_sage_audio::Audience::Ally
    } else {
        mint_sage_audio::Audience::Enemy
    }
}

pub(crate) fn hover_target_changed(
    previous: &mut Option<String>,
    target: String,
) -> Option<String> {
    if previous.as_deref() == Some(target.as_str()) {
        return None;
    }
    *previous = Some(target.clone());
    Some(target)
}

pub(crate) fn ground_at(scene: &MatchScene, x: f32, y: f32) -> f32 {
    mint_sage_terrain::surface_height(&scene.height_map, scene.terrain.vertical_scale, x, y)
        .unwrap_or(0.0)
}

fn move_hint_cell_fogged(state: mint_sage_shroud::CellVisibility) -> bool {
    state == mint_sage_shroud::CellVisibility::Explored
}

fn move_hint_point_fogged(state: &MatchState, x: f32, y: f32) -> bool {
    let Some(ids) = state.simulation.rules().ids() else {
        return false;
    };
    let viewer = state.shroud_viewer;
    crate::shroud::with_grid(
        state.simulation.state(),
        RotwkRules::shroud_ids(ids),
        |grid| {
            let (cx, cy) = grid.world_to_cell(x, y);
            move_hint_cell_fogged(grid.cell_state(viewer, cx, cy))
        },
    )
    .unwrap_or(false)
}

fn is_order_voice(kind: crate::voice::VoiceKind) -> bool {
    matches!(
        kind,
        crate::voice::VoiceKind::Move
            | crate::voice::VoiceKind::Attack
            | crate::voice::VoiceKind::Enter
    )
}

fn merge_order_votes<'a>(
    events: impl Iterator<Item = &'a crate::voice::VoiceEvent>,
    group_templates: impl Fn(mint_sim::Entity) -> Vec<u16>,
) -> (Vec<u16>, Option<crate::voice::VoiceEvent>) {
    let mut templates = Vec::new();
    let mut first = None;
    for event in events {
        if !is_order_voice(event.kind) {
            continue;
        }
        templates.extend(group_templates(event.entity));
        if first.is_none() {
            first = Some(event.clone());
        }
    }
    (templates, first)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PassabilitySources {
    Cliffs,
    Water,
    #[default]
    Objects,
    NoBoundary,
}

impl PassabilitySources {
    pub(crate) const ALL: [Self; 4] = [Self::Cliffs, Self::Water, Self::Objects, Self::NoBoundary];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Cliffs => "cliffs",
            Self::Water => "cliffs+water",
            Self::Objects => "cliffs+water+objects+boundary",
            Self::NoBoundary => "cliffs+water+objects (no boundary)",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Cliffs => Self::Water,
            Self::Water => Self::Objects,
            Self::Objects => Self::NoBoundary,
            Self::NoBoundary => Self::Cliffs,
        }
    }

    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        for (index, rung) in Self::ALL.iter().enumerate() {
            let ordinal = index.saturating_add(1).to_string();
            if raw.eq_ignore_ascii_case(rung.label()) || raw == ordinal {
                return Ok(*rung);
            }
        }
        Err(format!(
            "--passability wants one of {}, or 1..4; got {raw}",
            Self::ALL
                .iter()
                .map(|rung| rung.label())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    fn water(self) -> bool {
        self != Self::Cliffs
    }

    fn objects(self) -> bool {
        matches!(self, Self::Objects | Self::NoBoundary)
    }

    fn boundary(self) -> bool {
        self == Self::Objects
    }
}

pub(crate) fn selected_kind_of(units: &[UnitView]) -> Option<Kind> {
    units
        .iter()
        .find(|unit| unit.selected)
        .and_then(|unit| unit.kind)
        .map(Kind)
}

pub(crate) fn selected_told_kind_of(units: &[UnitView]) -> Option<Kind> {
    units
        .iter()
        .find(|unit| unit.selected)
        .and_then(|unit| unit.told_kind)
        .map(Kind)
}

fn play_effects(
    renderer: &mut Renderer,
    state: &mut MatchState,
    seconds: f32,
    report: &mut SecondReport,
) {
    let gate = FxShroud::of(state);
    let scene = &mut state.scene;
    let units = &state.units;
    let camera = &mut state.camera;
    let ground = mint_sage_scene::MapGround::new(&scene.height_map, scene.terrain.vertical_scale);
    let is_on_water = |x: f32, y: f32| {
        mint_sage_scene::water_present::is_over_water(&scene.wet, &scene.height_map, x, y)
    };
    let weather = if scene.snowy {
        mint_sage_ini::NuggetWeather::Snowy
    } else {
        mint_sage_ini::NuggetWeather::Normal
    };
    let rules = state.simulation.rules();
    state.visuals.fading_decals.set_clock(seconds);
    let decal_clock = state.visuals.fading_decals.now();
    let fading_decals = &mut state.visuals.fading_decals;
    fading_decals.fire(decal_clock, scene.particle_scene.take_collision_decals());
    let (mut fired, mut systems, mut unknown) = (0u64, 0u64, 0u64);
    let (mut at_bone_pos, mut bone_found, mut bone_missing, mut nested_missing) =
        (0u64, 0u64, 0u64, 0u64);
    let mut camera_shakes = 0u64;
    let mut shrouded = 0u64;
    for event in state.fx.take() {
        let name = rules.fx_name(event.list).unwrap_or_default();
        let at = crate::fx_present::place_of(&event, |x, y| {
            mint_sage_terrain::surface_height(&scene.height_map, scene.terrain.vertical_scale, x, y)
                .unwrap_or(0.0)
        });
        crate::crash::event(format_args!(
            "fx: {name} at {:.0},{:.0},{:.0}",
            at.x, at.y, at.z
        ));
        let object = event.on_object.then_some(event.caller);
        if gate.skips(&state.simulation, &scene.particle_scene, name, object, at) {
            shrouded = shrouded.saturating_add(1);
            continue;
        }
        let caller = units.drawable_by_id(event.caller.index());
        let victim = event
            .victim
            .and_then(|victim| units.drawable_by_id(victim.index()));
        let secondary = event.target.map(|(x, y)| {
            let z = mint_sage_terrain::surface_height(
                &scene.height_map,
                scene.terrain.vertical_scale,
                x,
                y,
            )
            .unwrap_or(0.0);
            Vec3::new(x, y, z)
        });
        match scene.particle_scene.fire_list(
            renderer,
            &state.vfs,
            &scene.resolver,
            name,
            at,
            secondary,
            &ground,
            caller,
            victim,
            &is_on_water,
            weather,
        ) {
            Some(started) => {
                fired = fired.saturating_add(1);
                systems =
                    systems.saturating_add(u64::try_from(started.systems).unwrap_or(u64::MAX));
                at_bone_pos = at_bone_pos
                    .saturating_add(u64::try_from(started.at_bone_pos).unwrap_or(u64::MAX));
                bone_found = bone_found
                    .saturating_add(u64::try_from(started.bone_found).unwrap_or(u64::MAX));
                bone_missing = bone_missing
                    .saturating_add(u64::try_from(started.bone_missing).unwrap_or(u64::MAX));
                nested_missing = nested_missing
                    .saturating_add(u64::try_from(started.nested_list_missing).unwrap_or(u64::MAX));
                for volume in &started.camera_shakes {
                    camera.shake(
                        at,
                        volume.radius,
                        volume.amplitude_degrees,
                        volume.duration_seconds,
                    );
                }
                camera_shakes = camera_shakes
                    .saturating_add(u64::try_from(started.camera_shakes.len()).unwrap_or(u64::MAX));
                fading_decals.fire(decal_clock, started.dynamic_decals);
            }
            None => unknown = unknown.saturating_add(1),
        }
    }
    let fx = &mut state.fx;
    fx.fired = fx.fired.saturating_add(fired);
    fx.systems = fx.systems.saturating_add(systems);
    fx.unknown = fx.unknown.saturating_add(unknown);
    fx.at_bone_pos = fx.at_bone_pos.saturating_add(at_bone_pos);
    fx.bone_found = fx.bone_found.saturating_add(bone_found);
    fx.bone_missing = fx.bone_missing.saturating_add(bone_missing);
    fx.nested_list_missing = fx.nested_list_missing.saturating_add(nested_missing);
    fx.camera_shakes = fx.camera_shakes.saturating_add(camera_shakes);
    fx.shrouded = fx.shrouded.saturating_add(shrouded);
    if report_second(seconds) && (fx.fired > 0 || fx.unknown > 0 || fx.shrouded > 0) {
        report.line(format!(
            "effects: {} FXList(s) played, {} skipped under fog or shroud, {} particle \
             system(s) started, {} event(s) naming no FXList, {} FXListAtBonePos ({} bone \
             found, {} fell back, {} named no nested list), {} camera shake(s) [{}, {} of {}]",
            fx.fired,
            fx.shrouded,
            fx.systems,
            fx.unknown,
            fx.at_bone_pos,
            fx.bone_found,
            fx.bone_missing,
            fx.nested_list_missing,
            fx.camera_shakes,
            fx.shown().label(),
            fx.shown().rung(),
            crate::fx_present::FxShown::ALL.len()
        ));
    }
}

struct ActiveBoneFx {
    drawable: u32,
    slot: usize,
    state: usize,
    frame_stop: u32,
    systems: Vec<mint_sage_particle::SystemId>,
}

#[derive(Default)]
pub(crate) struct BoneFxTracker {
    active: Vec<ActiveBoneFx>,
}

impl BoneFxTracker {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn start(
        &mut self,
        particles: &mut mint_sage_scene::ParticleScene,
        drawable: u32,
        slot: usize,
        state: usize,
        frame_stop: u32,
        systems: Vec<mint_sage_particle::SystemId>,
    ) {
        self.stop_one(particles, drawable, slot);
        if !systems.is_empty() {
            self.active.push(ActiveBoneFx {
                drawable,
                slot,
                state,
                frame_stop,
                systems,
            });
        }
    }

    fn stop_one(
        &mut self,
        particles: &mut mint_sage_scene::ParticleScene,
        drawable: u32,
        slot: usize,
    ) {
        if let Some(index) = self
            .active
            .iter()
            .position(|active| active.drawable == drawable && active.slot == slot)
        {
            for system in self.active.remove(index).systems {
                particles.stop(system);
            }
        }
    }

    fn sync(
        &mut self,
        particles: &mut mint_sage_scene::ParticleScene,
        units: &mint_sage_scene::UnitScene,
    ) {
        let mut index = 0;
        while let Some(active) = self.active.get(index) {
            let left = match units.state_slot(active.drawable) {
                None => true,
                Some((state, frame, _)) => {
                    state != active.state || frame >= active.frame_stop as f32
                }
            };
            if left {
                let active = self.active.remove(index);
                if std::env::var_os("MINT_ROTWK_DIAG").is_some() {
                    println!(
                        "fx bone stop: drawable {} slot {} ({} system(s))",
                        active.drawable,
                        active.slot,
                        active.systems.len()
                    );
                }
                for system in active.systems {
                    particles.stop(system);
                }
            } else {
                index = index.saturating_add(1);
            }
        }
    }
}

fn play_animation_fx(
    renderer: &mut Renderer,
    state: &mut MatchState,
    seconds: f32,
    report: &mut SecondReport,
) {
    state
        .bone_fx
        .sync(&mut state.scene.particle_scene, &state.units);
    if state.fx.shown() == crate::fx_present::FxShown::Off {
        state.units.take_fx_events().for_each(drop);
        state.units.take_bone_fx_events().for_each(drop);
        return;
    }
    let events: Vec<(u32, String)> = state.units.take_fx_events().collect();
    let bone_events: Vec<(u32, mint_sage_scene::BoneFxFire)> =
        state.units.take_bone_fx_events().collect();
    if !bone_events.is_empty() {
        fire_bone_fx_events(renderer, state, bone_events);
    }
    if events.is_empty() {
        return;
    }
    let (fired, systems, unknown) = fire_at_drawables(renderer, state, events);
    let fx = &mut state.fx;
    fx.anim_fired = fx.anim_fired.saturating_add(fired);
    fx.anim_systems = fx.anim_systems.saturating_add(systems);
    fx.anim_unknown = fx.anim_unknown.saturating_add(unknown);
    if report_second(seconds) && (fx.anim_fired > 0 || fx.anim_unknown > 0) {
        report.line(format!(
            "animation fx: {} FXEvent(s) fired ({} particle system(s) started), {} named no \
             FXList [{}]",
            fx.anim_fired,
            fx.anim_systems,
            fx.anim_unknown,
            fx.shown().label()
        ));
    }
}

fn fire_bone_fx_events(
    renderer: &mut Renderer,
    state: &mut MatchState,
    events: Vec<(u32, mint_sage_scene::BoneFxFire)>,
) {
    let gate = FxShroud::of(state);
    let scene = &mut state.scene;
    let ground = mint_sage_scene::MapGround::new(&scene.height_map, scene.terrain.vertical_scale);
    let is_on_water = |x: f32, y: f32| {
        mint_sage_scene::water_present::is_over_water(&scene.wet, &scene.height_map, x, y)
    };
    let weather = if scene.snowy {
        mint_sage_ini::NuggetWeather::Snowy
    } else {
        mint_sage_ini::NuggetWeather::Normal
    };
    for (id, fire) in events {
        let Some(caller) = state.units.drawable_by_id(id) else {
            continue;
        };
        let Some(entry_state) = caller.state() else {
            continue;
        };
        let at = caller.placement().transform_point(Vec3::ZERO);
        if gate.skips(
            &state.simulation,
            &scene.particle_scene,
            &fire.fx_list,
            None,
            at,
        ) {
            continue;
        }
        let Some(started) = scene.particle_scene.fire_list(
            renderer,
            &state.vfs,
            &scene.resolver,
            &fire.fx_list,
            at,
            None,
            &ground,
            Some(caller),
            None,
            &is_on_water,
            weather,
        ) else {
            continue;
        };
        state.bone_fx.start(
            &mut scene.particle_scene,
            id,
            fire.slot,
            entry_state,
            fire.frame_stop,
            started.bone_leaf_systems,
        );
    }
}

pub(crate) fn fire_at_drawables(
    renderer: &mut Renderer,
    state: &mut MatchState,
    events: Vec<(u32, String)>,
) -> (u64, u64, u64) {
    let gate = FxShroud::of(state);
    let scene = &mut state.scene;
    let ground = mint_sage_scene::MapGround::new(&scene.height_map, scene.terrain.vertical_scale);
    let is_on_water = |x: f32, y: f32| {
        mint_sage_scene::water_present::is_over_water(&scene.wet, &scene.height_map, x, y)
    };
    let weather = if scene.snowy {
        mint_sage_ini::NuggetWeather::Snowy
    } else {
        mint_sage_ini::NuggetWeather::Normal
    };
    let (mut fired, mut systems, mut unknown, mut shrouded) = (0u64, 0u64, 0u64, 0u64);
    for (id, name) in events {
        let Some(caller) = state.units.drawable_by_id(id) else {
            continue;
        };
        let at = caller.placement().transform_point(Vec3::ZERO);
        if gate.skips(&state.simulation, &scene.particle_scene, &name, None, at) {
            shrouded = shrouded.saturating_add(1);
            continue;
        }
        match scene.particle_scene.fire_list(
            renderer,
            &state.vfs,
            &scene.resolver,
            &name,
            at,
            None,
            &ground,
            Some(caller),
            None,
            &is_on_water,
            weather,
        ) {
            Some(started) => {
                fired = fired.saturating_add(1);
                systems =
                    systems.saturating_add(u64::try_from(started.systems).unwrap_or(u64::MAX));
            }
            None => unknown = unknown.saturating_add(1),
        }
    }
    state.fx.shrouded = state.fx.shrouded.saturating_add(shrouded);
    (fired, systems, unknown)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compose_and_render(
    renderer: &mut Renderer,
    state: &mut MatchState,
    blend: f32,
    interpolate: bool,
    obstacle_overlay: bool,
    drag: Option<ScreenRect>,
    apt_overlay: &[ScreenDraw<'_>],
    report: &mut SecondReport,
    move_hint_model: Option<&Arc<mint_sage_model::CachedModel>>,
    selector: PlayerId,
    hovered: Option<mint_sim::Entity>,
    all_health_bars: bool,
    order_queues: bool,
) -> mint_render::Result<()> {
    let mut clock = crate::render_perf::Clock::start();
    crate::snapshot::ensure(
        &mut state.snapshot,
        &state.simulation,
        &state.previous,
        state.shroud_viewer,
        selector,
    );
    let ids = state.simulation.rules().ids();
    let mut seconds = 0.0;
    let mut units: Vec<UnitView> = Vec::new();
    let mut projectiles = Vec::new();
    let mut enclosed = 0usize;
    let drawn_blend = interpolate.then_some(blend);
    if let Some(ids) = ids {
        let snapshot = &state.snapshot;
        if let Some(ticks) = snapshot.clock() {
            seconds =
                ticks as f32 / f32::from(u16::try_from(mint_app::SIMULATION_HZ).unwrap_or(30));
            if ticks > 0
                && ticks % u64::from(mint_app::SIMULATION_HZ) == 0
                && crate::diag::wants("report")
            {
                report.line(format!(
                    "match clock: {ticks} tick(s), blend {:.2}, interpolation {}, {} selected",
                    drawn_blend.map_or(blend, crate::render_state::clamp_blend),
                    if interpolate { "on" } else { "off" },
                    snapshot.selected_by_local()
                ));
            }
        }
        units = snapshot.units(drawn_blend);
        crate::render_state::apply_weather(&mut units, state.scene.snowy);
        crate::render_state::apply_kind_damage(&mut units, &state.kinds, state.damage_thresholds);
        let (unit_art, scene_units) = (&state.unit_art, &state.units);
        crate::render_state::apply_collapse(
            &mut units,
            snapshot.frame(),
            &|entity| snapshot.facts(entity).and_then(|facts| facts.collapse),
            &|unit, done| {
                let Some(KindArt::Model(slot)) = unit
                    .art_kind
                    .and_then(|kind| unit_art.get(usize::from(kind)))
                else {
                    return false;
                };
                scene_units.templates.get(*slot).is_some_and(|art| {
                    art.model_machine.select(done).is_some_and(|picked| {
                        art.model_machine
                            .required(picked)
                            .contains(mint_sage_scene::Conditions::POST_COLLAPSE)
                    })
                })
            },
        );
        projectiles = snapshot.projectiles(drawn_blend);
        enclosed = snapshot.enclosed();
        crate::tdfx::apply_transition_damage_fx(renderer, state, &units);
        if let Some(forest_entity) =
            crate::forest::singleton_of(state.simulation.state(), ids.forest)
        {
            if let Some(forest) = state.simulation.state().get(ids.forest, forest_entity) {
                crate::scene::apply_tree_poses(&mut state.scene, forest);
            }
        }
    }
    if report_second(seconds) && enclosed > 0 {
        report.line(format!(
            "contain: {enclosed} contained drawable(s) hidden - their container's ObjectStatusOfContained names ENCLOSED"
        ));
    }
    clock.lap(crate::render_perf::stage!("view"));

    if let (Some(ids), Some(view)) = (ids, state.shroud_view.as_mut()) {
        let frame = state.simulation.frame().get();
        let census = match state.shroud_source {
            Some(source) if source.frame == frame && source.viewer == state.shroud_viewer => {
                source.census
            }
            _ => {
                let census = crate::shroud::rebuild_view(
                    state.simulation.state(),
                    RotwkRules::shroud_ids(ids),
                    state.shroud_viewer,
                    view,
                );
                state.shroud_source = Some(ShroudSource {
                    frame,
                    viewer: state.shroud_viewer,
                    census,
                });
                census
            }
        };
        let hz = f32::from(u16::try_from(mint_app::SIMULATION_HZ).unwrap_or(30));
        let now = seconds + blend.clamp(0.0, 1.0) / hz;
        let moving = match state.shroud_clock.replace(now) {
            None => {
                view.snap();
                0
            }
            Some(before) => view.fade(now - before, state.shroud_fade),
        };
        let ring = if state.scene.script_view.effects.border_clear {
            CLEAR_ALPHA
        } else {
            SHROUD_ALPHA
        };
        let uploaded = if state.scene.terrain_shading.modulation.shroud == TerrainShroud::On {
            view.upload(renderer, ring)
        } else {
            view.upload_revealed(renderer, ring)
        };
        if let Err(error) = uploaded {
            eprintln!("shroud: texture update failed: {error}");
        }
        if report_second(seconds) {
            if let Some((unexplored, explored, visible)) = census {
                report.line(format!(
                    "shroud: drawing player {}'s view - {visible} cell(s) visible, \
                     {explored} remembered, {unexplored} never seen, {moving} still \n                     fading [{}]",
                    state.shroud_viewer.get(),
                    state.shroud_fade.label()
                ));
            }
        }
    }
    let shroud_drawn = state.scene.terrain_shading.modulation.shroud == TerrainShroud::On;
    crate::ghosts::drawn_visibility(&mut units, shroud_drawn, &|x, y| {
        state.snapshot.playable(x, y)
    });
    if let Some(ids) = ids {
        let viewer = state.shroud_viewer;
        let ghosts = &mut state.visuals.ghosts;
        let from_memory = crate::shroud::with_grid(
            state.simulation.state(),
            RotwkRules::shroud_ids(ids),
            |grid| {
                ghosts.apply(&mut units, viewer, shroud_drawn, &|x, y| {
                    let (cx, cy) = grid.world_to_cell(x, y);
                    grid.cell_state(viewer, cx, cy)
                })
            },
        )
        .unwrap_or(0);
        if report_second(seconds) && shroud_drawn {
            report.line(format!(
                "shroud: {} structure(s) remembered for player {}, {from_memory} drawn as last seen",
                ghosts.len(),
                viewer.get()
            ));
        }
    }
    drop_undrawn(&mut units, &state.unit_art);
    clock.lap(crate::render_perf::stage!("shroud"));

    let render_clock = if interpolate {
        seconds
            + blend.clamp(0.0, 1.0)
                / f32::from(u16::try_from(mint_app::SIMULATION_HZ).unwrap_or(30))
    } else {
        seconds
    };
    let render_clock = state
        .cinematic
        .client_clock(state.scene.script_view.cine.time_frozen, render_clock);
    play_effects(renderer, state, seconds, report);
    crate::scene::advance_scene(&mut state.scene, renderer, render_clock);
    state.scene.drawn.truncate(state.scene.composed_len);
    shroud_object_tints(state);
    clock.lap(crate::render_perf::stage!("scene"));
    let now = state.snapshot.frame();
    let goal = state.goal.and_then(|(point, set_at)| {
        let age = now.get().saturating_sub(set_at.get());
        (age <= u64::from(crate::visuals::MOVE_HINT_LIFETIME_FRAMES))
            .then(|| (point.x, point.y, ground_at(&state.scene, point.x, point.y)))
    });

    let orders = state.snapshot.build_orders().to_vec();
    if report_second(seconds) && !orders.is_empty() {
        report.line(format!("plans: {} shown", orders.len()));
    }
    let mut poses = {
        let colours = state.player_colors.clone();
        let world = state.simulation.state();
        let upgrades = state.simulation.rules().upgrade_state_id();
        let scene_units = &mut state.units;
        let mut look =
            |unit: &UnitView| crate::looks::look_of(world, upgrades, scene_units, unit.entity);
        let rules = state.simulation.rules();
        let snapshot = &state.snapshot;
        let opacity = |entity| {
            snapshot.facts(entity).map_or_else(
                || {
                    rules
                        .mount_opacity(world, entity)
                        .min(rules.disguise_opacity(world, entity))
                        .min(rules.corpse_fade_opacity(world, entity, now))
                        .min(rules.join_fade_opacity(world, entity, now))
                },
                |facts| facts.opacity,
            )
        };
        let casts_shadow = |entity: mint_sim::Entity| {
            snapshot.facts(entity).map_or_else(
                || {
                    !upgrades.is_some_and(|id| {
                        world.get(id, entity).is_some_and(|held| {
                            held.model_conditions.iter().any(|name| {
                                name.eq_ignore_ascii_case("CLIMBING")
                                    || name.eq_ignore_ascii_case("RAPPELLING")
                            })
                        })
                    })
                },
                |facts| facts.casts_shadow,
            )
        };
        let rules = state.simulation.rules();
        let hero_colours = |unit: &UnitView| {
            rules
                .ids()
                .and_then(|ids| rules.cah_colours(world, ids, unit.entity))
        };
        let scales = &state.placed_scales;
        let mut poses = unit_poses(
            &units,
            &state.unit_art,
            &colours,
            &state.kinds,
            &mut look,
            &opacity,
            &casts_shadow,
            &|entity| scales.get(&entity).copied().unwrap_or(1.0),
            &hero_colours,
        );
        let plan = state.units.intern_look(mint_sage_scene::UnitLook {
            conditions: vec![PLAN_CONDITION.into()],
            ..mint_sage_scene::UnitLook::default()
        });
        for (builder, owner, task) in &orders {
            let KindArt::Model(template) = KindArt::of(&state.unit_art, Some(task.kind.0)) else {
                continue;
            };
            poses.push(UnitPose {
                opacity: 1.0,
                id: builder.index(),
                generation: builder.generation(),
                template,
                placement: placement_at(
                    task.centre.x,
                    task.centre.y,
                    ground_at(&state.scene, task.centre.x, task.centre.y),
                    task.angle,
                ),
                conditions: mint_sage_scene::Conditions::NONE,
                attack_cycle: 0,
                attack_slot: None,
                speed: 0.0,
                weapon_cycle: 0.0,
                house_color: colours.of_drawn(*owner),
                hero_colours: None,
                shroud_masked: false,
                progress: None,
                ghost: Some(crate::visuals::plan_look()),
                casts_shadow: true,
                look: plan,
                ride: None,
            });
        }
        for (index, ghost) in state
            .visuals
            .span_ghosts
            .iter()
            .copied()
            .enumerate()
            .map(|(i, g)| (u32::try_from(i).unwrap_or(0), g))
            .chain(state.visuals.placement_ghost.map(|g| (0, g)))
        {
            if let KindArt::Model(template) = KindArt::of(&state.unit_art, Some(ghost.kind)) {
                poses.push(UnitPose {
                    opacity: 1.0,
                    id: crate::visuals::PLACEMENT_GHOST_ID.saturating_sub(index),
                    generation: 0,
                    template,
                    placement: placement_at(
                        ghost.x,
                        ghost.y,
                        ground_at(&state.scene, ghost.x, ghost.y),
                        ghost.facing,
                    ),
                    conditions: mint_sage_scene::Conditions::NONE,
                    attack_cycle: 0,
                    attack_slot: None,
                    speed: 0.0,
                    weapon_cycle: 0.0,
                    house_color: colours.of(LOCAL_PLAYER),
                    hero_colours: None,
                    shroud_masked: false,
                    progress: None,
                    ghost: Some(crate::visuals::ghost_look(&ghost, now.get())),
                    casts_shadow: true,
                    look: crate::visuals::placement_look(&mut state.units),
                    ride: None,
                });
            }
        }
        if let (Some(ids), Some(kind)) =
            (ids, state.kinds.kind(crate::visuals::RALLY_MARKER_TEMPLATE))
        {
            if let KindArt::Model(template) = KindArt::of(&state.unit_art, Some(kind.0)) {
                let world = state.simulation.state();
                let economy = RotwkRules::economy_ids(ids);
                let rally = units
                    .iter()
                    .filter(|unit| unit.selected && unit.owner == LOCAL_PLAYER)
                    .find_map(|unit| world.get(economy.producer, unit.entity)?.player_rally);
                if let Some(point) = rally {
                    poses.push(UnitPose {
                        opacity: 1.0,
                        id: crate::visuals::RALLY_FLAG_ID,
                        generation: 0,
                        template,
                        placement: placement_at(
                            point.x,
                            point.y,
                            ground_at(&state.scene, point.x, point.y),
                            state.downwind_angle,
                        ),
                        conditions: mint_sage_scene::Conditions::NONE,
                        attack_cycle: 0,
                        attack_slot: None,
                        speed: 0.0,
                        weapon_cycle: 0.0,
                        house_color: colours.of(LOCAL_PLAYER),
                        hero_colours: None,
                        shroud_masked: false,
                        progress: None,
                        ghost: None,
                        casts_shadow: true,
                        look: state
                            .units
                            .intern_look(mint_sage_scene::UnitLook::default()),
                        ride: None,
                    });
                }
            }
        }
        let planted = state.simulation.rules().ids().and_then(|ids| {
            let world = state.simulation.state();
            crate::forest::singleton_of(world, ids.forest)
                .and_then(|entity| world.get(ids.forest, entity))
                .map(crate::forest::Forest::planted_trees)
        });
        let plain = state
            .units
            .intern_look(mint_sage_scene::UnitLook::default());
        for tree in planted.unwrap_or_default() {
            let KindArt::Model(template) = KindArt::of(&state.unit_art, Some(tree.kind.0)) else {
                continue;
            };
            let ground = ground_at(&state.scene, tree.pos.x, tree.pos.y);
            let standing = placement_at(tree.pos.x, tree.pos.y, ground, 0.0);
            let placement = crate::scene::topple_transform(
                standing,
                mint_render::Vec3::new(tree.pos.x, tree.pos.y, ground),
                &tree.pose,
            );
            poses.push(UnitPose {
                opacity: 1.0,
                id: crate::visuals::PLANTED_TREE_ID.saturating_sub(tree.serial),
                generation: 0,
                template,
                placement,
                conditions: mint_sage_scene::Conditions::NONE,
                attack_cycle: 0,
                attack_slot: None,
                speed: 0.0,
                weapon_cycle: 0.0,
                house_color: NO_HOUSE_COLOUR,
                hero_colours: None,
                shroud_masked: false,
                progress: None,
                ghost: None,
                casts_shadow: true,
                look: plain,
                ride: None,
            });
        }
        poses
    };
    {
        let riders = state.snapshot.riders();
        if !riders.is_empty() {
            for pose in poses.iter_mut().filter(|pose| pose.ghost.is_none()) {
                let Some(rider) = riders
                    .binary_search_by_key(&pose.id, |rider| rider.rider.index())
                    .ok()
                    .and_then(|at| riders.get(at))
                else {
                    continue;
                };
                let container = rider.container.index();
                pose.ride = state
                    .units
                    .passenger_pivot(container, &rider.prefix, rider.slot)
                    .map(|pivot| mint_sage_scene::Ride { container, pivot });
            }
        }
    }
    let (mut shot_poses, streaks, cubes) = state
        .projectile_art
        .sort(state.simulation.rules(), &projectiles);
    if report_second(seconds) && !projectiles.is_empty() {
        report.line(format!(
            "projectiles: {} in flight - {} model(s), {} streak ribbon(s), {} cube(s)",
            projectiles.len(),
            shot_poses.len(),
            streaks.len(),
            cubes.len()
        ));
    }
    poses.append(&mut shot_poses);
    clock.lap(crate::render_perf::stage!("poses"));
    if state
        .units
        .prepare_looks(renderer, &state.vfs, &state.scene.resolver, &poses)
        > 0
    {
        for line in state.units.look_lines() {
            report.line(line);
        }
    }
    state.units.sync(&poses, render_clock);
    crate::selection_flash::play_tints(state, &units, render_clock, report);
    clock.lap(crate::render_perf::stage!("sync"));
    state.units.set_profiling(crate::perf::enabled());
    let views = unit_views(renderer, state);
    let mut health_bars = std::mem::take(&mut state.visuals.bar_quads);
    health_bars.clear();
    {
        let camera = state.camera.camera();
        let view_proj = camera
            .projection_with_depth(renderer.depth_direction(), renderer.aspect_ratio())
            .mul_mat4(camera.view());
        let kinds = &state.kinds;
        let colours = &state.player_colors;
        crate::visuals::push_health_bars(
            &units,
            &|unit| {
                crate::visuals::HealthBox::of(
                    unit.kind
                        .and_then(|kind| kinds.get(crate::unit::Kind(kind)))
                        .map(|entry| &entry.record),
                    unit.height,
                )
            },
            view_proj,
            renderer.size(),
            hovered,
            all_health_bars,
            state.visuals.bar_style,
            &|player| colours.of(player),
            &mut health_bars,
        );
    }
    {
        let camera = state.camera.camera();
        let view_proj = camera
            .projection_with_depth(renderer.depth_direction(), renderer.aspect_ratio())
            .mul_mat4(camera.view());
        let kinds = &state.kinds;
        let simulation = &state.simulation;
        let rules = simulation.rules();
        let strings = &state.strings;
        let text_for = |unit: &UnitView| -> Option<String> {
            if unit
                .kind
                .and_then(|kind| kinds.get(Kind(kind)))
                .is_some_and(|entry| {
                    entry.record.has_kind("WALL_SEGMENT") || entry.record.has_kind("DEFENSIVE_WALL")
                })
            {
                return None;
            }
            let elapsed = unit.build_elapsed?;
            let total = match unit.build_total {
                Some(frames) => frames,
                None => {
                    let record = unit
                        .kind
                        .and_then(|kind| kinds.get(Kind(kind)))
                        .map(|entry| &entry.record)?;
                    rules
                        .price_for(simulation.state(), ids?, unit.owner, record)?
                        .1
                }
            };
            if total == 0 {
                return None;
            }
            let percent = (f64::from(elapsed) / f64::from(total) * 100.0).clamp(0.0, 100.0);
            Some(mint_sage_str::format_placeholder(
                &strings.fetch("CONTROLBAR:UnderConstructionDesc"),
                mint_sage_str::Placeholder::Float(percent),
            ))
        };
        let font = mint_sage_apt_viewer::resolved_font_file(mint_ui::TIMES_NEW_ROMAN.logical_name)
            .and_then(|key| state.fonts.get(key));
        let boxes = |unit: &UnitView| {
            crate::visuals::HealthBox::of(
                unit.kind
                    .and_then(|kind| kinds.get(Kind(kind)))
                    .map(|entry| &entry.record),
                unit.height,
            )
        };
        state.visuals.push_construction_labels(
            renderer,
            &units,
            &boxes,
            &text_for,
            font,
            crate::visuals::caption_text_px(state.caption_points, renderer.size().0),
            view_proj,
            renderer.size(),
            &mut health_bars,
        );
        let world = state.simulation.state();
        let viewer = state.shroud_viewer;
        let clear = |at: Vec3| {
            !shroud_drawn
                || ids
                    .and_then(|ids| {
                        crate::shroud::with_grid(world, RotwkRules::shroud_ids(ids), |grid| {
                            let (cx, cy) = grid.world_to_cell(at.x, at.y);
                            grid.cell_state(viewer, cx, cy)
                                == mint_sage_shroud::CellVisibility::Visible
                        })
                    })
                    .unwrap_or(true)
        };
        let labels =
            crate::floating_text::deposit_labels(&state.floating, &state.player_colors, clear);
        let in_flight = state.floating.texts.len();
        let claim_text = state
            .visuals
            .claim_share
            .map(|claim| (format!("{}%", claim.percent), claim));
        let labels: Vec<crate::floating_text::Label<'_>> = labels
            .into_iter()
            .chain(
                claim_text
                    .as_ref()
                    .map(|(text, claim)| crate::floating_text::Label {
                        text,
                        at: claim.at,
                        rise_px: 0.0,
                        colour: claim.colour(),
                    }),
            )
            .collect();
        state.visuals.push_floating_texts(
            renderer,
            &labels,
            font,
            view_proj,
            renderer.size(),
            &mut health_bars,
        );
        let viewport = renderer.size();
        if let Some(caption) = state.cinematic.caption.as_ref() {
            state.visuals.push_caption(
                renderer,
                caption,
                &state.cinematic.style,
                font,
                viewport,
                &mut health_bars,
            );
        }
        let timers: Vec<(String, bool)> = state
            .scene
            .script_view
            .hud
            .timers
            .iter()
            .map(|timer| {
                (
                    crate::named_timer::line_text(&state.strings.fetch(&timer.label), timer),
                    crate::named_timer::is_ready(timer),
                )
            })
            .collect();
        if !timers.is_empty() && !state.scene.script_view.ui_hidden {
            state.visuals.push_named_timers(
                renderer,
                &timers,
                &state.named_timer_style,
                font,
                viewport,
                &mut health_bars,
            );
        }
        if report_second(seconds) && state.floating.shown > 0 {
            report.line(format!(
                "floating text: {} deposit(s) shown, {} in flight",
                state.floating.shown, in_flight
            ));
        }
    }
    state
        .units
        .draw(&poses, render_clock, &mut state.scene.drawn, &views);
    play_animation_fx(renderer, state, seconds, report);
    crate::level_up::play_fx(renderer, state, seconds, report);
    state.scene.condition_particles.sync(
        renderer,
        &state.vfs,
        &state.scene.resolver,
        &mut state.scene.particle_scene,
        &state.units,
    );
    if let Some(camera_view) = views.first() {
        let revealed = state.scene.terrain_shading.modulation.shroud == TerrainShroud::Revealed;
        let viewer = state.shroud_viewer;
        let scene = &mut state.scene;
        let units = &state.units;
        let held = ids.and_then(|ids| {
            crate::shroud::with_grid(
                state.simulation.state(),
                RotwkRules::shroud_ids(ids),
                |grid| {
                    let obscured = |point: Vec3| shroud_obscured(grid, viewer, revealed, point);
                    scene.particle_scene.hold_unseen_attachments(
                        camera_view,
                        units,
                        &scene.proxies,
                        &obscured,
                        seconds,
                    );
                },
            )
        });
        if held.is_none() {
            scene.particle_scene.hold_unseen_attachments(
                camera_view,
                units,
                &scene.proxies,
                &|_| false,
                seconds,
            );
        }
    }
    clock.lap(crate::render_perf::stage!("units"));
    if report_second(seconds) && state.scene.condition_particles.attached() > 0 {
        report.line(format!(
            "construction/damage particles: {} drawable(s) with a system attached",
            state.scene.condition_particles.attached()
        ));
    }
    if report_second(seconds) {
        if let Some(ids) = ids {
            report.lines(crate::economy::census(
                state.simulation.state(),
                RotwkRules::economy_ids(ids),
            ));
            report.lines(
                state
                    .simulation
                    .rules()
                    .skirmish_ai_census(state.simulation.state(), ids),
            );
        }
        report.line(state.units.house_color_line());
        report.lines(state.units.body_lines());
        report.lines(state.units.state_lines());
        report.lines(horde_report(&state.simulation));
    }
    decals::push_ground_decals(renderer, state, &units, selector);
    let drew_a_model = |unit: &UnitView| {
        matches!(
            KindArt::of(&state.unit_art, unit.art_kind),
            KindArt::Model(_)
        )
    };
    state
        .visuals
        .push(&units, &cubes, &drew_a_model, &mut state.scene.drawn);
    if let Some((x, y, z)) = goal {
        state.visuals.push_move_hint(
            move_hint_model,
            x,
            y,
            z,
            render_clock,
            &mut state.scene.drawn,
        );
    }
    let eye = state.camera.camera().eye;
    state
        .visuals
        .push_streaks(renderer, &streaks, eye, &mut state.scene.drawn);
    if order_queues {
        let rules = state.simulation.rules();
        let strips: Vec<(Vec<[f32; 3]>, Option<usize>)> = ids
            .map(|ids| rules.selected_queue_strips(state.simulation.state(), ids, selector))
            .unwrap_or_default()
            .into_iter()
            .map(|strip| {
                let lifted = strip
                    .points
                    .iter()
                    .map(|p| {
                        [
                            p.x,
                            p.y,
                            rules.ground_height(p.x, p.y) + crate::visuals::QUEUE_LINE_LIFT,
                        ]
                    })
                    .collect();
                (lifted, strip.loop_from)
            })
            .collect();
        state.visuals.push_order_queues(
            renderer,
            (&state.vfs, &state.scene.resolver),
            &strips,
            eye,
            render_clock,
            &mut state.scene.drawn,
        );
    }
    if obstacle_overlay {
        let cell_size = state.simulation.rules().passability().geometry.cell_size();
        let (static_cells, dynamic_cells) =
            obstacle_overlay_geometry(&state.simulation, &state.scene, state.camera.focus());
        state.visuals.push_obstacle_overlay(
            &static_cells,
            &dynamic_cells,
            cell_size,
            &mut state.scene.drawn,
        );
    }
    clock.lap(crate::render_perf::stage!("visuals"));
    state.scene.particle_scene.draw(
        renderer,
        &state.camera.camera(),
        &mut state.scene.drawn,
        state.units.bones().len(),
    );
    let particle_bones = state.scene.particle_scene.bones();
    let palette: &[mint_render::Mat4] = if particle_bones.is_empty() {
        state.units.bones()
    } else {
        state.scene.palette.clear();
        state.scene.palette.extend_from_slice(state.units.bones());
        state.scene.palette.extend_from_slice(particle_bones);
        &state.scene.palette
    };
    renderer
        .set_bones(palette)
        .map_err(|error| error.about("the bone palette"))?;
    let weather_kind = ids.map_or(mint_sage_modules::WeatherKind::None, |ids| {
        crate::rules::RotwkRules::weather_kind(state.simulation.state(), ids)
    });
    let raining = weather_kind == mint_sage_modules::WeatherKind::Rainy;
    let playable = state.simulation.rules().passability().playable_high;
    state
        .scene
        .snow
        .draw(
            renderer,
            &state.camera.camera(),
            render_clock,
            raining,
            [playable.x, playable.y],
            &mut state.scene.drawn,
        )
        .map_err(|error| error.about("the falling snow"))?;
    clock.lap(crate::render_perf::stage!("particles"));

    state
        .scene
        .cloud_darkening
        .advance(cloud_darkening_look(weather_kind), render_clock);
    let rig = if state.scene.cloud_darkening_on {
        state
            .scene
            .cloud_darkening
            .apply(&state.scene.rig, &state.scene.cloud_setting)
    } else {
        state.scene.rig
    };

    let shadow = match &state.scene.shadows {
        Some(pass) => pass.render(
            renderer,
            state.camera.camera(),
            &rig,
            &state.scene.drawn,
            &state.scene.tuning,
            state.scene.world_bounds,
        )?,
        None => None,
    };
    clock.lap(crate::render_perf::stage!("shadow"));

    {
        let camera = state.camera.camera();
        let seconds = state.scene.particle_seconds;
        state.scene.fx_water.fog = crate::scene::drawn_fog(&state.scene);
        state.scene.fx_water.push(renderer, camera, &rig, seconds);
        let shroud_on = crate::scene::shroud_strength(&state.scene) > 0.0;
        let start = state.scene.fx_span.start;
        for (n, material) in state
            .scene
            .fx_water
            .materials(shroud_on)
            .into_iter()
            .enumerate()
        {
            if let Some(instance) = state.scene.drawn.get_mut(start.saturating_add(n)) {
                instance.custom_material = material;
            }
        }
        state.scene.wave_tracks.push(renderer, seconds);
    }
    clock.lap(crate::render_perf::stage!("water"));

    let view = &state.scene.script_view;
    let mut main_camera = state.camera.camera();
    if let Some(multiplier) = view
        .clip_depth_multiplier
        .filter(|m| m.is_finite() && *m > 0.0)
    {
        main_camera.far *= multiplier;
    }
    let skipped = if view.terrain_render_disabled {
        state.scene.terrain_len.min(state.scene.drawn.len())
    } else {
        0
    };

    state.scene.mirror_counts = crate::reflection::MirrorCounts::default();
    if state.scene.reflection.planar() {
        let frame = crate::reflection::FrameView {
            camera: main_camera,
            target: state.scene.render_target,
            first: skipped,
        };
        if let Err(error) = crate::reflection::render_planar(
            renderer,
            &mut state.scene,
            state.camera.camera(),
            shadow,
            &frame,
        ) {
            eprintln!("water reflection: planar pass failed: {error}");
        }
    }
    let mirror_counts = state.scene.mirror_counts;
    state.scene.mirror_tally.add(mirror_counts);
    clock.lap(crate::render_perf::stage!("mirror"));
    let mut scene = mint_sage_scene::terrain_scene(
        main_camera,
        state.scene.drawn.get(skipped..).unwrap_or_default(),
        &rig,
        shadow,
    );
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene.fog = crate::scene::drawn_fog(&state.scene);
    scene.visibility_mask = crate::scene::drawn_visibility_mask(&state.scene);
    state.scene.model_lights.apply(
        &mut scene,
        state
            .scene
            .terrain_shading
            .cloud_cookie(&state.scene.tuning),
    );
    scene.post = state.scene.post.effects(state.scene.script_bloom);
    scene.contact_shadows = state
        .scene
        .shadows
        .as_ref()
        .and_then(|pass| pass.contact_shadows(&state.scene.tuning));
    let (fill, outline) = drag.map_or((Vec::new(), Vec::new()), drag_geometry);
    let drag_overlay = [
        UiGeometry::new()
            .with_triangles(&fill)
            .with_color([0.25, 1.0, 0.35, 0.12]),
        UiGeometry::new()
            .with_lines(&outline)
            .with_color([0.25, 1.0, 0.35, 0.9]),
    ];
    let size = renderer.size();
    #[allow(clippy::cast_precision_loss)]
    let bars = state.cinematic.letterbox.bars(size.0 as f32, size.1 as f32);
    let bar_triangles: Vec<[f32; 2]> = bars.map_or_else(Vec::new, |bars| {
        [Some(bars.top), bars.bottom]
            .into_iter()
            .flatten()
            .flat_map(|[l, t, r, b]| [[l, t], [r, t], [r, b], [l, t], [r, b], [l, b]])
            .collect()
    });
    let bar_overlay = UiGeometry::new()
        .with_triangles(&bar_triangles)
        .with_color([0.0, 0.0, 0.0, bars.map_or(0.0, |bars| bars.alpha)]);
    let mut overlay: Vec<UiGeometry<'_>> = Vec::new();
    if drag.is_some() {
        overlay.extend(drag_overlay);
    }
    if bars.is_some() {
        overlay.push(bar_overlay);
    }
    if !overlay.is_empty() {
        scene.ui_geometry = &overlay;
    }
    scene.ui_quads = &health_bars;
    let fade_progress = crate::rules::map_scripts::script_frame_progress(
        state.snapshot.frame().get(),
        if interpolate { blend } else { 1.0 },
    );
    scene.screen_fade = state
        .scene
        .script_view
        .fade
        .map(|fade| mint_render::ScreenFade {
            blend: match fade.kind {
                crate::rules::map_scripts::FadeKind::Add => mint_render::FadeBlend::Add,
                crate::rules::map_scripts::FadeKind::Subtract => mint_render::FadeBlend::Subtract,
                crate::rules::map_scripts::FadeKind::Saturate => mint_render::FadeBlend::Saturate,
                crate::rules::map_scripts::FadeKind::Multiply => mint_render::FadeBlend::Multiply,
            },
            value: fade.drawn(fade_progress),
        });
    scene.ordered_screen = apt_overlay;
    let rendered = match state.scene.render_target {
        Some(target) => renderer.render_to_target(target, &scene),
        None => renderer.render(&scene),
    };
    state.visuals.bar_quads = health_bars;
    clock.lap_less(
        crate::render_perf::stage!("render"),
        crate::render_perf::stage!("present"),
        renderer.present_wait(),
    );
    if crate::perf::enabled() {
        let units = state.units.draw_stats();
        crate::render_perf::finish_frame(crate::render_perf::FrameCounts {
            instances: state.scene.drawn.len(),
            drawn: renderer.drawn_instances(),
            culled: renderer.culled_instances(),
            draw_calls: renderer.draw_calls(),
            shadow_drawn: renderer.shadow_drawn_instances(),
            shadow_draw_calls: renderer.shadow_draw_calls(),
            unit_drawables: units.drawables,
            unit_sampled: units.sampled,
            unit_out_of_view: units.out_of_view,
            unit_reused: units.reused,
            unit_instances: units.instances,
            skinned_vertices: units.skinned_vertices,
            uploaded_bytes: units.uploaded_bytes,
            unit_sample_ms: units.sample_s * 1000.0,
            unit_skin_ms: units.skin_s * 1000.0,
            unit_place_ms: units.place_s * 1000.0,
            unit_pair_ms: units.pair_s * 1000.0,
            unit_pose_threads: units.pose_threads,
            unit_pose_cpu_ms: units.pose_cpu_s * 1000.0,
            unit_pose_worst_ms: units.pose_worst_s * 1000.0,
            unit_pose_sample_ms: units.pose_sample_s * 1000.0,
            unit_state_entries: units.state_entries,
            mirror: state.scene.mirror_counts,
            submits: renderer.submits(),
            passes: renderer.passes(),
        });
    }
    rendered
}

fn cloud_darkening_look(
    kind: mint_sage_modules::WeatherKind,
) -> mint_sage_scene::CloudDarkeningLook {
    match kind {
        mint_sage_modules::WeatherKind::Cloudy => mint_sage_scene::CloudDarkeningLook::Cloudy,
        mint_sage_modules::WeatherKind::Rainy => mint_sage_scene::CloudDarkeningLook::Rainy,
        _ => mint_sage_scene::CloudDarkeningLook::Other,
    }
}

fn unit_views(renderer: &Renderer, state: &MatchState) -> Vec<mint_render::Frustum> {
    let camera = state.camera.camera();
    let projection =
        camera.projection_with_depth(renderer.depth_direction(), renderer.aspect_ratio());
    let mut views = vec![mint_render::Frustum::from_view_proj(
        projection.mul_mat4(camera.view()),
    )];
    if state.scene.reflection.reflects_units() {
        for (plane, _) in &state.scene.water_scene.planes {
            let mirrored = crate::reflection::mirrored_camera(camera, *plane);
            views.push(mint_render::Frustum::from_view_proj(
                projection.mul_mat4(mirrored.view()),
            ));
        }
    }
    if let Some(pass) = &state.scene.shadows {
        if let Some(light) = pass.light_view_proj(
            renderer,
            camera,
            &state.scene.rig,
            &state.scene.tuning,
            state.scene.world_bounds,
        ) {
            views.push(mint_render::Frustum::from_view_proj(light));
        }
    }
    views
}

fn drag_geometry(rect: ScreenRect) -> (Vec<[f32; 2]>, Vec<[f32; 2]>) {
    let (x0, y0, x1, y1) = (rect.min_x, rect.min_y, rect.max_x, rect.max_y);
    let (a, b, c, d) = ([x0, y0], [x1, y0], [x1, y1], [x0, y1]);
    let fill = vec![a, b, c, a, c, d];
    let outline = vec![a, b, b, c, c, d, d, a];
    (fill, outline)
}

const OBSTACLE_OVERLAY_CELLS: i32 = 40;

fn obstacle_overlay_geometry(
    simulation: &Simulation<RotwkRules>,
    scene: &MatchScene,
    focus: Vec3,
) -> (Vec<Vec3>, Vec<Vec3>) {
    let passability = simulation.rules().passability();
    let grid = &passability.grid;
    let geometry = passability.geometry;
    let extent = grid.extent();
    let Some(centre) = passability.cell_at(mint_movement::Vec2::new(focus.x, focus.y)) else {
        return (Vec::new(), Vec::new());
    };
    let layer = simulation.rules().ids().and_then(|ids| {
        simulation
            .state()
            .store(ids.obstacles)
            .and_then(|store| store.entities().first().copied())
            .and_then(|entity| simulation.state().get(ids.obstacles, entity))
    });
    let mut static_cells = Vec::new();
    let mut dynamic_cells = Vec::new();
    for dy in -OBSTACLE_OVERLAY_CELLS..=OBSTACLE_OVERLAY_CELLS {
        for dx in -OBSTACLE_OVERLAY_CELLS..=OBSTACLE_OVERLAY_CELLS {
            let (Ok(x), Ok(y)) = (
                u16::try_from(i32::from(centre.x).saturating_add(dx)),
                u16::try_from(i32::from(centre.y).saturating_add(dy)),
            ) else {
                continue;
            };
            let cell = mint_movement::Cell::new(x, y);
            if !extent.contains(cell) {
                continue;
            }
            let static_blocked = mint_movement::Passability::flags(grid, cell)
                .intersects(mint_sage_passability::GROUND.blocked_by);
            let dynamic_blocked = layer.is_some_and(|layer| {
                extent
                    .index(cell)
                    .and_then(|index| usize::try_from(index).ok())
                    .is_some_and(|index| layer.counts().get(index).is_some_and(|count| *count > 0))
            });
            if !static_blocked && !dynamic_blocked {
                continue;
            }
            let world = geometry.centre_of(cell);
            let point = Vec3::new(world.x, world.y, ground_at(scene, world.x, world.y));
            if dynamic_blocked {
                dynamic_cells.push(point);
            } else {
                static_cells.push(point);
            }
        }
    }
    (static_cells, dynamic_cells)
}

fn rotwk_pan_inset_policy(inset: f32, extent_x: f32) -> f32 {
    if !inset.is_finite() {
        return 0.0;
    }
    if extent_x.is_finite() && inset > extent_x * 0.25 {
        return 0.0;
    }
    inset
}

fn active_bounds(state: &MatchState) -> mint_render::Aabb {
    let index = state.simulation.rules().ids().map_or(0, |ids| {
        crate::rules::border::active_index(state.simulation.state(), ids)
    });
    state.scene.terrain.boundary_bounds(index)
}

pub(crate) fn frame_camera(camera: &mut RtsCamera, scene: &MatchScene) {
    let bounds = scene.terrain.playable_bounds();
    if bounds.is_empty() {
        return;
    }
    let center = bounds.center();
    camera.set_focus(Vec3::new(center.x, center.y, center.z));
    let extent = bounds.max() - bounds.min();
    let diagonal = Vec3::new(extent.x, extent.y, 0.0).length();
    set_distance(camera, (diagonal * 0.6).clamp(40.0, 3_000.0));
    camera.pan_bounds = Some(scene.terrain.active_boundary_bounds());
    camera.pan_inset_policy = Some(rotwk_pan_inset_policy);
    let mut world = scene.world_bounds;
    for instance in &scene.drawn {
        if let Some(local) = &instance.bounds {
            world.union(&mint_render::transformed_bounds(local, instance.transform));
        }
    }
    camera.keep_in_view(&world);
}

pub(crate) fn focus_on(camera: &mut RtsCamera, focus: Vec3, distance: f32) {
    camera.set_focus(focus);
    set_distance(camera, distance);
}

fn set_distance(camera: &mut RtsCamera, distance: f32) {
    if !distance.is_finite() || distance <= 0.0 {
        return;
    }
    for _ in 0..64 {
        let current = camera.distance_target();
        if !current.is_finite() || current <= 0.0 {
            return;
        }
        let ratio = current / distance;
        if ratio > 0.99 && ratio < 1.01 {
            break;
        }
        camera.zoom(if ratio > 1.0 { 1.0 } else { -1.0 });
    }
    camera.snap_zoom();
}

fn placement_drag_angle(
    anchor_px: (f32, f32),
    at_px: (f32, f32),
    anchor_world: Vec3,
    at_world: Vec3,
    current: f32,
) -> f32 {
    let (dx, dy) = (at_px.0 - anchor_px.0, at_px.1 - anchor_px.1);
    if dx * dx + dy * dy < PLACEMENT_DRAG_THRESHOLD_PIXELS * PLACEMENT_DRAG_THRESHOLD_PIXELS {
        return current;
    }
    mint_math::f32::atan2(at_world.y - anchor_world.y, at_world.x - anchor_world.x)
}

fn captures_the_release(button: mint_app::PointerButton, pressed: bool, dragging: bool) -> bool {
    !pressed && dragging && matches!(button, mint_app::PointerButton::Primary)
}

impl App for Game {
    fn input(&mut self, event: &InputEvent) {
        self.ensure_snapshot();
        if self.menu_open() && self.menu_input(event) {
            return;
        }
        if self.script_input_disabled() && !cinematic::passes_input_lock(event) {
            return;
        }
        match *event {
            InputEvent::Resized { width, height } => {
                self.viewport = (width, height);
                apply_tactical_lens(&mut self.state.camera, &self.data.camera, self.viewport);
                #[allow(clippy::cast_precision_loss)]
                if let Some(overlay) = self.apt_overlay.as_mut() {
                    overlay.resize(width as f32, height as f32);
                }
            }
            InputEvent::PointerLeft => self.pointer.left(),
            InputEvent::PointerMoved { x, y } => {
                let mut over_ui = false;
                if let Some(targets) = self.command_bar_targets() {
                    route_pointer_moved(&mut self.ui_router, &targets, [x, y]);
                    over_ui = self.ui_router.hovered().is_some();
                }
                if self.overlay_enabled && self.mode == Mode::Match {
                    if let Some(overlay) = self.apt_overlay.as_mut() {
                        over_ui |= overlay.route_native_pointer_move([x, y]);
                        over_ui |= overlay
                            .minimap_rect()
                            .is_some_and(|[l, t, r, b]| x >= l && x <= r && y >= t && y <= b);
                        overlay.refine_command_hover(self.ui_router.hovered());
                    }
                }
                if self.pointer.rotating() && !self.camera_rotation_locked() {
                    if let Some((last_x, last_y)) = self.last_pointer {
                        self.state.camera.rotate(
                            (x - last_x) * ROTATE_SENSITIVITY,
                            (y - last_y) * ROTATE_SENSITIVITY,
                        );
                    }
                }
                self.update_placement_drag_angle((x, y));
                self.pointer_over_ui = over_ui;
                self.last_pointer = Some((x, y));
                self.pointer.moved(x, y);
                if crate::diag::active("input") {
                    let target = self.pointer_target_description(x, y);
                    if let Some(target) =
                        hover_target_changed(&mut self.pointer_hover_target, target)
                    {
                        crate::diag::diag!("input: hover {target}");
                    }
                }
            }
            InputEvent::PointerButton { button, pressed } => {
                if pressed && crate::diag::active("input") {
                    if let Some(position) = self.pointer.position() {
                        let target = self.pointer_target_description(position.0, position.1);
                        crate::diag::diag!(
                            "input: button {button:?} down at ({:.1},{:.1}) under {target}",
                            position.0,
                            position.1
                        );
                    } else {
                        crate::diag::diag!("input: button {button:?} down");
                    }
                } else {
                    crate::diag::diag!(
                        "input: button {button:?} {}",
                        if pressed { "down" } else { "up" }
                    );
                }
                if captures_the_release(button, pressed, self.pointer.drag_rect().is_some()) {
                    if let Some(intent) = self.pointer.button(button, pressed, self.ui_clock) {
                        self.act(intent);
                    }
                    return;
                }
                let mut claimed = false;
                let mut resolved = None;
                let mut pressed_button = None;
                let mut slot_enabled = false;
                if let Some(position) = self.pointer.position() {
                    if let Some(targets) = self.command_bar_targets() {
                        let ui_button = ui_pointer_button(button);
                        let routing = route_pointer_button(
                            &mut self.ui_router,
                            &targets,
                            [position.0, position.1],
                            ui_button,
                            pressed,
                        );
                        if let PointerRouting::Widget(_) = routing {
                            claimed = true;
                            if let Some(id) = slot_widget_id(routing, pressed) {
                                resolved = self.apt_overlay.as_ref().and_then(|o| o.action_for(id));
                                pressed_button =
                                    self.apt_overlay.as_ref().and_then(|o| o.button_for(id));
                                slot_enabled =
                                    self.apt_overlay.as_ref().is_some_and(|o| o.enabled_for(id));
                            }
                        }
                    }
                    if matches!(button, mint_app::PointerButton::Primary)
                        && self.overlay_enabled
                        && self.mode == Mode::Match
                    {
                        if let Some(overlay) = self.apt_overlay.as_mut() {
                            let hit = overlay.route_native_pointer_move([position.0, position.1]);
                            overlay.route_native_pointer_button(pressed);
                            claimed |= hit;
                        }
                    }
                    if self.overlay_enabled && self.mode == Mode::Match {
                        if let Some(rect) =
                            self.apt_overlay.as_mut().and_then(AptOverlay::minimap_rect)
                        {
                            let [l, t, r, b] = rect;
                            if position.0 >= l
                                && position.0 <= r
                                && position.1 >= t
                                && position.1 <= b
                            {
                                claimed = true;
                                let selection_empty = !self
                                    .units()
                                    .iter()
                                    .any(|unit| unit.selected && unit.owner == self.acting_player);
                                let action = radar_click(
                                    button,
                                    pressed,
                                    selection_empty,
                                    self.preferences.alternate_mouse_setup,
                                    self.radar_armed(),
                                );
                                if let Some(action) = action {
                                    let u = ((position.0 - l) / (r - l)).clamp(0.0, 1.0);
                                    let v = ((position.1 - t) / (b - t)).clamp(0.0, 1.0);
                                    if let Some((x, y)) = minimap_world_at(
                                        &self.state.scene.terrain.active_boundary_bounds(),
                                        u,
                                        v,
                                    ) {
                                        match action {
                                            RadarAction::LookAt => {
                                                let z = ground_at(&self.state.scene, x, y);
                                                self.state.camera.jump_to(Vec3::new(x, y, z));
                                                println!("radar: camera to ({x:.1}, {y:.1})");
                                            }
                                            RadarAction::Move => {
                                                let fogged =
                                                    move_hint_point_fogged(&self.state, x, y);
                                                if crate::visuals::shows_move_hint(
                                                    &self.units(),
                                                    fogged,
                                                ) {
                                                    self.state.goal = Some((
                                                        Vec3::new(x, y, 0.0),
                                                        self.state.simulation.frame(),
                                                    ));
                                                }
                                                self.submit(RotwkCommand::MoveTo {
                                                    x,
                                                    y,
                                                    queue: self.order_mode_waypoint,
                                                });
                                                println!("radar: order move to ({x:.1}, {y:.1})");
                                            }
                                            RadarAction::AttackMove => {
                                                self.attack_move_armed = false;
                                                let fogged =
                                                    move_hint_point_fogged(&self.state, x, y);
                                                if crate::visuals::shows_move_hint(
                                                    &self.units(),
                                                    fogged,
                                                ) {
                                                    self.state.goal = Some((
                                                        Vec3::new(x, y, 0.0),
                                                        self.state.simulation.frame(),
                                                    ));
                                                }
                                                self.submit(RotwkCommand::AttackMove {
                                                    x,
                                                    y,
                                                    queue: self.order_mode_waypoint,
                                                });
                                                println!(
                                                    "radar: order attack-move to ({x:.1}, {y:.1})"
                                                );
                                            }
                                            RadarAction::Cast => {
                                                if let Some(power_target) = self.power_target.take()
                                                {
                                                    match self.power_target_order_at(
                                                        &power_target,
                                                        (x, y),
                                                    ) {
                                                        Ok(command) => {
                                                            self.issue_targeted(command);
                                                            println!(
                                                                "radar: cast at ({x:.1}, {y:.1})"
                                                            );
                                                        }
                                                        Err(reason) => {
                                                            println!(
                                                                "radar: cast at ({x:.1}, {y:.1}) refused - {reason}; still armed"
                                                            );
                                                            self.power_target = Some(power_target);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if claimed {
                    if let Some(action) = resolved {
                        if matches!(button, mint_app::PointerButton::Secondary) {
                            self.slot_cancel(&action);
                        } else if slot_enabled {
                            if let Some(name) = &pressed_button {
                                self.state.cinematic.button_flashes.pressed(name);
                            }
                            self.slot_action(action);
                        }
                    }
                    return;
                }
                if pressed && matches!(button, mint_app::PointerButton::Primary) {
                    if let Some(placement) = self.placement.as_mut() {
                        placement.anchor = self.pointer.position();
                    }
                }
                if let Some(intent) = self.pointer.button(button, pressed, self.ui_clock) {
                    self.act(intent);
                }
            }
            InputEvent::Scroll { lines } => {
                crate::diag::diag!("input: wheel {lines}");
                if self.mode == Mode::Match && !self.camera_zoom_locked() {
                    self.state.camera.zoom(lines);
                }
            }
            InputEvent::Key { key, pressed } => self.on_key(key, pressed),
            InputEvent::Modifiers { ctrl } => self.ctrl_held = ctrl,
            _ => {}
        }
    }

    fn frame(&mut self, dt: f32) {
        self.mode_frames.record(dt);
        self.test_controls
            .meter_speed(dt, self.state.simulation.frame());
        self.ui_clock = self
            .ui_clock
            .saturating_add(Duration::from_secs_f32(dt.max(0.0)));
        self.audio.music_frame(dt);
        self.end_game_frame(Duration::from_secs_f32(dt.max(0.0)));
        if self.mode != Mode::Match {
            self.state.camera.set_pan_input(0.0, 0.0);
            self.state.camera.update(dt);
            return;
        }
        self.ensure_snapshot();
        if self.overlay_enabled && self.apt_overlay.is_some() {
            let units = self.units();
            let mut hud = crate::hud::read_hud(&self.state, &self.data, &units, self.viewport);
            if let Some(overlay) = self.apt_overlay.as_ref() {
                let provisional = overlay.provisional_spells();
                if !provisional.is_empty() {
                    hud.spell_book = crate::hud::provisional_holdings(&self.state, provisional);
                }
            }
            self.drive_button_flashes(&mut hud);
            let world_hover = self.world_hover(&units);
            let portrait_hover = hud
                .portrait_object
                .and_then(|entity| self.object_hover(&units, entity));
            if let Some(overlay) = self.apt_overlay.as_mut() {
                overlay.set_world_hover(world_hover, portrait_hover);
            }
            let mut events = Vec::new();
            let mut purchases = Vec::new();
            let mut menu_requests = Vec::new();
            self.feed_objectives_overlay();
            let script_ui = self.state.scene.script_view.ui;
            if let Some(overlay) = self.apt_overlay.as_mut() {
                overlay.set_script_ui(script_ui);
                overlay.frame(&hud, Duration::from_secs_f32(dt.max(0.0)));
                events = overlay.take_events();
                purchases = overlay.take_spell_purchases();
                menu_requests = overlay.take_menu_requests();
                if self.mode == Mode::Match {
                    let map = self.state.scene.terrain.active_boundary_bounds();
                    let rect = overlay.minimap_rect();
                    overlay.drive_radar_pings(
                        &self.radar_events,
                        self.ui_clock.as_secs_f32(),
                        |(x, y)| {
                            let uv = minimap_uv_of(&map, x, y)?;
                            let [l, t, r, b] = rect?;
                            Some((uv[0] * (r - l), uv[1] * (b - t)))
                        },
                    );
                }
            }
            for request in menu_requests {
                self.on_menu_request(request);
            }
            self.sync_ui_panels();
            let menu_open = self.menu_open();
            if menu_open != self.menu_was_open {
                self.menu_was_open = menu_open;
                if menu_open {
                    self.audio.pause();
                } else {
                    self.audio.resume();
                }
                println!(
                    "quit menu: the match is {} at simulation frame {}",
                    if menu_open { "paused" } else { "running" },
                    self.state.simulation.frame().get()
                );
            }
            for science in purchases {
                self.purchase_science(&science);
            }
            for event in events {
                let Some(name) = event.command() else {
                    continue;
                };
                if name == crate::apt_overlay::PLAY_SOUND {
                    if let Some(sound) = event.args().filter(|sound| !sound.is_empty()) {
                        let outcome = self.audio.play(sound, None, None);
                        crate::diag::diag!(
                            "button sound: movie plays {sound} ({})",
                            outcome.label()
                        );
                    }
                    continue;
                }
                if name == crate::hud::SPELL_BAR_PRESSED {
                    if let Some(row) = event.args().and_then(|a| a.parse::<usize>().ok()) {
                        let units = self.units();
                        let hud =
                            crate::hud::read_hud(&self.state, &self.data, &units, self.viewport);
                        if let Some(slot) = hud.spell_bar.get(row) {
                            self.state
                                .cinematic
                                .button_flashes
                                .pressed(&slot.command_button);
                            self.cast_spell(slot.button);
                        }
                    }
                    continue;
                }
                let resolved = crate::hud::hero_button(name).or_else(|| {
                    name.ends_with("_OnBttnHeroSelect")
                        .then(|| self.apt_overlay.as_ref().and_then(AptOverlay::pressed_hero))
                        .flatten()
                        .map(|n| crate::hud::HeroButton::Slot(usize::from(n)))
                });
                let double_click = resolved.is_some_and(|button| {
                    crate::hud::hero_double_click(
                        self.last_hero_press,
                        button,
                        self.ui_clock,
                        crate::hud::HERO_DOUBLE_CLICK_MS,
                    )
                });
                if let Some(button) = resolved {
                    self.last_hero_press = Some((button, self.ui_clock));
                }
                match resolved {
                    Some(crate::hud::HeroButton::Slot(1)) if hud.builders.is_empty() => {
                        let hero = crate::hud::hero_at_slot(&hud, 1).map(|h| h.entity);
                        if let Some(entity) = hero {
                            self.submit(RotwkCommand::SetSelection {
                                replace: true,
                                units: vec![entity],
                            });
                            if double_click {
                                self.jump_camera_to(&units, entity);
                            }
                        }
                    }
                    Some(crate::hud::HeroButton::Slot(1)) => {
                        if let Some(builder) = self.select_idle_builder() {
                            if double_click {
                                self.jump_camera_to(&units, builder);
                            }
                        }
                    }
                    Some(crate::hud::HeroButton::Slot(n)) => {
                        let hero = crate::hud::hero_at_slot(&hud, n).map(|h| h.entity);
                        if let Some(entity) = hero {
                            self.submit(RotwkCommand::SetSelection {
                                replace: true,
                                units: vec![entity],
                            });
                            if double_click {
                                self.jump_camera_to(&units, entity);
                            }
                        }
                    }
                    Some(crate::hud::HeroButton::All) => {
                        let selected: Vec<mint_sim::Entity> =
                            hud.fielded_heroes.iter().map(|h| h.entity).collect();
                        if !selected.is_empty() {
                            self.submit(RotwkCommand::SetSelection {
                                replace: true,
                                units: selected.clone(),
                            });
                            if double_click {
                                self.jump_camera_to_group(&units, &selected);
                            }
                        }
                    }
                    None if name.starts_with("OnBttn") => {
                        println!("palantir: {name} clicked - no screen wired for it yet");
                    }
                    None => {}
                }
            }
        }
        self.sync_script_input();
        self.drive_cinematics(dt);
        let (forward, right) = self.pan_axes();
        let focus_before = self.state.camera.focus();
        self.state.camera.set_pan_input(forward, right);
        self.state.camera.pan_bounds = Some(active_bounds(&self.state));
        self.state.camera.update(dt);
        if forward != 0.0 || right != 0.0 {
            let focus = self.state.camera.focus();
            let (dx, dy) = (focus.x - focus_before.x, focus.y - focus_before.y);
            self.state.scroll_pending += (dx * dx + dy * dy).sqrt();
        }
        let camera_dt = self.script_camera_dt(dt);
        self.drive_script_camera(camera_dt);
    }

    fn update(&mut self, _step: u64) {
        self.check_victory();
        if self.sim_paused() {
            return;
        }
        let scrolled = std::mem::take(&mut self.state.scroll_pending);
        if scrolled > 0.0 {
            self.submit(RotwkCommand::CameraScrolled { distance: scrolled });
        }
        let stepped = match self.state.simulation.rules().ids() {
            Some(ids) => crate::render_state::step_remembering(
                &mut self.state.simulation,
                &mut self.state.previous,
                ids,
            ),
            None => self.state.simulation.tick(),
        };
        crate::perf::note_tick_returned();
        self.state.snapshot.invalidate();
        self.ensure_snapshot();
        if let Err(error) = &stepped {
            eprintln!(
                "simulation tick failed: {}",
                crate::replay_run::chain(error)
            );
            if !self.tick_failed {
                self.tick_failed = true;
                self.failure = Some(crate::crash::Failure::error("simulation", error));
            }
        }
        self.force_update_crash();
        crate::crash::set_tick(self.state.simulation.frame().get());
        let objects = self.state.simulation.state().iter_entities().count();
        if objects != self.live_objects {
            crate::crash::event(format_args!("objects: {} -> {objects}", self.live_objects));
            self.live_objects = objects;
        }
        self.test_controls
            .after_tick(&self.state.simulation, &self.state.kinds);
        if let (Some(stats), Some(ticked)) = (
            self.hero_stats.as_mut(),
            self.state.simulation.frame().get().checked_sub(1),
        ) {
            stats.after_tick(&self.state.simulation, Frame::new(ticked));
        }

        let focus = self.state.camera.camera().target;
        self.audio.set_listener(
            &self.state.camera,
            ground_at(&self.state.scene, focus.x, focus.y),
        );
        let state = &mut self.state;
        let gate = FxShroud::of(state);
        let rules = state.simulation.rules();
        let weather = if state.scene.snowy {
            mint_sage_ini::NuggetWeather::Snowy
        } else {
            mint_sage_ini::NuggetWeather::Normal
        };
        for event in state.fx.collect(&state.simulation) {
            let Some(name) = rules.fx_name(event.list) else {
                continue;
            };
            let at = crate::fx_present::place_of(event, |x, y| ground_at(&state.scene, x, y));
            let object = event.on_object.then_some(event.caller);
            if gate.skips(
                &state.simulation,
                &state.scene.particle_scene,
                name,
                object,
                at,
            ) {
                continue;
            }
            let Some(list) = state.scene.particle_scene.fx_list(name) else {
                continue;
            };
            let world = state.simulation.state();
            let primary = event
                .on_object
                .then(|| rules.fx_facts(world, event.caller, LOCAL_PLAYER))
                .flatten();
            let secondary = event
                .victim
                .and_then(|victim| rules.fx_facts(world, victim, LOCAL_PLAYER));
            let primary = primary
                .as_ref()
                .map(crate::fx_present::ObjectFacts::as_facts);
            let secondary = secondary
                .as_ref()
                .map(crate::fx_present::ObjectFacts::as_facts);
            let kinds = &state.kinds;
            let equivalent = |a: &str, b: &str| {
                kinds
                    .kind(a)
                    .zip(kinds.kind(b))
                    .is_some_and(|(a, b)| kinds.equivalent(a, b))
            };
            let mask = crate::fx_present::admit(
                &list.nuggets,
                primary.as_ref(),
                secondary.as_ref(),
                weather,
                &equivalent,
            );
            for (index, nugget) in list.nuggets.iter().enumerate() {
                if !mask.admits(index) {
                    continue;
                }
                if nugget.keyword.eq_ignore_ascii_case("Sound") {
                    if let Some(sound) = nugget.name.as_deref() {
                        self.audio.play(sound, Some(at), None);
                    }
                } else if nugget.eva_event.is_some() {
                    self.audio.report_fx_eva_event(
                        rules,
                        world,
                        &state.scene.particle_scene,
                        name,
                        event.on_object,
                        event.caller,
                        LOCAL_PLAYER,
                        state.simulation.frame(),
                        [at.x, at.y, at.z],
                    );
                }
            }
        }
        let mut captures: Vec<(u32, [f32; 3])> = Vec::new();
        if let Some(frame) = self
            .state
            .simulation
            .frame()
            .get()
            .checked_sub(1)
            .map(Frame::new)
        {
            let state = &self.state;
            let rules = state.simulation.rules();
            let listener = self.acting_player;
            for event in rules.misc_sounds(state.simulation.state(), frame) {
                let at = Vec3::new(
                    event.at.x,
                    event.at.y,
                    ground_at(&state.scene, event.at.x, event.at.y),
                );
                self.audio.play_misc(
                    event.sound.field(),
                    Some(at),
                    Some(audience_of(rules, listener, event.player)),
                );
            }
            for event in rules.capture_flashes(state.simulation.state(), frame) {
                let [r, g, b, _] = state.player_colors.of(event.capturer);
                captures.push((event.target.index(), [r, g, b]));
            }
        }
        for (id, color) in captures {
            self.state.selection_flash.queue_capture(id, color);
        }
        let mut units = self.units();
        crate::render_state::apply_kind_damage(
            &mut units,
            &self.state.kinds,
            self.state.damage_thresholds,
        );
        if stepped.is_ok() && self.shell_game.is_none() {
            crate::level_up::observe(&mut self.state, &mut self.audio, &units);
            crate::floating_text::float_deposits(&mut self.state, &self.data.floating_text, &units);
            let facts = crate::music_script::MatchFacts {
                local: LOCAL_PLAYER,
                units: &units,
                kinds: &self.state.kinds,
                rules: self.state.simulation.rules(),
                world: self.state.simulation.state(),
            };
            self.audio.music_tick(&facts);
            if let Some(frame) = self.state.simulation.frame().get().checked_sub(1) {
                let announce_frame = Frame::new(frame);
                for player in self
                    .state
                    .simulation
                    .rules()
                    .forfeit_warnings(self.state.simulation.state(), frame)
                {
                    println!(
                        "forfeit: player {}: {}",
                        player.get(),
                        self.state.strings.fetch("GUI:AIForfeitsInWOTRSkirmishNow")
                    );
                }
                let defeats = self
                    .state
                    .simulation
                    .rules()
                    .defeat_facts(self.state.simulation.state(), announce_frame);
                let facts = crate::announcer::AnnouncerFacts {
                    local: LOCAL_PLAYER,
                    units: &units,
                    kinds: &self.state.kinds,
                    rules: self.state.simulation.rules(),
                    world: self.state.simulation.state(),
                    frame: announce_frame,
                    defeats,
                };
                let scene = &self.state.scene;
                self.audio.announce(&facts, &|x, y| ground_at(scene, x, y));
                if let Some(combat) = self
                    .state
                    .simulation
                    .rules()
                    .combat_facts(self.state.simulation.state())
                {
                    crate::radar_events::observe_under_attack(
                        &mut self.radar_events,
                        &units,
                        &self.state.kinds,
                        combat.struck(Frame::new(frame)),
                        LOCAL_PLAYER,
                        self.ui_clock.as_secs_f32(),
                    );
                }
                for (x, y) in self
                    .radar_events
                    .take_unsounded(self.ui_clock.as_secs_f32())
                {
                    self.audio.play_misc("GenericRadarEvent", None, None);
                    println!("radar event: created at ({x:.1}, {y:.1})");
                }
            }
        }
        if crate::diag::wants("fade") {
            let rules = self.state.simulation.rules();
            let world = self.state.simulation.state();
            let now = self.state.simulation.frame();
            self.fade_trace.observe(
                now.get(),
                units.iter().map(|unit| {
                    (
                        unit.entity.index(),
                        rules
                            .mount_opacity(world, unit.entity)
                            .min(rules.disguise_opacity(world, unit.entity))
                            .min(rules.corpse_fade_opacity(world, unit.entity, now))
                            .min(rules.join_fade_opacity(world, unit.entity, now)),
                    )
                }),
            );
        }
        if let Some(frame) = self
            .state
            .simulation
            .frame()
            .get()
            .checked_sub(1)
            .map(Frame::new)
            .filter(|_| self.shell_game.is_none())
        {
            let state = &self.state;
            let rules = state.simulation.rules();
            let world = state.simulation.state();
            let listener = self.acting_player;
            let mut attacked: Vec<(mint_sim::Entity, u16)> = Vec::new();
            let resolve = |event: &crate::voice::VoiceEvent| {
                let raw = rules.voice_target(world, event.entity, LOCAL_PLAYER);
                let target = raw.map(|(position, kind, visible, owner)| {
                    let at = Vec3::new(
                        position.x,
                        position.y,
                        ground_at(&state.scene, position.x, position.y),
                    );
                    let audience = owner.map_or(mint_sage_audio::Audience::Enemy, |owner| {
                        audience_of(rules, listener, owner)
                    });
                    (at, kind, visible, audience)
                });
                let owner = raw.and_then(|(_, _, _, owner)| owner);
                let facts = event
                    .target
                    .and_then(|target| rules.voice_target_facts(world, event.entity, target));
                let move_facts = (event.kind == crate::voice::VoiceKind::Move)
                    .then(|| rules.move_voice_facts(world, event.entity))
                    .flatten();
                (target, facts, move_facts, owner)
            };
            let events = rules.voice_events(world, frame);
            for event in events {
                if event.kind == crate::voice::VoiceKind::Attack {
                    for member in rules.voice_group(world, event.entity) {
                        if let Some((_, kind, _, _)) =
                            rules.voice_target(world, member, LOCAL_PLAYER)
                        {
                            attacked.push((member, kind));
                        }
                    }
                }
            }
            let (order_templates, order_event) = merge_order_votes(events.iter(), |entity| {
                rules
                    .voice_group(world, entity)
                    .into_iter()
                    .filter_map(|member| {
                        rules
                            .voice_target(world, member, LOCAL_PLAYER)
                            .map(|(_, kind, _, _)| kind)
                    })
                    .collect()
            });
            for event in events {
                if is_order_voice(event.kind) {
                    continue;
                }
                let (target, facts, move_facts, _) = resolve(event);
                if target.is_none() {
                    if let Some(owner) = rules.voice_owner(world, event.entity) {
                        let audience = audience_of(rules, listener, owner);
                        self.audio.play_unplaced(event, audience);
                        continue;
                    }
                }
                self.audio
                    .play_voice(event, target, facts, move_facts, frame.get());
            }
            if let Some(event) = order_event {
                crate::diag::diag!(
                    "voice vote: order reached {} member(s)",
                    order_templates.len()
                );
                let (target, facts, move_facts, _) = resolve(&event);
                self.audio.play_voice_vote(
                    &event,
                    &order_templates,
                    target,
                    facts,
                    move_facts,
                    frame.get(),
                );
            }
            for event in events {
                if !is_order_voice(event.kind) {
                    continue;
                }
                let (target, facts, move_facts, owner) = resolve(event);
                if owner.is_some_and(|owner| {
                    rules.command_source(owner) == mint_sage_combat::CommandSource::Player
                }) {
                    self.audio.play_enter_state_voice(
                        event,
                        target,
                        facts,
                        move_facts,
                        frame.get(),
                    );
                }
            }
            for (entity, kind) in attacked {
                self.audio.attack_ordered(entity, kind, frame.get());
            }
        }
        let rules = self.state.simulation.rules();
        let listener = self.acting_player;
        let ability_loops = rules
            .ids()
            .map(|ids| crate::render_state::ability_loop_sounds(self.state.simulation.state(), ids))
            .unwrap_or_default();
        self.audio.observe(&units, &ability_loops, |owner| {
            audience_of(rules, listener, owner)
        });
        if let Some(ids) = rules.ids() {
            let kind = crate::rules::RotwkRules::weather_kind(self.state.simulation.state(), ids);
            self.audio.update_weather(kind);
        }
        if let Some(frame) = self
            .state
            .simulation
            .frame()
            .get()
            .checked_sub(1)
            .map(Frame::new)
        {
            let state = &self.state;
            let rules = state.simulation.rules();
            for event in rules.death_phase_events(state.simulation.state(), frame) {
                let (_, lists) = rules.death_phase_lists(event);
                let Some(name) = crate::fx_present::death_phase_pick(
                    lists,
                    event,
                    frame,
                    crate::fx_present::DeathList::Sound,
                ) else {
                    continue;
                };
                let (x, y) = (event.at.x, event.at.y);
                let z = event.at.z.unwrap_or_else(|| ground_at(&state.scene, x, y));
                let owner = crate::audio::owner_id(event.entity);
                let outcome = self.audio.play(name, Some(Vec3::new(x, y, z)), Some(owner));
                crate::diag::diag!(
                    "death: entity {} {} plays sound {name}: {}",
                    event.entity.index(),
                    event.phase.label(),
                    outcome.label()
                );
            }
        }
    }

    fn speed(&self) -> f64 {
        self.test_controls.speed()
    }

    fn paused(&self) -> bool {
        self.sim_paused()
    }

    fn draw(&mut self, renderer: &mut Renderer, blend: f32) -> mint_render::Result<()> {
        self.draw_frame(renderer, blend, None)
    }
}

impl Game {
    pub(crate) fn draw_world(
        &mut self,
        renderer: &mut Renderer,
        blend: f32,
        extra: &[ScreenDraw<'_>],
    ) -> mint_render::Result<()> {
        self.draw_frame(renderer, blend, Some(extra))
    }

    fn draw_frame(
        &mut self,
        renderer: &mut Renderer,
        blend: f32,
        extra: Option<&[ScreenDraw<'_>]>,
    ) -> mint_render::Result<()> {
        let hud_started = std::time::Instant::now();
        let drag = (self.mode == Mode::Match && self.placement.is_none())
            .then(|| self.pointer.drag_rect())
            .flatten();
        self.last_blend = blend;
        self.ensure_snapshot();
        self.state.scene.script_bloom = self.state.snapshot.script_bloom();
        let view = self.state.snapshot.script_view().clone();
        if view.terrain_render_disabled != self.state.scene.script_view.terrain_render_disabled
            || view.clip_depth_multiplier != self.state.scene.script_view.clip_depth_multiplier
            || view.ui_hidden != self.state.scene.script_view.ui_hidden
        {
            crate::diag::diag!(
                "script: view terrain_off={} clip={} ui_hidden={}",
                view.terrain_render_disabled,
                view.clip_depth_multiplier.unwrap_or(1.0),
                view.ui_hidden
            );
        }
        self.start_script_shake(view.effects);
        self.state.scene.script_view = view;
        self.force_draw_crash(renderer)?;
        self.state.visuals.placement_ghost = self.placement_ghost();
        let mut pieces_shown = self.span_ghosts();
        if let Some(ghost) = self.state.visuals.placement_ghost {
            pieces_shown.extend(self.castle_ghosts(&ghost));
        }
        self.state.visuals.span_ghosts = pieces_shown;
        let (preview, arrow) = self.formation_drag_preview();
        if preview.len() != self.state.visuals.formation_preview.len()
            || arrow.len() != self.state.visuals.formation_arrow.len()
        {
            crate::diag::diag!(
                "formation: preview shows {} decal(s) [{}]{}",
                preview.len(),
                formation_preview_summary(&preview),
                formation_arrow_summary(&arrow)
            );
        }
        self.state.visuals.formation_preview = preview;
        self.state.visuals.formation_arrow = arrow;
        self.state.visuals.radius_cursor = self.radius_cursor_draw(renderer);
        self.state.visuals.claim_ring = self.claim_ring(renderer);
        let claim_share = self.claim_share();
        if claim_share.map(|claim| claim.percent)
            != self.state.visuals.claim_share.map(|claim| claim.percent)
        {
            if let (Some(claim), Some(ghost)) = (claim_share, self.state.visuals.placement_ghost) {
                crate::diag::diag!(
                    "claims: placing at ({:.0}, {:.0}) shows {}%",
                    ghost.x,
                    ghost.y,
                    claim.percent
                );
            }
        }
        self.state.visuals.claim_share = claim_share;
        let mut units: Option<Vec<UnitView>> = None;
        let hovered = if extra.is_some() || self.shell_game.is_some() {
            None
        } else {
            let units = units.insert(self.units());
            self.world_hover(units).map(|(entity, ..)| entity)
        };
        let end_screen = self.mode == Mode::Results && self.end_game.started();
        let overlay_draws: Vec<ScreenDraw<'_>> = if let Some(extra) = extra {
            extra.to_vec()
        } else if self.overlay_enabled && (self.mode == Mode::Match || end_screen) {
            let hud_hidden = self.state.scene.script_view.ui_hidden;
            let units = units.get_or_insert_with(|| self.units());
            let dots = crate::hud::minimap_dots(units, &self.state);
            let view_box = minimap_view_box(&self.state, self.viewport);
            match self.apt_overlay.as_mut() {
                Some(overlay) => overlay.build_and_draw(renderer, &dots, view_box, hud_hidden),
                None => Vec::new(),
            }
        } else if self.mode == Mode::Results && !end_screen {
            let lines = self.results_banner_lines();
            match self.apt_overlay.as_mut() {
                Some(overlay) => overlay.draw_banner(renderer, &lines),
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        let mut overlay_draws = overlay_draws;
        if self.cover_alpha > 0.0 {
            let (width, height) = self.viewport;
            overlay_draws.push(ScreenDraw::Quad(
                mint_render::UiQuad::new(0.0, 0.0, width as f32, height as f32)
                    .with_color_transform([0.0, 0.0, 0.0, self.cover_alpha.min(1.0)], [0.0; 4]),
            ));
        }
        crate::render_perf::add_current(
            crate::render_perf::stage!("hud"),
            hud_started.elapsed().as_secs_f64() * 1000.0,
        );
        let camera = &self.state.camera;
        self.state.scene.fog_shift = if camera.is_scripted() || camera.is_posed() {
            0.0
        } else {
            let limits = crate::camera_limits::CameraLimits::resolve(
                self.state.scene.camera_keys,
                self.data.camera,
            );
            crate::scene::fog_shift(camera.distance(), camera.pitch(), limits.max_height())
        };
        let result = compose_and_render(
            renderer,
            &mut self.state,
            blend,
            true,
            self.obstacle_overlay,
            drag,
            &overlay_draws,
            &mut self.report,
            self.move_hint_model.as_ref(),
            self.acting_player,
            hovered,
            self.preferences.health_bars && self.shell_game.is_none(),
            self.order_mode_waypoint,
        );
        let post_started = std::time::Instant::now();
        crate::audio::play_animation_sounds(&mut self.audio, &mut self.state.units);
        self.print_state_trace();
        self.report.flush();
        crate::crash::frame(format_args!(
            "{} instance(s), {} particle system(s), {} particle(s)",
            self.state.scene.drawn.len(),
            self.state.scene.particle_scene.live_systems(),
            self.state.scene.particle_scene.live_particles()
        ));
        if crate::perf::enabled() {
            crate::render_perf::add_to_last(
                crate::render_perf::stage!("post"),
                post_started.elapsed().as_secs_f64() * 1000.0,
            );
            crate::render_perf::set_audio(self.audio.hitch_counters());
        }
        result
    }
}

fn script_camera_spend(owed: f32, dt: f32, tick: f32) -> f32 {
    dt.max(0.0).min(owed).max(owed - tick)
}

impl Game {
    fn script_camera_dt(&mut self, dt: f32) -> f32 {
        let now = self.state.simulation.frame().get();
        let (seen, owed) = self.state.script_camera_clock;
        let tick = 1.0 / f32::from(u16::try_from(mint_sim::SIMULATION_HZ).unwrap_or(30));
        #[allow(clippy::cast_precision_loss)]
        let owed = owed + now.saturating_sub(seen) as f32 * tick;
        let spend = script_camera_spend(owed, dt, tick);
        self.state.script_camera_clock = (now, owed - spend);
        spend
    }

    fn drive_script_camera(&mut self, dt: f32) {
        let snapshot = self.snapshot();
        let view = snapshot.script_view().clone();
        let (follow_at, look_object_at) = (snapshot.follow_at(), snapshot.look_object_at());
        drop(snapshot);
        let rules = self.state.simulation.rules();
        let defaults = crate::script_camera::DefaultView {
            max_height: self.data.camera.max_height,
            pitch: self.data.camera.pitch_degrees.to_radians(),
            aspect: viewport_aspect(self.viewport),
            limits: crate::camera_limits::CameraLimits::resolve(
                self.state.scene.camera_keys,
                self.data.camera,
            ),
        };
        let input = crate::script_camera::CameraInput {
            moves: view.camera_move.as_ref(),
            ops: &view.camera_ops,
            follow_at,
            look_object_at,
        };
        let started = self.state.scene.script_camera.update(
            &input,
            &mut self.state.camera,
            defaults,
            |x, y| rules.ground_height(x, y),
            dt,
        );
        let pose_dt = if self.menu_open() { 0.0 } else { dt };
        let scene = &mut self.state.scene;
        scene.script_pose.update(
            view.camera_ops.animation.as_ref(),
            &scene.camera_animations,
            started || scene.script_camera.follows(),
            &mut self.state.camera,
            defaults.aspect,
            pose_dt,
        );
        if scene.script_pose.playing() {
            scene.script_camera.end_blend();
        }
        scene.camera_freeze.update(
            view.camera_ops.freeze_seq,
            scene.script_camera.busy() || scene.script_pose.playing(),
        );
        self.drive_camera_restriction();
        let Some(command) = view.camera_move.as_ref().filter(|_| started) else {
            return;
        };
        crate::diag::diag!(
            "script: camera seq={} target={:.0},{:.0} named={} ms={}",
            command.seq,
            command.target.x,
            command.target.y,
            command.camera.is_some(),
            command.ms
        );
    }

    fn sim_paused(&self) -> bool {
        self.menu_open()
            || !crate::mode::steps_simulation(self.mode)
            || self.state.scene.camera_freeze.holds_simulation()
    }

    fn menu_open(&self) -> bool {
        self.overlay_enabled && self.apt_overlay.as_ref().is_some_and(AptOverlay::menu_open)
    }

    pub(super) fn toggle_quit_menu(&mut self) {
        match self.apt_overlay.as_mut() {
            Some(overlay) if self.overlay_enabled => {
                self.pointer.cancel();
                self.ui_router.release_capture();
                overlay.toggle_quit_menu();
            }
            _ => println!("quit menu: the APT overlay is off or did not load - nothing to open"),
        }
    }

    fn menu_input(&mut self, event: &InputEvent) -> bool {
        match *event {
            InputEvent::PointerMoved { x, y } => {
                if let Some(overlay) = self.apt_overlay.as_mut() {
                    overlay.route_native_pointer_move([x, y]);
                }
                self.pointer_over_ui = true;
                self.last_pointer = Some((x, y));
                self.pointer.moved(x, y);
                true
            }
            InputEvent::PointerButton { button, pressed } => {
                if matches!(button, mint_app::PointerButton::Primary) {
                    if let (Some(overlay), Some((x, y))) =
                        (self.apt_overlay.as_mut(), self.last_pointer)
                    {
                        overlay.route_native_pointer_move([x, y]);
                        overlay.route_native_pointer_button(pressed);
                    }
                }
                true
            }
            InputEvent::Key { key, pressed: true } => {
                if key == Key::Escape {
                    self.toggle_quit_menu();
                }
                true
            }
            InputEvent::Scroll { .. } | InputEvent::TextInput { .. } => true,
            _ => false,
        }
    }

    fn on_menu_request(&mut self, request: crate::in_game_menu::MenuRequest) {
        match request {
            crate::in_game_menu::MenuRequest::ExitMission => {
                self.wants_shell = true;
                self.wants_score = true;
                self.quit_local = true;
            }
            crate::in_game_menu::MenuRequest::RestartMission => self.wants_restart = true,
            crate::in_game_menu::MenuRequest::LoadSave(path) => self.load_request = Some(path),
            crate::in_game_menu::MenuRequest::SaveGame { .. } => {}
            crate::in_game_menu::MenuRequest::ApplyOptions(preferences) => {
                self.apply_options(preferences);
            }
        }
    }

    fn apply_options(&mut self, preferences: crate::options::OptionsPreferences) {
        use mint_sage_ini::audio::Submix;
        #[allow(clippy::cast_precision_loss)]
        let gain = |percent: u32| percent.min(100) as f32 / 100.0;
        for (submix, percent) in [
            (Submix::SoundFx, preferences.sound_fx_volume),
            (Submix::Voice, preferences.voice_volume),
            (Submix::Ambient, preferences.ambient_volume),
            (Submix::Movie, preferences.movie_volume),
            (Submix::Music, preferences.music_volume),
        ] {
            self.audio.set_submix_gain(submix, gain(percent));
        }
        println!(
            "options: applied volumes sfx {}% voice {}% ambient {}% movie {}% music {}%",
            preferences.sound_fx_volume,
            preferences.voice_volume,
            preferences.ambient_volume,
            preferences.movie_volume,
            preferences.music_volume
        );
        crate::user_options::store_all_health_bars(preferences.health_bars);
        self.preferences = preferences;
    }

    fn end_match_or_return_to_shell(&mut self) {
        self.pointer.cancel();
        match self.mode {
            Mode::Match => {
                if let Some(next) = transition(self.mode, Event::MatchEnded) {
                    self.mode = next;
                    println!("match ended - press any key to return to the shell");
                }
            }
            Mode::Results => {
                if transition(self.mode, Event::Acknowledged).is_some() {
                    self.wants_shell = true;
                    self.wants_score = true;
                }
            }
            _ => {}
        }
    }

    fn end_game_frame(&mut self, dt: Duration) {
        if !matches!(self.mode, Mode::Match | Mode::Results) {
            return;
        }
        let Some(ids) = self.state.simulation.rules().ids() else {
            return;
        };
        let simulation = &self.state.simulation;
        let allied_victory = crate::clock_entity(simulation.state(), ids)
            .and_then(|clock| {
                simulation
                    .state()
                    .get(RotwkRules::defeat_ids(ids).outcome, clock)
            })
            .map(|outcome| simulation.rules().allied(outcome.winner, LOCAL_PLAYER));
        let allied_victory = match self.mission_end() {
            Some(mint_sage_campaign::MissionEnd::Victory) => Some(true),
            Some(mint_sage_campaign::MissionEnd::Defeat) => Some(false),
            None => allied_victory,
        };
        let local_defeated =
            simulation
                .rules()
                .player_defeated(simulation.state(), ids, LOCAL_PLAYER);
        let mut step = self.end_game.step(allied_victory, local_defeated, dt);
        let victory_screen_seq = self.state.scene.script_view.victory_screen_seq;
        if victory_screen_seq != self.victory_screen_seen && step.is_none() {
            self.victory_screen_seen = victory_screen_seq;
            step = step.or_else(|| self.end_game.show_victory_screen());
        }
        if let (Some(crate::end_game::EndStep::Show(screen)), Some(stats)) =
            (step, self.hero_stats.as_mut())
        {
            stats.finish(screen == crate::end_game::EndScreen::Victory, false);
        }
        let Some(overlay) = self.apt_overlay.as_mut() else {
            return;
        };
        match step {
            Some(crate::end_game::EndStep::Show(screen)) => {
                let shown = overlay.show_end_game(
                    screen.caption_label(),
                    screen.show_arguments(self.state.local_evil),
                );
                println!(
                    "end game: {screen:?} - GuiFX ShowEndGame{}",
                    if shown {
                        ""
                    } else {
                        " not sent, the movie did not load"
                    }
                );
            }
            Some(crate::end_game::EndStep::Hide) => {
                overlay.hide_end_game();
                println!("end game: HideEndGame");
            }
            None => {}
        }
        let sounds = overlay.end_screen_frame(dt);
        let hidden = overlay.end_screen_hidden();
        for sound in sounds {
            self.play_end_game_sound(&sound);
        }
        if self.mode == Mode::Results
            && self.end_game.hidden()
            && hidden
            && !self.wants_shell
            && transition(self.mode, Event::Acknowledged).is_some()
        {
            println!("end game: hidden - on to the post-match screens");
            self.wants_shell = true;
            self.wants_score = true;
        }
    }

    fn play_end_game_sound(&mut self, sound: &str) {
        if sound.is_empty() {
            return;
        }
        let outcome = self.audio.play(sound, None, None);
        println!("end game: sound {sound} ({})", outcome.label());
    }

    fn check_victory(&mut self) {
        if self.mode != Mode::Match || self.shell_game.is_some() {
            return;
        }
        let Some(ids) = self.state.simulation.rules().ids() else {
            return;
        };
        if self.mission_end().is_some() {
            if let Some(next) = transition(self.mode, Event::MatchEnded) {
                self.mode = next;
                println!("match ended - the mission's script ended it");
            }
            return;
        }
        let defeat_ids = RotwkRules::defeat_ids(ids);
        let Some(clock) = crate::clock_entity(self.state.simulation.state(), ids) else {
            return;
        };
        let Some(outcome) = self.state.simulation.state().get(defeat_ids.outcome, clock) else {
            return;
        };
        if let Some(next) = transition(self.mode, Event::MatchEnded) {
            self.mode = next;
            println!("match ended - player {} wins", outcome.winner.get());
        }
    }

    fn results_banner_lines(&self) -> Vec<String> {
        results_banner_lines(&self.state.simulation)
    }
}

pub(crate) fn results_banner_lines(simulation: &Simulation<RotwkRules>) -> Vec<String> {
    let mut lines = Vec::new();
    let outcome = simulation.rules().ids().and_then(|ids| {
        let defeat_ids = RotwkRules::defeat_ids(ids);
        let clock = crate::clock_entity(simulation.state(), ids)?;
        simulation.state().get(defeat_ids.outcome, clock)
    });
    match outcome {
        Some(outcome) => {
            let you_won = simulation.rules().allied(outcome.winner, LOCAL_PLAYER);
            lines.push(String::from(if you_won { "VICTORY" } else { "DEFEAT" }));
            lines.push(format!(
                "player {} wins at frame {}",
                outcome.winner.get(),
                outcome.frame.get()
            ));
        }
        None => lines.push(String::from("MATCH ENDED")),
    }
    lines.push(String::new());
    lines.extend(match_census(simulation));
    lines.push(String::new());
    lines.push(String::from("press any key or click to return to the menu"));
    lines
}

fn formation_preview_summary(items: &[crate::rules::FormationPreviewItem]) -> String {
    let mut parts = Vec::new();
    let mut at = 0;
    while let Some(first) = items.get(at) {
        let kind = if first.item_decal { "item" } else { "slot" };
        let mut count = 1usize;
        while items
            .get(at.saturating_add(count))
            .is_some_and(|item| item.slot == first.slot)
        {
            count = count.saturating_add(1);
        }
        parts.push(format!("slot {}: {kind} x{count}", first.slot));
        at = at.saturating_add(count);
    }
    parts.join(", ")
}

fn formation_arrow_summary(pieces: &[crate::rules::ArrowPiece]) -> String {
    if pieces.is_empty() {
        return String::new();
    }
    let centres: Vec<String> = pieces
        .iter()
        .map(|piece| format!("({:.0},{:.0})", piece.centre.x, piece.centre.y))
        .collect();
    format!(", arrow at {}", centres.join(" "))
}

pub(crate) fn queue_button(key: char) -> Option<u8> {
    match key {
        '1'..='9' => u8::try_from(u32::from(key).saturating_sub(u32::from('1'))).ok(),
        _ => None,
    }
}

pub(crate) fn parse_modes(search: Option<&str>, smoothing: Option<&str>) -> Result<Modes, String> {
    let mut modes = Modes::default();
    if let Some(name) = search {
        modes.search = match name {
            "straight-line" => SearchMode::StraightLine,
            "a*" | "astar" => SearchMode::AStar,
            other => return Err(format!("--search wants straight-line or a*, got {other}")),
        };
    }
    if let Some(name) = smoothing {
        modes.smoothing = match name {
            "off" => SmoothingMode::Off,
            "string-pull" => SmoothingMode::StringPull,
            other => return Err(format!("--smoothing wants off or string-pull, got {other}")),
        };
    }
    modes.avoidance = AvoidanceMode::Off;
    Ok(modes)
}

pub(crate) fn write_tga(
    path: &std::path::Path,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> std::io::Result<()> {
    let (w, h) = (
        u16::try_from(width).unwrap_or(u16::MAX),
        u16::try_from(height).unwrap_or(u16::MAX),
    );
    let mut out = Vec::with_capacity(18usize.saturating_add(rgba.len()));
    out.extend_from_slice(&[0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&[32, 0x20]);
    for pixel in rgba.chunks_exact(4) {
        match pixel {
            [r, g, b, a] => out.extend_from_slice(&[*b, *g, *r, *a]),
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "pixel buffer length was not a multiple of 4",
                ))
            }
        }
    }
    std::fs::write(path, out)
}
