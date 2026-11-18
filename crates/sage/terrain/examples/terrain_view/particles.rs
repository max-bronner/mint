use std::collections::BTreeMap;

use mint_render::{Camera, Instance, Renderer, TextureHandle, Vec3};
use mint_sage_ini::{
    DefinitionForm, EmissionVelocity, EmissionVolume, FxListIndex, FxParticleNugget, ParticleIndex,
    ParticleSystemDef,
};
use mint_sage_map::{HeightMapData, MAP_XY_FACTOR};
use mint_sage_model::{load_texture, ModelCache, Resolver};
use mint_sage_particle::{
    BillboardMode, CollisionEvent, DetailLevel, DrawShape, FlipbookMode, FxFrame, FxSpawn, Ground,
    ParticleWorld, ShapeMode, SortMode, SystemId, TemplateHandle, WindMode, MAX_COMPOUND_DEPTH,
};
use mint_vfs::Vfs;

pub(crate) struct MapGround<'a> {
    height_map: &'a HeightMapData,
    vertical_scale: f32,
}

impl<'a> MapGround<'a> {
    pub(crate) fn new(height_map: &'a HeightMapData, vertical_scale: f32) -> Self {
        Self {
            height_map,
            vertical_scale,
        }
    }

    fn sample(&self, x: i64, y: i64) -> Option<f32> {
        let x = u32::try_from(x).ok()?;
        let y = u32::try_from(y).ok()?;
        Some(f32::from(self.height_map.sample(x, y)?) * self.vertical_scale)
    }
}

