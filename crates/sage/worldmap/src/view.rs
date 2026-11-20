use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use mint_render::{
    Blend, Camera, DirectionalLight, Instance, Mat4, Ray, Renderer, RibbonStyle, Ribbons, Scene,
    TextureHandle, Vec3, MAX_DIRECTIONAL_LIGHTS,
};
use mint_sage_ini::living_world::campaign::ActNugget;
use mint_sage_ini::living_world::effects::{EffectDef, RegionEffectsDef};
use mint_sage_ini::living_world::presentation::{ArmyIconDef, MapInfoDef};
use mint_sage_ini::living_world::{LivingWorldData, Rgb};
use mint_sage_livingworld::{ArmyId, LivingWorldState, Phase, PlayerId, RegionId};
use mint_sage_model::{
    load_house_colors, load_model, load_texture, CachedModel, ModelCache, Resolver,
};
use mint_vfs::Vfs;

use crate::ambient::Ambient;
use crate::camera::WorldCamera;
use crate::controls::ScrollControls;
use crate::curve::{Curve, CLIENT_HZ};
use crate::layout::{
    sample_line, walk_speed_scale, walks, ArmyMark, Layout, Slots, TerritoryView, Walk,
};
use crate::pick::{IconPick, MapPick, RegionPicker};
use crate::sound::{self, MapSound};

