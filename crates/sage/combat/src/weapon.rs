use mint_sim::{Frame, LogicFrames, Pcg32, StateHasher};

use crate::nugget::Nugget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRange {
    min: LogicFrames,
    max: LogicFrames,
}

impl FrameRange {
    pub const ZERO: Self = Self {
        min: LogicFrames::ZERO,
        max: LogicFrames::ZERO,
    };

    #[must_use]
    pub const fn fixed(frames: LogicFrames) -> Self {
        Self {
            min: frames,
            max: frames,
        }
    }

    #[must_use]
    pub fn new(min: LogicFrames, max: LogicFrames) -> Self {
        Self {
            min,
            max: if max.get() < min.get() { min } else { max },
        }
    }

    #[must_use]
    pub const fn min(self) -> LogicFrames {
        self.min
    }

    #[must_use]
    pub const fn max(self) -> LogicFrames {
        self.max
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.min.is_zero() && self.max.is_zero()
    }

    pub fn draw(self, rng: &mut Pcg32) -> LogicFrames {
        let span = self.max.get().saturating_sub(self.min.get());
        if span == 0 {
            return self.min;
        }
        LogicFrames::from_frames(
            self.min
                .get()
                .saturating_add(rng.below(span.saturating_add(1))),
        )
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        self.min.hash_into(hasher);
        self.max.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreAttackType {
    #[default]
    PerShot,
    PerAttack,
    PerPosition,
    PerClip,
}

impl PreAttackType {
    #[must_use]
    pub fn parse(token: &str) -> Self {
        match token.trim().to_ascii_uppercase().as_str() {
            "PER_ATTACK" => Self::PerAttack,
            "PER_POSITION" => Self::PerPosition,
            "PER_CLIP" => Self::PerClip,
            _ => Self::PerShot,
        }
    }

    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::PerShot => 0,
            Self::PerAttack => 1,
            Self::PerPosition => 2,
            Self::PerClip => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FxTrigger {
    #[default]
    None,
    CatapultRock,
    TrebuchetRock,
}

impl FxTrigger {
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        [
            ("NONE", Self::None),
            ("CATAPULT_ROCK", Self::CatapultRock),
            ("TREBUCHET_ROCK", Self::TrebuchetRock),
        ]
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(token))
        .map(|(_, trigger)| trigger)
    }

    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::None => 0,
            Self::CatapultRock => 1,
            Self::TrebuchetRock => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WeaponStatus {
    PreAttack,
    #[default]
    ReadyToFire,
    BetweenFiringShots,
    ReloadingClip,
    OutOfAmmo,
}

impl WeaponStatus {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::PreAttack => 0,
            Self::ReadyToFire => 1,
            Self::BetweenFiringShots => 2,
            Self::ReloadingClip => 3,
            Self::OutOfAmmo => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AntiMask(u8);

impl AntiMask {
    pub const GROUND: Self = Self(1 << 0);
    pub const AIRBORNE_VEHICLE: Self = Self(1 << 1);
    pub const AIRBORNE_INFANTRY: Self = Self(1 << 2);
    pub const AIRBORNE_MONSTER: Self = Self(1 << 3);
    pub const STRUCTURE: Self = Self(1 << 4);
    pub const DEFAULT: Self = Self::GROUND;

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

impl Default for AntiMask {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeaponTemplate {
    pub attack_range: f32,
    pub minimum_attack_range: f32,
    pub delay_between_shots: FrameRange,
    pub pre_attack_delay: LogicFrames,
    pub firing_duration: LogicFrames,
    pub idle_after_firing: Option<LogicFrames>,
    pub hold_after_firing: LogicFrames,
    pub pre_attack_type: PreAttackType,
    pub clip_size: u32,
    pub clip_reload_time: FrameRange,
    pub clip_reload_step: LogicFrames,
    pub auto_reloads_clip: bool,
    pub auto_reload_when_idle: LogicFrames,
    pub hold_during_reload: bool,
    pub melee: bool,
    pub aiming: bool,
    pub weapon_speed: f32,
    pub min_weapon_speed: f32,
    pub max_weapon_speed: f32,
    pub scale_weapon_speed: bool,
    pub leech_range: bool,
    pub can_swoop: bool,
    pub nuggets: Vec<Nugget>,
    pub radius_affects: crate::victim::RadiusAffects,
    pub projectile_collides: crate::collide::CollideMask,
    pub damage_at_self: bool,
    pub fire_fx: Option<String>,
    pub fire_flank_fx: Option<String>,
    pub pre_attack_fx: Option<String>,
    pub anti_mask: AntiMask,
    pub aim_delta: f32,
    pub hit_chance: f32,
    pub scatter_radius: f32,
    pub scatter_vs_infantry: f32,
    pub range_bonus: RangeBonus,
    pub pre_attack_random: LogicFrames,
    pub hit_stored_target: bool,
    pub can_be_dodged: bool,
    pub hit_passenger_chance: f32,
    pub use_innate_attributes: bool,
    pub passenger_proportional_attack: bool,
    pub max_attack_passengers: u8,
    pub no_victim_needed: bool,
    pub can_fire_while_moving: bool,
    pub can_fire_while_charging: bool,
    pub aim_direction: f32,
    pub continue_attack_range: f32,
    pub projectile_filter_in_container: Vec<String>,
    pub projectile_self: bool,
    pub fx_trigger: FxTrigger,
    pub instant_load_clip_on_activate: bool,
    pub finish_attack_once_started: bool,
    pub chase_weapon: bool,
    pub lock_when_using: bool,
    pub bombard_type: bool,
    pub disable_scatter_for_targets_on_wall: bool,
    pub bonus: crate::weapon_bonus::WeaponBonusSet,
}

impl Default for WeaponTemplate {
    fn default() -> Self {
        Self {
            attack_range: 0.0,
            minimum_attack_range: 0.0,
            delay_between_shots: FrameRange::ZERO,
            pre_attack_delay: LogicFrames::ZERO,
            firing_duration: LogicFrames::ZERO,
            idle_after_firing: None,
            hold_after_firing: LogicFrames::ZERO,
            pre_attack_type: PreAttackType::PerShot,
            clip_size: 0,
            clip_reload_time: FrameRange::ZERO,
            clip_reload_step: LogicFrames::ZERO,
            auto_reloads_clip: true,
            auto_reload_when_idle: LogicFrames::ZERO,
            hold_during_reload: false,
            melee: false,
            aiming: false,
            weapon_speed: 0.0,
            min_weapon_speed: 0.0,
            max_weapon_speed: 0.0,
            scale_weapon_speed: false,
            leech_range: false,
            can_swoop: false,
            nuggets: Vec::new(),
            radius_affects: crate::victim::RadiusAffects::default(),
            projectile_collides: crate::collide::CollideMask::default(),
            damage_at_self: false,
            fire_fx: None,
            fire_flank_fx: None,
            pre_attack_fx: None,
            anti_mask: AntiMask::DEFAULT,
            aim_delta: 0.0,
            hit_chance: 1.0,
            scatter_radius: 0.0,
            scatter_vs_infantry: 0.0,
            range_bonus: RangeBonus::default(),
            pre_attack_random: LogicFrames::ZERO,
            hit_stored_target: false,
            can_be_dodged: false,
            hit_passenger_chance: 0.0,
            use_innate_attributes: false,
            passenger_proportional_attack: false,
            max_attack_passengers: 0,
            no_victim_needed: false,
            can_fire_while_moving: false,
            can_fire_while_charging: false,
            aim_direction: 0.0,
            continue_attack_range: 0.0,
            projectile_filter_in_container: Vec::new(),
            projectile_self: false,
            fx_trigger: FxTrigger::None,
            instant_load_clip_on_activate: false,
            finish_attack_once_started: false,
            chase_weapon: false,
            lock_when_using: false,
            bombard_type: false,
            disable_scatter_for_targets_on_wall: false,
            bonus: crate::weapon_bonus::WeaponBonusSet::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RangeBonus {
    pub min_height: f32,
    pub bonus: f32,
    pub per_foot: f32,
    pub restricted_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShotOutcome {
    Struck,
    Missed([f32; 2]),
}

#[must_use]
pub fn scatter_landing(rng: &mut Pcg32, radius: f32) -> ShotOutcome {
    if radius <= 0.0 || !radius.is_finite() {
        return ShotOutcome::Missed([0.0, 0.0]);
    }
    let distance = rng.next_f32() * radius;
    let bearing = rng.next_f32() * core::f32::consts::TAU;
    let (sin, cos) = mint_math::f32::sin_cos(bearing);
    ShotOutcome::Missed([distance * cos, distance * sin])
}

const PATHFIND_CELL_SIZE: f32 = 10.0;

pub const RANGE_TRIM: f32 = PATHFIND_CELL_SIZE * 0.25;

const MIN_AIM_TOLERANCE: f32 = 0.035;

impl WeaponTemplate {
    fn reload_draw(&self, rng: &mut Pcg32) -> LogicFrames {
        let drawn = self.clip_reload_time.draw(rng).get();
        let step = self.clip_reload_step.get();
        LogicFrames::from_frames(drawn.saturating_sub(drawn.checked_rem(step).unwrap_or(0)))
    }

    #[must_use]
    pub fn is_contact(&self) -> bool {
        self.attack_range - PATHFIND_CELL_SIZE * 0.25 < PATHFIND_CELL_SIZE
    }

    #[must_use]
    pub fn reach(&self, bonus: f32) -> f32 {
        self.reach_at(bonus, 0.0)
    }

    #[must_use]
    pub fn reach_at(&self, bonus: f32, height_above: f32) -> f32 {
        let trim = if self.melee { 0.0 } else { RANGE_TRIM };
        let mut reach = self.attack_range * bonus - trim;
        let height = self.range_bonus;
        if height_above >= height.min_height {
            reach += height.bonus + (height_above - height.min_height) * height.per_foot;
        }
        if height.restricted_height > 0.0 && height_above.abs() > height.restricted_height {
            reach = 0.0;
        }
        reach.max(0.0)
    }

    #[must_use]
    pub fn ranged_reach(&self, bonus: f32) -> f32 {
        (self.attack_range * bonus - RANGE_TRIM).max(0.0)
    }

    #[must_use]
    pub fn minimum_reach(&self) -> f32 {
        if self.melee {
            self.minimum_attack_range
        } else {
            (self.minimum_attack_range - RANGE_TRIM).max(0.0)
        }
    }

    #[must_use]
    pub fn aim_tolerance(&self) -> f32 {
        self.aim_delta.max(MIN_AIM_TOLERANCE)
    }

    #[must_use]
    pub fn winds_up(&self) -> bool {
        !self.pre_attack_delay.is_zero() || !self.pre_attack_random.is_zero()
    }

    #[must_use]
    pub fn fit_wind_up(&self, state: &WeaponState, moved: bool, pre_attack: f32) -> LogicFrames {
        let winds_up = match self.pre_attack_type {
            PreAttackType::PerShot | PreAttackType::PerAttack => true,
            PreAttackType::PerPosition => moved,
            PreAttackType::PerClip => self.clip_size == 0 || state.ammo_in_clip >= self.clip_size,
        };
        if !winds_up {
            return LogicFrames::ZERO;
        }
        self.pre_attack_under(pre_attack)
            .saturating_add(state.pre_attack_draw)
    }

    #[must_use]
    pub fn pre_attack_under(&self, pre_attack: f32) -> LogicFrames {
        #[allow(clippy::float_cmp)]
        if pre_attack == 1.0 || !pre_attack.is_finite() || pre_attack < 0.0 {
            return self.pre_attack_delay;
        }
        #[allow(clippy::cast_precision_loss)]
        let scaled = self.pre_attack_delay.get() as f32 * pre_attack;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = scaled.min(u32::MAX as f32) as u32;
        LogicFrames::from_frames(whole)
    }

    #[must_use]
    pub const fn starting_ammo(&self) -> u32 {
        if self.clip_size == 0 {
            u32::MAX
        } else {
            self.clip_size
        }
    }

    #[must_use]
    pub fn launch_speed(&self, distance: f32) -> f32 {
        if !self.scale_weapon_speed || !distance.is_finite() {
            return self.weapon_speed;
        }
        let band = self.attack_range - self.minimum_attack_range;
        if band <= 0.0 {
            return self.weapon_speed;
        }
        let top = self.weapon_speed.max(self.min_weapon_speed);
        let ratio = (distance * distance - self.minimum_attack_range) / band;
        let speed = ratio * (top - self.min_weapon_speed) + self.min_weapon_speed;
        if !speed.is_finite() {
            return if self.max_weapon_speed > 0.0 {
                self.max_weapon_speed
            } else {
                self.weapon_speed
            };
        }
        let speed = speed.max(self.min_weapon_speed);
        if self.max_weapon_speed > 0.0 {
            speed.min(self.max_weapon_speed)
        } else {
            speed
        }
    }

    #[must_use]
    pub fn has_effect(&self) -> bool {
        self.nuggets.iter().any(|n| {
            matches!(
                n,
                Nugget::Damage(_) | Nugget::Projectile(_) | Nugget::Dot(_) | Nugget::Modifier(_)
            )
        })
    }

    #[must_use]
    pub fn damage_delay(&self, distance: f32, hz: f32) -> Option<LogicFrames> {
        if self.launches_projectile() {
            return None;
        }
        if !self.weapon_speed.is_finite()
            || self.weapon_speed <= 0.0
            || !distance.is_finite()
            || !hz.is_finite()
            || hz <= 0.0
        {
            return None;
        }
        let frames = distance * hz / self.weapon_speed;
        if !frames.is_finite() || frames < 1.0 {
            return None;
        }
        let frames = frames.ceil();
        if frames >= f32::from(u16::MAX) {
            return Some(LogicFrames::from_frames(u32::from(u16::MAX)));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(LogicFrames::from_frames(frames as u32))
    }

    #[must_use]
    pub fn launches_projectile(&self) -> bool {
        self.nuggets
            .iter()
            .any(|nugget| matches!(nugget, Nugget::Projectile(_)))
    }

    pub fn resolve_shot(
        &self,
        rng: &mut Pcg32,
        infantry_victim: bool,
        victim_on_wall: bool,
    ) -> ShotOutcome {
        let scatters_here = !(self.disable_scatter_for_targets_on_wall && victim_on_wall);
        let inaccurate = infantry_victim && self.scatter_vs_infantry > 0.0 && scatters_here;
        let radius = if inaccurate {
            self.scatter_radius + self.scatter_vs_infantry
        } else {
            self.scatter_radius
        };
        let struck = if inaccurate {
            false
        } else if self.hit_chance >= 1.0 {
            true
        } else if self.hit_chance > 0.0 {
            rng.next_f32() < self.hit_chance
        } else {
            false
        };
        if struck {
            return ShotOutcome::Struck;
        }
        scatter_landing(rng, radius)
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.attack_range);
        hasher.write_f32(self.minimum_attack_range);
        self.delay_between_shots.hash_into(hasher);
        self.pre_attack_delay.hash_into(hasher);
        self.firing_duration.hash_into(hasher);
        hasher.write_bool(self.idle_after_firing.is_some());
        self.idle_after_firing
            .unwrap_or(LogicFrames::ZERO)
            .hash_into(hasher);
        self.hold_after_firing.hash_into(hasher);
        hasher.write_u8(self.pre_attack_type.code());
        hasher.write_u32(self.clip_size);
        self.clip_reload_time.hash_into(hasher);
        self.clip_reload_step.hash_into(hasher);
        hasher.write_bool(self.auto_reloads_clip);
        self.auto_reload_when_idle.hash_into(hasher);
        hasher.write_bool(self.hold_during_reload);
        hasher.write_bool(self.melee);
        hasher.write_f32(self.weapon_speed);
        hasher.write_f32(self.min_weapon_speed);
        hasher.write_f32(self.max_weapon_speed);
        hasher.write_bool(self.scale_weapon_speed);
        hasher.write_bool(self.leech_range);
        hasher.write_bool(self.can_swoop);
        hasher.write_usize(self.nuggets.len());
        for nugget in &self.nuggets {
            nugget.hash_into(hasher);
        }
        hasher.write_u8(self.radius_affects.bits());
        hasher.write_u16(self.projectile_collides.bits());
        hasher.write_bool(self.damage_at_self);
        hasher.write_u8(self.anti_mask.bits());
        hasher.write_f32(self.hit_chance);
        hasher.write_f32(self.scatter_radius);
        hasher.write_f32(self.scatter_vs_infantry);
        hasher.write_f32(self.range_bonus.min_height);
        hasher.write_f32(self.range_bonus.bonus);
        hasher.write_f32(self.range_bonus.per_foot);
        hasher.write_f32(self.range_bonus.restricted_height);
        self.pre_attack_random.hash_into(hasher);
        hasher.write_bool(self.hit_stored_target);
        hasher.write_bool(self.can_be_dodged);
        hasher.write_f32(self.hit_passenger_chance);
        hasher.write_bool(self.use_innate_attributes);
        hasher.write_bool(self.passenger_proportional_attack);
        hasher.write_u8(self.max_attack_passengers);
        hasher.write_bool(self.no_victim_needed);
        hasher.write_bool(self.can_fire_while_moving);
        hasher.write_bool(self.can_fire_while_charging);
        hasher.write_f32(self.aim_direction);
        hasher.write_f32(self.continue_attack_range);
        hasher.write_usize(self.projectile_filter_in_container.len());
        for token in &self.projectile_filter_in_container {
            hasher.write_bytes(token.as_bytes());
        }
        hasher.write_bool(self.projectile_self);
        hasher.write_u8(self.fx_trigger.code());
        hasher.write_bool(self.instant_load_clip_on_activate);
        hasher.write_bool(self.finish_attack_once_started);
        hasher.write_bool(self.chase_weapon);
        hasher.write_bool(self.lock_when_using);
        hasher.write_bool(self.bombard_type);
        hasher.write_bool(self.disable_scatter_for_targets_on_wall);
        self.bonus.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WeaponId(u32);

impl WeaponId {
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u32(self.0);
    }
}

#[derive(Debug, Clone, Default)]
pub struct WeaponStore {
    templates: Vec<WeaponTemplate>,
    sorted: Vec<(String, u32)>,
}

impl WeaponStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: &str, template: WeaponTemplate) -> Option<WeaponId> {
        let key = name.trim().to_ascii_lowercase();
        if key.is_empty() {
            return None;
        }
        match self.sorted.binary_search_by(|(k, _)| k.as_str().cmp(&key)) {
            Ok(at) => {
                let id = self.sorted.get(at).map(|(_, id)| *id)?;
                *self.templates.get_mut(id as usize)? = template;
                Some(WeaponId(id))
            }
            Err(at) => {
                let id = u32::try_from(self.templates.len()).ok()?;
                self.templates.push(template);
                self.sorted.insert(at, (key, id));
                Some(WeaponId(id))
            }
        }
    }

    #[must_use]
    pub fn estimated_damage(&self, id: WeaponId, armour: &crate::armor::ArmorTemplate) -> f32 {
        self.estimate(id, armour, 1)
    }

    #[must_use]
    pub fn valid_against(&self, id: WeaponId, armour: &crate::armor::ArmorTemplate) -> bool {
        self.valid(id, armour, 2)
    }

    fn valid(&self, id: WeaponId, armour: &crate::armor::ArmorTemplate, depth: u8) -> bool {
        let Some(template) = self.get(id) else {
            return false;
        };
        template.nuggets.iter().any(|nugget| match nugget {
            crate::nugget::Nugget::Damage(damage) => {
                !damage.requires_upgrade
                    && armour.adjust_damage(damage.damage_type, damage.damage)
                        * damage.damage_scalar
                        > 0.0
            }
            crate::nugget::Nugget::Dot(dot) => {
                !dot.requires_upgrade && armour.adjust_damage(dot.damage_type, dot.damage) > 0.0
            }
            crate::nugget::Nugget::Projectile(projectile) => {
                !projectile.requires_upgrade
                    && depth > 0
                    && self
                        .id(&projectile.warhead)
                        .is_some_and(|warhead| self.valid(warhead, armour, depth.saturating_sub(1)))
            }
            crate::nugget::Nugget::MetaImpact(_) | crate::nugget::Nugget::Modifier(_) => true,
            crate::nugget::Nugget::Unmodelled(keyword) => {
                !keyword.eq_ignore_ascii_case("FireLogicNugget")
            }
        })
    }

    fn estimate(&self, id: WeaponId, armour: &crate::armor::ArmorTemplate, depth: u8) -> f32 {
        let Some(template) = self.get(id) else {
            return 0.0;
        };
        let mut total = 0.0;
        for nugget in &template.nuggets {
            match nugget {
                crate::nugget::Nugget::Damage(damage) if !damage.requires_upgrade => {
                    total += armour.adjust_damage(damage.damage_type, damage.damage)
                        * damage.damage_scalar;
                }
                crate::nugget::Nugget::Projectile(projectile)
                    if depth > 0 && !projectile.requires_upgrade =>
                {
                    if let Some(warhead) = self.id(&projectile.warhead) {
                        total += self.estimate(warhead, armour, depth.saturating_sub(1));
                    }
                }
                _ => {}
            }
        }
        total
    }

    #[must_use]
    pub fn id(&self, name: &str) -> Option<WeaponId> {
        let key = name.trim().to_ascii_lowercase();
        self.sorted
            .binary_search_by(|(k, _)| k.as_str().cmp(&key))
            .ok()
            .and_then(|at| self.sorted.get(at))
            .map(|(_, id)| WeaponId(*id))
    }

    #[must_use]
    pub fn get(&self, id: WeaponId) -> Option<&WeaponTemplate> {
        self.templates.get(id.index())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (WeaponId, &WeaponTemplate)> {
        self.templates
            .iter()
            .enumerate()
            .filter_map(|(index, template)| {
                u32::try_from(index).ok().map(|id| (WeaponId(id), template))
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponState {
    when_we_can_fire_again: Frame,
    when_last_reload_started: Frame,
    when_pre_attack_finished: Frame,
    ammo_in_clip: u32,
    status: WeaponStatus,
    leech_until: Frame,
    last_shot: Option<Frame>,
    forced_reload_at: Option<Frame>,
    cycle_started_at: Frame,
    attack_cycle: u32,
    pre_attack_draw: LogicFrames,
}

impl WeaponState {
    #[must_use]
    pub fn new(template: &WeaponTemplate) -> Self {
        Self {
            when_we_can_fire_again: Frame::ZERO,
            when_last_reload_started: Frame::ZERO,
            when_pre_attack_finished: Frame::ZERO,
            ammo_in_clip: template.starting_ammo(),
            status: WeaponStatus::ReadyToFire,
            leech_until: Frame::ZERO,
            last_shot: None,
            forced_reload_at: None,
            cycle_started_at: Frame::ZERO,
            attack_cycle: 0,
            pre_attack_draw: LogicFrames::ZERO,
        }
    }

    #[must_use]
    pub fn activated(template: &WeaponTemplate, now: Frame, rate: f32, rng: &mut Pcg32) -> Self {
        let mut state = Self::new(template);
        if !template.instant_load_clip_on_activate {
            state.status = WeaponStatus::ReloadingClip;
            state.when_last_reload_started = now;
            state.when_we_can_fire_again = now
                .checked_add(at_rate(template.reload_draw(rng), rate))
                .unwrap_or(Frame::FOREVER);
        }
        state
    }

    pub fn abort_wind_up(&mut self) {
        self.when_pre_attack_finished = Frame::ZERO;
        if self.status == WeaponStatus::PreAttack {
            self.status = WeaponStatus::ReadyToFire;
        }
        self.leech_until = Frame::ZERO;
    }

    pub fn start_cycle(&mut self, now: Frame) {
        self.cycle_started_at = now;
        self.attack_cycle = self.attack_cycle.wrapping_add(1);
    }

    #[must_use]
    pub const fn cycle_started_at(&self) -> Frame {
        self.cycle_started_at
    }

    #[must_use]
    pub const fn attack_cycle(&self) -> u32 {
        self.attack_cycle
    }

    #[must_use]
    pub fn leeching(&self, now: Frame) -> bool {
        now < self.leech_until
    }

    pub fn end_leech(&mut self) {
        self.leech_until = Frame::ZERO;
    }

    pub fn status(&mut self, now: Frame) -> WeaponStatus {
        if now < self.when_pre_attack_finished {
            return WeaponStatus::PreAttack;
        }
        if now >= self.when_we_can_fire_again {
            self.status = if self.ammo_in_clip > 0 {
                WeaponStatus::ReadyToFire
            } else {
                WeaponStatus::OutOfAmmo
            };
        }
        self.status
    }

    #[must_use]
    pub const fn ammo_in_clip(&self) -> u32 {
        self.ammo_in_clip
    }

    #[must_use]
    pub const fn when_we_can_fire_again(&self) -> Frame {
        self.when_we_can_fire_again
    }

    #[must_use]
    pub const fn when_pre_attack_finished(&self) -> Frame {
        self.when_pre_attack_finished
    }

    #[must_use]
    pub const fn when_last_reload_started(&self) -> Frame {
        self.when_last_reload_started
    }

    #[must_use]
    pub const fn last_shot(&self) -> Option<Frame> {
        self.last_shot
    }

    pub fn reload_when_idle(
        &mut self,
        template: &WeaponTemplate,
        now: Frame,
        rate: f32,
        rng: &mut Pcg32,
    ) -> bool {
        if self.forced_reload_at.is_none_or(|at| now < at) {
            return false;
        }
        self.forced_reload_at = None;
        if template.clip_size > 0 && self.ammo_in_clip == template.clip_size {
            return false;
        }
        self.reload_at_rate(template, now, rate, rng);
        true
    }

    pub fn holds_orders(&mut self, template: &WeaponTemplate, now: Frame) -> bool {
        template.hold_during_reload && self.status(now) == WeaponStatus::ReloadingClip
    }

    pub fn begin_attack(
        &mut self,
        template: &WeaponTemplate,
        now: Frame,
        first_shot: bool,
        moved: bool,
        pre_attack: f32,
        rng: &mut Pcg32,
    ) {
        self.pre_attack_draw =
            FrameRange::new(LogicFrames::ZERO, template.pre_attack_random).draw(rng);
        let applies = match template.pre_attack_type {
            PreAttackType::PerShot => true,
            PreAttackType::PerAttack => first_shot,
            PreAttackType::PerPosition => moved,
            PreAttackType::PerClip => {
                template.clip_size == 0 || self.ammo_in_clip >= template.clip_size
            }
        };
        if !applies || !template.winds_up() {
            return;
        }
        let wound = template.pre_attack_under(pre_attack);
        let delay = wound.get().saturating_add(self.pre_attack_draw.get());
        if delay == 0 {
            return;
        }
        self.status = WeaponStatus::PreAttack;
        self.when_pre_attack_finished = now
            .checked_add(LogicFrames::from_frames(delay))
            .unwrap_or(Frame::FOREVER);
        if template.leech_range {
            self.leech_until = now
                .checked_add(wound.saturating_add(template.firing_duration))
                .unwrap_or(Frame::FOREVER);
        }
    }

    pub fn fire(&mut self, template: &WeaponTemplate, now: Frame, rng: &mut Pcg32) -> bool {
        self.fire_at_rate(template, now, 1.0, rng)
    }

    pub fn fire_at_rate(
        &mut self,
        template: &WeaponTemplate,
        now: Frame,
        rate: f32,
        rng: &mut Pcg32,
    ) -> bool {
        self.last_shot = Some(now);
        if template.leech_range {
            self.leech_until = now
                .checked_add(template.firing_duration.max(LogicFrames::from_frames(1)))
                .unwrap_or(Frame::FOREVER);
        }
        self.forced_reload_at = (template.auto_reload_when_idle > LogicFrames::ZERO)
            .then(|| now.checked_add(template.auto_reload_when_idle))
            .flatten();
        self.ammo_in_clip = self.ammo_in_clip.saturating_sub(1);
        if self.ammo_in_clip == 0 {
            if template.auto_reloads_clip {
                self.reload_at_rate(template, now, rate, rng);
                return true;
            }
            self.status = WeaponStatus::OutOfAmmo;
            self.when_we_can_fire_again = Frame::FOREVER;
            return false;
        }
        self.status = WeaponStatus::BetweenFiringShots;
        self.when_last_reload_started = now;
        self.when_we_can_fire_again = now
            .checked_add(at_rate(template.delay_between_shots.draw(rng), rate))
            .unwrap_or(Frame::FOREVER);
        false
    }

    pub fn reload(&mut self, template: &WeaponTemplate, now: Frame, rng: &mut Pcg32) {
        self.reload_at_rate(template, now, 1.0, rng);
    }

    pub fn reload_at_rate(
        &mut self,
        template: &WeaponTemplate,
        now: Frame,
        rate: f32,
        rng: &mut Pcg32,
    ) {
        if template.clip_size > 0 && self.ammo_in_clip == template.clip_size {
            return;
        }
        self.ammo_in_clip = template.starting_ammo();
        self.status = WeaponStatus::ReloadingClip;
        self.when_last_reload_started = now;
        self.when_we_can_fire_again = now
            .checked_add(at_rate(template.reload_draw(rng), rate))
            .unwrap_or(Frame::FOREVER);
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        self.when_we_can_fire_again.hash_into(hasher);
        self.when_last_reload_started.hash_into(hasher);
        self.when_pre_attack_finished.hash_into(hasher);
        hasher.write_u32(self.ammo_in_clip);
        hasher.write_u8(self.status.code());
        self.leech_until.hash_into(hasher);
        for stamp in [self.last_shot, self.forced_reload_at] {
            hasher.write_bool(stamp.is_some());
            if let Some(frame) = stamp {
                frame.hash_into(hasher);
            }
        }
        self.cycle_started_at.hash_into(hasher);
        hasher.write_u32(self.attack_cycle);
        self.pre_attack_draw.hash_into(hasher);
    }
}

fn at_rate(frames: LogicFrames, rate: f32) -> LogicFrames {
    if !rate.is_finite() || rate <= 0.0 {
        return frames;
    }
    #[allow(clippy::cast_precision_loss)]
    let scaled = frames.get() as f32 / rate;
    if !scaled.is_finite() || scaled < 0.0 {
        return frames;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = scaled.min(u32::MAX as f32) as u32;
    LogicFrames::from_frames(whole)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn rng() -> Pcg32 {
        Pcg32::new(1)
    }

    #[test]
    fn a_shooter_high_over_its_victim_reaches_further() {
        let weapon = WeaponTemplate {
            attack_range: 200.0,
            range_bonus: RangeBonus {
                min_height: 30.0,
                bonus: 1.0,
                per_foot: 0.6,
                restricted_height: 0.0,
            },
            ..WeaponTemplate::default()
        };
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(near(weapon.reach_at(1.0, 80.0), 228.5));
        assert!(near(weapon.reach_at(1.0, 29.0), 197.5));
        assert!(near(weapon.reach_at(1.0, -80.0), 197.5));
        let restricted = WeaponTemplate {
            range_bonus: RangeBonus {
                restricted_height: 60.0,
                ..weapon.range_bonus
            },
            ..weapon
        };
        assert_eq!(restricted.reach_at(1.0, -80.0), 0.0);
        assert_eq!(restricted.reach_at(1.0, 61.0), 0.0);
        assert!(near(restricted.reach_at(1.0, 50.0), 197.5 + 1.0 + 12.0));
    }

    #[test]
    fn travel_under_a_frame_lands_at_once_and_longer_travel_rounds_up() {
        let weapon = WeaponTemplate {
            weapon_speed: 30.0,
            ..WeaponTemplate::default()
        };
        let hz = 30.0;
        assert_eq!(weapon.damage_delay(0.5, hz), None);
        assert_eq!(weapon.damage_delay(1.0, hz), Some(LogicFrames::ONE));
        assert_eq!(
            weapon.damage_delay(1.2, hz),
            Some(LogicFrames::from_frames(2))
        );
        assert_eq!(
            WeaponTemplate::default().damage_delay(600.0, hz),
            None,
            "an undeclared WeaponSpeed is instantaneous"
        );
    }

    #[test]
    fn a_shot_delay_band_is_drawn_from_and_a_plain_one_is_not() {
        let banded = WeaponTemplate {
            delay_between_shots: FrameRange::new(
                LogicFrames::from_frames(10),
                LogicFrames::from_frames(20),
            ),
            ..WeaponTemplate::default()
        };
        let mut rng = Pcg32::new(7);
        let mut seen = Vec::new();
        for shot in 0..16u64 {
            let mut state = WeaponState::new(&banded);
            state.fire(&banded, Frame::new(shot), &mut rng);
            let delay = state.when_we_can_fire_again().get().saturating_sub(shot);
            assert!((10..=20).contains(&delay), "{delay}");
            seen.push(delay);
        }
        assert!(seen.iter().any(|d| *d != seen[0]), "{seen:?}");

        let plain = template();
        let mut untouched = Pcg32::new(7);
        let mut state = WeaponState::new(&plain);
        state.fire(&plain, Frame::ZERO, &mut untouched);
        assert_eq!(state.when_we_can_fire_again(), Frame::new(30));
        assert_eq!(untouched.next_u32(), Pcg32::new(7).next_u32());
    }

    #[test]
    fn a_scaled_weapon_scales_with_the_squared_distance() {
        let mut t = WeaponTemplate {
            attack_range: 300.0,
            minimum_attack_range: 0.0,
            weapon_speed: 321.0,
            min_weapon_speed: 241.0,
            max_weapon_speed: 481.0,
            scale_weapon_speed: true,
            ..WeaponTemplate::default()
        };
        assert_eq!(t.launch_speed(0.0), 241.0);
        assert_eq!(t.launch_speed(15.0), 301.0);
        assert_eq!(t.launch_speed(80.0), 481.0, "capped at the maximum");
        t.scale_weapon_speed = false;
        assert_eq!(t.launch_speed(80.0), 321.0);
    }

    #[test]
    fn a_ten_unit_range_is_contact_and_twelve_and_a_half_is_not() {
        let contact = WeaponTemplate {
            attack_range: 10.0,
            ..WeaponTemplate::default()
        };
        let aimed = WeaponTemplate {
            attack_range: 12.5,
            ..WeaponTemplate::default()
        };
        assert!(contact.is_contact());
        assert!(!aimed.is_contact());
    }

    #[test]
    fn anti_mask_unions_and_removes_bits_independently() {
        let mask = AntiMask::DEFAULT.union(AntiMask::STRUCTURE);
        assert!(mask.contains(AntiMask::GROUND));
        assert!(mask.contains(AntiMask::STRUCTURE));
        let ground_only = mask.without(AntiMask::STRUCTURE);
        assert!(ground_only.contains(AntiMask::GROUND));
        assert!(!ground_only.contains(AntiMask::STRUCTURE));
        assert_eq!(AntiMask::default(), AntiMask::GROUND);
    }

    #[test]
    fn a_rate_of_fire_divides_the_delay_and_the_reload() {
        let t = template();
        let mut state = WeaponState::new(&t);
        state.fire_at_rate(&t, Frame::ZERO, 2.0, &mut rng());
        assert_eq!(state.when_we_can_fire_again(), Frame::new(15));
        state.fire_at_rate(&t, Frame::new(15), 2.0, &mut rng());
        state.fire_at_rate(&t, Frame::new(30), 2.0, &mut rng());
        assert_eq!(state.when_we_can_fire_again(), Frame::new(60), "60 / 2");
        let mut state = WeaponState::new(&t);
        state.fire_at_rate(&t, Frame::ZERO, 3.0, &mut rng());
        assert_eq!(state.when_we_can_fire_again(), Frame::new(10));
        for rate in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut state = WeaponState::new(&t);
            state.fire_at_rate(&t, Frame::ZERO, rate, &mut rng());
            assert_eq!(state.when_we_can_fire_again(), Frame::new(30), "{rate}");
        }
    }

    #[test]
    fn a_clip_reload_rounds_down_to_its_step_before_the_rate() {
        let t = WeaponTemplate {
            clip_reload_time: FrameRange::fixed(LogicFrames::from_frames(50)),
            clip_reload_step: LogicFrames::from_frames(18),
            ..template()
        };
        for (rate, expected) in [(1.0, 36), (2.0, 18)] {
            let mut state = WeaponState::new(&t);
            state.fire(&t, Frame::ZERO, &mut rng());
            assert_eq!(state.when_we_can_fire_again(), Frame::new(30));
            state.reload_at_rate(&t, Frame::ZERO, rate, &mut rng());
            assert_eq!(
                state.when_we_can_fire_again(),
                Frame::new(expected),
                "{rate}"
            );
        }
    }

    #[test]
    fn a_leech_weapon_with_no_wind_up_is_armed_at_the_shot() {
        let t = WeaponTemplate {
            leech_range: true,
            firing_duration: LogicFrames::from_frames(6),
            ..WeaponTemplate::default()
        };
        let mut state = WeaponState::new(&t);
        state.fire(&t, Frame::new(100), &mut rng());
        assert!(state.leeching(Frame::new(105)));
        assert!(!state.leeching(Frame::new(106)));
        let bare = WeaponTemplate {
            leech_range: true,
            ..WeaponTemplate::default()
        };
        let mut state = WeaponState::new(&bare);
        state.fire(&bare, Frame::new(100), &mut rng());
        assert!(state.leeching(Frame::new(100)));
        assert!(!state.leeching(Frame::new(101)));
    }

    #[test]
    fn auto_reload_when_idle_reloads_a_clipless_weapon_for_its_clip_reload_time() {
        let t = WeaponTemplate {
            delay_between_shots: FrameRange::fixed(LogicFrames::from_frames(180)),
            clip_reload_time: FrameRange::fixed(LogicFrames::from_frames(90)),
            auto_reload_when_idle: LogicFrames::from_frames(60),
            hold_during_reload: true,
            ..WeaponTemplate::default()
        };
        let mut state = WeaponState::new(&t);
        state.fire(&t, Frame::new(100), &mut rng());
        let tick = |state: &mut WeaponState, frame: u64| {
            state.reload_when_idle(&t, Frame::new(frame), 1.0, &mut rng())
        };
        assert!(!tick(&mut state, 159), "not before AutoReloadWhenIdle");
        assert_eq!(
            state.status(Frame::new(159)),
            WeaponStatus::BetweenFiringShots
        );
        assert!(!state.holds_orders(&t, Frame::new(159)));
        assert!(tick(&mut state, 160));
        assert!(!tick(&mut state, 161), "once per shot");
        assert_eq!(state.status(Frame::new(160)), WeaponStatus::ReloadingClip);
        assert!(state.holds_orders(&t, Frame::new(249)));
        assert_eq!(state.status(Frame::new(250)), WeaponStatus::ReadyToFire);
        assert_eq!(state.last_shot(), Some(Frame::new(100)));

        let mut state = WeaponState::new(&t);
        state.fire(&t, Frame::new(0), &mut rng());
        state.fire(&t, Frame::new(40), &mut rng());
        assert!(
            !tick(&mut state, 60),
            "the second shot moved the reload to 100"
        );
        assert!(tick(&mut state, 100));
    }

    #[test]
    fn auto_reload_when_idle_leaves_a_full_clip_alone() {
        let t = WeaponTemplate {
            clip_size: 1,
            clip_reload_time: FrameRange::fixed(LogicFrames::from_frames(40)),
            auto_reload_when_idle: LogicFrames::from_frames(1),
            ..WeaponTemplate::default()
        };
        let mut state = WeaponState::new(&t);
        state.fire(&t, Frame::new(10), &mut rng());
        let before = state;
        assert!(!state.reload_when_idle(&t, Frame::new(11), 1.0, &mut rng()));
        assert_eq!(
            state.when_we_can_fire_again(),
            before.when_we_can_fire_again()
        );
        assert_eq!(state.status(Frame::new(49)), WeaponStatus::ReloadingClip);
        assert_eq!(state.status(Frame::new(50)), WeaponStatus::ReadyToFire);
    }

    fn template() -> WeaponTemplate {
        WeaponTemplate {
            attack_range: 150.0,
            delay_between_shots: FrameRange::fixed(LogicFrames::from_frames(30)),
            pre_attack_delay: LogicFrames::from_frames(15),
            clip_size: 3,
            clip_reload_time: FrameRange::fixed(LogicFrames::from_frames(60)),
            ..WeaponTemplate::default()
        }
    }

    #[test]
    fn a_fresh_weapon_is_ready_with_a_full_clip() {
        let t = template();
        let mut state = WeaponState::new(&t);
        assert_eq!(state.ammo_in_clip(), 3);
        assert_eq!(state.status(Frame::ZERO), WeaponStatus::ReadyToFire);
    }

    #[test]
    fn a_clip_size_of_zero_is_unlimited_rather_than_empty() {
        let t = WeaponTemplate {
            clip_size: 0,
            ..template()
        };
        let mut state = WeaponState::new(&t);
        assert_eq!(state.ammo_in_clip(), u32::MAX);
        for shot in 0..100u64 {
            let now = Frame::new(shot.saturating_mul(30));
            assert_eq!(state.status(now), WeaponStatus::ReadyToFire, "shot {shot}");
            state.fire(&t, now, &mut rng());
        }
    }

    #[test]
    fn the_state_machine_runs_pre_attack_then_shots_then_a_reload() {
        let t = template();
        let mut state = WeaponState::new(&t);
        state.begin_attack(&t, Frame::ZERO, true, true, 1.0, &mut rng());
        assert_eq!(state.status(Frame::ZERO), WeaponStatus::PreAttack);
        assert_eq!(state.status(Frame::new(14)), WeaponStatus::PreAttack);
        assert_eq!(state.status(Frame::new(15)), WeaponStatus::ReadyToFire);

        state.fire(&t, Frame::new(15), &mut rng());
        assert_eq!(state.ammo_in_clip(), 2);
        assert_eq!(
            state.status(Frame::new(16)),
            WeaponStatus::BetweenFiringShots
        );
        assert_eq!(state.status(Frame::new(45)), WeaponStatus::ReadyToFire);

        state.fire(&t, Frame::new(45), &mut rng());
        assert_eq!(state.ammo_in_clip(), 1);
        state.fire(&t, Frame::new(75), &mut rng());
        assert_eq!(state.ammo_in_clip(), 3);
        assert_eq!(state.status(Frame::new(76)), WeaponStatus::ReloadingClip);
        assert_eq!(state.status(Frame::new(134)), WeaponStatus::ReloadingClip);
        assert_eq!(state.status(Frame::new(135)), WeaponStatus::ReadyToFire);
    }

    #[test]
    fn a_weapon_that_does_not_auto_reload_runs_out_of_ammo_forever() {
        let t = WeaponTemplate {
            auto_reloads_clip: false,
            clip_size: 1,
            ..template()
        };
        let mut state = WeaponState::new(&t);
        state.fire(&t, Frame::new(10), &mut rng());
        assert_eq!(state.status(Frame::new(11)), WeaponStatus::OutOfAmmo);
        assert_eq!(state.status(Frame::new(1_000_000)), WeaponStatus::OutOfAmmo);
        state.reload(&t, Frame::new(1_000_000), &mut rng());
        assert_eq!(state.ammo_in_clip(), 1);
    }

    #[test]
    fn reloading_a_full_clip_does_not_restart_the_delay() {
        let t = template();
        let mut state = WeaponState::new(&t);
        state.reload(&t, Frame::new(100), &mut rng());
        assert_eq!(state.when_we_can_fire_again(), Frame::ZERO);
        assert_eq!(state.status(Frame::new(100)), WeaponStatus::ReadyToFire);
    }

    #[test]
    fn the_pre_attack_type_decides_whether_the_wind_up_applies() {
        for (kind, new_target, moved, expected) in [
            (PreAttackType::PerShot, true, false, WeaponStatus::PreAttack),
            (
                PreAttackType::PerShot,
                false,
                false,
                WeaponStatus::PreAttack,
            ),
            (
                PreAttackType::PerAttack,
                true,
                false,
                WeaponStatus::PreAttack,
            ),
            (
                PreAttackType::PerAttack,
                false,
                true,
                WeaponStatus::ReadyToFire,
            ),
            (
                PreAttackType::PerPosition,
                true,
                false,
                WeaponStatus::ReadyToFire,
            ),
            (
                PreAttackType::PerPosition,
                false,
                true,
                WeaponStatus::PreAttack,
            ),
        ] {
            let t = WeaponTemplate {
                pre_attack_type: kind,
                ..template()
            };
            let mut state = WeaponState::new(&t);
            state.begin_attack(&t, Frame::ZERO, new_target, moved, 1.0, &mut rng());
            assert_eq!(
                state.status(Frame::ZERO),
                expected,
                "{kind:?} {new_target} {moved}"
            );
        }
    }

    #[test]
    fn a_zero_pre_attack_delay_does_not_set_pre_attack() {
        let t = WeaponTemplate {
            pre_attack_delay: LogicFrames::ZERO,
            ..template()
        };
        let mut state = WeaponState::new(&t);
        state.begin_attack(&t, Frame::new(5), true, true, 1.0, &mut rng());
        assert_eq!(state.status(Frame::new(5)), WeaponStatus::ReadyToFire);

        state.fire(&t, Frame::new(5), &mut rng());
        assert_eq!(
            state.status(Frame::new(6)),
            WeaponStatus::BetweenFiringShots
        );
        state.begin_attack(&t, Frame::new(6), true, true, 1.0, &mut rng());
        assert_eq!(
            state.status(Frame::new(6)),
            WeaponStatus::BetweenFiringShots,
            "a zero-delay wind-up overwrote the between-shots status"
        );
    }

    #[test]
    fn an_unrecognised_pre_attack_token_reads_as_per_shot() {
        assert_eq!(PreAttackType::parse("PER_ATTACK"), PreAttackType::PerAttack);
        assert_eq!(
            PreAttackType::parse("per_position"),
            PreAttackType::PerPosition
        );
        assert_eq!(PreAttackType::parse("PER_CLIP"), PreAttackType::PerClip);
        assert_eq!(PreAttackType::parse("NONSENSE"), PreAttackType::PerShot);
        assert_eq!(PreAttackType::parse(""), PreAttackType::PerShot);
    }

    #[test]
    fn every_field_of_a_weapon_state_changes_its_hash() {
        let t = template();
        let base = WeaponState::new(&t);
        let hash_of = |s: &WeaponState| {
            let mut hasher = StateHasher::new();
            s.hash_into(&mut hasher);
            hasher.finish()
        };
        let mut fired = base;
        fired.when_we_can_fire_again = Frame::new(9);
        let mut reloaded = base;
        reloaded.when_last_reload_started = Frame::new(9);
        let mut winding = base;
        winding.when_pre_attack_finished = Frame::new(9);
        let mut spent = base;
        spent.ammo_in_clip = 1;
        let mut other = base;
        other.status = WeaponStatus::OutOfAmmo;
        let mut leeching = base;
        leeching.leech_until = Frame::new(9);
        for s in [fired, reloaded, winding, spent, other, leeching] {
            assert_ne!(hash_of(&base), hash_of(&s), "{s:?}");
        }
    }

    #[test]
    fn the_store_replaces_a_redefined_name_and_keeps_its_id() {
        let mut store = WeaponStore::new();
        let a = store.insert("Bow", template()).unwrap();
        let b = store
            .insert(
                "bow",
                WeaponTemplate {
                    attack_range: 999.0,
                    ..template()
                },
            )
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(store.len(), 1);
        assert!((store.get(a).unwrap().attack_range - 999.0).abs() < 1e-3);
        assert_eq!(store.id("BOW"), Some(a));
        assert_eq!(store.id("missing"), None);
    }

    #[test]
    fn a_certain_hit_or_miss_is_decided_without_a_draw() {
        let always = WeaponTemplate::default();
        let never = WeaponTemplate {
            hit_chance: 0.0,
            ..WeaponTemplate::default()
        };
        let mut stream = rng();
        for _ in 0..50 {
            assert_eq!(
                always.resolve_shot(&mut stream, false, false),
                ShotOutcome::Struck
            );
            assert_eq!(
                never.resolve_shot(&mut stream, false, false),
                ShotOutcome::Missed([0.0, 0.0])
            );
        }
        assert_eq!(stream.next_u32(), rng().next_u32());
    }

    #[test]
    fn a_half_chance_weapon_strikes_about_half_its_shots_the_same_way_each_run() {
        let t = WeaponTemplate {
            hit_chance: 0.5,
            scatter_radius: 50.0,
            ..WeaponTemplate::default()
        };
        let run = |seed| {
            let mut stream = Pcg32::new(seed);
            (0..200)
                .map(|_| t.resolve_shot(&mut stream, false, false))
                .collect::<Vec<_>>()
        };
        let shots = run(7);
        let struck = shots
            .iter()
            .filter(|shot| **shot == ShotOutcome::Struck)
            .count();
        assert!((70..=130).contains(&struck), "{struck} of 200 struck");
        for shot in &shots {
            if let ShotOutcome::Missed([dx, dy]) = shot {
                assert!(dx * dx + dy * dy <= 50.0 * 50.0 + 1e-3, "{dx},{dy}");
            }
        }
        assert_eq!(shots, run(7));
    }
    #[test]
    fn a_weapon_inaccurate_against_infantry_misses_every_infantry_victim() {
        let t = WeaponTemplate {
            hit_chance: 1.0,
            scatter_radius: 50.0,
            scatter_vs_infantry: 40.0,
            ..WeaponTemplate::default()
        };
        let mut stream = Pcg32::new(3);
        let mut farthest = 0.0f32;
        for _ in 0..200 {
            let outcome = t.resolve_shot(&mut stream, true, false);
            assert!(
                matches!(outcome, ShotOutcome::Missed(_)),
                "a shot at infantry struck"
            );
            if let ShotOutcome::Missed([dx, dy]) = outcome {
                farthest = farthest.max(dx * dx + dy * dy);
            }
        }
        assert!(
            farthest > 50.0 * 50.0 && farthest <= 90.0 * 90.0 + 1e-3,
            "{farthest}"
        );
        assert_eq!(
            t.resolve_shot(&mut stream, false, false),
            ShotOutcome::Struck
        );
    }

    #[test]
    fn an_aborted_wind_up_leaves_the_weapon_ready() {
        let t = WeaponTemplate {
            pre_attack_delay: LogicFrames::from_frames(30),
            ..WeaponTemplate::default()
        };
        let mut state = WeaponState::new(&t);
        let mut stream = Pcg32::new(1);
        state.begin_attack(&t, Frame::new(10), true, true, 1.0, &mut stream);
        assert_eq!(state.status(Frame::new(15)), WeaponStatus::PreAttack);
        state.abort_wind_up();
        assert_eq!(state.status(Frame::new(15)), WeaponStatus::ReadyToFire);
    }

    #[test]
    fn disable_scatter_for_targets_on_wall_spares_a_victim_on_a_wall() {
        let t = WeaponTemplate {
            hit_chance: 1.0,
            scatter_vs_infantry: 40.0,
            disable_scatter_for_targets_on_wall: true,
            ..WeaponTemplate::default()
        };
        let mut stream = Pcg32::new(3);
        assert_eq!(t.resolve_shot(&mut stream, true, true), ShotOutcome::Struck);
        assert!(matches!(
            t.resolve_shot(&mut stream, true, false),
            ShotOutcome::Missed(_)
        ));
        let plain = WeaponTemplate {
            disable_scatter_for_targets_on_wall: false,
            ..t
        };
        assert!(matches!(
            plain.resolve_shot(&mut stream, true, true),
            ShotOutcome::Missed(_)
        ));
    }

    #[test]
    fn fit_wind_up_includes_this_cycles_random_draw() {
        let t = WeaponTemplate {
            pre_attack_delay: LogicFrames::from_frames(15),
            pre_attack_random: LogicFrames::from_frames(20),
            ..WeaponTemplate::default()
        };
        let mut state = WeaponState::new(&t);
        state.pre_attack_draw = LogicFrames::from_frames(12);
        assert_eq!(
            t.fit_wind_up(&state, false, 1.0),
            LogicFrames::from_frames(27),
            "PreAttackDelay + this cycle's draw"
        );
    }

    #[test]
    fn fit_wind_up_for_per_position_depends_on_whether_the_shooter_moved() {
        let t = WeaponTemplate {
            pre_attack_type: PreAttackType::PerPosition,
            pre_attack_delay: LogicFrames::from_frames(15),
            ..WeaponTemplate::default()
        };
        let mut state = WeaponState::new(&t);
        state.pre_attack_draw = LogicFrames::from_frames(5);
        assert_eq!(
            t.fit_wind_up(&state, false, 1.0),
            LogicFrames::ZERO,
            "stationary: no wind-up"
        );
        assert_eq!(
            t.fit_wind_up(&state, true, 1.0),
            LogicFrames::from_frames(20),
            "moved: delay + draw"
        );
    }

    #[test]
    fn fit_wind_up_for_per_attack_always_winds_up_unlike_begin_attack() {
        let t = WeaponTemplate {
            pre_attack_type: PreAttackType::PerAttack,
            pre_attack_delay: LogicFrames::from_frames(15),
            ..WeaponTemplate::default()
        };
        let mut state = WeaponState::new(&t);
        state.pre_attack_draw = LogicFrames::from_frames(5);
        assert_eq!(
            t.fit_wind_up(&state, false, 1.0),
            LogicFrames::from_frames(20)
        );
    }
}
