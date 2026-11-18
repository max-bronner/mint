use mint_render::{Instance, Mat4, Renderer};
use mint_sage_asset::Resolver;
use mint_sage_ini::ObjectIndex;
use mint_sage_scene::{Conditions, ParticleScene, UnitDrawable, UnitPose, UnitScene};
use mint_vfs::Vfs;

const SUBJECT_ID: u32 = 0;

pub struct Subject {
    units: UnitScene,
    particles: mint_sage_scene::ConditionParticles,
    name: String,
    asked: Vec<String>,
    poses: Vec<UnitPose>,
    failure: Option<String>,
    clock: f32,
    visible: bool,
}

impl Default for Subject {
    fn default() -> Self {
        Self::new()
    }
}

impl Subject {
    #[must_use]
    pub fn new() -> Self {
        Self {
            units: UnitScene::new(),
            particles: mint_sage_scene::ConditionParticles::new(),
            name: String::new(),
            asked: Vec::new(),
            poses: Vec::new(),
            failure: None,
            clock: 0.0,
            visible: true,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn visible(&self) -> bool {
        self.visible
    }

    pub const fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    #[must_use]
    pub fn drawable(&self) -> Option<&UnitDrawable> {
        self.units.drawable_by_id(SUBJECT_ID)
    }

    pub fn show(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        objects: &ObjectIndex,
        name: &str,
    ) {
        if self.name.eq_ignore_ascii_case(name) {
            return;
        }
        self.name = name.to_owned();
        self.asked.clear();
        self.poses.clear();
        self.failure = None;
        self.units.sync(&[], self.clock);
        let Some(template) = self
            .units
            .template_for(renderer, vfs, resolver, objects, name)
        else {
            self.failure = Some(match objects.lookup(name) {
                mint_sage_ini::Template::NoModel(reason) => format!("{reason:?}"),
                _ => "its model would not load".to_owned(),
            });
            return;
        };
        self.poses.push(UnitPose {
            opacity: 1.0,
            id: SUBJECT_ID,
            generation: 0,
            template,
            placement: Mat4::IDENTITY,
            conditions: Conditions::NONE,
            attack_cycle: 0,
            attack_slot: None,
            speed: 0.0,
            weapon_cycle: 0.0,
            house_color: [0.0, 0.0, 0.0, 1.0],
            hero_colours: None,
            shroud_masked: false,
            progress: None,
            ghost: None,
            casts_shadow: true,
            look: mint_sage_scene::LookKey::PLAIN,
            ride: None,
        });
    }

    pub fn set_conditions(&mut self, tokens: &[String]) {
        self.asked = tokens.to_vec();
        let Some(pose) = self.poses.first_mut() else {
            return;
        };
        let Some(template) = self.units.templates.get(pose.template) else {
            return;
        };
        pose.conditions = template.extras.resolve(tokens.iter().map(String::as_str));
    }

    pub fn face(&mut self, bearing: f32) {
        let (sin, cos) = mint_math::f32::sin_cos(bearing);
        let placement = Mat4::from_cols([
            [cos, sin, 0.0, 0.0],
            [-sin, cos, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
        for pose in &mut self.poses {
            pose.placement = placement;
        }
    }

    #[must_use]
    pub fn asked(&self) -> &[String] {
        &self.asked
    }

    pub fn restart(&mut self) {
        self.units.sync(&[], self.clock);
        self.particles = mint_sage_scene::ConditionParticles::new();
    }

    pub fn advance(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        scene: &mut ParticleScene,
        seconds: f32,
        out: &mut Vec<Instance>,
    ) -> Vec<(u32, String)> {
        if self.poses.is_empty() {
            return Vec::new();
        }
        self.clock += seconds;
        self.units.sync(&self.poses, self.clock);
        let mut drawn = Vec::new();
        self.units.draw(&self.poses, self.clock, &mut drawn, &[]);
        if self.visible {
            out.append(&mut drawn);
        }
        let events: Vec<(u32, String)> = self.units.take_fx_events().collect();
        self.particles
            .sync(renderer, vfs, resolver, scene, &self.units);
        events
    }

    #[must_use]
    pub fn bones(&self) -> &[Mat4] {
        self.units.bones()
    }

    #[must_use]
    pub fn status(&self) -> String {
        if self.name.is_empty() {
            return "no object".to_owned();
        }
        if let Some(reason) = self.failure.as_deref() {
            return format!("object {}: no model - {reason}", self.name);
        }
        let asked = if self.asked.is_empty() {
            "(none)".to_owned()
        } else {
            self.asked.join(" ")
        };
        let chosen = self
            .units
            .state_line(SUBJECT_ID)
            .unwrap_or_else(|| "not drawn yet".to_owned());
        let frame = self.units.clip_frame(SUBJECT_ID).map_or_else(
            || "frame -".to_owned(),
            |(frame, frames)| format!("frame {frame:.1}/{frames}"),
        );
        format!(
            "object {} asked [{asked}] -> {chosen}, {frame}, model {}",
            self.name,
            if self.visible { "shown" } else { "hidden" }
        )
    }
}
