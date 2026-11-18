use super::*;

const CLOSE_ORDER_DISTANCE: f32 = 100.0;

const SHORE_SEARCH_REACH: f32 = 300.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AmphibiousUnload {
    destination: Vec2,
    started: bool,
    pending: Vec<Entity>,
}

impl Component for AmphibiousUnload {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.destination.x);
        hasher.write_f32(self.destination.y);
        hasher.write_bool(self.started);
        hasher.write_u32(u32::try_from(self.pending.len()).unwrap_or(u32::MAX));
        for entity in &self.pending {
            entity.hash_into(hasher);
        }
    }
}

impl RotwkRules {
    pub(super) fn is_amphibious_ship(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        self.record_of(tick, ids, entity).is_some_and(|record| {
            record.kind_of.iter().any(|k| k == "SHIP")
                && record.kind_of.iter().any(|k| k == "AMPHIBIOUS")
        }) && self.contain_view(tick.state(), entity).is_some()
    }

    pub(super) fn order_amphibious_ship(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        ship: Entity,
        from: Vec2,
        goal: Vec2,
        mover: mint_movement::MoverClass,
    ) -> Result<()> {
        if from.minus(goal).length() < CLOSE_ORDER_DISTANCE {
            return self.start_amphibious_unload(tick, ids, ship, goal);
        }
        let Some(cell) = self.passable_cell_near(tick, ids, goal, SHORE_SEARCH_REACH, mover) else {
            diag!(
                "amphibious: entity {} found no shore near ({:.0},{:.0}) within {SHORE_SEARCH_REACH:.0}",
                ship.index(),
                goal.x,
                goal.y
            );
            return Ok(());
        };
        let point = self.passability.geometry.centre_of(cell);
        crate::unit::give_path(
            tick.state_mut(),
            (ids.body, ids.path),
            ship,
            Path {
                route: Route::to_point(point),
                goal: point,
                kind: PathKind::Amphibious,
                facing: None,
            },
        )?;
        self.apply_locomotor_override(tick, ids, ship, Some("TAXIING"))?;
        tick.state_mut().insert(
            ids.amphibious_unload,
            ship,
            AmphibiousUnload {
                destination: goal,
                started: false,
                pending: Vec::new(),
            },
        )?;
        diag!(
            "amphibious: entity {} taxiing to ({:.0},{:.0}) for ({:.0},{:.0})",
            ship.index(),
            point.x,
            point.y,
            goal.x,
            goal.y
        );
        Ok(())
    }

    fn start_amphibious_unload(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        ship: Entity,
        destination: Vec2,
    ) -> Result<()> {
        tick.state_mut().remove(ids.path, ship);
        self.apply_locomotor_override(tick, ids, ship, None)?;
        self.schedule_exit(tick, ids, ship)?;
        let pending = tick
            .state()
            .get(ids.passengers, ship)
            .map_or_else(Vec::new, |list| list.0.clone());
        tick.state_mut().insert(
            ids.amphibious_unload,
            ship,
            AmphibiousUnload {
                destination,
                started: true,
                pending,
            },
        )?;
        Ok(())
    }

    pub(super) fn step_amphibious(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let ships: Vec<Entity> = tick
            .state()
            .store(ids.amphibious_unload)
            .map_or_else(Vec::new, |store| store.entities().to_vec());
        for ship in ships {
            self.step_one_amphibious_unload(tick, ids, ship)?;
        }
        Ok(())
    }

    fn step_one_amphibious_unload(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        ship: Entity,
    ) -> Result<()> {
        let Some(unload) = tick.state().get(ids.amphibious_unload, ship).cloned() else {
            return Ok(());
        };
        if !unload.started {
            if tick.state().contains(ids.path, ship) {
                return Ok(());
            }
            return self.start_amphibious_unload(tick, ids, ship, unload.destination);
        }
        let mut remaining = Vec::with_capacity(unload.pending.len());
        for passenger in unload.pending {
            if tick.state().contains(ids.contained, passenger) {
                remaining.push(passenger);
                continue;
            }
            if tick.state().is_live(passenger) {
                crate::unit::give_path(
                    tick.state_mut(),
                    (ids.body, ids.path),
                    passenger,
                    Path {
                        route: Route::to_point(unload.destination),
                        goal: unload.destination,
                        kind: PathKind::Ordered,
                        facing: None,
                    },
                )?;
            }
        }
        if remaining.is_empty() {
            tick.state_mut().remove(ids.amphibious_unload, ship);
        } else {
            tick.state_mut().insert(
                ids.amphibious_unload,
                ship,
                AmphibiousUnload {
                    destination: unload.destination,
                    started: true,
                    pending: remaining,
                },
            )?;
        }
        Ok(())
    }
}
