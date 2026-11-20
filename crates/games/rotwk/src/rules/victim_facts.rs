use mint_sage_combat::{
    DamageContainedNugget, GrabNugget, MetaImpactNugget, VictimFacts, WeaponId, WeaponTemplate,
};

use super::*;

#[derive(Clone, Copy)]
pub(super) struct RulesFacts<'a, 'b> {
    pub(super) rules: &'a RotwkRules,
    pub(super) tick: &'a Tick<'b>,
    pub(super) ids: Ids,
    pub(super) firer: Entity,
    pub(super) victim: Entity,
}

impl RotwkRules {
    pub(super) fn passenger_share(
        &self,
        world: &World,
        ids: Ids,
        template: &WeaponTemplate,
        attacker: Entity,
    ) -> f32 {
        if !template.passenger_proportional_attack
            || template.max_attack_passengers == 0
            || self.contain_view(world, attacker).is_none()
        {
            return 1.0;
        }
        let held = world
            .get(ids.passengers, attacker)
            .map_or(0, |list| list.0.len());
        #[allow(clippy::cast_precision_loss)]
        let held = held as f32;
        (held / f32::from(template.max_attack_passengers)).min(1.0)
    }

    pub(super) fn paralyze_accepts(&self, tick: &Tick<'_>, entity: Entity) -> bool {
        !self.carries_object_status(tick.state(), entity, "IGNORE_PARALYZE_NUGGET")
            && !self.holds_model_condition(tick, entity, "BURNINGDEATH")
    }

    pub(super) fn damage_contained_accepts(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        victim: Entity,
        nugget: &DamageContainedNugget,
    ) -> bool {
        let world = tick.state();
        let garrison = self
            .contain_view(world, victim)
            .is_some_and(|view| view.garrison_family);
        if !garrison
            || self
                .modules
                .as_ref()
                .is_some_and(|wiring| wiring.immune_to_clear_building(world, victim))
        {
            return false;
        }
        let Some(held) = world.get(ids.passengers, victim) else {
            return false;
        };
        held.0.iter().any(|occupant| {
            !world.contains(ids.doomed, *occupant)
                && world
                    .get(ids.health, *occupant)
                    .is_none_or(|health| !health.0.is_dead())
                && self.record_of(tick, ids, *occupant).is_some_and(|record| {
                    let has = |kind: &String| {
                        record
                            .kind_of
                            .iter()
                            .any(|held| held.eq_ignore_ascii_case(kind))
                    };
                    nugget.kill_kind_of.iter().all(has) && !nugget.kill_kind_of_not.iter().any(has)
                })
        })
    }
}

impl VictimFacts for RulesFacts<'_, '_> {
    fn passenger_share(&self, template: &WeaponTemplate) -> f32 {
        self.rules
            .passenger_share(self.tick.state(), self.ids, template, self.firer)
    }

    fn grab_accepts(&self, nugget: &GrabNugget) -> bool {
        self.rules.grab_accepts(
            self.tick.state(),
            self.ids,
            self.firer,
            self.victim,
            nugget.remove_target_from_other_contain,
        )
    }

    fn paralyze_accepts(&self) -> bool {
        self.rules.paralyze_accepts(self.tick, self.victim)
    }

    fn meta_impact_accepts(&self, nugget: &MetaImpactNugget) -> bool {
        self.rules
            .meta_impact_filter(self.tick, self.ids, self.victim, nugget)
    }

    fn damage_contained_accepts(&self, nugget: &DamageContainedNugget) -> bool {
        self.rules
            .damage_contained_accepts(self.tick, self.ids, self.victim, nugget)
    }

    fn slave_weapon(&self) -> Option<(WeaponId, Box<dyn VictimFacts + '_>)> {
        let slave = self
            .rules
            .watched_slave(self.tick.state(), self.ids, self.firer)?;
        let primary = self
            .rules
            .selected_set(self.tick, self.ids, slave)
            .and_then(|(_, slots)| slots.arms.first().copied().flatten())
            .map(|arm| arm.weapon)
            .or_else(|| {
                self.tick
                    .state()
                    .get(self.ids.armed, slave)
                    .map(|armed| armed.weapon)
            })?;
        Some((
            primary,
            Box::new(RulesFacts {
                firer: slave,
                ..*self
            }),
        ))
    }
}
