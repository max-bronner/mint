use super::*;
use crate::diag::diag;
use crate::unit::Kind;

impl RotwkRules {
    pub(super) fn advance_tree_motion(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) else {
            return Ok(());
        };
        let Some(forest) = tick.state_mut().get_mut(ids.forest, forest_entity) else {
            return Ok(());
        };
        let played = forest.advance_motion(&self.tree_templates);
        for (kind, fx) in played {
            let motion = self.tree_templates.motion(kind);
            diag!(
                "forest: frame {} kind {} {fx:?}",
                tick.frame().get(),
                kind.0
            );
            let (list, at) = match fx {
                crate::forest::TreeFx::Topple { at } => {
                    (motion.topple_fx.as_deref(), crate::fx::FxPoint::from(at))
                }
                crate::forest::TreeFx::Bounce { at, height } => (
                    motion.bounce_fx.as_deref(),
                    crate::fx::FxPoint {
                        x: at.x,
                        y: at.y,
                        z: Some(self.ground_height(at.x, at.y) + height),
                    },
                ),
            };
            if let Some(list) = list {
                self.emit_fx_named(
                    tick,
                    ids,
                    list,
                    at,
                    crate::fx::FxSource::Weapon,
                    forest_entity,
                );
            }
        }
        Ok(())
    }

    pub(super) fn plant_tree(&self, tick: &mut Tick<'_>, ids: Ids, kind: Kind, pos: Vec2) -> bool {
        let Some(traits) = self
            .kinds
            .get(kind)
            .and_then(|entry| self.tree_templates.traits(kind, &entry.record))
        else {
            return false;
        };
        let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) else {
            return false;
        };
        let Some(forest) = tick.state_mut().get_mut(ids.forest, forest_entity) else {
            return false;
        };
        forest.plant(pos, kind, traits);
        diag!(
            "forest: planted kind {} at ({:.0}, {:.0}), {} box(es)",
            kind.0,
            pos.x,
            pos.y,
            traits.supply
        );
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fell_trees_for_money(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        (source, owner): (Entity, PlayerId),
        centre: Vec2,
        radius: f32,
        fx: Option<&str>,
        (multiplier, cap): (f32, f32),
    ) -> Result<()> {
        let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) else {
            return Ok(());
        };
        let felled = match tick.state_mut().get_mut(ids.forest, forest_entity) {
            Some(forest) => forest.fell_for_supply(centre, radius),
            None => return Ok(()),
        };
        let money_mult = self.multiplay.money(self.players);
        let total = devastation_money(
            felled.iter().map(|(_, boxes)| *boxes),
            self.value_per_supply_box,
            money_mult,
            multiplier,
            cap,
        );
        if let Some(list) = fx {
            for (pos, _) in &felled {
                self.emit_fx_named(tick, ids, list, *pos, crate::fx::FxSource::Power, source);
            }
        }
        if total > 0 {
            if let Some(player_entity) =
                crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
            {
                let mut purse = tick
                    .state()
                    .get(ids.purse, player_entity)
                    .map_or(mint_sage_economy::Money::new(0), |held| held.0);
                purse.deposit(total);
                tick.state_mut()
                    .insert(ids.purse, player_entity, Purse(purse))?;
            }
        }
        diag!(
            "power: {} tree(s) felled within {radius:.0} of ({:.0}, {:.0}) for {total} for player {}",
            felled.len(),
            centre.x,
            centre.y,
            owner.get()
        );
        Ok(())
    }

    pub(super) fn morph_trees(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        centre: Vec2,
        radius: f32,
        tainted: bool,
    ) {
        let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) else {
            return;
        };
        let Some(forest) = tick.state_mut().get_mut(ids.forest, forest_entity) else {
            return;
        };
        let removed = forest.morph_within(centre, radius, tainted);
        if removed > 0 {
            diag!(
                "power: {removed} toppled tree(s) sank under a morph within {radius:.0} of ({:.0}, {:.0})",
                centre.x,
                centre.y
            );
        }
    }
}

pub(super) fn devastation_money(
    boxes: impl Iterator<Item = u32>,
    per_box: f32,
    money_mult: f32,
    multiplier: f32,
    cap: f32,
) -> i64 {
    let mut total = 0.0f32;
    for count in boxes {
        #[allow(clippy::cast_precision_loss)]
        let raw = count as f32 * per_box;
        if raw <= 0.0 {
            continue;
        }
        let scaled = (raw * money_mult).trunc();
        total += scaled * multiplier;
    }
    #[allow(clippy::cast_possible_truncation)]
    let paid = total.min(cap).max(0.0) as i64;
    paid
}

#[cfg(test)]
mod tests {
    use super::devastation_money;

    #[test]
    fn devastation_pays_each_tree_truncated_then_caps_the_cast() {
        for (boxes, money_mult, multiplier, expected) in [
            (vec![90, 90], 1.0, 0.5, 900),
            (vec![90, 90, 90, 90], 1.0, 0.5, 1500),
            (vec![1, 1, 1], 0.35, 1.0, 9),
        ] {
            let paid =
                devastation_money(boxes.iter().copied(), 10.0, money_mult, multiplier, 1500.0);
            assert_eq!(paid, expected, "{boxes:?} x {money_mult} x {multiplier}");
        }
    }
}
