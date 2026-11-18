use super::*;
use mint_sage_modules::Effect;

const PASS_REACH: f32 = 50.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChargePass {
    victim: Entity,
    fired: bool,
}

impl Component for ChargePass {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.victim.hash_into(hasher);
        hasher.write_bool(self.fired);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChargeStep {
    Run,
    Fire,
    Done,
}

impl RotwkRules {
    pub(super) fn charge_pass_step(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
        (ranges, aim_tolerance): ((f32, f32), f32),
        budgets: &mut PathBudgets,
    ) -> Result<ChargeStep> {
        let held = tick
            .state()
            .get(ids.charge_pass, entity)
            .copied()
            .filter(|pass| pass.victim == victim.entity);
        let running = tick
            .state()
            .get(ids.path, entity)
            .is_some_and(|path| path.kind == PathKind::Closing);
        let Some(pass) = held else {
            tick.state_mut().insert(
                ids.charge_pass,
                entity,
                ChargePass {
                    victim: victim.entity,
                    fired: false,
                },
            )?;
            self.run_at(tick, ids, entity, me.position, victim.position, budgets)?;
            return Ok(ChargeStep::Run);
        };
        if pass.fired || !running {
            return Ok(if running {
                ChargeStep::Run
            } else {
                ChargeStep::Done
            });
        }
        if victim.position.minus(me.position).length() <= PASS_REACH {
            self.run_at(tick, ids, entity, me.position, victim.position, budgets)?;
        }
        let facing = tick.state().get(ids.body, entity).map(|body| body.0.facing);
        let faces = facing
            .zip(Facing::from_vector(victim.position.minus(me.position)))
            .is_some_and(|(facing, bearing)| facing.delta_to(bearing).abs() < aim_tolerance);
        let (min_range, max_range) = ranges;
        let in_reach = in_range(
            me.position,
            me.radius,
            victim.position,
            victim.radius,
            min_range,
            max_range,
        );
        Ok(if in_reach && faces {
            ChargeStep::Fire
        } else {
            ChargeStep::Run
        })
    }

    pub(super) fn charge_fired(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
        budgets: &mut PathBudgets,
    ) -> Result<Option<Effect>> {
        tick.state_mut().insert(
            ids.charge_pass,
            entity,
            ChargePass {
                victim: victim.entity,
                fired: true,
            },
        )?;
        let through = victim
            .position
            .minus(me.position)
            .normalised()
            .unwrap_or(Vec2::ZERO);
        let past = victim.position.plus(through.scale(PASS_REACH));
        self.run_at(tick, ids, entity, me.position, past, budgets)?;
        Ok(self.modules.as_ref().map(|_| charging(true)))
    }

    pub(super) fn end_charge_pass(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<Effect> {
        tick.state_mut()
            .remove(ids.charge_pass, entity)
            .and_then(|_| self.modules.as_ref().map(|_| charging(false)))
    }

    fn run_at(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        from: Vec2,
        to: Vec2,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        self.close_on_target(tick, ids, entity, from, to, 0.0, PathKind::Closing, budgets)
    }
}

fn charging(on: bool) -> Effect {
    let name = vec!["CHARGING".to_owned()];
    let (add, remove) = if on {
        (name, Vec::new())
    } else {
        (Vec::new(), name)
    };
    Effect::ModelConditions {
        add: mint_sage_modules::effects::names(add),
        remove: mint_sage_modules::effects::names(remove),
    }
}
