use mint_sage_combat::DamageTypeId;
use mint_sim::{Component, Entity, Frame, PlayerId, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Struck {
    pub(crate) victim: Entity,
    pub(crate) attacker: Entity,
    pub(crate) damage_type: DamageTypeId,
    pub(crate) dealt: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CombatFacts {
    frame: Frame,
    struck: Vec<Struck>,
    kills: Vec<(Entity, Entity)>,
    deaths: Vec<(Entity, bool)>,
    attaches: Vec<(Entity, Option<PlayerId>)>,
    attach_losses: Vec<Entity>,
    last_member_deaths: Vec<Entity>,
    detections: Vec<(Entity, Entity)>,
    wall_climbs: Vec<(Entity, Entity)>,
}

impl CombatFacts {
    fn begin(&mut self, now: Frame) {
        if self.frame != now {
            self.struck.clear();
            self.kills.clear();
            self.deaths.clear();
            self.attaches.clear();
            self.attach_losses.clear();
            self.last_member_deaths.clear();
            self.detections.clear();
            self.wall_climbs.clear();
            self.frame = now;
        }
    }

    pub(crate) fn push_struck(&mut self, now: Frame, struck: Struck) {
        self.begin(now);
        self.struck.push(struck);
    }

    pub(crate) fn push_kill(&mut self, now: Frame, victim: Entity, killer: Entity) {
        self.begin(now);
        self.kills.push((victim, killer));
    }

    pub(crate) fn push_death(&mut self, now: Frame, entity: Entity, faded: bool) {
        self.begin(now);
        self.deaths.push((entity, faded));
    }

    pub(crate) fn push_attach(
        &mut self,
        now: Frame,
        entity: Entity,
        parent_owner: Option<PlayerId>,
    ) {
        self.begin(now);
        self.attaches.push((entity, parent_owner));
    }

    pub(crate) fn push_attach_loss(&mut self, now: Frame, entity: Entity) {
        self.begin(now);
        self.attach_losses.push(entity);
    }

    pub(crate) fn push_last_member_death(&mut self, now: Frame, entity: Entity) {
        self.begin(now);
        self.last_member_deaths.push(entity);
    }

    pub(crate) fn push_detection(&mut self, now: Frame, detected: Entity, detector: Entity) {
        self.begin(now);
        self.detections.push((detected, detector));
    }

    pub(crate) fn deaths(&self, now: Frame) -> &[(Entity, bool)] {
        if self.frame == now {
            &self.deaths
        } else {
            &[]
        }
    }

    pub(crate) fn struck(&self, now: Frame) -> &[Struck] {
        if self.frame == now {
            &self.struck
        } else {
            &[]
        }
    }

    pub(crate) fn kills(&self, now: Frame) -> &[(Entity, Entity)] {
        if self.frame == now {
            &self.kills
        } else {
            &[]
        }
    }

    pub(crate) fn attaches(&self, now: Frame) -> &[(Entity, Option<PlayerId>)] {
        if self.frame == now {
            &self.attaches
        } else {
            &[]
        }
    }

    pub(crate) fn attach_losses(&self, now: Frame) -> &[Entity] {
        if self.frame == now {
            &self.attach_losses
        } else {
            &[]
        }
    }

    pub(crate) fn last_member_deaths(&self, now: Frame) -> &[Entity] {
        if self.frame == now {
            &self.last_member_deaths
        } else {
            &[]
        }
    }

    pub(crate) fn detections(&self, now: Frame) -> &[(Entity, Entity)] {
        if self.frame == now {
            &self.detections
        } else {
            &[]
        }
    }

    pub(crate) fn push_wall_climb(&mut self, now: Frame, climber: Entity, wall: Entity) {
        self.begin(now);
        self.wall_climbs.push((climber, wall));
    }

    pub(crate) fn wall_climbs(&self, now: Frame) -> &[(Entity, Entity)] {
        if self.frame == now {
            &self.wall_climbs
        } else {
            &[]
        }
    }
}

impl Component for CombatFacts {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        hasher.write_usize(self.struck.len());
        for struck in &self.struck {
            struck.victim.hash_into(hasher);
            struck.attacker.hash_into(hasher);
            struck.damage_type.hash_into(hasher);
            hasher.write_f32(struck.dealt);
        }
        hasher.write_usize(self.kills.len());
        for (victim, killer) in &self.kills {
            victim.hash_into(hasher);
            killer.hash_into(hasher);
        }
        hasher.write_usize(self.deaths.len());
        for (entity, faded) in &self.deaths {
            entity.hash_into(hasher);
            hasher.write_bool(*faded);
        }
        hasher.write_usize(self.attaches.len());
        for (entity, parent_owner) in &self.attaches {
            entity.hash_into(hasher);
            hasher.write_bool(parent_owner.is_some());
            if let Some(owner) = parent_owner {
                owner.hash_into(hasher);
            }
        }
        hasher.write_usize(self.attach_losses.len());
        for entity in &self.attach_losses {
            entity.hash_into(hasher);
        }
        hasher.write_usize(self.last_member_deaths.len());
        for entity in &self.last_member_deaths {
            entity.hash_into(hasher);
        }
        hasher.write_usize(self.detections.len());
        for (detected, detector) in &self.detections {
            detected.hash_into(hasher);
            detector.hash_into(hasher);
        }
        hasher.write_usize(self.wall_climbs.len());
        for (climber, wall) in &self.wall_climbs {
            climber.hash_into(hasher);
            wall.hash_into(hasher);
        }
    }
}