impl Ground for MapGround<'_> {
    fn height_at(&self, x: f32, y: f32) -> Option<f32> {
        let border = f32::from(u16::try_from(self.height_map.border_size).unwrap_or(u16::MAX));
        let cx = x / MAP_XY_FACTOR + border;
        let cy = y / MAP_XY_FACTOR + border;
        if !cx.is_finite() || !cy.is_finite() {
            return None;
        }
        let (fx, fy) = (cx.floor(), cy.floor());
        #[allow(clippy::cast_possible_truncation)]
        let (ix, iy) = (fx as i64, fy as i64);
        let (tx, ty) = (cx - fx, cy - fy);
        let z00 = self.sample(ix, iy)?;
        let z10 = self.sample(ix.saturating_add(1), iy)?;
        let z01 = self.sample(ix, iy.saturating_add(1))?;
        let z11 = self.sample(ix.saturating_add(1), iy.saturating_add(1))?;
        let top = z00 + (z10 - z00) * tx;
        let bottom = z01 + (z11 - z01) * tx;
        Some(top + (bottom - top) * ty)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BoneRung {
    Off,
    #[default]
    Default,
    All,
}

impl BoneRung {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Off => Self::Default,
            Self::Default => Self::All,
            Self::All => Self::Off,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Default => "default",
            Self::All => "all",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "off" => Some(Self::Off),
            "default" => Some(Self::Default),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BoneReport {
    pub(crate) placements: usize,
    pub(crate) wanted: usize,
    pub(crate) spawned: usize,
    pub(crate) dangling: usize,
    pub(crate) lod_refused: usize,
    pub(crate) bone_missing: usize,
    pub(crate) bone_none: usize,
    pub(crate) following: usize,
    pub(crate) by_system: BTreeMap<String, usize>,
    pub(crate) busiest: Option<(usize, [f32; 3])>,
    pub(crate) bounds: Option<([f32; 3], [f32; 3])>,
}

impl BoneReport {
    pub(crate) fn placement_at(&mut self, count: usize, at: Vec3) {
        if self.busiest.is_none_or(|(most, _)| count > most) {
            self.busiest = Some((count, [at.x, at.y, at.z]));
        }
    }

    pub(crate) fn saw(&mut self, at: Vec3) {
        let point = [at.x, at.y, at.z];
        match &mut self.bounds {
            None => self.bounds = Some((point, point)),
            Some((low, high)) => {
                for slot in 0..3 {
                    let (Some(low), Some(high), Some(value)) =
                        (low.get_mut(slot), high.get_mut(slot), point.get(slot))
                    else {
                        continue;
                    };
                    *low = low.min(*value);
                    *high = high.max(*value);
                }
            }
        }
    }
}

pub(crate) struct FollowingBone {
    pub(crate) system: SystemId,
    pub(crate) group: usize,
    pub(crate) placement: usize,
    pub(crate) pivot: Option<usize>,
    pub(crate) follow_bone: bool,
}

fn fx_spawn(template: TemplateHandle, nugget: &FxParticleNugget) -> FxSpawn {
    let [x, y, z] = nugget.offset;
    FxSpawn {
        template,
        offset: Vec3::new(x, y, z),
        delay: nugget.initial_delay,
        at_ground_height: nugget.create_at_ground_height,
        orient_to_object: nugget.orient_to_object,
        attach_to_object: nugget.attach_to_object,
        set_target_matrix: nugget.set_target_matrix,
    }
}

fn hollow_label(volume: &str, hollow: bool) -> String {
    if hollow {
        format!("{volume} hollow")
    } else {
        volume.to_owned()
    }
}

pub(crate) struct ParticleScene {
    world: ParticleWorld,
    index: ParticleIndex,
    lists: FxListIndex,
    models: ModelCache,
    textures: BTreeMap<String, Option<TextureHandle>>,
    templates: BTreeMap<String, TemplateHandle>,
    following: Vec<FollowingBone>,
    debug_system: Option<SystemId>,
    name: String,
    fx_name: String,
    draw: bool,
    bones: BoneRung,
}

impl ParticleScene {
    pub(crate) fn new(index: ParticleIndex, lists: FxListIndex, name: &str) -> Self {
        Self {
            world: ParticleWorld::default(),
            index,
            lists,
            models: ModelCache::new(),
            textures: BTreeMap::new(),
            templates: BTreeMap::new(),
            following: Vec::new(),
            debug_system: None,
            name: name.to_owned(),
            fx_name: String::new(),
            draw: true,
            bones: BoneRung::default(),
        }
    }

    pub(crate) fn fx_name(&self) -> &str {
        &self.fx_name
    }

    pub(crate) fn set_fx_name(&mut self, name: &str) {
        if self.fx_name == name {
            return;
        }
        self.fx_name = name.to_owned();
        if name.is_empty() {
            println!("particle.fxlist is empty - the fire key does nothing");
            return;
        }
        match self.lists.get(name) {
            None => println!("particle.fxlist = {name:?}: no such FXList"),
            Some(list) => {
                let nuggets = list
                    .nuggets
                    .iter()
                    .filter(|nugget| nugget.is_particle_system())
                    .count();
                let kinds: BTreeMap<&str, usize> =
                    list.nuggets.iter().fold(BTreeMap::new(), |mut held, n| {
                        *held.entry(n.keyword.as_str()).or_default() += 1;
                        held
                    });
                let mut line = String::new();
                for (kind, count) in &kinds {
                    line.push_str(&format!("{kind} {count}  "));
                }
                println!(
                    "particle.fxlist = {}: {nuggets} of {} nugget(s) fire a system",
                    list.name,
                    list.nuggets.len()
                );
                println!("  press 0 to fire it at what the camera is looking at");
                println!("  nuggets: {}", line.trim_end());
            }
        }
    }

    pub(crate) fn fire_fx(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        at: Vec3,
        ground: &dyn Ground,
    ) {
        if self.fx_name.is_empty() {
            println!("particle.fxlist is empty - name one in the tuning file");
            return;
        }
        let name = self.fx_name.clone();
        let Some(nuggets) = self.lists.get(&name).map(|list| {
            list.nuggets
                .iter()
                .filter_map(|nugget| Some((nugget.name.clone()?, nugget.particle.clone()?)))
                .collect::<Vec<(String, FxParticleNugget)>>()
        }) else {
            println!("particle.fxlist = {name:?}: no such FXList");
            return;
        };
        let wanted = nuggets.len();
        let mut dangling = 0usize;
        let mut orienting = 0usize;
        let mut attaching = 0usize;
        let mut spawns: Vec<FxSpawn> = Vec::with_capacity(wanted);
        for (system, nugget) in &nuggets {
            orienting = orienting.saturating_add(usize::from(nugget.orient_to_object));
            attaching = attaching.saturating_add(usize::from(nugget.attach_to_object));
            match self.template(renderer, vfs, resolver, system) {
                Some(template) => spawns.push(fx_spawn(template, nugget)),
                None => dangling = dangling.saturating_add(1),
            }
        }
        let (started, _attached) = self.world.fire(&spawns, FxFrame::at(at), ground);
        println!(
            "fired {name} at ({:.1}, {:.1}, {:.1}): {started} of {wanted} started",
            at.x, at.y, at.z
        );
        println!(
            "  {dangling} dangling, {} refused by the lod or the budget",
            wanted.saturating_sub(started).saturating_sub(dangling)
        );
        if orienting > 0 || attaching > 0 {
            println!("  {orienting} ask OrientToObject, {attaching} AttachToObject");
            println!("  there is no caller object to give them, so both are inert");
        }
    }

    pub(crate) fn bones(&self) -> BoneRung {
        self.bones
    }

    pub(crate) fn set_bones(&mut self, bones: BoneRung) {
        self.bones = bones;
        println!("particle-bones = {} (--particle-bones)", bones.label());
    }

    pub(crate) fn motion(&self) -> bool {
        self.world.motion()
    }

    pub(crate) fn wind(&self) -> WindMode {
        self.world.wind()
    }

    pub(crate) fn set_wind(&mut self, wind: WindMode) {
        self.world.set_wind(wind);
        println!(
            "particle-wind = {} (--particle-wind), {} of {} live system(s) have wind",
            wind.label(),
            self.world.live_with_wind(),
            self.world.live_systems()
        );
    }

    pub(crate) fn billboard(&self) -> BillboardMode {
        self.world.billboard()
    }

    pub(crate) fn set_billboard(&mut self, billboard: BillboardMode) {
        self.world.set_billboard(billboard);
        println!(
            "particle-billboard = {} (--particle-billboard)",
            billboard.label()
        );
    }

    pub(crate) fn sort(&self) -> SortMode {
        self.world.sort()
    }

    pub(crate) fn set_sort(&mut self, sort: SortMode) {
        self.world.set_sort(sort);
        println!("particle-sort = {} (--particle-sort)", sort.label());
    }

    pub(crate) fn flipbook(&self) -> FlipbookMode {
        self.world.flipbook()
    }

    pub(crate) fn set_flipbook(&mut self, flipbook: FlipbookMode) {
        self.world.set_flipbook(flipbook);
        println!(
            "particle-flipbook = {} (--particle-flipbook)",
            flipbook.label()
        );
    }

    pub(crate) fn shape(&self) -> ShapeMode {
        self.world.shape()
    }

    pub(crate) fn set_shape(&mut self, shape: ShapeMode) {
        self.world.set_shape(shape);
        println!("particle-shape = {} (--particle-shape)", shape.label());
    }

    pub(crate) fn compound(&self) -> bool {
        self.world.compound()
    }

    pub(crate) fn set_compound(&mut self, compound: bool) {
        self.world.set_compound(compound);
        println!(
            "particle-compound = {} (--particle-compound)",
            if compound { "on" } else { "off" }
        );
    }

    pub(crate) fn shape_line(&self) -> String {
        let mut line = String::new();
        for (shape, count) in self.world.live_by_shape() {
            if count == 0 {
                continue;
            }
            line.push_str(&format!("{} {count}  ", shape.label()));
        }
        line.push_str(&format!(
            "; {} with a flipbook (step 12), {} with wind (step 11)",
            self.world.live_with_flipbook(),
            self.world.live_with_wind()
        ));
        if line.starts_with(';') {
            "no live systems".to_owned()
        } else {
            line.trim_end().to_owned()
        }
    }

    pub(crate) fn detail(&self) -> DetailLevel {
        self.world.budget().detail
    }

    pub(crate) fn set_detail(&mut self, detail: DetailLevel) {
        let mut budget = self.world.budget();
        budget.detail = detail;
        self.world.set_budget(budget);
        println!(
            "particle-lod = {} (--particle-lod), gamelod.ini's own ceiling for it is {}",
            detail.label(),
            match detail.retail_max_particles() {
                Some(count) => count.to_string(),
                None => "not set".to_owned(),
            }
        );
    }

    pub(crate) fn set_budget_numbers(
        &mut self,
        max_particles: Option<usize>,
        skip_mask: u32,
    ) -> bool {
        let mut budget = self.world.budget();
        if budget.max_particles == max_particles && budget.skip_mask == skip_mask {
            return false;
        }
        budget.max_particles = max_particles;
        budget.skip_mask = skip_mask;
        self.world.set_budget(budget);
        true
    }

    pub(crate) fn budget_line(&self) -> String {
        let budget = self.world.budget();
        let report = self.world.refusals();
        let mut line = format!(
            "particle budget = {} live (map.particle_max_count), skip mask {}              (map.particle_skip_mask), lod {} (l, --particle-lod)\n               {} counted + {} ALWAYS_RENDER exempt live, {} evicted, {} refused, {} skipped,              {} system(s) refused by the lod",
            match budget.max_particles {
                Some(count) => count.to_string(),
                None => "unlimited".to_owned(),
            },
            budget.skip_mask,
            budget.detail.label(),
            report.counted,
            report.exempt,
            report.evicted,
            report.refused,
            report.skipped,
            report.lod_refused,
        );
        line.push_str(&format!(
            "
               {} terrain collision(s) fired, {} system(s) refused by the              compound-depth cap",
            report.collisions, report.depth_refused
        ));
        line
    }

    pub(crate) fn lod_refused(&self) -> u64 {
        self.world.refusals().lod_refused
    }

    pub(crate) fn set_motion(&mut self, motion: bool) {
        self.world.set_motion(motion);
        println!(
            "particle-motion = {} (--particle-motion)",
            if motion { "on" } else { "off" }
        );
    }

    pub(crate) fn smooth(&self) -> bool {
        self.world.smooth()
    }

    pub(crate) fn set_smooth(&mut self, smooth: bool) {
        self.world.set_smooth(smooth);
        println!(
            "particle-smooth = {} (;), simulation still {} Hz",
            if smooth { "on" } else { "off" },
            mint_sage_particle::PARTICLE_HZ
        );
    }

    pub(crate) fn clear(&mut self) {
        self.world.clear();
        self.following.clear();
        self.debug_system = None;
    }

    pub(crate) fn attach(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        frame: mint_render::Mat4,
    ) -> Option<SystemId> {
        let handle = self.template(renderer, vfs, resolver, name)?;
        let origin = frame.transform_point(Vec3::new(0.0, 0.0, 0.0));
        let id = self.world.spawn(handle, origin)?;
        self.world.set_frame(id, frame);
        Some(id)
    }

    pub(crate) fn follow(&mut self, following: FollowingBone) {
        self.following.push(following);
    }

    pub(crate) fn following(&self) -> &[FollowingBone] {
        &self.following
    }

    pub(crate) fn move_frame(&mut self, system: SystemId, frame: mint_render::Mat4) {
        self.world.set_frame(system, frame);
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn drawn(&self) -> bool {
        self.draw
    }

    pub(crate) fn set_drawn(&mut self, draw: bool) {
        self.draw = draw;
        println!(
            "particles = {} (--particles)",
            if draw { "on" } else { "off" }
        );
    }

    pub(crate) fn keys(&self) -> bool {
        self.world.keys()
    }

    pub(crate) fn set_keys(&mut self, keys: bool) {
        self.world.set_keys(keys);
        println!(
            "particle-keys = {} (--particle-keys)",
            if keys { "on" } else { "off" }
        );
    }

    pub(crate) fn live(&self) -> (usize, usize) {
        (self.world.live_systems(), self.world.live_particles())
    }

    pub(crate) fn respawn(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        at: Vec3,
    ) {
        if let Some(previous) = self.debug_system.take() {
            self.world.remove(previous);
        }
        self.name = name.to_owned();
        if name.is_empty() {
            println!("particle.system is empty - nothing is spawned");
            return;
        }
        let Some(handle) = self.template(renderer, vfs, resolver, name) else {
            println!("particle.system = {name:?}: no such particle system");
            return;
        };
        let Some(definition) = self.world.definition(handle).cloned() else {
            return;
        };

        println!(
            "particle.system = {} at ({:.1}, {:.1}, {:.1})",
            definition.name, at.x, at.y, at.z
        );
        println!(
            "  {} form, Shader = {}{}, Type = {}, Draw = {}",
            match definition.form {
                DefinitionForm::Block => "block",
                DefinitionForm::Flat => "flat",
            },
            definition.shader().as_str(),
            if definition.declared_shader.is_none() {
                " (absent, read as ADDITIVE)"
            } else {
                ""
            },
            definition.kind.as_str(),
            definition.draw_module.as_deref().unwrap_or("(none)")
        );
        println!(
            "  volume = {}, velocity = {}, texture {}",
            match &definition.volume {
                EmissionVolume::Point => "POINT".to_owned(),
                EmissionVolume::Line { .. } => "LINE".to_owned(),
                EmissionVolume::Box { hollow, .. } => hollow_label("BOX", *hollow),
                EmissionVolume::Sphere { hollow, .. } => hollow_label("SPHERE", *hollow),
                EmissionVolume::Cylinder { hollow, .. } => hollow_label("CYLINDER", *hollow),
                EmissionVolume::Lightning { .. } => "LIGHTNING".to_owned(),
                EmissionVolume::Unread { module } => format!("{module} NOT DECODED"),
            },
            match &definition.velocity {
                EmissionVelocity::Ortho => "ORTHO".to_owned(),
                EmissionVelocity::Spherical { .. } => "SPHERICAL".to_owned(),
                EmissionVelocity::Hemispherical { .. } => "HEMISPHERICAL".to_owned(),
                EmissionVelocity::Cylindrical { .. } => "CYLINDRICAL".to_owned(),
                EmissionVelocity::Outward { .. } => "OUTWARD".to_owned(),
                EmissionVelocity::Unread { module } => format!("{module} NOT DECODED"),
            },
            match definition.particle_name.as_deref() {
                None => "(none)".to_owned(),
                Some(texture) => match self.textures.get(&texture.to_ascii_lowercase()) {
                    Some(Some(_)) => texture.to_owned(),
                    _ => format!("{texture} NOT SHIPPED - drawn white"),
                },
            }
        );
        println!(
            "  lifetime {:.0}-{:.0} f, burst {:.0}-{:.0} every {:.1}-{:.1} f, size {:.1}-{:.1}, \
             batch {}",
            definition.lifetime.low,
            definition.lifetime.high,
            definition.burst_count.low,
            definition.burst_count.high,
            definition.burst_delay.low,
            definition.burst_delay.high,
            definition.size.low,
            definition.size.high,
            self.world.capacity(handle).unwrap_or(0)
        );
        println!(
            "  Priority = {}{}, SortLevel = {}, IsGroundAligned = {}",
            definition.priority().as_str(),
            if definition.priority().is_exempt() {
                " (exempt from the budget)"
            } else {
                ""
            },
            match definition.sort_level {
                Some(level) => level.to_string(),
                None => "absent".to_owned(),
            },
            if definition.ground_aligned {
                "Yes"
            } else {
                "No"
            }
        );
        println!(
            "  {} colour key(s), {} alpha key(s){}, ColorScale {} {}",
            definition.colors_authored,
            definition.alphas_authored,
            if definition.has_alpha_block {
                ""
            } else {
                " (no Alpha block: alpha stays 0, which only ADDITIVE ignores)"
            },
            definition.color_scale.low,
            definition.color_scale.high
        );
        println!(
            "  SystemLifetime = {}, InitialDelay {:.0}-{:.0} f{}",
            match definition.system_lifetime {
                Some(frames) => format!("{frames:.0} f (emits for that long, then stops)"),
                None => "absent (emits forever)".to_owned(),
            },
            definition.initial_delay.low,
            definition.initial_delay.high,
            if definition.one_shot {
                ", IsOneShot"
            } else {
                ""
            }
        );
        let shape = DrawShape::of(
            definition.kind,
            definition.draw_module.as_deref(),
            definition.particle_name.as_deref(),
        );
        println!(
            "  drawn as {}{}",
            shape.label(),
            match definition.flipbook {
                None => String::new(),
                Some(book) => format!(
                    ", flipbook {}x{} cell(s) at speed {} ({})",
                    book.frames_per_row,
                    book.rows(),
                    book.speed_multiplier,
                    if book.animates() {
                        "animates"
                    } else {
                        "one cell, so it is a static sprite"
                    }
                ),
            }
        );
        if shape == DrawShape::Model {
            let parts = self.world.model_parts(handle);
            println!(
                "  render object {}: {parts} drawable part(s){}",
                definition.particle_name.as_deref().unwrap_or("(none)"),
                if parts == 0 {
                    " - NOT LOADED, so this system runs and draws nothing"
                } else {
                    ""
                }
            );
        }
        if definition.slave_system.is_some()
            || definition.per_particle_system.is_some()
            || definition.event.is_some()
        {
            println!(
                "  SlaveSystem = {}, PerParticleAttachedSystem = {}, Event = {}",
                definition.slave_system.as_deref().unwrap_or("(none)"),
                definition
                    .per_particle_system
                    .as_deref()
                    .unwrap_or("(none)"),
                match &definition.event {
                    None => "(none)".to_owned(),
                    Some(event) => format!(
                        "TerrainCollision -> {} (kill {}, offset {:.0})",
                        event.fx,
                        event.kill_after_event,
                        event.height_offset.midpoint()
                    ),
                }
            );
        }
        if definition.detail_texture.is_some() {
            println!(
                "  DetailTexture = {} - NOT DECODED (particles.md section 7e)",
                definition.detail_texture.as_deref().unwrap_or_default()
            );
        }
        if definition.size.high <= 0.0 {
            println!("  NOTE: Size is 0 - this system grows by SizeRate, which is step 6");
        }

        self.debug_system = self.world.spawn(handle, at);
    }

    fn template(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
    ) -> Option<TemplateHandle> {
        self.template_at(renderer, vfs, resolver, name, 0)
    }

    fn template_at(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        depth: u32,
    ) -> Option<TemplateHandle> {
        let key = name.to_ascii_lowercase();
        if let Some(handle) = self.templates.get(&key) {
            return Some(*handle);
        }
        let definition = self.index.get(name)?.clone();
        let texture = self.texture_for(renderer, vfs, resolver, &definition);
        let parts = self.model_parts(renderer, vfs, resolver, &definition);
        let handle = self.world.register_model(&definition, texture, parts);
        self.templates.insert(key, handle);
        if depth >= MAX_COMPOUND_DEPTH {
            return Some(handle);
        }
        let slave = definition.slave_system.as_deref().and_then(|slave| {
            self.template_at(renderer, vfs, resolver, slave, depth.saturating_add(1))
        });
        let attached = definition
            .per_particle_system
            .as_deref()
            .and_then(|attached| {
                self.template_at(renderer, vfs, resolver, attached, depth.saturating_add(1))
            });
        let event = definition.event.as_ref().map(|event| {
            let fired: Vec<(String, FxParticleNugget)> = self
                .lists
                .get(&event.fx)
                .into_iter()
                .flat_map(|list| list.nuggets.iter())
                .filter_map(|nugget| Some((nugget.name.clone()?, nugget.particle.clone()?)))
                .collect();
            let fires = fired
                .iter()
                .filter_map(|(name, nugget)| {
                    let template =
                        self.template_at(renderer, vfs, resolver, name, depth.saturating_add(1))?;
                    Some(fx_spawn(template, nugget))
                })
                .collect();
            CollisionEvent {
                fires,
                height_offset: event.height_offset.midpoint(),
                kill_after: event.kill_after_event,
                follow_up: None,
                orient: event.orient_to_terrain,
            }
        });
        self.world.link(handle, slave, attached, event);
        Some(handle)
    }

    fn texture_for(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        definition: &ParticleSystemDef,
    ) -> Option<TextureHandle> {
        let texture = definition.particle_name.as_deref()?;
        let key = texture.to_ascii_lowercase();
        if let Some(held) = self.textures.get(&key) {
            return *held;
        }
        let handle = if key.ends_with(".w3d") {
            None
        } else {
            load_texture(
                vfs,
                resolver,
                texture,
                renderer.supports_block_compression(),
            )
            .ok()
            .and_then(|loaded| renderer.upload_texture(&loaded.data).ok())
        };
        self.textures.insert(key, handle);
        handle
    }

    fn model_parts(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        definition: &ParticleSystemDef,
    ) -> Vec<Instance> {
        if DrawShape::of(
            definition.kind,
            definition.draw_module.as_deref(),
            definition.particle_name.as_deref(),
        ) != DrawShape::Model
        {
            return Vec::new();
        }
        let Some(name) = definition.particle_name.as_deref() else {
            return Vec::new();
        };
        let Some(model) = self.models.get_or_load(renderer, vfs, resolver, name) else {
            return Vec::new();
        };
        model.parts.iter().map(|part| part.instance).collect()
    }

    pub(crate) fn advance(&mut self, seconds: f32, ground: &dyn Ground) {
        self.world.advance(seconds, ground);
    }

    pub(crate) fn warm_up(&mut self, seconds: f32, ground: &dyn Ground) {
        self.world.warm_up(seconds, ground);
    }

    pub(crate) fn draw(
        &mut self,
        renderer: &mut Renderer,
        camera: &Camera,
        out: &mut Vec<Instance>,
    ) -> mint_render::Result<()> {
        if !self.draw {
            return Ok(());
        }
        self.world.draw(renderer, camera, out)
    }
}
