use mint_movement::{Facing, Vec2};
use mint_sage_economy::{ItemId, ItemTable};
use mint_sage_modules::{
    CreateLocation, Effect, EffectLog, ModifierRule, OclFrom, PowerPlace, ReplaceCarry,
};
use mint_sim::{
    Activation, Effects, Entity, Frame, LogicFrames, PlayerId, Result, Rules as _, Tick, World,
};

use super::{Ids, ModuleWiring, RotwkCommand, RotwkRules, SuspendedOrder};
use crate::defeat::ObjectFilter;
use crate::diag::diag;
use crate::economy::PlayerUpgrades;
use crate::powers::{is_ready, start_recharge, ShroudNeed};
use crate::sciences::{PlayerScience, ScienceId, SpellBook, SpellEntry, SpellPlan};
use crate::templates::TargetKind;
use crate::unit::{Kind, Owner};
use mint_sage_shroud::CellVisibility;

const MAX_CREATE_COUNT: i64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PowerButton {
    Selection(u8),
    Spell(u8),
}

const SPREAD_PROBE_RADIUS: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SpellBarReadiness {
    Disabled,
    OnCooldown { percent: u8 },
    Ready,
    Static,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CreationVariant {
    Object,
    At,
}

impl RotwkRules {
    pub(crate) fn with_spell_books(
        mut self,
        sciences: std::sync::Arc<crate::sciences::ScienceTable>,
        ranks: std::sync::Arc<crate::sciences::Ranks>,
        plans: Vec<SpellPlan>,
        power_sciences: Vec<Vec<ScienceId>>,
    ) -> Self {
        self.sciences = sciences;
        self.ranks = ranks;
        self.spell_plans = plans;
        self.power_sciences = power_sciences;
        self
    }

    pub(crate) fn with_creation_lists(
        mut self,
        ocls: std::sync::Arc<mint_sage_ini::ObjectCreationListIndex>,
    ) -> Self {
        self.ocls = ocls;
        self
    }

    pub(crate) fn ranks_of(&self, player: PlayerId) -> &crate::sciences::Ranks {
        self.spell_plans
            .get(usize::from(player.get()))
            .and_then(|plan| plan.ranks.as_deref())
            .unwrap_or(&self.ranks)
    }

    pub(super) fn setup_spell_books(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player_entities: &[Entity],
    ) -> Result<()> {
        for (slot, plan) in self.spell_plans.iter().enumerate() {
            let Ok(slot_id) = u8::try_from(slot) else {
                break;
            };
            let player = PlayerId::new(slot_id);
            let Some(&entity) = player_entities.get(slot) else {
                if let Some(kind) = plan.spell_book {
                    let book = tick.state_mut().spawn()?;
                    tick.state_mut().insert(ids.owner, book, Owner(player))?;
                    tick.state_mut().insert(ids.kind, book, kind)?;
                    self.attach_upgrade_state(tick, ids, book, Some(kind))?;
                }
                continue;
            };
            let mut state = PlayerScience::start(
                &plan.intrinsic,
                self.ranks_of(player),
                plan.rank_cap,
                plan.intrinsic_purchase_points,
            );
            if let Some(kind) = plan.spell_book {
                let book = tick.state_mut().spawn()?;
                tick.state_mut().insert(ids.owner, book, Owner(player))?;
                tick.state_mut().insert(ids.kind, book, kind)?;
                self.attach_upgrade_state(tick, ids, book, Some(kind))?;
                state.book = Some(book);
            }
            let starting_sciences = state.sciences.clone();
            tick.state_mut().insert(ids.player_science, entity, state)?;
            self.sciences_acquired(tick, ids, player, &starting_sciences)?;
        }
        Ok(())
    }

    pub(crate) fn holds_science(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        name: &str,
    ) -> bool {
        let Some(id) = self.sciences.id(name) else {
            return false;
        };
        self.science_of(world, ids, player)
            .is_some_and(|state| state.sciences.contains(&id))
    }

    pub(crate) fn reached_level_cap(&self, world: &World, ids: Ids, player: PlayerId) -> bool {
        self.science_of(world, ids, player)
            .is_some_and(PlayerScience::at_rank_cap)
    }

    pub(crate) fn science_of<'w>(
        &self,
        world: &'w World,
        ids: Ids,
        player: PlayerId,
    ) -> Option<&'w PlayerScience> {
        let entity = crate::economy::player_entity(world, Self::economy_ids(ids), player)?;
        world.get(ids.player_science, entity)
    }

    pub(super) fn script_set_rank_level(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        level: i32,
    ) -> Result<()> {
        let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        else {
            return Ok(());
        };
        let Some(mut state) = tick.state().get(ids.player_science, entity).cloned() else {
            return Ok(());
        };
        let extra = self
            .spell_plans
            .get(usize::from(player.get()))
            .map_or(0, |plan| plan.intrinsic_purchase_points);
        let level = u16::try_from(level.max(1)).unwrap_or(u16::MAX);
        let granted = state.set_rank_level(level, self.ranks_of(player), extra);
        diag!(
            "rank: script set player {} to rank {} - {} power point(s) to spend",
            player.get(),
            state.rank,
            state.purchase_points
        );
        tick.state_mut().insert(ids.player_science, entity, state)?;
        self.sciences_acquired(tick, ids, player, &granted)
    }

    pub(super) fn award_skill_points(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        award: u32,
    ) -> Result<()> {
        if award == 0 {
            return Ok(());
        }
        let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        else {
            return Ok(());
        };
        let Some(mut state) = tick.state().get(ids.player_science, entity).cloned() else {
            return Ok(());
        };
        let rank = state.rank;
        let award = self.wotr_scaled_award(tick.state(), player, rank, award);
        let power_before = state.purchase_points;
        let granted = state.add_skill_points(award, self.ranks_of(player));
        #[allow(clippy::cast_precision_loss)]
        self.note_points(
            tick,
            ids,
            player,
            award as f32,
            state.purchase_points.saturating_sub(power_before),
        );
        if state.rank != rank {
            diag!(
                "rank: player {} reached rank {} - {} power point(s) to spend",
                player.get(),
                state.rank,
                state.purchase_points
            );
        }
        tick.state_mut().insert(ids.player_science, entity, state)?;
        self.sciences_acquired(tick, ids, player, &granted)
    }

    pub(crate) fn purchase_points_of(&self, world: &World, player: PlayerId) -> u32 {
        let Some(ids) = self.ids else {
            return 0;
        };
        self.science_of(world, ids, player)
            .map_or(0, |state| state.purchase_points)
    }

    pub(super) fn award_spell_points(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        points: u32,
    ) -> Result<()> {
        if points == 0 {
            return Ok(());
        }
        let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        else {
            return Ok(());
        };
        let Some(mut state) = tick.state().get(ids.player_science, entity).cloned() else {
            return Ok(());
        };
        state.purchase_points = state.purchase_points.saturating_add(points);
        self.note_points(tick, ids, player, 0.0, points);
        diag!(
            "spell book: player {} +{points} power - {} power point(s) to spend",
            player.get(),
            state.purchase_points
        );
        tick.state_mut().insert(ids.player_science, entity, state)?;
        Ok(())
    }

    pub(super) fn purchase_science(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        science: u16,
    ) -> Result<()> {
        let id = ScienceId::new(science);
        let name = self
            .sciences
            .get(id)
            .map_or_else(|| format!("science {science}"), |s| s.name.clone());
        let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        else {
            return Ok(());
        };
        let Some(mut state) = tick.state().get(ids.player_science, entity).cloned() else {
            diag!(
                "spell book: player {} has none - {name} refused",
                player.get()
            );
            return Ok(());
        };
        let cost = match state.check_purchase(id, &self.sciences) {
            Ok(cost) => cost,
            Err(why) => {
                diag!(
                    "spell book: player {} cannot buy {name} - {}",
                    player.get(),
                    why.describe()
                );
                return Ok(());
            }
        };
        state.purchase(id, cost);
        diag!(
            "spell book: player {} bought {name} for {cost} - {} power point(s) left",
            player.get(),
            state.purchase_points
        );
        tick.state_mut().insert(ids.player_science, entity, state)?;
        self.sciences_acquired(tick, ids, player, &[id])
    }

    pub(super) fn sciences_acquired(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        sciences: &[ScienceId],
    ) -> Result<()> {
        if sciences.is_empty() {
            return Ok(());
        }
        let Some(book) = self
            .science_of(tick.state(), ids, player)
            .and_then(|s| s.book)
        else {
            return Ok(());
        };
        let Some(entry) = tick
            .state()
            .get(ids.kind, book)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
        else {
            return Ok(());
        };
        let passives: Vec<String> = entry
            .spell_buttons
            .iter()
            .filter(|button| button.passive)
            .filter(|button| {
                self.power_names.id(&button.power).is_some_and(|power| {
                    self.power_sciences
                        .get(usize::from(power.get()))
                        .is_some_and(|needs| needs.iter().any(|need| sciences.contains(need)))
                })
            })
            .map(|button| button.power.clone())
            .collect();
        for power in passives {
            diag!(
                "spell book: player {} - passive {power} takes effect",
                player.get()
            );
            self.activate_power(tick, ids, book, &power, None, None, false)?;
        }
        let world = tick.state();
        let books: Vec<(Entity, crate::powers::PowerBook)> = world
            .store(ids.power_book)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, _)| {
                        world
                            .get(ids.owner, *entity)
                            .is_some_and(|owner| owner.0 == player)
                    })
                    .map(|(entity, book)| (entity, book.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let now = tick.frame();
        for (entity, mut book) in books {
            let mut changed = false;
            for instance in &mut book.0 {
                let opens = self
                    .power_sciences
                    .get(usize::from(instance.power.get()))
                    .is_some_and(|needs| needs.iter().any(|need| sciences.contains(need)));
                if opens {
                    instance.available_on = now;
                    instance.reload = LogicFrames::ZERO;
                    changed = true;
                }
            }
            if changed {
                tick.state_mut().insert(ids.power_book, entity, book)?;
            }
        }
        Ok(())
    }

    pub(super) fn power_science_met(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        power: &str,
    ) -> bool {
        let Some(needs) = self
            .power_names
            .id(power)
            .and_then(|id| self.power_sciences.get(usize::from(id.get())))
            .filter(|needs| !needs.is_empty())
        else {
            return true;
        };
        self.science_of(world, ids, player)
            .is_some_and(|state| needs.iter().any(|need| state.holds(*need)))
    }

    pub(super) fn power_requirement_met(
        &self,
        world: &World,
        ids: Ids,
        kind: Kind,
        player: PlayerId,
        power: &str,
    ) -> bool {
        let Some(entry) = self.kinds.get(kind) else {
            return true;
        };
        let Some(tokens) = entry
            .record
            .declared_modules
            .iter()
            .filter(|module| {
                module
                    .field("SpecialPowerTemplate")
                    .and_then(|f| f.args.first())
                    .is_some_and(|name| name.eq_ignore_ascii_case(power))
            })
            .find_map(|module| module.field("RequirementsFilterMPSkirmish"))
            .map(|field| field.args.clone())
        else {
            return true;
        };
        let filter = crate::defeat::ObjectFilter::parse(&tokens);
        if !filter.passes_relation(true, true, false) {
            return false;
        }
        let Some(owners) = world.store(ids.owner) else {
            return false;
        };
        owners.iter().any(|(entity, owner)| {
            owner.0 == player
                && world.is_live(entity)
                && !world.contains(ids.doomed, entity)
                && world.get(ids.kind, entity).is_some_and(|kind| {
                    self.kinds
                        .get(*kind)
                        .is_some_and(|e| filter.passes(&e.record.kind_of, &e.record.name))
                })
        })
    }

    pub(super) fn cast_spell(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        button: u8,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
    ) -> Result<()> {
        let Some(book) = self
            .science_of(tick.state(), ids, player)
            .and_then(|s| s.book)
        else {
            diag!("spell: player {} has no spell book - refused", player.get());
            return Ok(());
        };
        let Some(spell) = tick
            .state()
            .get(ids.kind, book)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .and_then(|entry| entry.spell_buttons.get(usize::from(button)))
            .cloned()
        else {
            diag!(
                "spell: player {}'s spell book has no button {} - refused",
                player.get(),
                u16::from(button).saturating_add(1)
            );
            return Ok(());
        };
        if spell.passive {
            diag!(
                "spell: {} is passive (NONPRESSABLE) - it is never cast",
                spell.power
            );
            return Ok(());
        }
        self.fire_power_from(
            tick,
            ids,
            player,
            book,
            &spell.power,
            spell.target,
            &ObjectFilter::default(),
            target,
            at,
            false,
        )
        .map(|_| ())
    }

    fn power_cast_refusal(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        player: PlayerId,
        power_name: &str,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
    ) -> Option<&'static str> {
        let rule = self
            .power_names
            .id(power_name)
            .and_then(|id| self.power_cast_rules.get(usize::from(id.get())))?;
        let source_pos = world.get(ids.body, source).map(|body| body.0.position);
        let cast_point = match target {
            Some(entity) => world.get(ids.body, entity).map(|body| body.0.position),
            None => at.map(|(x, y)| Vec2::new(x, y)).or(source_pos),
        };
        if let (Some(max_range), Some(src), Some(dst)) =
            (rule.max_cast_range, source_pos, cast_point)
        {
            if dst.minus(src).length_squared() > max_range * max_range {
                return Some("target is beyond MaxCastRange");
            }
        }
        if let (Some(filter), Some(target)) = (&rule.object_filter, target) {
            let other = world.get(ids.owner, target).map(|owner| owner.0);
            let (same, allied, hostile) = super::spell_effects::standing(self, player, other);
            let kind_ok = world.get(ids.kind, target).is_some_and(|kind| {
                self.kinds
                    .get(*kind)
                    .is_some_and(|entry| filter.passes(&entry.record.kind_of, &entry.record.name))
            });
            if !kind_ok || !filter.passes_relation(same, allied, hostile) {
                return Some("target fails ObjectFilter");
            }
        }
        if let (Some((filter, range)), Some(point)) = (&rule.forbidden, cast_point) {
            let blocked = world.store(ids.kind).is_some_and(|store| {
                store.entities().iter().any(|&entity| {
                    world.is_live(entity)
                        && !world.contains(ids.doomed, entity)
                        && world.get(ids.body, entity).is_some_and(|body| {
                            body.0.position.minus(point).length_squared() <= range * range
                        })
                        && world.get(ids.kind, entity).is_some_and(|kind| {
                            self.kinds.get(*kind).is_some_and(|entry| {
                                filter.passes(&entry.record.kind_of, &entry.record.name)
                            })
                        })
                })
            });
            if blocked {
                return Some("a forbidden object stands within ForbiddenObjectRange");
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn power_target_gate(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        source: Entity,
        power_name: &str,
        target_kind: TargetKind,
        relation: &ObjectFilter,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
    ) -> std::result::Result<Option<Entity>, &'static str> {
        let target_ok = match target_kind {
            TargetKind::None | TargetKind::Confirm => target.is_none() && at.is_none(),
            TargetKind::Position => at.is_some() && target.is_none(),
            TargetKind::Object => {
                (target.is_some() && at.is_none())
                    || (target.is_none()
                        && at.is_some_and(|at| {
                            self.grabs_map_tree(world, ids, source, power_name, at)
                        }))
            }
        };
        if !target_ok {
            return Err("wrong target kind");
        }
        if let (TargetKind::Object, Some(target_entity)) = (target_kind, target) {
            let other = world.get(ids.owner, target_entity).map(|o| o.0);
            let (same, allied, hostile) = super::spell_effects::standing(self, player, other);
            if !relation.passes_relation(same, allied, hostile) {
                return Err("target fails the button's own relation");
            }
        }
        let target = match target {
            Some(aimed) if self.power_captures(power_name) => {
                Some(self.capture_target(world, player, aimed)?)
            }
            other => other,
        };
        match self.power_cast_refusal(world, ids, source, player, power_name, target, at) {
            Some(refusal) => Err(refusal),
            None => Ok(target),
        }
    }

    pub(crate) fn power_target_refusal(
        &self,
        world: &World,
        player: PlayerId,
        button: PowerButton,
        relation: &ObjectFilter,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
    ) -> Option<&'static str> {
        let ids = self.ids?;
        let casters: Vec<(Entity, String, TargetKind)> = match button {
            PowerButton::Selection(button) => self
                .command_subjects_in(world, ids, player)
                .into_iter()
                .filter(|&subject| {
                    world
                        .get(ids.owner, subject)
                        .is_some_and(|owner| owner.0 == player)
                })
                .filter_map(|subject| {
                    let entry = world
                        .get(ids.kind, subject)
                        .and_then(|k| self.kinds.get(*k))?;
                    let set = self.command_set_override(world, subject);
                    let (power, kind) = entry
                        .power_buttons_under(set)
                        .get(usize::from(button))?
                        .clone();
                    Some((subject, power, kind))
                })
                .collect(),
            PowerButton::Spell(button) => {
                let book = self.science_of(world, ids, player)?.book?;
                let spell = world
                    .get(ids.kind, book)
                    .and_then(|kind| self.kinds.get(*kind))
                    .and_then(|entry| entry.spell_buttons.get(usize::from(button)))?;
                vec![(book, spell.power.clone(), spell.target)]
            }
        };
        let mut refusal = Some("no selected object offers the button");
        for (source, power, kind) in casters {
            let verdict = self
                .power_target_gate(
                    world, ids, player, source, &power, kind, relation, target, at,
                )
                .err()
                .or_else(|| {
                    at.and_then(|(x, y)| {
                        self.location_cast_refusal(world, ids, player, &power, Vec2::new(x, y))
                    })
                });
            match verdict {
                None => return None,
                Some(reason) => refusal = Some(reason),
            }
        }
        refusal
    }

    fn location_cast_refusal(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        power_name: &str,
        at: Vec2,
    ) -> Option<&'static str> {
        let rule = self
            .power_names
            .id(power_name)
            .and_then(|id| self.power_cast_rules.get(usize::from(id.get())))?;
        if rule.shroud != ShroudNeed::Any {
            let state = crate::shroud::with_grid(world, Self::shroud_ids(ids), |grid| {
                let (x, y) = grid.world_to_cell(at.x, at.y);
                grid.cell_state(player, x, y)
            });
            let refused = match (rule.shroud, state) {
                (ShroudNeed::Visible, Some(state)) => state != CellVisibility::Visible,
                (ShroudNeed::NotShrouded, Some(state)) => state == CellVisibility::Unexplored,
                _ => false,
            };
            if refused {
                return Some("the position is under the shroud");
            }
        }
        let radius = rule.radius_cursor.filter(|r| *r > 0.0).unwrap_or(0.0);
        if !rule.water_ok && self.water_under(at, radius) {
            return Some("water under the cast");
        }
        if rule.needs_object_filter {
            let filter = rule.object_filter.as_ref()?;
            let admitted = world.store(ids.kind).is_some_and(|store| {
                store.entities().iter().any(|&entity| {
                    world.is_live(entity)
                        && !world.contains(ids.doomed, entity)
                        && world.get(ids.body, entity).is_some_and(|body| {
                            body.0.position.minus(at).length_squared() <= radius * radius
                        })
                        && world.get(ids.kind, entity).is_some_and(|kind| {
                            self.kinds.get(*kind).is_some_and(|entry| {
                                filter.passes(&entry.record.kind_of, &entry.record.name)
                            })
                        })
                        && {
                            let other = world.get(ids.owner, entity).map(|o| o.0);
                            let (same, allied, hostile) =
                                super::spell_effects::standing(self, player, other);
                            filter.passes_relation(same, allied, hostile)
                        }
                })
            });
            if !admitted {
                return Some("no object the ObjectFilter admits under the cast");
            }
        }
        None
    }

    fn water_under(&self, at: Vec2, radius: f32) -> bool {
        use mint_movement::Passability as _;
        use mint_sage_passability::{DEEP_WATER, WATER};
        let wet = |cell| {
            self.passability
                .grid
                .flags(cell)
                .intersects(WATER.union(DEEP_WATER))
        };
        if radius <= 0.0 {
            return self.passability.cell_at(at).is_some_and(wet);
        }
        let Some(rect) = self.passability.covering_rect(at, radius) else {
            return false;
        };
        let geometry = self.passability.geometry;
        let half = geometry.cell_size() * 0.5;
        (rect.y0..=rect.y1).any(|y| {
            (rect.x0..=rect.x1).any(|x| {
                let cell = mint_movement::Cell { layer: 0, y, x };
                let centre = geometry.centre_of(cell);
                let dx = ((at.x - centre.x).abs() - half).max(0.0);
                let dy = ((at.y - centre.y).abs() - half).max(0.0);
                dx * dx + dy * dy < radius * radius && wet(cell)
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fire_power_from(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        source: Entity,
        power_name: &str,
        target_kind: TargetKind,
        relation: &ObjectFilter,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
        ai_sourced: bool,
    ) -> Result<bool> {
        let now = tick.frame();
        let target = match self.power_target_gate(
            tick.state(),
            ids,
            player,
            source,
            power_name,
            target_kind,
            relation,
            target,
            at,
        ) {
            Ok(target) => target,
            Err(refusal) => {
                diag!(
                    "power: {power_name} at entity {} refused - {refusal}",
                    source.index()
                );
                return Ok(false);
            }
        };
        if !self.power_science_met(tick.state(), ids, player, power_name) {
            diag!(
                "power: {power_name} refused - player {} holds none of its RequiredSciences",
                player.get()
            );
            return Ok(false);
        }
        let Some(kind) = tick.state().get(ids.kind, source).copied() else {
            return Ok(false);
        };
        if !self.power_requirement_met(tick.state(), ids, kind, player, power_name) {
            diag!(
                "power: {power_name} refused - player {} owns nothing its \
                 RequirementsFilterMPSkirmish passes",
                player.get()
            );
            return Ok(false);
        }
        let slot = self
            .kinds
            .get(kind)
            .and_then(|entry| entry.record.power(power_name));
        let paused = slot.is_some_and(|slot| slot.starts_paused)
            && !self.power_released(tick.state(), source, power_name);
        let Some(power_id) = self.power_names.id(power_name) else {
            diag!("power: {power_name} has no ReloadTime table entry - refused");
            return Ok(false);
        };
        let (_, shared) = self
            .power_reload
            .get(usize::from(power_id.get()))
            .copied()
            .unwrap_or((None, false));
        let player_entity =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player);
        let available_on = if shared {
            player_entity
                .and_then(|pe| tick.state().get(ids.player_power_cooldowns, pe).cloned())
                .map_or(Frame::ZERO, |cooldowns| cooldowns.available_on(power_id))
        } else {
            tick.state()
                .get(ids.power_book, source)
                .and_then(|book| book.instance(power_id))
                .map_or(Frame::ZERO, |instance| instance.available_on)
        };
        if !is_ready(paused, available_on, now) {
            diag!(
                "power: {power_name} at entity {} not ready{}",
                source.index(),
                if paused { " (paused)" } else { "" }
            );
            return Ok(false);
        }
        self.release_power(tick, ids, source, power_name, target, at, ai_sourced)?;
        super::map_scripts::note_special_power_triggered(
            self, tick, ids, player, power_name, source,
        );
        Ok(true)
    }

    pub(super) fn fire_power_scripted(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power_name: &str,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
    ) -> Result<()> {
        self.release_power(tick, ids, source, power_name, target, at, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn release_power(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power_name: &str,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
        ai_sourced: bool,
    ) -> Result<()> {
        let slot = tick
            .state()
            .get(ids.kind, source)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .and_then(|entry| entry.record.power(power_name));
        let defers_recharge = slot.is_some_and(|slot| slot.defers_recharge);
        if let Some(sound) = slot.and_then(|slot| slot.initiate_sound.as_deref()) {
            self.emit_power_voice(tick, ids, source, std::sync::Arc::from(sound));
        }
        if defers_recharge {
            diag!(
                "power: {power_name} at entity {} fired - cooldown deferred to preparation",
                source.index()
            );
        } else {
            self.begin_recharge(tick, ids, source, power_name)?;
            self.trigger_special_power(tick, ids, source, power_name, at)?;
        }
        self.activate_power(tick, ids, source, power_name, target, at, ai_sourced)
    }

    pub(super) fn power_defers_recharge(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power_name: &str,
    ) -> bool {
        let Some(kind) = tick.state().get(ids.kind, source).copied() else {
            return false;
        };
        self.kinds
            .get(kind)
            .and_then(|entry| entry.record.power(power_name))
            .is_some_and(|slot| slot.defers_recharge)
    }

    pub(super) fn begin_recharge(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power_name: &str,
    ) -> Result<()> {
        let now = tick.frame();
        let Some(power_id) = self.power_names.id(power_name) else {
            return Ok(());
        };
        let (reload, shared) = self
            .power_reload
            .get(usize::from(power_id.get()))
            .copied()
            .unwrap_or((None, false));
        let player = tick.state().get(ids.owner, source).map(|owner| owner.0);
        let player_entity = player.and_then(|player| {
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        });
        let reload = crate::experience::recharge_time(
            reload.unwrap_or(LogicFrames::ZERO),
            self.bonuses_of(tick.state(), ids, source).recharge,
        );
        let discounted = self
            .power_cast_rules
            .get(usize::from(power_id.get()))
            .is_some_and(|rule| rule.recharge_discount);
        let modifier = player_entity
            .filter(|_| discounted)
            .and_then(|pe| tick.state().get(ids.spell_recharge, pe))
            .map(|spell| spell.modifier)
            .filter(|modifier| *modifier != 0.0);
        let reload = match modifier {
            Some(modifier) => LogicFrames::from_frames(
                crate::experience::scaled_frames(reload.get(), 1.0 + modifier).max(1),
            ),
            None => reload,
        };
        let new_available = start_recharge(now, reload);
        if shared {
            if let Some(pe) = player_entity {
                let mut cooldowns = tick
                    .state()
                    .get(ids.player_power_cooldowns, pe)
                    .cloned()
                    .unwrap_or_default();
                cooldowns.set_available_on(power_id, new_available, reload);
                tick.state_mut()
                    .insert(ids.player_power_cooldowns, pe, cooldowns)?;
            }
        } else if let Some(mut book) = tick.state().get(ids.power_book, source).cloned() {
            book.set_available_on(power_id, new_available, reload);
            tick.state_mut().insert(ids.power_book, source, book)?;
        }
        diag!(
            "power: {power_name} fired at entity {} - ready again at frame {}",
            source.index(),
            new_available.get()
        );
        Ok(())
    }

    pub(crate) fn button_readiness_power<'a>(
        &'a self,
        world: &World,
        ids: Ids,
        source: Entity,
        power: &'a str,
    ) -> &'a str {
        world
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.power(power))
            .and_then(|slot| slot.chained_button.as_deref())
            .and_then(|button| self.ai_power_buttons.get(button))
            .and_then(|button| button.special_power.as_deref())
            .unwrap_or(power)
    }

    pub(crate) fn power_ready_report(
        &self,
        world: &World,
        ids: Ids,
        now: Frame,
        source: Entity,
        power_name: &str,
    ) -> String {
        match self.power_availability(world, ids, source, power_name) {
            Err(why) => why,
            Ok((paused, available_on)) => format!(
                "{power_name}: entity {} ready {} (paused {paused}, available_on {}, now {})",
                source.index(),
                is_ready(paused, available_on, now),
                available_on.get(),
                now.get()
            ),
        }
    }

    pub(super) fn power_is_ready(
        &self,
        world: &World,
        ids: Ids,
        now: Frame,
        source: Entity,
        power_name: &str,
    ) -> bool {
        self.power_availability(world, ids, source, power_name)
            .is_ok_and(|(paused, available_on)| is_ready(paused, available_on, now))
    }

    pub(super) fn power_availability(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        power_name: &str,
    ) -> std::result::Result<(bool, Frame), String> {
        let Some(power_id) = self.power_names.id(power_name) else {
            return Err(format!("{power_name}: no ReloadTime table entry"));
        };
        let Some(kind) = world.get(ids.kind, source).copied() else {
            return Err(format!(
                "{power_name}: entity {} has no Kind",
                source.index()
            ));
        };
        let slot = self
            .kinds
            .get(kind)
            .and_then(|entry| entry.record.power(power_name));
        let paused = slot.is_some_and(|slot| slot.starts_paused)
            && !self.power_released(world, source, power_name);
        let (_, shared) = self
            .power_reload
            .get(usize::from(power_id.get()))
            .copied()
            .unwrap_or((None, false));
        let player = world.get(ids.owner, source).map(|owner| owner.0);
        let player_entity = player.and_then(|player| {
            crate::economy::player_entity(world, Self::economy_ids(ids), player)
        });
        let available_on = if shared {
            player_entity
                .and_then(|pe| world.get(ids.player_power_cooldowns, pe).cloned())
                .map_or(Frame::ZERO, |cooldowns| cooldowns.available_on(power_id))
        } else {
            world
                .get(ids.power_book, source)
                .and_then(|book| book.instance(power_id))
                .map_or(Frame::ZERO, |instance| instance.available_on)
        };
        Ok((paused, available_on))
    }

    pub(super) fn trigger_special_power(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power_name: &str,
        at: Option<(f32, f32)>,
    ) -> Result<()> {
        let rule = self
            .power_names
            .id(power_name)
            .and_then(|id| self.power_cast_rules.get(usize::from(id.get())));
        if let Some(name) = rule.and_then(|rule| rule.initiate_sound.as_deref()) {
            self.emit_object_sound(tick, ids, source, std::sync::Arc::from(name));
        }
        if at.is_some() {
            if let Some(name) = rule.and_then(|rule| rule.initiate_at_location_sound.as_deref()) {
                self.emit_object_sound(tick, ids, source, std::sync::Arc::from(name));
            }
        }
        let Some(kind) = tick.state().get(ids.kind, source).copied() else {
            return Ok(());
        };
        let recharges = self
            .kinds
            .get(kind)
            .and_then(|entry| entry.record.power(power_name)?.on_trigger_recharge.clone());
        if let Some(other) = recharges {
            self.begin_recharge(tick, ids, source, &other)?;
        }
        if self.power_defers_recharge(tick, ids, source, power_name) {
            let (trigger_fx, modifier) = self
                .kinds
                .get(kind)
                .and_then(|entry| entry.record.power(power_name))
                .map_or((None, None), |slot| {
                    (slot.trigger_fx.clone(), slot.modifier.clone())
                });
            let Some(centre) = tick
                .state()
                .get(ids.body, source)
                .map(|body| body.0.position)
            else {
                return Ok(());
            };
            if let Some(fx) = trigger_fx {
                self.emit_fx_object_named(
                    tick,
                    ids,
                    &fx,
                    centre,
                    crate::fx::FxSource::Power,
                    source,
                    None,
                );
            }
            if let Some(rule) = &modifier {
                if let Some(owner) = tick.state().get(ids.owner, source).map(|o| o.0) {
                    let given = self.area_modifier(tick, ids, source, owner, centre, rule)?;
                    diag!(
                        "power: {} given to {given} object(s) within {:.0} of entity {}'s \
                         preparation start",
                        rule.list.as_deref().unwrap_or("(no list)"),
                        rule.radius,
                        source.index()
                    );
                }
            }
        }
        let Some((flag, lasts, disable)) = self.kinds.get(kind).and_then(|entry| {
            let slot = entry.record.power(power_name)?;
            let (flag, lasts) = slot.set_model_condition.clone()?;
            Some((flag, lasts, slot.disable_during_anim))
        }) else {
            return Ok(());
        };
        self.apply_effects(
            tick,
            ids,
            vec![(
                source,
                Effect::TimedModelCondition {
                    flag: mint_sage_modules::effects::Name::from(flag),
                    lasts,
                    only_if_absent: false,
                },
            )],
        )?;
        if disable {
            let until = tick.frame().checked_add(lasts).unwrap_or(Frame::FOREVER);
            self.hold_until(tick, ids, source, until)?;
        }
        Ok(())
    }

    pub(super) fn reset_power_ready(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power_name: &str,
        now: Frame,
    ) -> Result<()> {
        let Some(power_id) = self.power_names.id(power_name) else {
            return Ok(());
        };
        let (reload, shared) = self
            .power_reload
            .get(usize::from(power_id.get()))
            .copied()
            .unwrap_or((None, false));
        let reload = reload.unwrap_or(LogicFrames::ZERO);
        let player = tick.state().get(ids.owner, source).map(|owner| owner.0);
        let player_entity = player.and_then(|player| {
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        });
        if shared {
            if let Some(pe) = player_entity {
                let mut cooldowns = tick
                    .state()
                    .get(ids.player_power_cooldowns, pe)
                    .cloned()
                    .unwrap_or_default();
                cooldowns.set_available_on(power_id, now, reload);
                tick.state_mut()
                    .insert(ids.player_power_cooldowns, pe, cooldowns)?;
            }
        } else if let Some(mut book) = tick.state().get(ids.power_book, source).cloned() {
            book.set_available_on(power_id, now, reload);
            tick.state_mut().insert(ids.power_book, source, book)?;
        }
        Ok(())
    }

    pub(super) fn disengage_for_ability(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        ai_sourced: bool,
    ) -> Result<()> {
        if ai_sourced && !tick.state().contains(ids.suspended_order, entity) {
            let suspended = SuspendedOrder {
                attack_order: tick.state().get(ids.attack_order, entity).copied(),
                player_order: tick.state().contains(ids.player_order, entity),
            };
            if suspended.attack_order.is_some() || suspended.player_order {
                tick.state_mut()
                    .insert(ids.suspended_order, entity, suspended)?;
            }
        }
        tick.state_mut().remove(ids.player_order, entity);
        tick.state_mut().remove(ids.attack_order, entity);
        if let Some(mut armed) = tick.state().get(ids.armed, entity).copied() {
            if armed.target.is_some() || armed.winding {
                armed.retarget(None);
                armed.winding = false;
                tick.state_mut().insert(ids.armed, entity, armed)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn activate_one(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        entity: Entity,
        activation: &Activation<'_>,
        now: Frame,
        power: &str,
        ai_sourced: bool,
        log: &mut EffectLog,
    ) -> Result<bool> {
        let answered = wiring.activate(
            tick.state_mut(),
            entity,
            activation,
            now,
            Effects::into_sink(log),
        );
        if answered {
            self.disengage_for_ability(tick, ids, entity, ai_sourced)?;
            let mut destroy = mint_sim::module::DestroyRequests::default();
            let (world, rng) = tick.state_and_rng_mut();
            wiring.end_other_activations(
                world,
                entity,
                power,
                now,
                rng,
                &mut destroy,
                Effects::into_sink(log),
            );
        }
        Ok(answered)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn activate_power(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power: &str,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
        ai_sourced: bool,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        if self.siege_power(tick, ids, source, power, target)? {
            return Ok(());
        }
        if let (Some(max), Some((x, y))) = (
            self.record_of(tick, ids, source)
                .and_then(|record| record.teleport_max_distance),
            at,
        ) {
            if let Some(body) = tick.state().get(ids.body, source) {
                let source_z = self
                    .passability
                    .heights
                    .height_at(body.0.position.x, body.0.position.y)
                    .unwrap_or(0.0);
                let target_z = self.passability.heights.height_at(x, y).unwrap_or(0.0);
                let dx = body.0.position.x - x;
                let dy = body.0.position.y - y;
                let dz = source_z - target_z;
                let distance_squared = dx * dx + dy * dy + dz * dz;
                if distance_squared > max * max {
                    diag!(
                        "power: {power} refused - ({x:.0},{y:.0}) is past MaxDistance {max:.0} \
                         of entity {}",
                        source.index()
                    );
                    return Ok(());
                }
            }
        }
        let mut log = EffectLog::new();
        let activation = Activation { power, target, at };
        let now = tick.frame();
        let members = tick
            .state()
            .get(ids.horde, source)
            .map(|horde| horde.members.clone())
            .unwrap_or_default();
        let at_object = target.is_some();
        let mut answered = false;
        if !at_object {
            answered = self.activate_one(
                tick,
                ids,
                wiring,
                source,
                &activation,
                now,
                power,
                ai_sourced,
                &mut log,
            )?;
        }
        let mut reached = 0usize;
        for member in members {
            if tick.state().is_live(member)
                && self.activate_one(
                    tick,
                    ids,
                    wiring,
                    member,
                    &activation,
                    now,
                    power,
                    ai_sourced,
                    &mut log,
                )?
            {
                reached = reached.saturating_add(1);
            }
        }
        if at_object {
            if let Some(target) = target {
                diag!(
                    "power: {power} dispatch on horde entity {} aims its own call at entity {}",
                    source.index(),
                    target.index()
                );
            }
            answered = self.activate_one(
                tick,
                ids,
                wiring,
                source,
                &activation,
                now,
                power,
                ai_sourced,
                &mut log,
            )?;
        }
        if reached > 0 {
            diag!(
                "power: {power} handed to {reached} member(s) of horde entity {}",
                source.index()
            );
            answered = true;
        }
        if !answered {
            diag!("power: {power} - no registered module carries it out; only its cooldown runs");
            return Ok(());
        }
        let initiate_fx = tick
            .state()
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.power(power))
            .and_then(|slot| slot.initiate_fx.clone());
        if let Some(list) = initiate_fx {
            if let Some(at) = tick
                .state()
                .get(ids.body, source)
                .map(|body| body.0.position)
            {
                if let Some(id) = self.fx_names.id(&list) {
                    self.emit_fx_with_play(
                        tick,
                        ids,
                        id,
                        at,
                        crate::fx::FxSource::Power,
                        source,
                        None,
                        target,
                        true,
                    );
                }
            }
        }
        self.break_disguise_if_power_cancels(tick, ids, source, power)?;
        self.apply_effects(tick, ids, log.drain())
    }

    fn apply_create_objects_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::CreateObjects {
            list,
            location,
            at,
            from,
        } = effect
        else {
            return Ok(());
        };
        let Some(target) = self.place_of(tick.state(), ids, source, *at) else {
            diag!("power: {list} has nowhere to land - its source has no position");
            return Ok(());
        };
        let mut secondary = None;
        let (list_source, owner, variant) = match *from {
            OclFrom::Location => (source, owner, CreationVariant::At),
            OclFrom::Object { secondary: other } => {
                secondary = other
                    .and_then(|other| tick.state().get(ids.body, other))
                    .map(|body| body.0.position);
                (source, owner, CreationVariant::Object)
            }
            OclFrom::For { primary, min_gap } => {
                let Some(primary_owner) = tick.state().get(ids.owner, primary).map(|o| o.0) else {
                    return Ok(());
                };
                let now = tick.frame();
                if let Some(gap) = min_gap {
                    let quiet = tick
                        .state()
                        .get(ids.die_ocl_stamp, primary)
                        .is_none_or(|stamp| {
                            stamp
                                .0
                                .checked_add(LogicFrames::from_frames(gap))
                                .is_some_and(|end| now >= end)
                        });
                    if !quiet {
                        return Ok(());
                    }
                    tick.state_mut().insert(
                        ids.die_ocl_stamp,
                        primary,
                        super::damage_creation::DieOclStamp(now),
                    )?;
                }
                (primary, primary_owner, CreationVariant::At)
            }
        };
        let (anchor, move_goal) = match location {
            CreateLocation::AtEdgeNearSource => {
                let source_pos = tick
                    .state()
                    .get(ids.body, source)
                    .map_or(target, |body| body.0.position);
                (self.closest_edge_point(source_pos), None)
            }
            CreateLocation::AtEdgeNearTarget => (self.closest_edge_point(target), None),
            CreateLocation::AtEdgeFarthestFromTarget => (self.farthest_edge_point(target), None),
            CreateLocation::AtEdgeNearTargetAndMove => {
                (self.closest_edge_point(target), Some(target))
            }
            CreateLocation::UseSecondaryObject(filter) => {
                secondary = self.nearest_object_passing(tick, ids, owner, filter, target);
                if secondary.is_none() {
                    diag!(
                        "power: {list}'s USE_SECONDARY_OBJECT_LOCATION found nothing \
                             passing NearestSecondaryObjectFilter"
                    );
                }
                (target, None)
            }
            CreateLocation::Other(name) => {
                diag!("power: {list}'s CreateLocation {name} is created at the target");
                (target, None)
            }
            CreateLocation::AtLocation
            | CreateLocation::AboveLocation
            | CreateLocation::UseOwnerObject => (target, None),
        };
        let mut created = Vec::new();
        let sink = move_goal.is_some().then_some(&mut created);
        let made = self.run_creation_list(
            tick,
            ids,
            list_source,
            list,
            anchor,
            variant,
            owner,
            secondary,
            sink,
        )?;
        if let Some(goal) = move_goal {
            for entity in created {
                let mut budgets = super::movement::PathBudgets::new(Some(entity), Vec::new());
                self.walk_toward_ordered(tick, ids, entity, anchor, goal, &mut budgets, true)?;
            }
        }
        diag!(
            "power: {list} created {made} object(s) for player {} at ({:.0}, {:.0})",
            owner.get(),
            anchor.x,
            anchor.y
        );
        Ok(())
    }

    fn apply_teleport_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::Teleport { to, face } = effect else {
            return Ok(());
        };
        let Some(at) = self.place_of(tick.state(), ids, source, *to) else {
            diag!("power: teleport has nowhere to land - its target has no position");
            return Ok(());
        };
        let Some(mut body) = tick.state().get(ids.body, source).copied() else {
            return Ok(());
        };
        let from = body.0.position;
        if *face {
            if let Some(facing) = Facing::from_vector(at.minus(from)) {
                body.0.facing = facing;
            }
        }
        body.0.position = at;
        body.0.waypoint = 0;
        tick.state_mut().insert(ids.body, source, body)?;
        tick.state_mut().remove(ids.path, source);
        if tick.state().contains(ids.horde, source) {
            self.snap_horde_members_to_slots(tick, ids, source)?;
        }
        diag!(
            "power: entity {} teleported from ({:.0},{:.0}) to ({:.0},{:.0})",
            source.index(),
            from.x,
            from.y,
            at.x,
            at.y
        );
        Ok(())
    }

    fn apply_store_objects_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::StoreObjects {
            power,
            radius,
            at,
            slot,
        } = effect
        else {
            return Ok(());
        };
        let Some(centre) = self.place_of(tick.state(), ids, source, *at) else {
            return Ok(());
        };
        let found =
            self.store_objects_scan(tick.state(), ids, source, owner, power, centre, *radius);
        let mut lists = tick
            .state()
            .get(ids.stored_objects, source)
            .cloned()
            .unwrap_or_default();
        diag!(
            "power: {power} stored {} object(s) within {radius:.0} of ({:.0}, {:.0})",
            found.len(),
            centre.x,
            centre.y
        );
        lists.store(*slot, found);
        tick.state_mut().insert(ids.stored_objects, source, lists)?;
        Ok(())
    }

    fn apply_teleport_group_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::TeleportGroup {
            power,
            radius,
            min_ring,
            max_ring,
            trigger_fx,
            target_fx,
            at,
        } = effect
        else {
            return Ok(());
        };
        let Some(target_point) = self.place_of(tick.state(), ids, source, *at) else {
            return Ok(());
        };
        let Some(caster_pos) = tick.state().get(ids.body, source).map(|b| b.0.position) else {
            return Ok(());
        };
        let mut lists = tick
            .state()
            .get(ids.stored_objects, source)
            .cloned()
            .unwrap_or_default();
        let (list, dest) = if let Some(stored) = lists.first_non_empty() {
            let list = std::mem::take(stored);
            tick.state_mut().insert(ids.stored_objects, source, lists)?;
            (list, target_point)
        } else {
            let gathered = self.teleport_gather_scan(
                tick.state(),
                ids,
                source,
                owner,
                power,
                target_point,
                *radius,
            );
            (gathered, caster_pos)
        };
        if let Some(fx) = target_fx {
            for entity in &list {
                self.emit_fx_on_horde_or_object(tick, ids, fx, crate::fx::FxSource::Power, *entity);
            }
        }
        let moved = self.teleport_list(tick, ids, &list, dest, *min_ring, *max_ring)?;
        if let Some(fx) = target_fx {
            for entity in &moved {
                self.emit_fx_on_horde_or_object(tick, ids, fx, crate::fx::FxSource::Power, *entity);
            }
        }
        if let Some(fx) = trigger_fx {
            self.emit_fx_named(
                tick,
                ids,
                fx,
                caster_pos,
                crate::fx::FxSource::Power,
                source,
            );
        }
        diag!(
            "power: {power} teleported {} of {} object(s) to ({:.0}, {:.0})",
            moved.len(),
            list.len(),
            dest.x,
            dest.y
        );
        Ok(())
    }

    fn apply_replace_in_area_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::ReplaceInArea {
            entries,
            radius,
            fx,
            scatter,
            at,
        } = effect
        else {
            return Ok(());
        };
        let Some(centre) = self.place_of(tick.state(), ids, source, *at) else {
            return Ok(());
        };
        let Some(neutral) = self.neutral_player() else {
            diag!("power: replace-in-area needs a neutral player, which this match has none of");
            return Ok(());
        };
        let mut replaced = 0usize;
        let mut killed = Vec::new();
        for entry in entries.iter() {
            let victims = self.replace_in_area_scan(
                tick.state(),
                ids,
                source,
                owner,
                &entry.filter,
                centre,
                *radius,
            );
            for victim in victims {
                let Some((pos, facing)) = tick
                    .state()
                    .get(ids.body, victim)
                    .map(|body| (body.0.position, body.0.facing))
                else {
                    continue;
                };
                let Ok(n) = u32::try_from(entry.replacements.len()) else {
                    continue;
                };
                if let Some(name) = (n > 0)
                    .then(|| tick.rng().below(n))
                    .and_then(|roll| entry.replacements.get(roll as usize))
                {
                    if let Some(kind) = self.kinds.kind(name) {
                        let made = self.spawn_kind_at(tick, ids, kind, pos, facing, neutral)?;
                        if let Some(animal) = made {
                            if let Some(fx) = fx {
                                self.emit_fx_object_named(
                                    tick,
                                    ids,
                                    fx,
                                    pos,
                                    crate::fx::FxSource::Power,
                                    animal,
                                    None,
                                );
                            }
                            if *scatter {
                                let flee_distance = tick
                                    .state()
                                    .get(ids.kind, animal)
                                    .copied()
                                    .and_then(|kind| self.kinds.get(kind))
                                    .and_then(|kind_entry| kind_entry.record.ai.animal_wander)
                                    .map(|wander| wander.flee_distance);
                                if let Some(flee_distance) = flee_distance {
                                    self.start_animal_flee(
                                        tick,
                                        ids,
                                        animal,
                                        source,
                                        flee_distance,
                                    )?;
                                }
                            }
                            replaced += 1;
                        }
                    } else {
                        diag!("power: {name} is not in this match's templates");
                    }
                }
                killed.extend(self.kill(
                    tick,
                    ids,
                    victim,
                    Some(source),
                    crate::combat::DeathNaming::Normal,
                )?);
            }
        }
        self.resolve_deaths(tick, ids, &killed)?;
        diag!(
            "power: replace-in-area replaced {replaced} object(s) around ({:.0}, {:.0})",
            centre.x,
            centre.y
        );
        Ok(())
    }

    fn apply_area_heal_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::AreaHeal {
            at,
            radius,
            amount,
            as_percent,
            affects,
            fx,
        } = effect
        else {
            return Ok(());
        };
        let Some(centre) = self.place_of(tick.state(), ids, source, *at) else {
            return Ok(());
        };
        let (admitted, healed) = self.area_heal(
            tick,
            ids,
            owner,
            centre,
            *radius,
            *amount,
            *as_percent,
            affects,
        )?;
        if let Some(list) = fx {
            for entity in &admitted {
                if let Some(at) = tick.state().get(ids.body, *entity).map(|b| b.0.position) {
                    self.emit_fx_object_named(
                        tick,
                        ids,
                        list,
                        at,
                        crate::fx::FxSource::Power,
                        *entity,
                        None,
                    );
                }
            }
        }
        diag!(
            "power: healed {healed} of {} object(s) of player {} or an ally within \
                 {radius:.0} of ({:.0}, {:.0})",
            admitted.len(),
            owner.get(),
            centre.x,
            centre.y
        );
        Ok(())
    }

    fn apply_area_level_up_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::AreaLevelUp {
            at,
            radius,
            experience,
            affects,
            fx,
        } = effect
        else {
            return Ok(());
        };
        let Some(centre) = self.place_of(tick.state(), ids, source, *at) else {
            return Ok(());
        };
        let granted = self.level_grant(
            tick,
            ids,
            source,
            owner,
            centre,
            *radius,
            *experience,
            affects,
        )?;
        if let Some(list) = fx {
            for entity in &granted {
                self.emit_fx_on_horde_or_object(
                    tick,
                    ids,
                    list,
                    crate::fx::FxSource::Power,
                    *entity,
                );
            }
        }
        diag!(
            "power: granted {experience} experience to {} object(s) within {radius:.0} \
                 of ({:.0}, {:.0})",
            granted.len(),
            centre.x,
            centre.y
        );
        Ok(())
    }

    fn apply_trigger_module_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::TriggerModule { tag, target, at } = effect else {
            return Ok(());
        };
        let Some(kind) = tick.state().get(ids.kind, source).copied() else {
            return Ok(());
        };
        let power_name = self.kinds.get(kind).and_then(|entry| {
            entry
                .record
                .declared_modules
                .iter()
                .find(|module| module.tag.as_deref() == Some(tag.as_ref()))
                .and_then(|module| module.field("SpecialPowerTemplate"))
                .and_then(|field| field.args.first())
                .cloned()
        });
        let Some(power_name) = power_name else {
            if !self.flip_detector(tick, ids, source, tag)? {
                diag!(
                    "power: TriggerSpecialPower names tag {tag}, which entity {} declares \
                         no module for",
                    source.index()
                );
            }
            return Ok(());
        };
        diag!(
            "power: {tag} triggers {power_name} on entity {}",
            source.index()
        );
        self.activate_power(tick, ids, source, &power_name, *target, *at, false)?;
        Ok(())
    }

    fn apply_self_timed_modifier_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::SelfTimedModifier { list, lasts } = effect else {
            return Ok(());
        };
        let Some(id) = self.combat.modifiers.id(list) else {
            diag!("power: ModifierList {list} is not declared");
            return Ok(());
        };
        let until = match lasts {
            Some(lasts) => tick.frame().checked_add(*lasts).unwrap_or(Frame::FOREVER),
            None => self
                .combat
                .modifiers
                .get(id)
                .and_then(|list| list.duration_ms)
                .filter(|ms| *ms > 0)
                .map_or(Frame::FOREVER, |ms| {
                    tick.frame()
                        .checked_add(mint_sage_modules::retail_duration_ticks(ms))
                        .unwrap_or(Frame::FOREVER)
                }),
        };
        let mut timed = tick
            .state()
            .get(ids.timed_modifiers, source)
            .cloned()
            .unwrap_or_default();
        let new = timed.give(id, until);
        tick.state_mut()
            .insert(ids.timed_modifiers, source, timed)?;
        if new {
            self.gain_power_list(tick, ids, source, id)?;
        }
        self.recompute_bonuses(tick, ids, source)?;
        diag!(
            "power: {list} given to entity {} until frame {}",
            source.index(),
            until.get()
        );
        Ok(())
    }

    fn apply_clear_self_timed_modifier_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::ClearSelfTimedModifier(list) = effect else {
            return Ok(());
        };
        let Some(id) = self.combat.modifiers.id(list) else {
            diag!("power: ModifierList {list} is not declared");
            return Ok(());
        };
        let Some(mut timed) = tick.state().get(ids.timed_modifiers, source).cloned() else {
            return Ok(());
        };
        if timed.remove(id) {
            if timed.0.is_empty() {
                tick.state_mut().remove(ids.timed_modifiers, source);
            } else {
                tick.state_mut()
                    .insert(ids.timed_modifiers, source, timed)?;
            }
            self.recompute_bonuses(tick, ids, source)?;
            diag!("power: {list} dropped early from entity {}", source.index());
        }
        Ok(())
    }

    fn apply_timed_model_condition_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::TimedModelCondition {
            flag,
            lasts,
            only_if_absent,
        } = effect
        else {
            return Ok(());
        };
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let already_held = tick
            .state()
            .get(wiring.state, source)
            .is_some_and(|state| state.has_model_condition(flag));
        if *only_if_absent && already_held {
            return Ok(());
        }
        let until = tick.frame().checked_add(*lasts).unwrap_or(Frame::FOREVER);
        let mut timed = tick
            .state()
            .get(ids.timed_conditions, source)
            .cloned()
            .unwrap_or_default();
        let new = timed.give(mint_sage_modules::effects::Name::clone(flag), until);
        tick.state_mut()
            .insert(ids.timed_conditions, source, timed)?;
        if new {
            self.apply_effects(
                tick,
                ids,
                vec![(
                    source,
                    Effect::ModelConditions {
                        add: mint_sage_modules::effects::names(vec![flag.to_string()]),
                        remove: mint_sage_modules::effects::names(Vec::new()),
                    },
                )],
            )?;
        }
        diag!(
            "power: model state [{flag}] on entity {} until frame {}",
            source.index(),
            until.get()
        );
        Ok(())
    }

    fn apply_fire_weapon_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::FireWeapon {
            weapon,
            at,
            offset,
            repeat,
            alive_only,
            reload_slot,
        } = effect
        else {
            return Ok(());
        };
        let Some(weapon_id) = self.combat.weapons.id(weapon) else {
            diag!("power: weapon {weapon} is not declared");
            return Ok(());
        };
        let Some(point) = self.place_of(tick.state(), ids, source, *at) else {
            return Ok(());
        };
        let point = self.offset_in_frame(tick.state(), ids, source, point, *offset);
        let victim = match at {
            PowerPlace::Object(target) if reload_slot.is_some() => Some(*target),
            _ => None,
        };
        self.queue_shot(
            tick,
            ids,
            crate::spell_state::PendingShot {
                source,
                weapon: weapon_id,
                at: point,
                repeat: *repeat,
                alive_only: *alive_only,
                reload_slot: *reload_slot,
                owner_if_gone: None,
                victim,
            },
        )?;
        Ok(())
    }

    fn apply_play_fx_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::PlayFx {
            list,
            at,
            on_object,
            secondary,
        } = effect
        else {
            return Ok(());
        };
        if let Some(point) = self.place_of(tick.state(), ids, source, *at) {
            if *on_object {
                self.emit_fx_object_named(
                    tick,
                    ids,
                    list,
                    point,
                    crate::fx::FxSource::Power,
                    source,
                    *secondary,
                );
            } else {
                self.emit_fx_named(tick, ids, list, point, crate::fx::FxSource::Power, source);
            }
        }
        Ok(())
    }

    fn apply_create_object_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::CreateObject { name, at } = effect else {
            return Ok(());
        };
        let Some(point) = self.place_of(tick.state(), ids, source, *at) else {
            return Ok(());
        };
        let Some(kind) = self.kinds.kind(name) else {
            diag!("power: {name} is not in this match's templates");
            return Ok(());
        };
        let facing = tick
            .state()
            .get(ids.body, source)
            .map_or(Facing::EAST, |body| body.0.facing);
        let made = self.spawn_kind_at(tick, ids, kind, point, facing, owner)?;
        diag!(
            "power: {name} {} for player {} at ({:.0}, {:.0})",
            if made.is_some() {
                "created"
            } else {
                "not created"
            },
            owner.get(),
            point.x,
            point.y
        );
        Ok(())
    }

    fn apply_spell_recharge_count_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::SpellRechargeCount {
            player,
            delta,
            percentages,
        } = effect
        else {
            return Ok(());
        };
        let player = player.unwrap_or(owner);
        if let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        {
            let moved = tick
                .state()
                .get(ids.spell_recharge, player_entity)
                .copied()
                .unwrap_or_default()
                .moved(*delta, percentages);
            tick.state_mut()
                .insert(ids.spell_recharge, player_entity, moved)?;
            diag!(
                "power: player {} counts {} spell recharge modifier(s), modifier {}",
                player.get(),
                moved.count,
                moved.modifier
            );
        }
        Ok(())
    }

    fn apply_command_point_bonus_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::CommandPointBonus(points) = effect else {
            return Ok(());
        };
        if let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        {
            tick.state_mut().insert(
                ids.player_point_bonus,
                player_entity,
                crate::spell_state::PointBonus(*points),
            )?;
            diag!(
                "power: player {} gains {points} command point(s)",
                owner.get()
            );
        }
        Ok(())
    }

    fn apply_set_weather_effect_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::SetWeatherEffect {
            list,
            anti,
            affects,
            affect_good,
            affect_evil,
            lasts,
        } = effect
        else {
            return Ok(());
        };
        let given = self.set_weather_effect(
            tick,
            ids,
            owner,
            list.as_ref(),
            anti,
            affects,
            *affect_good,
            *affect_evil,
            *lasts,
        )?;
        diag!(
            "power: weather effect given to {given} object(s) for {} frame(s)",
            lasts.get()
        );
        Ok(())
    }

    fn apply_add_object_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::AddObject { name, at } = effect else {
            return Ok(());
        };
        let Some(point) = self.place_of(tick.state(), ids, source, *at) else {
            return Ok(());
        };
        let Some(kind) = self.kinds.kind(name) else {
            diag!("power: {name} is not in this match's templates");
            return Ok(());
        };
        if !self.plant_tree(tick, ids, kind, point) {
            let facing = tick
                .state()
                .get(ids.body, source)
                .map_or(Facing::EAST, |body| body.0.facing);
            self.spawn_kind_at(tick, ids, kind, point, facing, owner)?;
        }
        Ok(())
    }

    fn apply_dominate_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::Dominate {
            at,
            radius,
            affects,
            permanent,
            fx,
        } = effect
        else {
            return Ok(());
        };
        let Some(centre) = self.place_of(tick.state(), ids, source, *at) else {
            return Ok(());
        };
        let taken = if let PowerPlace::Object(target) = *at {
            self.dominate_clicked(tick, ids, source, owner, target, affects, *permanent)?
        } else {
            self.dominate(
                tick, ids, source, owner, centre, *radius, affects, *permanent,
            )?
        };
        if let Some(list) = fx {
            for entity in &taken {
                self.emit_fx_on_horde_or_object(
                    tick,
                    ids,
                    list,
                    crate::fx::FxSource::Power,
                    *entity,
                );
            }
        }
        diag!(
            "power: {} object(s) within {radius:.0} of ({:.0}, {:.0}) dominated by \
                 player {}{}",
            taken.len(),
            centre.x,
            centre.y,
            owner.get(),
            if *permanent { "" } else { ", temporarily" }
        );
        Ok(())
    }

    fn apply_curse_powers_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::CursePowers { at, share, fx } = effect else {
            return Ok(());
        };
        let PowerPlace::Object(target) = *at else {
            diag!("power: a curse with no object target curses nothing");
            return Ok(());
        };
        let cursed = self.curse_powers(tick, ids, target, *share)?;
        if let Some(list) = fx {
            if let Some(at) = tick.state().get(ids.body, target).map(|b| b.0.position) {
                self.emit_fx_object_named(
                    tick,
                    ids,
                    list,
                    at,
                    crate::fx::FxSource::Power,
                    target,
                    None,
                );
            }
        }
        diag!(
            "power: entity {} has {cursed} power(s) set back by {:.0}% of their reload",
            target.index(),
            share * 100.0
        );
        Ok(())
    }

    fn apply_defect_for_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::DefectFor { back_to, lasts } = effect else {
            return Ok(());
        };
        if tick.state().contains(ids.defect_revert, source) {
            diag!(
                "capture: entity {} is already defecting - the new dominate does not \
                     refresh it",
                source.index()
            );
            return Ok(());
        }
        let since = tick.frame();
        let until = since
            .checked_add(*lasts)
            .unwrap_or(mint_sim::Frame::FOREVER);
        tick.state_mut().insert(
            ids.defect_revert,
            source,
            crate::rules::capture::DefectRevert {
                to: *back_to,
                by: source,
                since,
                until,
            },
        )?;
        diag!(
            "capture: entity {} defects to player {} for {} frame(s)",
            source.index(),
            owner.get(),
            lasts.get()
        );
        Ok(())
    }

    fn apply_arrow_storm_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::ArrowStorm {
            weapon,
            at,
            radius,
            shots_per_burst,
            shots_per_target,
            max_shots,
            can_shoot_empty_ground,
        } = effect
        else {
            return Ok(());
        };
        let Some(centre) = self.place_of(tick.state(), ids, source, *at) else {
            return Ok(());
        };
        let fired = self.arrow_storm_burst(
            tick,
            ids,
            owner,
            source,
            weapon,
            centre,
            *radius,
            *shots_per_burst,
            *shots_per_target,
            *max_shots,
            *can_shoot_empty_ground,
        )?;
        diag!(
            "power: arrow storm fired {fired} shot(s) of {weapon} within {radius:.0} of \
                 ({:.0}, {:.0})",
            centre.x,
            centre.y
        );
        Ok(())
    }

    fn apply_grant_invisibility_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        effect: &Effect,
    ) -> Result<()> {
        let Effect::GrantInvisibility {
            filter,
            radius,
            duration,
            nugget,
        } = effect
        else {
            return Ok(());
        };
        let world = tick.state();
        if let (Some(owner), Some(centre)) = (
            world.get(ids.owner, source).map(|o| o.0),
            world.get(ids.body, source).map(|body| body.0.position),
        ) {
            let admitted = self.admitted_within(
                world,
                ids,
                owner,
                centre,
                *radius,
                &super::spell_effects::filter_of(filter),
            );
            let granted = self.grant_invisibility(tick, ids, &admitted, *duration, nugget)?;
            diag!(
                "power: InvisibilitySpecialPower granted {granted} of {} admitted entity(ies)",
                admitted.len()
            );
        }
        Ok(())
    }

    pub(super) fn apply_power_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        effect: &Effect,
    ) -> Result<()> {
        let Some(owner) = tick.state().get(ids.owner, source).map(|owner| owner.0) else {
            return Ok(());
        };
        match effect {
            Effect::CreateObjects { .. } => {
                return self.apply_create_objects_effect(tick, ids, source, owner, effect);
            }
            Effect::Teleport { .. } => {
                return self.apply_teleport_effect(tick, ids, source, effect);
            }
            Effect::StoreObjects { .. } => {
                return self.apply_store_objects_effect(tick, ids, source, owner, effect);
            }
            Effect::TeleportGroup { .. } => {
                return self.apply_teleport_group_effect(tick, ids, source, owner, effect);
            }
            Effect::ReplaceInArea { .. } => {
                return self.apply_replace_in_area_effect(tick, ids, source, owner, effect);
            }
            Effect::AreaHeal { .. } => {
                return self.apply_area_heal_effect(tick, ids, source, owner, effect);
            }
            Effect::AreaModifier { at, rule } => {
                let Some(centre) = self.place_of(tick.state(), ids, source, *at) else {
                    return Ok(());
                };
                let given = self.area_modifier(tick, ids, source, owner, centre, rule)?;
                diag!(
                    "power: {} given to {given} object(s) within {:.0} of ({:.0}, {:.0})",
                    rule.list.as_deref().unwrap_or("(no list)"),
                    rule.radius,
                    centre.x,
                    centre.y
                );
            }
            Effect::AreaLevelUp { .. } => {
                return self.apply_area_level_up_effect(tick, ids, source, owner, effect);
            }
            Effect::TriggerModule { .. } => {
                return self.apply_trigger_module_effect(tick, ids, source, effect);
            }
            Effect::ToggleStealth {
                lasts,
                palantir_timer,
            } => {
                self.toggle_stealth(tick, ids, source, *lasts, *palantir_timer)?;
            }
            Effect::SelfTimedModifier { .. } => {
                return self.apply_self_timed_modifier_effect(tick, ids, source, effect);
            }
            Effect::ClearSelfTimedModifier(..) => {
                return self.apply_clear_self_timed_modifier_effect(tick, ids, source, effect);
            }
            Effect::TimedModelCondition { .. } => {
                return self.apply_timed_model_condition_effect(tick, ids, source, effect);
            }
            Effect::GrantPlayerUpgrade(name) => match self.upgrade_bits.bit(name) {
                Some(bit) => self.grant_player_upgrade(tick, ids, owner, bit)?,
                None => diag!("power: {name} is not an Upgrade - nothing granted"),
            },
            Effect::GrantUpgrade(name) => match self.upgrade_bits.bit(name) {
                Some(bit) if self.upgrade_bits.is_player(bit) => {
                    self.grant_player_upgrade(tick, ids, owner, bit)?;
                }
                Some(bit) => self.grant_object_upgrade(tick, ids, source, bit)?,
                None => diag!("upgrade: {name} is not an Upgrade - nothing granted"),
            },
            Effect::FireWeapon { .. } => {
                return self.apply_fire_weapon_effect(tick, ids, source, effect);
            }
            Effect::FireWeaponAtDeath {
                weapon,
                offset,
                delay,
            } => {
                let Some(weapon_id) = self.combat.weapons.id(weapon) else {
                    diag!("death: weapon {weapon} is not declared");
                    return Ok(());
                };
                let Some(body) = tick.state().get(ids.body, source).copied() else {
                    return Ok(());
                };
                let at = self.offset_in_frame(
                    tick.state(),
                    ids,
                    source,
                    body.0.position,
                    (offset.0, offset.1),
                );
                let ground = self.ground_height(at.x, at.y);
                let feet = tick
                    .state()
                    .get(ids.ground_offset, source)
                    .map_or(0.0, |lift| lift.0);
                let due = tick.frame().checked_add(*delay).unwrap_or(Frame::FOREVER);
                self.queue_delayed(
                    tick,
                    ids,
                    crate::spell_state::DelayedHit {
                        source,
                        weapon: weapon_id,
                        victim: None,
                        at,
                        height: ground + feet + offset.2,
                        due,
                        nugget: None,
                        owner_if_gone: Some(owner),
                        blow: None,
                    },
                )?;
            }
            Effect::LoseStealth => {
                self.apply_lose_stealth_trigger(tick, ids, source)?;
            }
            Effect::PlayWeaponPreFireFX { weapon } => {
                let Some(weapon_id) = self.combat.weapons.id(weapon) else {
                    diag!("power: weapon {weapon} is not declared");
                    return Ok(());
                };
                if let Some(point) = tick.state().get(ids.body, source).map(|b| b.0.position) {
                    self.emit_weapon_pre_attack_fx(tick, ids, weapon_id, point, source);
                }
            }
            Effect::PlayFx { .. } => {
                return self.apply_play_fx_effect(tick, ids, source, effect);
            }
            Effect::Spawn {
                name,
                requester,
                ordinal,
            } => {
                self.spawn_for_module(tick, ids, (source, owner), name, *requester, *ordinal)?;
            }
            Effect::CreateObject { .. } => {
                return self.apply_create_object_effect(tick, ids, source, owner, effect);
            }
            Effect::Flood(spec) => {
                self.make_flood(tick, ids, source, owner, spec)?;
            }
            Effect::CreateSlave {
                name,
                offset,
                destroy_when_sold,
            } => {
                self.spawn_slave_at_offset(tick, ids, source, name, *offset, *destroy_when_sold)?;
            }
            Effect::SpellRechargeCount { .. } => {
                return self.apply_spell_recharge_count_effect(tick, ids, owner, effect);
            }
            Effect::CommandPointBonus(..) => {
                return self.apply_command_point_bonus_effect(tick, ids, owner, effect);
            }
            Effect::SetWeatherEffect { .. } => {
                return self.apply_set_weather_effect_effect(tick, ids, owner, effect);
            }
            Effect::ClearWeatherEffect => {
                let lifted = self.clear_weather_effect(tick, ids)?;
                diag!("power: weather effect lifted on {lifted} object(s)");
            }
            Effect::ChangeWeather { kind, lasts } => {
                self.set_weather_kind(tick, ids, *kind, *lasts)?;
            }
            Effect::FearEveryoneAdmitted { affects } => {
                let feared = self.fear_everyone_admitted(tick, ids, source, owner, affects)?;
                diag!("power: UNCONTROLLABLE_FEAR posted to {feared} object(s)");
            }
            Effect::SpacedObjects { name, spacing, .. } => {
                let made = self.spaced_objects(tick, ids, name, *spacing)?;
                diag!("power: {made} {name}(s) placed {spacing:.0} apart over the map");
            }
            Effect::CashBounty(fraction) => {
                self.set_cash_bounty(tick, ids, source, owner, *fraction)?;
            }
            Effect::AddObject { .. } => {
                return self.apply_add_object_effect(tick, ids, source, owner, effect);
            }
            Effect::FellTreesForMoney {
                at,
                radius,
                fx,
                multiplier,
                cap,
            } => {
                if let Some(centre) = self.place_of(tick.state(), ids, source, *at) {
                    self.fell_trees_for_money(
                        tick,
                        ids,
                        (source, owner),
                        centre,
                        *radius,
                        fx.as_deref(),
                        (*multiplier, *cap),
                    )?;
                }
            }
            Effect::MorphTrees {
                at,
                radius,
                tainted,
            } => {
                if let Some(centre) = self.place_of(tick.state(), ids, source, *at) {
                    self.morph_trees(tick, ids, centre, *radius, *tainted);
                }
            }
            Effect::Dominate { .. } => {
                return self.apply_dominate_effect(tick, ids, source, owner, effect);
            }
            Effect::CursePowers { .. } => {
                return self.apply_curse_powers_effect(tick, ids, effect);
            }
            Effect::DefectFor { .. } => {
                return self.apply_defect_for_effect(tick, ids, source, owner, effect);
            }
            Effect::Capture {
                at,
                radius,
                affects,
            } => {
                let Some(centre) = self.place_of(tick.state(), ids, source, *at) else {
                    return Ok(());
                };
                let taken = self.capture(tick, ids, owner, centre, *radius, affects)?;
                diag!(
                    "power: {taken} object(s) within {radius:.0} of ({:.0}, {:.0}) join player {}",
                    centre.x,
                    centre.y,
                    owner.get()
                );
            }
            Effect::ProductionBonus {
                kinds,
                multiplier,
                lasts,
            } => {
                let named = self.production_bonus(tick, ids, owner, kinds, *multiplier, *lasts)?;
                diag!(
                    "power: {named} kind(s) built {multiplier}x as fast for {} frame(s)",
                    lasts.get()
                );
            }
            Effect::CastEdge {
                power,
                target,
                edge,
            } => {
                self.cast_edge(tick, ids, source, power, *target, *edge)?;
                self.pick_up_edge(tick, ids, source, power, *target, *edge)?;
                self.power_enum_trigger(tick, ids, source, power, *edge)?;
                self.giant_bird_power_edge(tick, ids, source, power, *target, *edge)?;
            }
            Effect::Replace { template, carry } => {
                self.replace_object(tick, ids, source, template, *carry)?;
            }
            Effect::PressCommand(name) => self.press_command(tick, ids, source, name)?,
            Effect::ArrowStorm { .. } => {
                return self.apply_arrow_storm_effect(tick, ids, source, owner, effect);
            }
            Effect::Fling {
                power,
                velocity,
                landing_warhead,
            } => {
                self.fling_passengers(
                    tick,
                    ids,
                    source,
                    power,
                    *velocity,
                    landing_warhead.as_deref(),
                )?;
            }
            Effect::ClearArrowStorm => {
                tick.state_mut().remove(ids.arrow_storm_hits, source);
            }
            Effect::RefreshOtherPowers {
                except,
                affect_allies,
                range,
                affects,
            } => {
                let refreshed = self.refresh_other_powers(
                    tick,
                    ids,
                    source,
                    except,
                    (*affect_allies).then_some((*range, affects)),
                )?;
                diag!(
                    "power: {except} refreshed {refreshed} other power(s) from entity {}",
                    source.index()
                );
            }
            Effect::GrantInvisibility { .. } => {
                return self.apply_grant_invisibility_effect(tick, ids, source, effect);
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn offset_in_frame(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        point: Vec2,
        (x, y): (f32, f32),
    ) -> Vec2 {
        if x == 0.0 && y == 0.0 {
            return point;
        }
        let angle = world
            .get(ids.body, source)
            .map_or(0.0, |body| body.0.facing.radians());
        let (sin, cos) = (mint_math::f32::sin(angle), mint_math::f32::cos(angle));
        Vec2::new(point.x + x * cos - y * sin, point.y + x * sin + y * cos)
    }

    pub(super) fn place_of(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        place: PowerPlace,
    ) -> Option<Vec2> {
        let position = |entity: Entity| world.get(ids.body, entity).map(|body| body.0.position);
        match place {
            PowerPlace::At(x, y) if x.is_finite() && y.is_finite() => Some(Vec2::new(x, y)),
            PowerPlace::At(..) => None,
            PowerPlace::Object(target) => position(target),
            PowerPlace::Source => position(source),
        }
    }

    fn closest_edge_point(&self, from: Vec2) -> Vec2 {
        let high = self.passability.playable_high;
        let distances = [from.y, high.x - from.x, high.y - from.y, from.x];
        let nearest = distances
            .iter()
            .copied()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(0, |(index, _)| index);
        match nearest {
            0 => Vec2::new(from.x, 0.0),
            1 => Vec2::new(high.x, from.y),
            2 => Vec2::new(from.x, high.y),
            _ => Vec2::new(0.0, from.y),
        }
    }

    fn farthest_edge_point(&self, from: Vec2) -> Vec2 {
        let high = self.passability.playable_high;
        let x = if from.x < high.x * 0.5 { high.x } else { 0.0 };
        let y = if from.y < high.y * 0.5 { high.y } else { 0.0 };
        Vec2::new(x, y)
    }

    fn nearest_object_passing(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        filter: &ObjectFilter,
        near: Vec2,
    ) -> Option<Vec2> {
        let store = tick.state().store(ids.kind)?;
        let mut best: Option<(f32, Vec2)> = None;
        for &entity in store.entities() {
            if !tick.state().is_live(entity) {
                continue;
            }
            let Some(entry) = tick
                .state()
                .get(ids.kind, entity)
                .and_then(|kind| self.kinds.get(*kind))
            else {
                continue;
            };
            if !filter.passes(&entry.record.kind_of, &entry.record.name) {
                continue;
            }
            let other = tick.state().get(ids.owner, entity).map(|held| held.0);
            let (same, allied, hostile) = super::spell_effects::standing(self, owner, other);
            if !filter.passes_relation(same, allied, hostile) {
                continue;
            }
            let Some(body) = tick.state().get(ids.body, entity) else {
                continue;
            };
            let distance = body.0.position.minus(near).length_squared();
            if best.is_none_or(|(closest, _)| distance < closest) {
                best = Some((distance, body.0.position));
            }
        }
        best.map(|(_, position)| position)
    }

    fn creation_gate_passes(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        source: Entity,
        entry: &mint_sage_ini::CreationEntry,
    ) -> bool {
        if entry.required_upgrades.is_empty() && entry.forbidden_upgrades.is_empty() {
            return true;
        }
        let held = self.effective_upgrades(tick, ids, source);
        if held.intersects(&self.upgrade_bits.mask(&entry.forbidden_upgrades)) {
            return false;
        }
        entry.required_upgrades.is_empty()
            || held.intersects(&self.upgrade_bits.mask(&entry.required_upgrades))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_creation_list(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        list: &str,
        anchor: Vec2,
        variant: CreationVariant,
        owner: PlayerId,
        secondary: Option<Vec2>,
        mut created: Option<&mut Vec<Entity>>,
    ) -> Result<usize> {
        let Some(def) = self.ocls.get(list) else {
            diag!("power: ObjectCreationList {list} is not declared");
            return Ok(0);
        };
        let def = def.clone();
        let primary_facing = tick
            .state()
            .get(ids.body, source)
            .map_or(Facing::EAST, |body| body.0.facing);
        let mut made = 0usize;
        for entry in &def.create_object {
            if entry.names.is_empty() || !self.creation_gate_passes(tick, ids, source, entry) {
                continue;
            }
            let count = entry.count.unwrap_or(1).clamp(0, MAX_CREATE_COUNT);
            let count = u32::try_from(count).unwrap_or(0);
            let offset = entry.orientation_offset_degrees.unwrap_or(0.0).to_radians();
            let with_offset = Facing::from_radians(primary_facing.radians() + offset);
            let creation_angle = match variant {
                CreationVariant::Object => primary_facing,
                CreationVariant::At if entry.orient_in_primary_direction => with_offset,
                CreationVariant::At => Facing::EAST,
            };
            let facing_degrees = primary_facing.radians().to_degrees();
            let throw = crate::summons::Throw::of(entry, self.logic_hz());
            for placement in
                crate::summons::placements(entry, count, facing_degrees, self.logic_hz())
            {
                let pick = if entry.names.len() > 1 {
                    let span = u32::try_from(entry.names.len()).unwrap_or(1);
                    usize::try_from(tick.rng().below(span)).unwrap_or(0)
                } else {
                    0
                };
                let Some(name) = entry.names.get(pick) else {
                    continue;
                };
                let Some(kind) = self.kinds.kind(name) else {
                    diag!("power: {list} names {name}, which is not in this match's templates");
                    continue;
                };
                let mut position =
                    Vec2::new(anchor.x + placement.offset.x, anchor.y + placement.offset.y);
                if entry.spread_formation {
                    position = self
                        .spread_spot(tick, ids, entry, position)
                        .unwrap_or(anchor);
                }
                if self.plant_tree(tick, ids, kind, position) {
                    made = made.saturating_add(1);
                    continue;
                }
                let facing = placement.facing_degrees.map_or(creation_angle, |degrees| {
                    Facing::from_radians(degrees.to_radians())
                });
                let has = |flag: &str| {
                    entry
                        .disposition
                        .iter()
                        .any(|d| d.eq_ignore_ascii_case(flag))
                };
                let facing = if has(mint_sage_ini::DEFAULT_DISPOSITION) && !has("ANIMATED") {
                    Facing::from_radians(tick.rng().next_f32() * core::f32::consts::TAU)
                } else {
                    facing
                };
                let facing = match secondary {
                    Some(target) if entry.orient_in_secondary_direction => {
                        Facing::from_vector(target.minus(position)).map_or(facing, |toward| {
                            Facing::from_radians(toward.radians() + offset)
                        })
                    }
                    _ => facing,
                };
                diag!(
                    "power: {list} makes {name} at ({:.0}, {:.0}) toward {secondary:?} facing {:.1} degrees",
                    position.x,
                    position.y,
                    facing.radians().to_degrees()
                );
                let Some(object) = self.spawn_kind_at(tick, ids, kind, position, facing, owner)?
                else {
                    continue;
                };
                made = made.saturating_add(1);
                if let Some(sink) = &mut created {
                    sink.push(object);
                }
                if let Some(ms) = entry.invulnerable_time_ms.filter(|ms| *ms > 0.0) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let lasts =
                        mint_sage_modules::retail_duration_ticks(ms.min(86_400_000.0) as u32);
                    let until = tick.frame().checked_add(lasts).unwrap_or(Frame::FOREVER);
                    tick.state_mut().insert(
                        ids.undetected_defector,
                        object,
                        super::crates::UndetectedDefector(until),
                    )?;
                }
                if entry.clear_removables {
                    self.clear_trees_under(tick, ids, object, position)?;
                }
                if let (Some(throw), Some(_)) = (throw, self.physics_of(kind)) {
                    let throw = throw.velocity(|| tick.rng().next_f32());
                    let from = mint_sage_combat::Point3::new(
                        position.x,
                        position.y,
                        self.ground_height(position.x, position.y) + entry.offset[2],
                    );
                    self.launch_debris(tick, ids, (object, kind), from, throw)?;
                }
            }
        }
        Ok(made)
    }

    fn spread_spot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entry: &mint_sage_ini::CreationEntry,
        centre: Vec2,
    ) -> Option<Vec2> {
        use mint_sage_passability::{BOUNDARY, CLIFF, DEEP_WATER, FORD, OBSTACLE, WATER};
        let bound = |value: Option<f32>| value.filter(|v| v.is_finite()).unwrap_or(0.0);
        let [low, high] = entry.min_distance_formation.map(bound);
        let inner = low + (high - low) * tick.rng().next_f32();
        let start_angle = tick.rng().next_f32() * core::f32::consts::TAU;
        let has = |name: &str| {
            entry
                .disposition
                .iter()
                .any(|d| d.eq_ignore_ascii_case(name))
        };
        let mut blocked_by = BOUNDARY;
        if !has("USE_CLIFF") {
            blocked_by = blocked_by.union(CLIFF);
        }
        if !has("USE_WATER_SURFACE") {
            blocked_by = blocked_by.union(FORD).union(WATER).union(DEEP_WATER);
        }
        if !entry.ignore_all_objects {
            blocked_by = blocked_by.union(OBSTACLE);
        }
        let world = tick.state();
        let bodies: Vec<(Vec2, f32)> = if entry.ignore_all_objects {
            Vec::new()
        } else {
            world
                .store(ids.shape)
                .map(|store| {
                    store
                        .iter()
                        .filter_map(|(entity, shape)| {
                            let at = world.get(ids.body, entity)?.0.position;
                            Some((at, shape.radius))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let clear = |at: Vec2| {
            bodies
                .iter()
                .all(|(other, radius)| at.minus(*other).length() >= radius + SPREAD_PROBE_RADIUS)
        };
        mint_sage_passability::find_position_around(
            &self.dynamic_view(tick, ids),
            self.passability.geometry,
            centre,
            (inner, bound(entry.max_distance_formation), start_angle),
            mint_movement::MoverClass::new(blocked_by),
            clear,
        )
    }

    pub(super) fn replace_object(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        template: &str,
        carry: ReplaceCarry,
    ) -> Result<Option<Entity>> {
        let Some(kind) = self.kinds.kind(template) else {
            diag!("replace: {template} is not in this match's templates");
            return Ok(None);
        };
        let made = crate::replace::replace_object(
            tick,
            ids,
            entity,
            carry,
            |tick, position, facing, owner| {
                self.spawn_kind_at(tick, ids, kind, position, facing, owner)
            },
            |points| {
                self.level_tables
                    .get(kind)
                    .map(|table| u16::try_from(table.index_for(points)).unwrap_or(u16::MAX))
            },
        )?;
        match made {
            Some(replacement) => diag!(
                "replace: entity {} became {template} as entity {}",
                entity.index(),
                replacement.index()
            ),
            None => diag!(
                "replace: {template} could not replace entity {}",
                entity.index()
            ),
        }
        Ok(made)
    }

    pub(super) fn press_command(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        name: &str,
    ) -> Result<()> {
        let Some(owner) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
            return Ok(());
        };
        let Some(kind) = tick.state().get(ids.kind, entity).copied() else {
            return Ok(());
        };
        if let Some((power_name, target_kind)) = self.kinds.power_button(name) {
            self.fire_power_from(
                tick,
                ids,
                owner,
                entity,
                power_name,
                *target_kind,
                &ObjectFilter::default(),
                None,
                None,
                false,
            )?;
            return Ok(());
        }
        if self.press_weapon_toggle(tick, ids, entity, name, None)? {
            return Ok(());
        }
        for is_player in [false, true] {
            if self.press_upgrade_button(tick, ids, entity, kind, owner, name, is_player)? {
                return Ok(());
            }
        }
        diag!(
            "command: {name} on entity {} is not a recognised command yet",
            entity.index()
        );
        Ok(())
    }

    fn press_upgrade_button(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: crate::unit::Kind,
        owner: PlayerId,
        name: &str,
        is_player: bool,
    ) -> Result<bool> {
        let Some(target) = self
            .upgrade_buildables
            .iter()
            .position(|upgrade| upgrade.command_button.eq_ignore_ascii_case(name))
            .and_then(|index| u16::try_from(index).ok())
        else {
            return Ok(false);
        };
        let menu_of = if is_player {
            &self.player_upgrade_menu_of
        } else {
            &self.object_upgrade_menu_of
        };
        let menus = if is_player {
            &self.player_upgrade_menus
        } else {
            &self.object_upgrade_menus
        };
        let Some(menu) = menu_of.get(usize::from(kind.0)).copied().flatten() else {
            return Ok(false);
        };
        let Some(button) = menus
            .get(usize::from(menu))
            .and_then(|slots| slots.iter().position(|&slot| slot == target))
            .and_then(|index| u8::try_from(index).ok())
        else {
            return Ok(false);
        };
        let gate = |world: &World, structure: Entity, menu: u16, button: u8| {
            self.upgrade_gate(world, ids, structure, is_player, menu, button)
        };
        for line in crate::economy::queue_upgrade_at(
            tick,
            Self::economy_ids(ids),
            &self.kinds,
            &self.upgrade_bits,
            owner,
            &[entity],
            button,
            is_player,
            menus,
            &self.upgrade_buildables,
            &gate,
            self.production_cost_change_percent(owner),
            self.brutal_cheats_for(owner).build_cost_reduction,
            &|world, builder, bit| self.sub_upgrade_counted(world, ids, builder, bit),
        )? {
            diag!("{line}");
        }
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn area_heal(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        centre: Vec2,
        radius: f32,
        amount: f32,
        as_percent: bool,
        affects: &[mint_sage_modules::effects::Name],
    ) -> Result<(Vec<Entity>, usize)> {
        let reach = radius * radius;
        let world = tick.state();
        let targets: Vec<Entity> = world
            .store(ids.health)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, health)| {
                        !health.0.is_dead()
                            && !world.contains(ids.doomed, *entity)
                            && !world.contains(ids.construction, *entity)
                            && world
                                .get(ids.owner, *entity)
                                .is_some_and(|other| self.alliances.allied(owner, other.0))
                            && world.get(ids.body, *entity).is_some_and(|body| {
                                let dx = body.0.position.x - centre.x;
                                let dy = body.0.position.y - centre.y;
                                dx * dx + dy * dy <= reach
                            })
                            && world.get(ids.kind, *entity).is_some_and(|kind| {
                                self.kinds.get(*kind).is_some_and(|entry| {
                                    affects.iter().any(|token| entry.record.has_kind(token))
                                        && !entry.record.has_kind("WEBBED")
                                })
                            })
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        let mut admitted = Vec::new();
        let mut healed = 0usize;
        for entity in targets {
            let Some(health) = tick.state_mut().get_mut(ids.health, entity) else {
                continue;
            };
            let gain = if as_percent {
                health.0.max() * amount
            } else {
                amount
            };
            if gain <= 0.0 {
                continue;
            }
            let before = health.0.current();
            health.0.set_current(before + gain);
            if health.0.current() > before {
                healed = healed.saturating_add(1);
            }
            admitted.push(entity);
        }
        Ok((admitted, healed))
    }

    fn area_modifier(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        centre: Vec2,
        rule: &ModifierRule,
    ) -> Result<usize> {
        let now = tick.frame();
        let tokens: Vec<String> = rule.affects.iter().map(|name| name.to_string()).collect();
        let filter = crate::defeat::ObjectFilter::parse(&tokens);
        let reach = rule.radius * rule.radius;
        let world = tick.state();
        let mut targets: Vec<Entity> = world
            .store(ids.kind)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, kind)| {
                        *entity != source
                            && self.admits(world, ids, *entity, owner, &filter, rule.sides)
                            && !self.kinds.get(**kind).is_some_and(|entry| {
                                entry
                                    .record
                                    .kind_of
                                    .iter()
                                    .any(|flag| flag == "IGNORED_IN_GUI")
                            })
                            && world.get(ids.body, *entity).is_some_and(|body| {
                                let dx = body.0.position.x - centre.x;
                                let dy = body.0.position.y - centre.y;
                                dx * dx + dy * dy <= reach
                            })
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        if rule.affects_self {
            if world.contains(ids.horde, source) {
                targets.extend(self.horde_members(tick, ids, &[source]));
            } else {
                targets.push(source);
            }
            targets.sort_unstable_by_key(|entity| entity.index());
            targets.dedup();
        }
        let anti_stamp = (!rule.anti.is_empty()).then(|| match &rule.list {
            None => now,
            Some(list_name) => self
                .combat
                .modifiers
                .id(list_name)
                .and_then(|id| self.combat.modifiers.get(id))
                .and_then(|list| list.duration_ms)
                .filter(|ms| *ms > 0)
                .map_or(Frame::FOREVER, |ms| {
                    now.checked_add(mint_sage_modules::retail_duration_ticks(ms))
                        .unwrap_or(Frame::FOREVER)
                }),
        });
        let mut given = 0usize;
        for &entity in &targets {
            let other_owner = tick.state().get(ids.owner, entity).map(|o| o.0);
            let evil = other_owner.is_some_and(|player| self.is_evil_owner(player));
            if rule.affect_good && evil {
                continue;
            }
            if rule.affect_evil && !evil {
                continue;
            }
            if !rule.affect_allies && other_owner != Some(owner) {
                continue;
            }
            if let Some(list_name) = &rule.list {
                match self.combat.modifiers.id(list_name) {
                    Some(id) => {
                        let refused = self.combat.modifiers.get(id).is_some_and(|list| {
                            list.ignore_if_anti_category_active
                                && list.category.as_deref().is_some_and(|category| {
                                    self.category_suppressed(
                                        tick.state(),
                                        ids,
                                        entity,
                                        Some(category),
                                    )
                                })
                        });
                        if !refused {
                            let until = self
                                .combat
                                .modifiers
                                .get(id)
                                .and_then(|list| list.duration_ms)
                                .filter(|ms| *ms > 0)
                                .map_or(Frame::FOREVER, |ms| {
                                    now.checked_add(mint_sage_modules::retail_duration_ticks(ms))
                                        .unwrap_or(Frame::FOREVER)
                                });
                            self.give_timed_list(tick, ids, entity, id, until)?;
                            given = given.saturating_add(1);
                        }
                    }
                    None => diag!("power: ModifierList {list_name} is not declared"),
                }
            }
            if let Some(expiry) = anti_stamp {
                let lifts = rule.re_enable
                    && ((rule.affect_evil && evil)
                        || (rule.affect_good && !evil)
                        || (!rule.affect_evil
                            && !rule.affect_good
                            && other_owner
                                .is_some_and(|player| self.alliances.allied(owner, player))));
                let stamp = if lifts { now } else { expiry };
                let mut suppressed = tick
                    .state()
                    .get(ids.suppressed, entity)
                    .cloned()
                    .unwrap_or_default();
                for category in rule.anti.iter() {
                    suppressed.add(category, stamp);
                }
                tick.state_mut()
                    .insert(ids.suppressed, entity, suppressed)?;
                self.recompute_bonuses(tick, ids, entity)?;
                if let Some(fx) = &rule.anti_fx {
                    if let Some(at) = tick.state().get(ids.body, entity).map(|b| b.0.position) {
                        let power = crate::fx::FxSource::Power;
                        self.emit_fx_object_named(tick, ids, fx, at, power, entity, None);
                    }
                }
            }
            if let Some(fx) = &rule.modifier_fx {
                if !tick.state().contains(ids.horde, entity) {
                    if let Some(at) = tick.state().get(ids.body, entity).map(|b| b.0.position) {
                        let power = crate::fx::FxSource::Power;
                        self.emit_fx_object_named(tick, ids, fx, at, power, entity, None);
                    }
                }
            }
        }
        Ok(given)
    }

    pub(super) fn gain_power_list(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        id: mint_sage_combat::ModifierId,
    ) -> Result<()> {
        let Some(list) = self.combat.modifiers.get(id) else {
            return Ok(());
        };
        let (add, mult) = list.health_change();
        if add == 0.0 && mult == 1.0 {
            return Ok(());
        }
        if let Some(health) = tick.state_mut().get_mut(ids.health, entity) {
            health.0.apply_health_modifier(add, mult);
        }
        Ok(())
    }

    pub(super) fn expire_timed_modifiers(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let due: Vec<Entity> = tick
            .state()
            .store(ids.timed_modifiers)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, timed)| timed.0.iter().any(|(_, until)| *until <= now))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in due {
            let Some(mut timed) = tick.state().get(ids.timed_modifiers, entity).cloned() else {
                continue;
            };
            timed.expire(now);
            if timed.0.is_empty() {
                tick.state_mut().remove(ids.timed_modifiers, entity);
            } else {
                tick.state_mut()
                    .insert(ids.timed_modifiers, entity, timed)?;
            }
            self.recompute_bonuses(tick, ids, entity)?;
        }
        Ok(())
    }

    pub(super) fn expire_timed_conditions(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let due: Vec<Entity> = tick
            .state()
            .store(ids.timed_conditions)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, timed)| timed.0.iter().any(|(_, until)| *until <= now))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in due {
            let Some(mut timed) = tick.state().get(ids.timed_conditions, entity).cloned() else {
                continue;
            };
            let expired = timed.expire(now);
            if timed.0.is_empty() {
                tick.state_mut().remove(ids.timed_conditions, entity);
            } else {
                tick.state_mut()
                    .insert(ids.timed_conditions, entity, timed)?;
            }
            if expired.is_empty() {
                continue;
            }
            let remove: Vec<String> = expired.iter().map(ToString::to_string).collect();
            for flag in &remove {
                diag!(
                    "power: model state [{flag}] expired on entity {}",
                    entity.index()
                );
            }
            self.apply_effects(
                tick,
                ids,
                vec![(
                    entity,
                    Effect::ModelConditions {
                        add: mint_sage_modules::effects::names(Vec::new()),
                        remove: mint_sage_modules::effects::names(remove),
                    },
                )],
            )?;
        }
        Ok(())
    }

    pub(super) fn grant_player_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        bit: mint_sim::UpgradeBit,
    ) -> Result<()> {
        let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        else {
            return Ok(());
        };
        let mut mask = tick
            .state()
            .get(ids.player_upgrades, player_entity)
            .map_or_else(mint_sim::UpgradeMask::default, |held| held.0.clone());
        if mask.contains(bit) {
            return Ok(());
        }
        mask.set(bit);
        tick.state_mut()
            .insert(ids.player_upgrades, player_entity, PlayerUpgrades(mask))?;
        diag!(
            "upgrade: player {} granted bit {} by a power",
            owner.get(),
            bit.index()
        );
        let owned: Vec<Entity> = tick
            .state()
            .store(ids.kind)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, _)| {
                        tick.state()
                            .get(ids.owner, *entity)
                            .is_some_and(|held| held.0 == owner)
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in owned {
            self.offer_upgrades(tick, ids, entity)?;
        }
        Ok(())
    }

    pub(super) fn attach_lifetime(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: Kind,
    ) -> Result<()> {
        let Some((min, max, death)) = self.lifetimes.get(kind) else {
            return Ok(());
        };
        let delay = crate::summons::lifetime_delay(min, max, |span| {
            tick.rng().below(span.saturating_add(1))
        });
        let set_at = tick.frame();
        let dies_at = set_at.checked_add(delay).unwrap_or(Frame::FOREVER);
        tick.state_mut().insert(
            ids.lifetime,
            entity,
            crate::summons::Lifetime {
                dies_at,
                death,
                set_at: Some(set_at),
            },
        )?;
        Ok(())
    }

    pub(super) fn expire_lifetimes(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
    ) -> Result<Vec<(Entity, f32)>> {
        let now = tick.frame();
        let due: Vec<Entity> = tick
            .state()
            .store(ids.lifetime)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, lifetime)| lifetime.dies_at <= now)
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        let mut killed = Vec::new();
        for entity in due {
            let Some(lifetime) = tick.state_mut().remove(ids.lifetime, entity) else {
                continue;
            };
            killed.extend(self.kill(tick, ids, entity, None, lifetime.death)?);
        }
        Ok(killed)
    }

    pub(super) fn kill(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        killer: Option<Entity>,
        death: crate::combat::DeathNaming,
    ) -> Result<Option<(Entity, f32)>> {
        if tick.state().contains(ids.doomed, entity) {
            return Ok(None);
        }
        let (died, dealt) = match tick.state_mut().get_mut(ids.health, entity) {
            Some(health) => {
                let had = health.0.current();
                (health.0.kill(), had)
            }
            None => (true, 0.0),
        };
        if !died {
            return Ok(None);
        }
        tick.state_mut().insert(
            ids.killing_blow,
            entity,
            super::death::KillingBlow {
                killer,
                damage_type: self.combat.damage_types.lookup("UNRESISTABLE"),
                dealt,
                death,
                sub_type: None,
            },
        )?;
        Ok(Some((entity, 0.0)))
    }

    pub(super) fn death_option(
        &self,
        kind: Option<Kind>,
        index: u32,
    ) -> Option<&crate::summons::DeathOption> {
        self.death_plans
            .get(kind?)?
            .get(usize::try_from(index).ok()?)
    }

    pub(super) fn run_death_lists(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        lists: &[String],
    ) -> Result<()> {
        if lists.is_empty() {
            return Ok(());
        }
        let world = tick.state();
        let (Some(body), Some(owner)) = (
            world.get(ids.body, entity).map(|body| body.0),
            world.get(ids.owner, entity).map(|owner| owner.0),
        ) else {
            return Ok(());
        };
        for list in lists {
            let made = self.run_creation_list(
                tick,
                ids,
                entity,
                list,
                body.position,
                CreationVariant::At,
                owner,
                None,
                None,
            )?;
            diag!(
                "death: entity {} ran {list} - {made} object(s) for player {}",
                entity.index(),
                owner.get()
            );
        }
        Ok(())
    }

    pub(crate) fn spell_book(&self, world: &World, player: PlayerId) -> Option<SpellBook> {
        self.provisional_spell_book(world, player, &[])
    }

    pub(crate) fn provisional_spell_book(
        &self,
        world: &World,
        player: PlayerId,
        provisional: &[String],
    ) -> Option<SpellBook> {
        let ids = self.ids?;
        let mut state = std::borrow::Cow::Borrowed(self.science_of(world, ids, player)?);
        for id in provisional.iter().filter_map(|name| self.sciences.id(name)) {
            if let Ok(cost) = state.check_purchase(id, &self.sciences) {
                state.to_mut().purchase(id, cost);
            }
        }
        let state = state.as_ref();
        let plan = self.spell_plans.get(usize::from(player.get()))?;
        Some(SpellBook {
            points: state.purchase_points,
            rank: state.rank,
            skill_points: state.skill_points,
            next_rank_at: state.next_rank_at(self.ranks_of(player)),
            entries: plan
                .store
                .iter()
                .filter_map(|(button, id)| {
                    let science = self.sciences.get(*id)?;
                    Some(SpellEntry {
                        science: science.name.clone(),
                        id: id.get(),
                        button: button.clone(),
                        cost: science.cost,
                        state: crate::sciences::spell_state(state, &self.sciences, *id),
                    })
                })
                .collect(),
            held: state
                .sciences
                .iter()
                .filter_map(|id| self.sciences.get(*id))
                .map(|science| science.name.clone())
                .collect(),
        })
    }

    pub(crate) fn spell_census(&self, world: &World) -> Vec<String> {
        (0..self.players)
            .filter_map(|slot| {
                let player = PlayerId::new(slot);
                let book = self.spell_book(world, player)?;
                let owned: Vec<&str> = book
                    .entries
                    .iter()
                    .filter(|entry| entry.state == crate::sciences::SpellState::Owned)
                    .map(|entry| entry.science.as_str())
                    .collect();
                Some(format!(
                    "spell book: player {slot} rank {} ({} skill point(s), next rank at {}), {} \
                     power point(s), {} of {} store science(s) held {owned:?} of {} known",
                    book.rank,
                    book.skill_points,
                    book.next_rank_at
                        .map_or_else(|| String::from("the cap"), |at| at.to_string()),
                    book.points,
                    owned.len(),
                    book.entries.len(),
                    self.sciences.len()
                ))
            })
            .collect()
    }

    pub(crate) fn purchase_command(&self, science: &str) -> Option<RotwkCommand> {
        self.sciences
            .id(science)
            .map(|id| RotwkCommand::PurchaseScience { science: id.get() })
    }

    pub(super) fn step_ai_spells(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
    ) -> Result<()> {
        if tick.frame().get() % Self::AI_ECONOMY_INTERVAL_FRAMES != 0 {
            return Ok(());
        }
        let now = tick.frame();
        let Some(state) = self.science_of(tick.state(), ids, player).cloned() else {
            return Ok(());
        };
        if let Some(plan) = self.spell_plans.get(usize::from(player.get())) {
            let buy = plan
                .store
                .iter()
                .find(|(_, id)| state.check_purchase(*id, &self.sciences).is_ok())
                .map(|(_, id)| *id);
            if let Some(id) = buy {
                self.apply(
                    tick,
                    mint_sim::CommandKey::new(now, player, 5),
                    &RotwkCommand::PurchaseScience { science: id.get() },
                )?;
            }
        }
        let Some(book) = state.book else {
            return Ok(());
        };
        let Some(spells) = tick
            .state()
            .get(ids.kind, book)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| entry.spell_buttons.clone())
        else {
            return Ok(());
        };
        let castable: Vec<(u8, TargetKind)> = spells
            .iter()
            .enumerate()
            .filter(|(_, spell)| !spell.passive && spell.target != TargetKind::Object)
            .filter_map(|(at, spell)| Some((u8::try_from(at).ok()?, spell.target)))
            .filter(|(button, _)| self.spell_ready(tick.state(), player, *button, now))
            .collect();
        let Some(&(button, target)) = castable.first() else {
            return Ok(());
        };
        let living = self.living(tick, ids);
        let grid = crate::combat::TargetGrid::build(&living);
        let range = Self::AI_POWER_DETECTION_RANGE;
        let cells = crate::combat::grid_radius_cells(range);
        let mut near: Vec<u32> = Vec::new();
        let anchor = living
            .iter()
            .filter(|(_, owner)| *owner == player)
            .find(|(own, _)| {
                near.clear();
                grid.query_into(
                    crate::combat::grid_cell(own.position.x),
                    crate::combat::grid_cell(own.position.y),
                    cells,
                    &mut near,
                );
                near.iter().any(|index| {
                    living
                        .get(usize::try_from(*index).unwrap_or(usize::MAX))
                        .is_some_and(|(foe, _)| {
                            let dx = foe.position.x - own.position.x;
                            let dy = foe.position.y - own.position.y;
                            self.object_hostile(tick.state(), own.entity, foe.entity)
                                && dx * dx + dy * dy <= range * range
                        })
                })
            })
            .map(|(own, _)| own.position);
        let Some(anchor) = anchor else {
            return Ok(());
        };
        let at = (target == TargetKind::Position).then_some((anchor.x, anchor.y));
        self.apply(
            tick,
            mint_sim::CommandKey::new(now, player, 6),
            &RotwkCommand::CastSpell {
                button,
                target: None,
                at,
            },
        )
    }

    pub(crate) fn spell_target_kind(
        &self,
        world: &World,
        player: PlayerId,
        button: u8,
    ) -> Option<TargetKind> {
        let ids = self.ids?;
        let book = self.science_of(world, ids, player)?.book?;
        let kind = world.get(ids.kind, book)?;
        self.kinds
            .get(*kind)?
            .spell_buttons
            .get(usize::from(button))
            .map(|spell| spell.target)
    }

    pub(crate) fn spell_ready(
        &self,
        world: &World,
        player: PlayerId,
        button: u8,
        now: Frame,
    ) -> bool {
        let Some(ids) = self.ids else {
            return false;
        };
        let Some(book) = self.science_of(world, ids, player).and_then(|s| s.book) else {
            return false;
        };
        let Some(kind) = world.get(ids.kind, book).copied() else {
            return false;
        };
        let Some(spell) = self
            .kinds
            .get(kind)
            .and_then(|entry| entry.spell_buttons.get(usize::from(button)))
        else {
            return false;
        };
        if spell.passive
            || !self.power_science_met(world, ids, player, &spell.power)
            || !self.power_requirement_met(world, ids, kind, player, &spell.power)
        {
            return false;
        }
        self.power_clock(world, ids, book, &spell.power, now)
            .is_none()
    }

    pub(crate) fn power_clock(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        power: &str,
        now: Frame,
    ) -> Option<u8> {
        let power_id = self.power_names.id(power)?;
        let &(_, shared) = self.power_reload.get(usize::from(power_id.get()))?;
        let (available_on, reload) = if shared {
            let player = world.get(ids.owner, source)?.0;
            let pe = crate::economy::player_entity(world, Self::economy_ids(ids), player)?;
            world
                .get(ids.player_power_cooldowns, pe)?
                .cooldown(power_id)?
        } else {
            let instance = world.get(ids.power_book, source)?.instance(power_id)?;
            (instance.available_on, instance.reload)
        };
        let remaining = available_on
            .since(now)
            .filter(|r| *r != LogicFrames::ZERO)?;
        #[allow(clippy::cast_precision_loss)]
        let fraction = remaining.get() as f32 / reload.get().max(1) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(((1.0 - fraction.clamp(0.0, 1.0)) * 100.0).round() as u8)
    }

    pub(crate) fn spell_bar_state(
        &self,
        world: &World,
        player: PlayerId,
        button: u8,
        now: Frame,
    ) -> Option<(SpellBarReadiness, String)> {
        let ids = self.ids?;
        let book = self.science_of(world, ids, player)?.book?;
        let kind = world.get(ids.kind, book).copied()?;
        let spell = self
            .kinds
            .get(kind)?
            .spell_buttons
            .get(usize::from(button))?;
        if !spell.shown || !self.power_science_met(world, ids, player, &spell.power) {
            return None;
        }
        let readiness = if self.power_requirement_met(world, ids, kind, player, &spell.power) {
            match self.power_clock(world, ids, book, &spell.power, now) {
                Some(percent) => SpellBarReadiness::OnCooldown { percent },
                None if spell.passive => SpellBarReadiness::Static,
                None => SpellBarReadiness::Ready,
            }
        } else {
            SpellBarReadiness::Disabled
        };
        Some((readiness, spell.button.clone()))
    }

    pub(crate) fn rank_progress(&self, world: &World, player: PlayerId) -> f32 {
        let Some(ids) = self.ids else {
            return 1.0;
        };
        let Some(state) = self.science_of(world, ids, player) else {
            return 1.0;
        };
        let Some(next) = state.next_rank_at(self.ranks_of(player)) else {
            return 1.0;
        };
        let start = self.ranks_of(player).needed(state.rank).unwrap_or(0);
        let span = next.saturating_sub(start).max(1);
        let progressed = state.skill_points.saturating_sub(start);
        #[allow(clippy::cast_precision_loss)]
        let fraction = progressed as f32 / span as f32;
        fraction.clamp(0.0, 1.0)
    }

    pub(crate) fn build_progress(
        &self,
        world: &World,
        structure: Entity,
        button: u8,
    ) -> Option<f32> {
        let ids = self.ids?;
        let producer = world.get(ids.producer, structure)?;
        let item = *self
            .menus
            .get(usize::from(producer.menu?))?
            .get(usize::from(button))?;
        producer
            .queue
            .first_of(ItemTable::Objects, ItemId::new(u32::from(item)))
            .map(|entry| entry.fraction())
    }

    pub(crate) fn build_count(&self, world: &World, structure: Entity, button: u8) -> usize {
        let Some(ids) = self.ids else {
            return 0;
        };
        let Some(producer) = world.get(ids.producer, structure) else {
            return 0;
        };
        let Some(item) = producer
            .menu
            .and_then(|menu| self.menus.get(usize::from(menu)))
            .and_then(|menu| menu.get(usize::from(button)))
        else {
            return 0;
        };
        let item = ItemId::new(u32::from(*item));
        producer
            .queue
            .entries()
            .iter()
            .filter(|entry| entry.makes(ItemTable::Objects, item))
            .count()
    }

    pub(crate) fn object_upgrade_progress(
        &self,
        world: &World,
        structure: Entity,
        button: u8,
    ) -> Option<f32> {
        let ids = self.ids?;
        let upgrade_producer = world.get(ids.producer, structure)?;
        let item = *self
            .object_upgrade_menus
            .get(usize::from(upgrade_producer.object_menu?))?
            .get(usize::from(button))?;
        upgrade_producer
            .queue
            .first_of(ItemTable::Upgrades, ItemId::new(u32::from(item)))
            .map(|entry| entry.fraction())
    }

    pub(crate) fn player_upgrade_progress(
        &self,
        world: &World,
        structure: Entity,
        button: u8,
    ) -> Option<f32> {
        let ids = self.ids?;
        let upgrade_producer = world.get(ids.producer, structure)?;
        let item = *self
            .player_upgrade_menus
            .get(usize::from(upgrade_producer.player_menu?))?
            .get(usize::from(button))?;
        upgrade_producer
            .queue
            .first_of(ItemTable::Upgrades, ItemId::new(u32::from(item)))
            .map(|entry| entry.fraction())
    }
}