const MAX_STEPS_PER_ADVANCE: u32 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerColors {
    pub fill: [f32; 3],
    pub banner: [f32; 3],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Overlay {
    #[default]
    Ownership,
    Plain,
}

impl Overlay {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Ownership => Self::Plain,
            Self::Plain => Self::Ownership,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ownership => "ownership",
            Self::Plain => "plain",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreviewMarks {
    pub enabled: Vec<RegionId>,
    pub filled: Vec<(RegionId, Option<[f32; 3]>)>,
    pub homes: Vec<(RegionId, Option<[f32; 3]>)>,
    pub startable: Vec<RegionId>,
    pub hovered: Vec<RegionId>,
}

struct Marking {
    borders: Vec<usize>,
    fill: Vec<(usize, [f32; 3], Option<PlayerId>)>,
    homes: Vec<(usize, [f32; 3])>,
    hover: Vec<usize>,
    colour_of: Vec<[f32; 3]>,
}

#[derive(Debug, Clone, Default)]
pub struct LoadReport {
    pub missing: Vec<(String, String)>,
    pub effects: Vec<(String, usize, usize, usize)>,
    pub unmatched_regions: Vec<String>,
    pub pick_triangles: usize,
    pub center_offset: [f32; 2],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrawSummary {
    pub region_owners: Vec<Option<PlayerId>>,
    pub army_icons: usize,
    pub army_lines: usize,
    pub markers: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapEvent {
    pub territory: usize,
    pub title: &'static str,
    pub text: &'static str,
    pub eva: Option<String>,
    pub look_at: Option<[f32; 2]>,
    pub heading: f32,
    pub zoom: f32,
}

const UNIFIED_TITLE: &str = "LW:UnifiedRegionTitle";
const UNIFIED_TEXT: &str = "LW:UnifiedRegionText";

const DEFAULT_LINE_WIDTH: f32 = 6.0;
const DEFAULT_BEACON_HEIGHT: f32 = 100.0;

struct EffectModel {
    model: CachedModel,
    by_key: Vec<Vec<usize>>,
    curve: Curve,
}

struct Conquest {
    region: usize,
    start: u32,
    color: [f32; 3],
}

const TERRITORY_FLASH: [f32; 3] = [0.95, 0.95, 0.95];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtEnd {
    Hold,
    Hide,
}

impl EffectModel {
    fn push(
        &self,
        keys: &[usize],
        colour: [f32; 3],
        frames: u32,
        at_end: AtEnd,
        out: &mut Vec<Instance>,
    ) {
        let frame = frames.saturating_add(1);
        if at_end == AtEnd::Hide && self.curve.ended(frame) {
            return;
        }
        let k = self.curve.value(frame);
        for key in keys {
            for index in self.by_key.get(*key).into_iter().flatten() {
                if let Some(part) = self.model.parts.get(*index) {
                    let mut instance = part.instance;
                    instance.material_emissive = [colour[0] * k, colour[1] * k, colour[2] * k];
                    out.push(instance);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArmyLine {
    pub army: mint_sage_livingworld::ArmyId,
    pub color: [f32; 3],
    pub points: Vec<[f32; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineRelation {
    Unowned,
    Allied,
    Attacking,
}

fn lines_shown(phase: Phase) -> bool {
    matches!(phase, Phase::Planning | Phase::PlanRetreats)
}

fn line_color(info: &MapInfoDef, relation: LineRelation) -> [f32; 3] {
    let colour = match relation {
        LineRelation::Unowned => info.army_line_color_neutral,
        LineRelation::Allied => info.army_line_color_allied,
        LineRelation::Attacking => info.army_line_color_attacking,
    };
    rgb(colour, [1.0; 3])
}

fn region_conquered_sound(
    data: &LivingWorldData,
    campaign: &str,
    scenario: usize,
) -> Option<String> {
    data.campaigns
        .get(campaign)
        .and_then(|c| c.scenarios.get(scenario))
        .and_then(|s| s.region_campaign.as_deref())
        .and_then(|name| data.region_campaigns.get(name))
        .and_then(|rc| rc.region_conquered_sound.clone())
}

#[must_use]
pub fn light_ray_direction(x_deg: f32, y_deg: f32) -> [f32; 3] {
    let to_rad = core::f32::consts::PI / 180.0;
    let (sin_b, cos_b) = mint_math::f32::sin_cos((x_deg + 90.0) * to_rad);
    let (sin_a, cos_a) = mint_math::f32::sin_cos(y_deg * to_rad);
    [sin_b * cos_a, sin_b * sin_a, cos_b]
}

pub struct WorldMap {
    vfs: Arc<Vfs>,
    resolver: Resolver,
    cache: ModelCache,
    info: MapInfoDef,
    effects_def: RegionEffectsDef,
    pub layout: Layout,
    pub camera: WorldCamera,
    pub overlay: Overlay,
    colors: Vec<PlayerColors>,
    map: Option<Arc<CachedModel>>,
    fill: Option<EffectModel>,
    border: Option<EffectModel>,
    highlight: Option<EffectModel>,
    selection: Option<EffectModel>,
    mouseover: Option<EffectModel>,
    unified: Vec<EffectModel>,
    conquered_effects: Vec<EffectModel>,
    conquered: Vec<Conquest>,
    owners: Vec<Option<PlayerId>>,
    battles: Vec<RegionId>,
    conquered_sound: Option<String>,
    sounds: Vec<MapSound>,
    ambient: Ambient,
    icons: BTreeMap<String, ArmyIconDef>,
    icon_models: BTreeMap<String, Option<CachedModel>>,
    pick_models: BTreeMap<String, Option<Arc<[[Vec3; 3]]>>>,
    icon_picks: Vec<IconPick>,
    picker: RegionPicker,
    hovered: Option<(RegionId, u32)>,
    picked_army: Option<ArmyId>,
    hovered_army: Option<ArmyId>,
    icon_fades: BTreeMap<(ArmyId, String, usize), crate::fade::ObjectFade>,
    selected: Option<(RegionId, u32)>,
    frame: u32,
    frame_rest: f32,
    region_start: u32,
    events: VecDeque<MapEvent>,
    event_start: Option<u32>,
    started: Vec<MapEvent>,
    local: Option<PlayerId>,
    pub shift_held: bool,
    pub turbo_enabled: bool,
    pub controls: ScrollControls,
    pub hold_events: bool,
    ribbons: Ribbons,
    line_texture: Option<TextureHandle>,
    palantir: Vec<[f32; 2]>,
    slots: Slots,
    marks: Vec<ArmyMark>,
    walking: Vec<Walk>,
    instances: Vec<Instance>,
    summary: DrawSummary,
}

fn rgb(c: Option<Rgb>, fallback: [f32; 3]) -> [f32; 3] {
    c.map_or(fallback, |c| [c.r, c.g, c.b])
}

enum PickSource<'a> {
    Pickbox(&'a str),
    Drawn {
        model: &'a str,
        sub_objects: &'a [String],
    },
}

impl<'a> PickSource<'a> {
    fn of(pickbox: Option<&'a str>, model: &'a str, sub_objects: &'a [String]) -> Self {
        match pickbox {
            Some(pickbox) => Self::Pickbox(pickbox),
            None => Self::Drawn { model, sub_objects },
        }
    }

    fn model(&self) -> &str {
        match self {
            Self::Pickbox(name) => name,
            Self::Drawn { model, .. } => model,
        }
    }

    fn key(&self) -> String {
        match self {
            Self::Pickbox(name) => format!("pickbox:{}", name.to_ascii_lowercase()),
            Self::Drawn { model, sub_objects } => {
                format!("{}:{}", model.to_ascii_lowercase(), sub_objects.join(" "))
            }
        }
    }

    fn takes(&self, part: &mint_sage_model::Part) -> bool {
        match self {
            Self::Pickbox(_) => true,
            Self::Drawn {
                sub_objects: [], ..
            } => !part.hidden && !part.pick_proxy,
            Self::Drawn { sub_objects, .. } => {
                !part.pick_proxy
                    && sub_objects
                        .iter()
                        .any(|s| matches_sub_object(&part.name, s))
            }
        }
    }
}

fn matches_sub_object(mesh: &str, sub_object: &str) -> bool {
    mesh.eq_ignore_ascii_case(sub_object)
        || mesh
            .rsplit('.')
            .next()
            .is_some_and(|tail| tail.eq_ignore_ascii_case(sub_object))
}

impl WorldMap {
    pub fn load(
        renderer: &mut Renderer,
        vfs: Arc<Vfs>,
        data: &LivingWorldData,
        state: &LivingWorldState,
        colors: Vec<PlayerColors>,
    ) -> (Self, LoadReport) {
        let (mut world, report) = Self::load_scenario(
            renderer,
            vfs,
            data,
            &state.rules.campaign,
            state.rules.scenario,
            colors,
        );
        world.owners = state.regions.iter().map(|r| r.owner).collect();
        world.battles = state.battles.iter().map(|b| b.region).collect();
        let mut slots = Slots::default();
        world.marks = world.layout.place(state, &mut slots);
        world.slots = slots;
        (world, report)
    }

    pub fn load_preview(
        renderer: &mut Renderer,
        vfs: Arc<Vfs>,
        data: &LivingWorldData,
        campaign: &str,
        scenario: usize,
    ) -> (Self, LoadReport) {
        let (mut world, report) =
            Self::load_scenario(renderer, vfs, data, campaign, scenario, Vec::new());
        world.camera = WorldCamera::shell(&world.info);
        (world, report)
    }

    fn load_scenario(
        renderer: &mut Renderer,
        vfs: Arc<Vfs>,
        data: &LivingWorldData,
        campaign: &str,
        scenario: usize,
        colors: Vec<PlayerColors>,
    ) -> (Self, LoadReport) {
        let mut report = LoadReport::default();
        let resolver = Resolver::new();
        let mut cache = ModelCache::new();
        cache.set_house_colors(Arc::new(load_house_colors(&vfs).0));
        let info = data.map_info.clone().unwrap_or_default();
        let layout = Layout::of(data, campaign, scenario);
        let manager = data
            .campaigns
            .get(campaign)
            .and_then(|c| c.scenarios.get(scenario))
            .and_then(|s| s.region_campaign.as_deref())
            .and_then(|name| data.region_campaigns.get(name))
            .and_then(|rc| rc.region_effects_manager_name.clone());
        let effects_def = manager
            .and_then(|name| data.region_effects.get(&name).cloned())
            .or_else(|| data.region_effects.iter().next().cloned())
            .unwrap_or_default();

        let map = info.map_object.as_deref().and_then(|name| {
            let model = cache.get_or_load(renderer, &vfs, &resolver, name);
            if model.is_none() {
                report
                    .missing
                    .push((name.to_owned(), "did not load".to_owned()));
            }
            model
        });

        let region_keys: Vec<String> = layout
            .regions
            .iter()
            .map(|r| r.sub_object.clone())
            .collect();
        let mut effect = |name: &str, keys: &[String], report: &mut LoadReport| {
            let def: Option<&EffectDef> = effects_def.effect(name);
            let geometry = def.and_then(|d| d.geometry.first())?.clone();
            effect_model(
                renderer, &vfs, &resolver, &mut cache, &geometry, def, keys, report,
            )
        };
        let fill = effect("FilledOwnershipEffect", &region_keys, &mut report);
        let border = effect("BordersEffect", &region_keys, &mut report);
        let highlight = effect("HomeRegionHighlight", &region_keys, &mut report);
        let selection = effect("RegionSelectionEffect", &region_keys, &mut report);
        let mouseover = effect("MouseoverEffectFlareup", &region_keys, &mut report);
        let conquered_effects: Vec<EffectModel> =
            ["ConqueredEffectEvenglow", "ConqueredEffectFlareup"]
                .into_iter()
                .filter_map(|name| effect(name, &region_keys, &mut report))
                .collect();

        let mut unified = Vec::new();
        if let Some(def) = effects_def.effect("UnifiedEffect") {
            for geometry in &def.geometry {
                if let Some(model) = effect_model(
                    renderer,
                    &vfs,
                    &resolver,
                    &mut cache,
                    geometry,
                    Some(def),
                    &layout.territory_effects,
                    &mut report,
                ) {
                    unified.push(model);
                }
            }
        }

        let mut picker = RegionPicker::default();
        if let Some(object) = &effects_def.region_object {
            match load_model(&vfs, &resolver, object) {
                Ok(loaded) => {
                    let mut offsets = Vec::new();
                    for (index, view) in layout.regions.iter().enumerate() {
                        let region = RegionId(u16::try_from(index).unwrap_or(u16::MAX));
                        let mut matched = false;
                        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                        for part in loaded
                            .model
                            .parts
                            .iter()
                            .filter(|p| matches_sub_object(&p.name, &view.sub_object))
                        {
                            let world: Vec<Vec3> = part
                                .mesh
                                .vertices
                                .iter()
                                .map(|v| {
                                    let [x, y, z] = v.position;
                                    part.transform.transform_point(Vec3::new(x, y, z))
                                })
                                .collect();
                            for p in &world {
                                lo = [lo[0].min(p.x), lo[1].min(p.y)];
                                hi = [hi[0].max(p.x), hi[1].max(p.y)];
                            }
                            picker.add_mesh(region, &world, &part.mesh.indices);
                            matched = true;
                        }
                        if matched {
                            offsets.push([
                                (lo[0] + hi[0]) * 0.5 - view.center[0],
                                (lo[1] + hi[1]) * 0.5 - view.center[1],
                            ]);
                        } else {
                            report.unmatched_regions.push(view.name.clone());
                        }
                    }
                    if let Some(n) = u16::try_from(offsets.len()).ok().filter(|n| *n > 0) {
                        let n = f32::from(n);
                        let sum = offsets
                            .iter()
                            .fold([0.0f32; 2], |s, o| [s[0] + o[0], s[1] + o[1]]);
                        report.center_offset = [sum[0] / n, sum[1] / n];
                    }
                }
                Err(error) => report.missing.push((object.clone(), error.to_string())),
            }
        }
        report.pick_triangles = picker.len();

        let icons = data
            .army_icons
            .iter()
            .map(|icon| (icon.name.to_ascii_lowercase(), icon.clone()))
            .collect();
        let line_texture = info.army_line_texture_name.as_deref().and_then(|name| {
            let loaded = load_texture(&vfs, &resolver, name, renderer.supports_block_compression());
            let uploaded = match loaded {
                Ok(t) => renderer.upload_texture(&t.data).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            match uploaded {
                Ok(handle) => Some(handle),
                Err(why) => {
                    report.missing.push((name.to_owned(), why));
                    None
                }
            }
        });
        let camera = WorldCamera::new(&info);
        let conquered_sound = region_conquered_sound(data, campaign, scenario);
        let look_points = data
            .campaigns
            .get(campaign)
            .map(|c| {
                c.acts
                    .iter()
                    .flat_map(|act| &act.nuggets)
                    .filter_map(|nugget| match nugget {
                        ActNugget::EyeTowerPoints(points) => Some(&points.look_point),
                        _ => None,
                    })
                    .flatten()
                    .map(|p| [p.x, p.y])
                    .collect()
            })
            .unwrap_or_default();
        let ambient = Ambient::load(
            renderer,
            &vfs,
            &resolver,
            &mut cache,
            data,
            &info,
            look_points,
        );
        let opening: Vec<_> = data.sounds.iter().cloned().collect();
        let sounds = sound::opening(&info, &opening, camera.focus());
        let world = Self {
            vfs,
            resolver,
            cache,
            info,
            effects_def,
            layout,
            camera,
            overlay: Overlay::default(),
            colors,
            map,
            fill,
            border,
            highlight,
            selection,
            mouseover,
            unified,
            conquered_effects,
            conquered: Vec::new(),
            owners: Vec::new(),
            battles: Vec::new(),
            conquered_sound,
            sounds,
            ambient,
            icons,
            icon_models: BTreeMap::new(),
            pick_models: BTreeMap::new(),
            icon_picks: Vec::new(),
            picker,
            hovered: None,
            picked_army: None,
            hovered_army: None,
            icon_fades: BTreeMap::new(),
            selected: None,
            frame: 0,
            frame_rest: 0.0,
            region_start: 0,
            events: VecDeque::new(),
            event_start: None,
            started: Vec::new(),
            local: None,
            shift_held: false,
            turbo_enabled: false,
            controls: ScrollControls::default(),
            hold_events: false,
            ribbons: Ribbons::default(),
            line_texture,
            palantir: Vec::new(),
            slots: Slots::default(),
            marks: Vec::new(),
            walking: Vec::new(),
            instances: Vec::new(),
            summary: DrawSummary::default(),
        };
        (world, report)
    }

    pub fn observe(&mut self, state: &LivingWorldState) {
        let after = self.layout.place(state, &mut self.slots);
        let started = walks(&self.layout, &self.marks, &after);
        let moved: Vec<ArmyId> = started.iter().map(|w| w.army).collect();
        self.sounds
            .extend(sound::army_sounds(&self.icons, &self.marks, &after, &moved));
        for walk in started {
            self.walking.retain(|w| w.army != walk.army);
            self.walking.push(walk);
        }
        self.note_changes(state);
        self.walking.retain(|w| !w.done());
        self.marks = after;
        self.region_start = self.frame;
    }

    fn note_changes(&mut self, state: &LivingWorldState) {
        let owners: Vec<Option<PlayerId>> = state.regions.iter().map(|r| r.owner).collect();
        for region in sound::conquered_regions(&self.owners, &owners) {
            self.sounds
                .extend(self.conquered_sound.as_deref().map(MapSound::global));
            let color = owners
                .get(region)
                .copied()
                .flatten()
                .and_then(|p| self.colors.get(p.index()))
                .map_or([1.0; 3], |c| c.fill);
            self.conquered.retain(|c| c.region != region);
            self.conquered.push(Conquest {
                region,
                start: self.frame,
                color,
            });
        }
        self.owners = owners;
        let battles: Vec<RegionId> = state.battles.iter().map(|b| b.region).collect();
        if let Some(event) = self.info.battle_marker_created_sound.as_deref() {
            for _ in 0..sound::fresh(&self.battles, &battles) {
                self.sounds.push(MapSound::global(event));
            }
        }
        self.battles = battles;
    }

    pub fn take_sounds(&mut self) -> Vec<MapSound> {
        std::mem::take(&mut self.sounds)
    }

    pub fn army_selected(&mut self, army: ArmyId) {
        let sound = self
            .marks
            .iter()
            .find(|m| m.army == army)
            .and_then(|m| sound::selected_sound(&self.icons, m));
        self.sounds.extend(sound);
    }

    pub fn part_the_heavens(&mut self) -> bool {
        if !self.ambient.part_the_heavens(self.frame) {
            return false;
        }
        self.sounds.extend(
            self.info
                .anim_rays_create_sound
                .as_deref()
                .map(MapSound::global),
        );
        true
    }

    pub fn toggle_ambient(&mut self) -> bool {
        self.ambient.enabled = !self.ambient.enabled;
        self.ambient.enabled
    }

    #[must_use]
    pub fn ambient_summary(&self) -> String {
        self.ambient.summary()
    }

    pub fn set_local_player(&mut self, player: Option<PlayerId>) {
        self.local = player;
    }

    pub fn territory_unified(&mut self, player: PlayerId, territory: u16) {
        if self.local != Some(player) {
            return;
        }
        let index = usize::from(territory);
        let Some(view) = self.layout.territories.get(index) else {
            return;
        };
        let TerritoryView {
            eva,
            look_at,
            heading,
            zoom,
            ..
        } = view.clone();
        self.events.push_back(MapEvent {
            territory: index,
            title: UNIFIED_TITLE,
            text: UNIFIED_TEXT,
            eva,
            look_at,
            heading,
            zoom,
        });
    }

    pub fn take_started(&mut self) -> Vec<MapEvent> {
        std::mem::take(&mut self.started)
    }

    #[must_use]
    pub fn events_pending(&self) -> usize {
        self.events.len()
    }

    pub fn zoom_in_on(&mut self, region: RegionId) -> Option<([f32; 2], f32)> {
        let centre = self.layout.regions.get(region.index())?.center;
        let back = (self.camera.pan_point(), self.camera.zoom());
        self.camera.fly_to(centre, 0.0, self.camera.yaw());
        Some(back)
    }

    pub fn close_event(&mut self) {
        if self.event_start.take().is_some() {
            self.events.pop_front();
        }
    }

    pub fn set_palantir_markers(&mut self, points: Vec<[f32; 2]>) {
        self.palantir = points;
    }

    pub fn advance(&mut self, seconds: f32) {
        if seconds.is_finite() && seconds > 0.0 {
            self.frame_rest += seconds * CLIENT_HZ;
        }
        let whole = self.frame_rest.floor().clamp(0.0, 1.0e9);
        self.frame_rest -= whole;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = whole as u32;
        let steps = frames.min(MAX_STEPS_PER_ADVANCE);
        for _ in 0..steps {
            self.frame = self.frame.saturating_add(1);
            self.step_frame();
        }
        self.frame = self.frame.saturating_add(frames.saturating_sub(steps));
    }

    fn step_frame(&mut self) {
        self.step_fades();
        if self.event_start.is_none() {
            if let Some(event) = self.events.front() {
                self.event_start = Some(self.frame);
                if let Some(at) = event.look_at {
                    self.camera.fly_to(at, event.zoom, event.heading);
                }
                self.started.push(event.clone());
            }
        }
        if let Some(start) = self.event_start {
            let age = self.frame.saturating_sub(start).saturating_add(1);
            let over = self.unified.iter().all(|m| m.curve.ended(age));
            if over && !self.camera.flying() && !self.hold_events {
                self.close_event();
            }
        }
        if self.events.is_empty() && !self.walking.is_empty() {
            let scale = walk_speed_scale(self.shift_held, self.turbo_enabled);
            for walk in &mut self.walking {
                walk.step(scale);
            }
            self.walking.retain(|w| !w.done());
        }
        let frame = self.frame;
        let effects = &self.conquered_effects;
        self.conquered.retain(|c| {
            let age = frame.saturating_sub(c.start).saturating_add(1);
            effects.iter().any(|m| !m.curve.ended(age))
        });
        self.ambient.step(frame);
        self.controls.frame(&mut self.camera);
        self.camera.tick();
    }

    #[must_use]
    pub fn pick(&self, viewport: (u32, u32), x: f32, y: f32) -> Option<MapPick> {
        let (w, h) = (
            f32::from(u16::try_from(viewport.0).unwrap_or(u16::MAX)),
            f32::from(u16::try_from(viewport.1).unwrap_or(u16::MAX)),
        );
        let ray: Ray = self
            .camera
            .camera_for(w / h.max(1.0))
            .ray_through_pixel(viewport, x, y)?;
        self.picker.pick_map(ray, &self.icon_picks)
    }

    #[must_use]
    pub fn pick_region(&self, viewport: (u32, u32), x: f32, y: f32) -> Option<RegionId> {
        match self.pick(viewport, x, y)? {
            MapPick::Army { region, .. } | MapPick::Region(region) => Some(region),
        }
    }

    #[must_use]
    pub fn icon_pixel(&self, viewport: (u32, u32), army: ArmyId) -> Option<(f32, f32)> {
        let b = self.icon_picks.iter().find(|b| b.army == army)?;
        let centre = b.placement.transform_point(b.centre);
        let (w, h) = (
            f32::from(u16::try_from(viewport.0).ok()?),
            f32::from(u16::try_from(viewport.1).ok()?),
        );
        let aspect = w / h.max(1.0);
        let ndc = self
            .camera
            .camera_for(aspect)
            .view_projection(aspect)
            .transform_point(centre);
        Some(((ndc.x + 1.0) * 0.5 * w, (1.0 - ndc.y) * 0.5 * h))
    }

    pub fn set_hovered(&mut self, region: Option<RegionId>) {
        if self.hovered.map(|(r, _)| r) != region {
            self.hovered = region.map(|r| (r, self.frame));
        }
    }

    pub fn set_selected_army(&mut self, army: Option<ArmyId>) {
        self.picked_army = army;
    }

    pub fn set_hovered_army(&mut self, army: Option<ArmyId>) {
        self.hovered_army = army;
    }

    pub fn select(&mut self, region: Option<RegionId>) {
        self.selected = region.map(|r| (r, self.frame));
    }

    #[must_use]
    pub fn selected(&self) -> Option<RegionId> {
        self.selected.map(|(r, _)| r)
    }

    #[must_use]
    pub fn marks(&self) -> &[ArmyMark] {
        &self.marks
    }

    #[must_use]
    pub fn army_lines(&self, state: &LivingWorldState) -> Vec<ArmyLine> {
        if !lines_shown(state.phase) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for mark in &self.marks {
            let shown = self
                .local
                .is_none_or(|l| l == mark.player || state.allied(l, mark.player));
            let Some((from, to)) = self.layout.line_ends(mark).filter(|_| shown) else {
                continue;
            };
            let relation = match to
                .region
                .and_then(|r| state.region(r))
                .and_then(|r| r.owner)
            {
                None => LineRelation::Unowned,
                Some(owner) if state.allied(mark.player, owner) => LineRelation::Allied,
                Some(_) => LineRelation::Attacking,
            };
            let color = line_color(&self.info, relation);
            let samples = sample_line(from.at, to.at, self.layout.line_points);
            out.push(ArmyLine {
                army: mark.army,
                color,
                points: self
                    .picker
                    .drape(&samples, from.region, to.region, self.layout.line_bias),
            });
        }
        out
    }

    #[must_use]
    pub fn walking_count(&self) -> usize {
        self.walking.len()
    }

    #[must_use]
    pub fn summary(&self) -> &DrawSummary {
        &self.summary
    }

    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        state: &LivingWorldState,
    ) -> mint_render::Result<()> {
        self.draw_over(renderer, state, &[])
    }

    pub fn draw_over(
        &mut self,
        renderer: &mut Renderer,
        state: &LivingWorldState,
        ui: &[mint_render::ScreenDraw<'_>],
    ) -> mint_render::Result<()> {
        let (w, h) = renderer.size();
        let (w, h) = (
            f32::from(u16::try_from(w).unwrap_or(u16::MAX)),
            f32::from(u16::try_from(h).unwrap_or(u16::MAX)),
        );
        self.camera.set_aspect(w / h.max(1.0));
        let marking = self.session_marking(state);
        self.build(renderer, Some(state), &marking)?;
        let camera: Camera = self.camera.camera();
        let scene = self.scene(camera).with_ordered_screen(ui);
        renderer.render(&scene)
    }

    pub fn preview_scene(
        &mut self,
        renderer: &mut Renderer,
        marks: &PreviewMarks,
        aspect: f32,
    ) -> mint_render::Result<Scene<'_>> {
        let marking = self.preview_marking(marks);
        self.build(renderer, None, &marking)?;
        let camera = self.camera.camera_for(aspect);
        Ok(self.scene(camera))
    }

    fn scene(&self, camera: Camera) -> Scene<'_> {
        let mut scene = Scene::new(camera).with_instances(&self.instances);
        scene.lights = self.lights();
        scene.ambient_color = rgb(self.info.ambient, [0.0; 3]);
        scene.clear_color = [0.0, 0.0, 0.0, 1.0];
        scene
    }

    fn neutral(&self) -> [f32; 3] {
        rgb(self.effects_def.neutral_region_color, [1.0; 3])
    }

    fn session_marking(&self, state: &LivingWorldState) -> Marking {
        let regions = self.layout.regions.len();
        let neutral = self.neutral();
        let fill_color = |p: PlayerId| self.colors.get(p.index()).map(|c| c.fill);
        let colour_of: Vec<[f32; 3]> = (0..regions)
            .map(|i| {
                state
                    .regions
                    .get(i)
                    .and_then(|r| r.owner)
                    .and_then(fill_color)
                    .unwrap_or(neutral)
            })
            .collect();
        let fill = state
            .regions
            .iter()
            .enumerate()
            .filter(|(_, r)| r.enabled)
            .filter_map(|(i, r)| {
                let owner = r.owner?;
                Some((i, fill_color(owner)?, Some(owner)))
            })
            .collect();
        let homes = state
            .players
            .iter()
            .filter(|p| !p.defeated)
            .filter_map(|p| Some((p.base_region?.index(), fill_color(p.id)?)))
            .collect();
        let hover = self
            .hovered
            .map(|(r, _)| r.index())
            .filter(|i| *i < regions)
            .into_iter()
            .collect();
        Marking {
            borders: (0..regions)
                .filter(|i| state.regions.get(*i).is_some_and(|r| r.enabled))
                .collect(),
            fill,
            homes,
            hover,
            colour_of,
        }
    }

    fn preview_marking(&self, marks: &PreviewMarks) -> Marking {
        let regions = self.layout.regions.len();
        let neutral = self.neutral();
        let start = rgb(self.effects_def.shell_start_position_color, [1.0; 3]);
        let known = |r: &RegionId| r.index() < regions;
        let mut colour_of = vec![neutral; regions];
        for (region, colour) in &marks.filled {
            if let Some(slot) = colour_of.get_mut(region.index()) {
                *slot = colour.unwrap_or(neutral);
            }
        }
        Marking {
            borders: marks
                .enabled
                .iter()
                .filter(|r| known(r))
                .map(|r| r.index())
                .collect(),
            fill: marks
                .filled
                .iter()
                .filter(|(r, _)| known(r))
                .map(|(r, c)| (r.index(), c.unwrap_or(neutral), None))
                .collect(),
            homes: marks
                .homes
                .iter()
                .filter(|(r, _)| known(r))
                .map(|(r, c)| (r.index(), c.unwrap_or(neutral)))
                .chain(
                    marks
                        .startable
                        .iter()
                        .filter(|r| known(r))
                        .map(|r| (r.index(), start)),
                )
                .collect(),
            hover: marks
                .hovered
                .iter()
                .filter(|r| known(r))
                .map(|r| r.index())
                .collect(),
            colour_of,
        }
    }

    fn lights(&self) -> [DirectionalLight; MAX_DIRECTIONAL_LIGHTS] {
        let mut lights = [DirectionalLight::default(); MAX_DIRECTIONAL_LIGHTS];
        let rigs = [
            (self.info.sun_dir, self.info.sun_rgb),
            (self.info.accent1_dir, self.info.accent1_rgb),
            (self.info.accent2_dir, self.info.accent2_rgb),
        ];
        for (slot, (dir, color)) in lights.iter_mut().zip(rigs) {
            let (Some(dir), Some(color)) = (dir, color) else {
                continue;
            };
            let [x, y, z] = light_ray_direction(dir.x, dir.y);
            *slot = DirectionalLight {
                direction: Vec3::new(x, y, z),
                color: [color.r, color.g, color.b],
            };
        }
        lights
    }

    fn build(
        &mut self,
        renderer: &mut Renderer,
        state: Option<&LivingWorldState>,
        marking: &Marking,
    ) -> mint_render::Result<()> {
        let mut out = std::mem::take(&mut self.instances);
        out.clear();
        if let Some(map) = &self.map {
            map.place(Mat4::IDENTITY, &mut out);
            #[allow(clippy::cast_precision_loss)]
            let seconds = self.frame as f32 / CLIENT_HZ;
            map.apply_uv_mappers(1, &mut out, seconds);
        }
        self.ambient.push(renderer, self.frame, &mut out);
        let regions = self.layout.regions.len();
        let mut owners = vec![None; regions];
        let since = |start: u32| self.frame.saturating_sub(start);
        let neutral = self.neutral();
        let region_colour = |region: RegionId| {
            marking
                .colour_of
                .get(region.index())
                .copied()
                .unwrap_or(neutral)
        };

        if self.overlay != Overlay::Plain {
            if let Some(border) = &self.border {
                if self.effects_def.should_show_region_borders != Some(false) {
                    let tint = rgb(self.effects_def.region_border_color, [0.0; 3]);
                    let frames = since(self.region_start);
                    border.push(&marking.borders, tint, frames, AtEnd::Hold, &mut out);
                }
            }
        }
        if self.overlay == Overlay::Ownership {
            if let Some(fill) = &self.fill {
                for &(index, color, owner) in &marking.fill {
                    fill.push(
                        &[index],
                        color,
                        since(self.region_start),
                        AtEnd::Hold,
                        &mut out,
                    );
                    if let Some(slot) = owners.get_mut(index) {
                        *slot = owner;
                    }
                }
            }
            if let Some(highlight) = &self.highlight {
                for &(index, color) in &marking.homes {
                    let frames = since(self.region_start);
                    highlight.push(&[index], color, frames, AtEnd::Hold, &mut out);
                }
            }
        }
        for conquest in &self.conquered {
            for model in &self.conquered_effects {
                let frames = since(conquest.start);
                model.push(
                    &[conquest.region],
                    conquest.color,
                    frames,
                    AtEnd::Hide,
                    &mut out,
                );
            }
        }
        if let Some(start) = self.event_start {
            if let Some(event) = self.events.front() {
                for model in &self.unified {
                    let frames = since(start);
                    model.push(
                        &[event.territory],
                        TERRITORY_FLASH,
                        frames,
                        AtEnd::Hide,
                        &mut out,
                    );
                }
            }
        }
        if let Some(((_, start), model)) = self
            .hovered
            .zip(self.mouseover.as_ref().or(self.fill.as_ref()))
        {
            for &index in &marking.hover {
                let colour = marking.colour_of.get(index).copied().unwrap_or(neutral);
                model.push(&[index], colour, since(start), AtEnd::Hold, &mut out);
            }
        }
        if let (Some((selected, start)), Some(edge)) = (self.selected, &self.selection) {
            let frames = since(start);
            edge.push(
                &[selected.index()],
                region_colour(selected),
                frames,
                AtEnd::Hold,
                &mut out,
            );
        }

        self.summary = DrawSummary {
            region_owners: owners,
            army_icons: 0,
            army_lines: 0,
            markers: 0,
        };
        self.icon_picks.clear();
        let Some(state) = state else {
            self.instances = out;
            return Ok(());
        };
        let marks = self.marks.clone();
        for mark in &marks {
            let spot = self
                .walking
                .iter()
                .find(|w| w.army == mark.army)
                .map_or(mark.spot, Walk::position);
            let destination = mark
                .order
                .and_then(|r| self.layout.regions.get(r.index()))
                .map(|v| v.center);
            if self.push_icon(renderer, mark, spot, destination, &mut out) {
                self.summary.army_icons = self.summary.army_icons.saturating_add(1);
            }
        }
        self.push_markers(renderer, state, &mut out);
        self.push_lines(renderer, state, &mut out)?;
        self.instances = out;
        Ok(())
    }

    fn push_lines(
        &mut self,
        renderer: &mut Renderer,
        state: &LivingWorldState,
        out: &mut Vec<Instance>,
    ) -> mint_render::Result<()> {
        self.ribbons.begin();
        let eye = self.camera.camera().eye.to_array();
        let width = self.info.army_line_width.unwrap_or(DEFAULT_LINE_WIDTH);
        for line in self.army_lines(state) {
            let style = RibbonStyle {
                width,
                tile: 1.0,
                color: line.color,
                texture: self.line_texture,
                blend: Blend::Additive,
                uv_offset: [0.0; 2],
                depth_test: mint_render::DepthTest::Nearer,
            };
            self.ribbons
                .push(renderer, &line.points, eye, &style, out)?;
            self.summary.army_lines = self.summary.army_lines.saturating_add(1);
        }
        Ok(())
    }

    fn push_markers(
        &mut self,
        renderer: &mut Renderer,
        state: &LivingWorldState,
        out: &mut Vec<Instance>,
    ) {
        let centre = |region: RegionId| self.layout.regions.get(region.index()).map(|v| v.center);
        let battles = state.battles.iter().filter_map(|b| centre(b.region));
        let disputes = state
            .decisions
            .values()
            .filter(|d| !d.resolved)
            .filter_map(|d| centre(d.region));
        let placed: Vec<(Option<String>, [f32; 2])> = battles
            .map(|p| (self.info.battle_marker.clone(), p))
            .chain(disputes.map(|p| (self.info.region_award_dispute_marker.clone(), p)))
            .chain(
                self.palantir
                    .iter()
                    .map(|p| (self.info.palantir_marker.clone(), *p)),
            )
            .collect();
        let beacon = self.info.beacon_height.unwrap_or(DEFAULT_BEACON_HEIGHT);
        for (name, at) in placed {
            let Some(name) = name else { continue };
            let z = self.picker.ground(at[0], at[1]).unwrap_or(0.0) + beacon;
            let placement = Mat4::from_translation(Vec3::new(at[0], at[1], z));
            let Some(model) = self.icon_model(renderer, &name) else {
                continue;
            };
            let before = out.len();
            for part in model.parts.iter().filter(|p| !p.hidden) {
                let mut instance = part.instance;
                instance.transform = placement.mul_mat4(part.instance.transform);
                out.push(instance);
            }
            if out.len() > before {
                self.summary.markers = self.summary.markers.saturating_add(1);
            }
        }
    }

    fn step_fades(&mut self) {
        for ((army, icon, index), fade) in &mut self.icon_fades {
            let Some(object) = self.icons.get(icon).and_then(|i| i.objects.get(*index)) else {
                continue;
            };
            let selected = self.picked_army == Some(*army);
            let lit = selected || self.hovered_army == Some(*army);
            let flags = (u8::from(selected) * crate::fade::SELECTED)
                | (u8::from(lit) * crate::fade::HILIGHTED);
            fade.set_flags(&object.base, flags);
            fade.update();
        }
    }

    fn push_icon(
        &mut self,
        renderer: &mut Renderer,
        mark: &ArmyMark,
        spot: [f32; 2],
        rally: Option<[f32; 2]>,
        out: &mut Vec<Instance>,
    ) -> bool {
        let Some(icon) = self.icons.get(&mark.icon.to_ascii_lowercase()).cloned() else {
            return false;
        };
        let banner = self
            .colors
            .get(mark.player.index())
            .map_or([1.0; 3], |c| c.banner);
        let mut drawn = false;
        let key = mark.icon.to_ascii_lowercase();
        for (index, object) in icon.objects.iter().enumerate() {
            let base = &object.base;
            let extra = &object.extra;
            let fade = self
                .icon_fades
                .entry((mark.army, key.clone(), index))
                .or_insert_with(|| crate::fade::ObjectFade::new(base));
            if !fade.shown() {
                continue;
            }
            let alpha = fade.alpha();
            if !extra.visible_army_sizes.0.is_empty()
                && !extra.visible_army_sizes.0.contains(&mark.size)
            {
                continue;
            }
            let at = if extra.display_at_rally_point == Some(true)
                || extra.show_only_after_move_order == Some(true)
            {
                match rally {
                    Some(point) => point,
                    None => continue,
                }
            } else {
                spot
            };
            let Some(model_name) = base.model.as_deref() else {
                continue;
            };
            let ground = self.picker.ground(at[0], at[1]).unwrap_or(0.0);
            let z = ground + base.z_offset.unwrap_or(0.0);
            let scale = base.scale.unwrap_or(1.0);
            let angle = base.orient_angle.unwrap_or(0.0) * (core::f32::consts::PI / 180.0);
            let (sin, cos) = mint_math::f32::sin_cos(angle);
            let placement = Mat4::from_cols([
                [cos * scale, sin * scale, 0.0, 0.0],
                [-sin * scale, cos * scale, 0.0, 0.0],
                [0.0, 0.0, scale, 0.0],
                [at[0], at[1], z, 1.0],
            ]);
            let Some(model) = self.icon_model(renderer, model_name) else {
                continue;
            };
            let sub_objects = &base.sub_objects.0;
            let mut object_drawn = false;
            for part in &model.parts {
                let shown = if sub_objects.is_empty() {
                    !part.hidden
                } else {
                    sub_objects
                        .iter()
                        .any(|s| matches_sub_object(&part.name, s))
                };
                if !shown {
                    continue;
                }
                let mut instance = part.instance;
                instance.transform = placement.mul_mat4(part.instance.transform);
                instance.tint = crate::fade::tint(base.fade_method, instance.tint, alpha);
                if base.use_house_color == Some(true) {
                    if let Some(mask) = part.house_mask {
                        instance.texture2 = Some(mask);
                        instance.detail_combine = mint_render::DetailCombine::HouseColor;
                        instance.detail_alpha_combine = mint_render::DetailAlphaCombine::Disable;
                        instance.house_color = [banner[0], banner[1], banner[2], 1.0];
                    }
                }
                out.push(instance);
                object_drawn = true;
            }
            if object_drawn {
                drawn = true;
                let source = PickSource::of(base.pickbox.as_deref(), model_name, sub_objects);
                if let Some(pick) = self.pick_model(&source) {
                    self.icon_picks
                        .push(IconPick::new(mark.army, mark.region, pick, placement));
                }
            }
        }
        drawn
    }

    fn pick_model(&mut self, source: &PickSource) -> Option<Arc<[[Vec3; 3]]>> {
        let key = source.key();
        if let Some(found) = self.pick_models.get(&key) {
            return found.clone();
        }
        let loaded = load_model(&self.vfs, &self.resolver, source.model())
            .ok()
            .map(|loaded| {
                let mut triangles = Vec::new();
                for part in loaded.model.parts.iter().filter(|p| source.takes(p)) {
                    let world: Vec<Vec3> = part
                        .mesh
                        .vertices
                        .iter()
                        .map(|v| {
                            let [x, y, z] = v.position;
                            part.transform.transform_point(Vec3::new(x, y, z))
                        })
                        .collect();
                    for tri in part.mesh.indices.chunks_exact(3) {
                        let corner = |i: u32| world.get(usize::try_from(i).ok()?).copied();
                        if let [a, b, c] = tri {
                            if let (Some(a), Some(b), Some(c)) =
                                (corner(*a), corner(*b), corner(*c))
                            {
                                triangles.push([a, b, c]);
                            }
                        }
                    }
                }
                Arc::from(triangles)
            });
        self.pick_models.insert(key, loaded.clone());
        loaded
    }

    fn icon_model(&mut self, renderer: &mut Renderer, name: &str) -> Option<&CachedModel> {
        let key = name.to_ascii_lowercase();
        if !self.icon_models.contains_key(&key) {
            let loaded = self
                .cache
                .load_with_hidden(renderer, &self.vfs, &self.resolver, name)
                .ok();
            self.icon_models.insert(key.clone(), loaded);
        }
        self.icon_models.get(&key)?.as_ref()
    }
}

#[allow(clippy::too_many_arguments)]
fn effect_model(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    cache: &mut ModelCache,
    geometry: &str,
    def: Option<&EffectDef>,
    keys: &[String],
    report: &mut LoadReport,
) -> Option<EffectModel> {
    let model = match cache.load_with_hidden(renderer, vfs, resolver, geometry) {
        Ok(model) => model,
        Err(error) => {
            report
                .missing
                .push((geometry.to_owned(), error.to_string()));
            return None;
        }
    };
    let by_key: Vec<Vec<usize>> = keys
        .iter()
        .map(|key| {
            model
                .parts
                .iter()
                .enumerate()
                .filter(|(_, p)| !key.is_empty() && matches_sub_object(&p.name, key))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();
    report.effects.push((
        geometry.to_owned(),
        model.parts.len(),
        model.parts.iter().filter(|p| p.hidden).count(),
        by_key.iter().filter(|k| !k.is_empty()).count(),
    ));
    let curve = def.map_or_else(Curve::default, |d| {
        Curve::new(&d.color_intensity_control_points)
    });
    Some(EffectModel {
        model,
        by_key,
        curve,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_render::{Aabb, MeshData, Vertex};
    use mint_sage_ini::living_world::effects::ColorIntensityPoint;
    use mint_sage_ini::living_world::Millis;
    use mint_sage_model::CachedPart;
    use mint_vfs::Vfs;

    fn point(ms: u32, intensity: f32) -> ColorIntensityPoint {
        ColorIntensityPoint {
            intensity: Some(intensity),
            time: Some(Millis(ms)),
            ..ColorIntensityPoint::default()
        }
    }

    fn effect(renderer: &mut Renderer, curve: &[ColorIntensityPoint]) -> Option<EffectModel> {
        let vertex = |x: f32| Vertex {
            position: [x, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            ..Vertex::default()
        };
        let mesh = renderer
            .upload_mesh(&MeshData {
                vertices: vec![vertex(0.0), vertex(1.0), vertex(2.0)],
                indices: vec![0, 1, 2],
            })
            .ok()?;
        let mut instance = Instance::new(mesh);
        instance.tint = [0.3, 0.4, 0.5, 0.6];
        instance.material_diffuse = [0.2, 0.2, 0.2];
        let part = CachedPart {
            name: "LMR_Fill".to_owned(),
            instance,
            skin: None,
            pivot: None,
            house_mask: None,
            uv_mapper: None,
            texture_name: None,
            texture_address: Default::default(),
            hidden: false,
        };
        Some(EffectModel {
            model: CachedModel {
                name: String::new(),
                hierarchy: None,
                parts: vec![part],
                bounds: Aabb::default(),
                vertices: 3,
                triangles: 1,
            },
            by_key: vec![vec![0]],
            curve: Curve::new(curve),
        })
    }

    #[test]
    fn an_effect_writes_colour_times_curve_to_the_emissive_and_leaves_alpha_and_diffuse() {
        let Ok(mut renderer) = Renderer::offscreen(16, 16) else {
            eprintln!("SKIPPED: no graphics adapter");
            return;
        };
        let model = effect(&mut renderer, &[point(0, 0.5), point(1000, 0.5)]).expect("effect");
        let mut out = Vec::new();
        model.push(&[0], [0.8, 0.4, 0.2], 3, AtEnd::Hold, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].material_emissive, [0.4, 0.2, 0.1]);
        assert_eq!(out[0].tint, [0.3, 0.4, 0.5, 0.6]);
        assert_eq!(out[0].material_diffuse, [0.2, 0.2, 0.2]);
    }

    #[test]
    fn a_curve_that_ended_holds_its_last_value_or_hides_the_effect() {
        let Ok(mut renderer) = Renderer::offscreen(16, 16) else {
            eprintln!("SKIPPED: no graphics adapter");
            return;
        };
        let model = effect(&mut renderer, &[point(0, 0.0), point(1000, 1.0)]).expect("effect");
        let (mut held, mut hidden) = (Vec::new(), Vec::new());
        model.push(&[0], [1.0; 3], 500, AtEnd::Hold, &mut held);
        model.push(&[0], [1.0; 3], 500, AtEnd::Hide, &mut hidden);
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].material_emissive, [1.0; 3]);
        assert!(hidden.is_empty());
        model.push(&[0], [1.0; 3], 29, AtEnd::Hide, &mut hidden);
        assert_eq!(hidden.len(), 1);
    }

    fn bare() -> WorldMap {
        let info = MapInfoDef::default();
        let territory = TerritoryView {
            look_at: Some([300.0, 0.0]),
            zoom: 0.2,
            ..TerritoryView::default()
        };
        let mut layout = Layout::default();
        layout.territories = vec![territory];
        WorldMap {
            vfs: Arc::new(Vfs::default()),
            resolver: Resolver::new(),
            cache: ModelCache::new(),
            camera: WorldCamera::new(&info),
            info,
            effects_def: RegionEffectsDef::default(),
            layout,
            overlay: Overlay::default(),
            colors: Vec::new(),
            map: None,
            fill: None,
            border: None,
            highlight: None,
            selection: None,
            mouseover: None,
            unified: Vec::new(),
            conquered_effects: Vec::new(),
            conquered: Vec::new(),
            owners: Vec::new(),
            battles: Vec::new(),
            conquered_sound: None,
            sounds: Vec::new(),
            ambient: Ambient::default(),
            icons: BTreeMap::new(),
            icon_models: BTreeMap::new(),
            pick_models: BTreeMap::new(),
            icon_picks: Vec::new(),
            picker: RegionPicker::default(),
            hovered: None,
            picked_army: None,
            hovered_army: None,
            icon_fades: BTreeMap::new(),
            selected: None,
            frame: 0,
            frame_rest: 0.0,
            region_start: 0,
            events: VecDeque::new(),
            event_start: None,
            started: Vec::new(),
            local: None,
            shift_held: false,
            turbo_enabled: false,
            controls: ScrollControls::default(),
            hold_events: false,
            ribbons: Ribbons::default(),
            line_texture: None,
            palantir: Vec::new(),
            slots: Slots::default(),
            marks: Vec::new(),
            walking: Vec::new(),
            instances: Vec::new(),
            summary: DrawSummary::default(),
        }
    }

    fn part(name: &str, hidden: bool, pick_proxy: bool) -> mint_sage_model::Part {
        mint_sage_model::Part {
            name: name.to_owned(),
            hidden,
            pick_proxy,
            ..Default::default()
        }
    }

    #[test]
    fn a_pickbox_replaces_the_drawn_model_as_the_pick_model_with_all_its_parts() {
        let sub = ["LWBANNER".to_owned()];
        let boxed = PickSource::of(Some("ArmyAntsGlow"), "LWBanHRogash", &sub);
        assert_eq!(boxed.model(), "ArmyAntsGlow");
        assert!(boxed.takes(&part("GLOW", true, true)), "every part counts");
        let drawn = PickSource::of(None, "LWBanHRogash", &sub);
        assert_eq!(drawn.model(), "LWBanHRogash");
        assert_ne!(boxed.key(), drawn.key());
    }

    #[test]
    fn without_a_pickbox_the_pick_model_is_the_parts_the_object_draws() {
        let all = PickSource::of(None, "ArmyAntsS", &[]);
        assert!(all.takes(&part("ANTS", false, false)));
        assert!(!all.takes(&part("ANTS_HIDDEN", true, false)));
        assert!(!all.takes(&part("PICK", false, true)));
        let sub = ["LWBANNER".to_owned()];
        let some = PickSource::of(None, "LWBanH", &sub);
        assert!(some.takes(&part("LWBanH.LWBANNER", false, false)));
        assert!(some.takes(&part("LWBANNER", true, false)));
        assert!(!some.takes(&part("LWSTAFF", false, false)));
    }

    #[test]
    fn only_the_local_players_new_territory_plays_and_the_walks_wait_for_it() {
        let mut map = bare();
        map.set_local_player(Some(PlayerId(0)));
        map.territory_unified(PlayerId(1), 0);
        assert_eq!(map.events_pending(), 0);
        map.territory_unified(PlayerId(0), 0);
        assert_eq!(map.events_pending(), 1);
        map.walking.push(Walk::new(
            mint_sage_livingworld::ArmyId(1),
            [0.0, 0.0],
            vec![[50.0, 0.0]],
            5.0,
        ));
        map.advance(1.0 / CLIENT_HZ);
        assert_eq!(
            map.walking[0].position(),
            [0.0, 0.0],
            "paused for the event"
        );
        assert_eq!(map.take_started().len(), 1);
        map.advance(10.0);
        assert_eq!(map.events_pending(), 0);
        assert!(map.walking.is_empty() || map.walking[0].position()[0] > 0.0);
    }

    #[test]
    fn walks_run_tenfold_only_with_shift_held_and_the_turbo_setting() {
        let walked = |shift, turbo| {
            let mut map = bare();
            map.shift_held = shift;
            map.turbo_enabled = turbo;
            map.walking.push(Walk::new(
                mint_sage_livingworld::ArmyId(1),
                [0.0, 0.0],
                vec![[500.0, 0.0]],
                1.0,
            ));
            map.advance(2.0 / CLIENT_HZ);
            map.walking[0].position()[0]
        };
        assert_eq!(walked(true, true), 20.0);
        assert_eq!(walked(true, false), 2.0);
        assert_eq!(walked(false, true), 2.0);
    }

    #[test]
    fn army_lines_show_in_planning_phases_only_coloured_by_the_destination() {
        assert!(lines_shown(Phase::Planning) && lines_shown(Phase::PlanRetreats));
        assert!(!lines_shown(Phase::MoveArmies) && !lines_shown(Phase::ResolveBattles));
        let rgb = |r| Rgb { r, g: 0.0, b: 0.0 };
        let info = MapInfoDef {
            army_line_color_neutral: Some(rgb(0.1)),
            army_line_color_allied: Some(rgb(0.2)),
            army_line_color_attacking: Some(rgb(0.3)),
            ..MapInfoDef::default()
        };
        let red = |r| line_color(&info, r)[0];
        assert_eq!(red(LineRelation::Unowned), 0.1);
        assert_eq!(red(LineRelation::Allied), 0.2);
        assert_eq!(red(LineRelation::Attacking), 0.3);
    }

    #[test]
    fn a_light_rises_with_x_and_travels_toward_the_azimuth_y() {
        let [x, y, z] = light_ray_direction(30.0, 90.0);
        assert!(x.abs() < 1.0e-5 && (y - 0.866_025_4).abs() < 1.0e-5 && (z + 0.5).abs() < 1.0e-5);
    }
}
