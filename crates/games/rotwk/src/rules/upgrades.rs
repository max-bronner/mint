use mint_sage_combat::{
    best_match, ArmorId, CommandSource, ConditionMask, ConditionTable, SlotArm, SlotCandidate,
    SlotFitness, WeaponId, WeaponState, WeaponStatus, SLOT_COUNT,
};
use mint_sage_modules::{
    AIGateUpdate, AimWeaponBehavior, AutoAbilityBehavior, Changed, CitadelSlaughterHordeContain,
    DualWeaponBehavior, DynamicPortalBehaviour, Effect, EffectLog, FakePathfindPortalBehaviour,
    GateOpenAndCloseBehavior, HitReactionBehavior, HordeGarrisonContain, LargeGroupBonusUpdate,
    RebuildHoleBehavior, RebuildHoleExposeDie, RefundDie, SiegeEngineContain,
    SlaughterHordeContain, SlavedTo, SpawnBehavior, StructureCollapseUpdate, TransportContain,
    TunnelContain, UpgradeBits, UpgradeState, WallHubBehavior,
};
use mint_sim::module::{DestroyRequests, ModuleColumn, ModuleRegistry, ModuleSlot, OwnerChange};
use mint_sim::{
    Component, ComponentId, Effects, Entity, Frame, Pcg32, Prototype, PrototypeOutcome, Result,
    StateHasher, Tick, UpgradeBit, UpgradeMask, World,
};

use super::emotions::{KindEmotions, TRACKER_MODULE};
use super::{Ids, PlayerOrder, RotwkRules};
use crate::combat::{armour_of, Armed, Armour, WeaponSlots};
use crate::diag::diag;
use crate::economy::{ObjectUpgrades, PlayerUpgrades, Producer};
use crate::templates::KindTable;
use crate::unit::Kind;

pub(crate) struct ModuleWiring {
    pub(super) registry: ModuleRegistry,
    prototypes: Vec<Vec<Prototype>>,
    sets: Vec<KindSets>,
    emotions: Vec<Option<KindEmotions>>,
    pub(crate) state: ComponentId<UpgradeState>,
    pub(crate) collapse_state: ComponentId<ModuleColumn<StructureCollapseUpdate>>,
    pub(crate) spawn_state: ComponentId<ModuleColumn<SpawnBehavior>>,
    pub(crate) expose_die_state: ComponentId<ModuleColumn<RebuildHoleExposeDie>>,
    pub(crate) hole_state: ComponentId<ModuleColumn<RebuildHoleBehavior>>,
    pub(crate) refund_die_state: ComponentId<ModuleColumn<RefundDie>>,
    pub(crate) slaved_to: ComponentId<SlavedTo>,
    pub(crate) auto_ability_state: ComponentId<ModuleColumn<AutoAbilityBehavior>>,
    pub(crate) portal_state: ComponentId<ModuleColumn<FakePathfindPortalBehaviour>>,
    pub(crate) gate_cycle_state: ComponentId<ModuleColumn<GateOpenAndCloseBehavior>>,
    pub(crate) gate_trigger_state: ComponentId<ModuleColumn<AIGateUpdate>>,
    pub(crate) wall_hub_state: ComponentId<ModuleColumn<WallHubBehavior>>,
    pub(crate) replace_state: ComponentId<ModuleColumn<mint_sage_modules::ReplaceSelfUpgrade>>,
    pub(crate) emotion_state: ComponentId<ModuleColumn<mint_sage_modules::EmotionTrackerUpdate>>,
    garrison_state: ComponentId<ModuleColumn<HordeGarrisonContain>>,
    citadel_state: ComponentId<ModuleColumn<CitadelSlaughterHordeContain>>,
    slaughter_state: ComponentId<ModuleColumn<SlaughterHordeContain>>,
    tunnel_state: ComponentId<ModuleColumn<TunnelContain>>,
    transport_state: ComponentId<ModuleColumn<TransportContain>>,
    pub(crate) dual_weapon_state: ComponentId<ModuleColumn<DualWeaponBehavior>>,
    pub(crate) aim_state: ComponentId<ModuleColumn<AimWeaponBehavior>>,
    pub(crate) group_bonus_state: ComponentId<ModuleColumn<LargeGroupBonusUpdate>>,
    pub(crate) hit_reaction_state: ComponentId<ModuleColumn<HitReactionBehavior>>,
    pub(crate) ability_hold: ComponentId<AbilityHold>,
    mount_state: ComponentId<ModuleColumn<mint_sage_modules::ToggleMountedSpecialAbilityUpdate>>,
    pub(crate) spawn_point_state:
        ComponentId<ModuleColumn<mint_sage_modules::SpawnPointProductionExitUpdate>>,
    pub(crate) production_state: ComponentId<ModuleColumn<mint_sage_modules::ProductionUpdate>>,
    pub(crate) production_doors: ComponentId<mint_sage_modules::ProductionDoors>,
    pub(crate) pick_up_state: ComponentId<ModuleColumn<mint_sage_modules::AutoPickUpUpdate>>,
    pub(crate) pick_up_order: ComponentId<super::pick_up::PickUpOrder>,
    pub(crate) eating: ComponentId<super::pick_up::Eating>,
    pub(crate) ability_state: ComponentId<ModuleColumn<mint_sage_modules::SpecialAbilityUpdate>>,
    pub(crate) dynamic_portal_state: ComponentId<ModuleColumn<DynamicPortalBehaviour>>,
    siege_state: ComponentId<ModuleColumn<SiegeEngineContain>>,
    pub(crate) ai_power_state: ComponentId<ModuleColumn<mint_sage_modules::AISpecialPowerUpdate>>,
    pub(crate) salvage_crate_state:
        ComponentId<ModuleColumn<mint_sage_modules::SalvageCrateCollide>>,
    pub(crate) attach_state: ComponentId<ModuleColumn<mint_sage_modules::AttachUpdate>>,
    disguise_state: ComponentId<ModuleColumn<mint_sage_modules::SpecialDisguiseUpdate>>,
    pub(crate) tdfx_state: ComponentId<ModuleColumn<mint_sage_modules::TransitionDamageFX>>,
    pub(crate) report: WiringReport,
}

impl core::fmt::Debug for ModuleWiring {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ModuleWiring")
            .field("report", &self.report)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WiringReport {
    pub(crate) kinds: usize,
    pub(crate) prototypes: usize,
    pub(crate) refused: usize,
    pub(crate) hand_rolled: usize,
}

#[derive(Debug, Clone, Default)]
struct KindSets {
    conditions: ConditionTable,
    weapons: Vec<(ConditionMask, SlotSet)>,
    armors: Vec<(ConditionMask, Option<ArmorId>)>,
    kind_mask: ConditionMask,
    locomotor_sets: Vec<(String, crate::unit::Kinematics)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct SlotSet {
    pub(crate) arms: [Option<SlotArm>; SLOT_COUNT],
}

impl SlotSet {
    pub(crate) fn first_weapon(&self) -> Option<(usize, WeaponId)> {
        self.arms
            .iter()
            .enumerate()
            .find_map(|(at, arm)| Some((at, arm.as_ref()?.weapon)))
    }
}

impl KindSets {
    fn active(&self, flags: &[mint_sage_modules::effects::Name]) -> ConditionMask {
        flags.iter().fold(ConditionMask::NONE, |mask, flag| {
            mask.union(self.conditions.lookup(flag))
        })
    }
}

fn column_entities<T: mint_sim::ModuleKind>(
    world: &World,
    column: ComponentId<ModuleColumn<T>>,
) -> Vec<Entity> {
    let Some(store) = world.store(column) else {
        return Vec::new();
    };
    let mut out: Vec<Entity> = store
        .entities()
        .iter()
        .copied()
        .filter(|entity| world.is_live(*entity))
        .collect();
    out.sort_unstable_by_key(|entity| entity.index());
    out
}

fn first<T: mint_sim::ModuleKind>(
    world: &World,
    column: ComponentId<ModuleColumn<T>>,
    entity: Entity,
) -> Option<&T> {
    world.get(column, entity)?.0.first().map(|slot| &slot.state)
}

impl ModuleWiring {
    pub(crate) fn setup(
        world: &mut World,
        kinds: &KindTable,
        bits: &UpgradeBits,
        combat: &mint_sage_combat::CombatData,
        emotion_index: &mint_sage_ini::EmotionIndex,
    ) -> Self {
        let state = world.register::<UpgradeState>();
        let mut registry = ModuleRegistry::new(world);
        mint_sage_modules::register_live_families(&mut registry, world);
        let collapse_state = world.register::<ModuleColumn<StructureCollapseUpdate>>();
        let spawn_state = world.register::<ModuleColumn<SpawnBehavior>>();
        let expose_die_state = world.register::<ModuleColumn<RebuildHoleExposeDie>>();
        let hole_state = world.register::<ModuleColumn<RebuildHoleBehavior>>();
        let refund_die_state = world.register::<ModuleColumn<RefundDie>>();
        let slaved_to = world.register::<SlavedTo>();
        let auto_ability_state = world.register::<ModuleColumn<AutoAbilityBehavior>>();
        let portal_state = world.register::<ModuleColumn<FakePathfindPortalBehaviour>>();
        let gate_cycle_state = world.register::<ModuleColumn<GateOpenAndCloseBehavior>>();
        let gate_trigger_state = world.register::<ModuleColumn<AIGateUpdate>>();
        let wall_hub_state = world.register::<ModuleColumn<WallHubBehavior>>();
        let replace_state = world.register::<ModuleColumn<mint_sage_modules::ReplaceSelfUpgrade>>();
        let emotion_state =
            world.register::<ModuleColumn<mint_sage_modules::EmotionTrackerUpdate>>();
        let garrison_state = world.register::<ModuleColumn<HordeGarrisonContain>>();
        let citadel_state = world.register::<ModuleColumn<CitadelSlaughterHordeContain>>();
        let slaughter_state = world.register::<ModuleColumn<SlaughterHordeContain>>();
        let tunnel_state = world.register::<ModuleColumn<TunnelContain>>();
        let transport_state = world.register::<ModuleColumn<TransportContain>>();
        let dual_weapon_state = world.register::<ModuleColumn<DualWeaponBehavior>>();
        let aim_state = world.register::<ModuleColumn<AimWeaponBehavior>>();
        let group_bonus_state = world.register::<ModuleColumn<LargeGroupBonusUpdate>>();
        let hit_reaction_state = world.register::<ModuleColumn<HitReactionBehavior>>();
        let ability_hold = world.register::<AbilityHold>();
        let mount_state =
            world.register::<ModuleColumn<mint_sage_modules::ToggleMountedSpecialAbilityUpdate>>();
        let spawn_point_state =
            world.register::<ModuleColumn<mint_sage_modules::SpawnPointProductionExitUpdate>>();
        let production_state =
            world.register::<ModuleColumn<mint_sage_modules::ProductionUpdate>>();
        let production_doors = world.register::<mint_sage_modules::ProductionDoors>();
        let pick_up_state = world.register::<ModuleColumn<mint_sage_modules::AutoPickUpUpdate>>();
        let pick_up_order = world.register::<super::pick_up::PickUpOrder>();
        let eating = world.register::<super::pick_up::Eating>();
        let ability_state =
            world.register::<ModuleColumn<mint_sage_modules::SpecialAbilityUpdate>>();
        let dynamic_portal_state = world.register::<ModuleColumn<DynamicPortalBehaviour>>();
        let siege_state = world.register::<ModuleColumn<SiegeEngineContain>>();
        let ai_power_state =
            world.register::<ModuleColumn<mint_sage_modules::AISpecialPowerUpdate>>();
        let salvage_crate_state =
            world.register::<ModuleColumn<mint_sage_modules::SalvageCrateCollide>>();
        let attach_state = world.register::<ModuleColumn<mint_sage_modules::AttachUpdate>>();
        let disguise_state =
            world.register::<ModuleColumn<mint_sage_modules::SpecialDisguiseUpdate>>();
        let tdfx_state = world.register::<ModuleColumn<mint_sage_modules::TransitionDamageFX>>();
        let mut report = WiringReport {
            kinds: kinds.len(),
            ..WiringReport::default()
        };
        let mut prototypes = Vec::with_capacity(kinds.len());
        let mut sets = Vec::with_capacity(kinds.len());
        let mut emotions = Vec::with_capacity(kinds.len());
        let mut kind_vocabulary = ConditionTable::new();
        for index in 0..kinds.len() {
            let Some(entry) = u16::try_from(index).ok().and_then(|k| kinds.get(Kind(k))) else {
                continue;
            };
            for set in &entry.record.weapon_sets {
                for slot in &set.slots {
                    for kind in slot.preferred_against.iter().chain(&slot.only_against) {
                        kind_vocabulary.intern(kind);
                    }
                }
            }
        }
        for index in 0..kinds.len() {
            let Some(entry) = u16::try_from(index).ok().and_then(|k| kinds.get(Kind(k))) else {
                prototypes.push(Vec::new());
                sets.push(KindSets::default());
                emotions.push(None);
                continue;
            };
            let mut built = Vec::new();
            let mut kind_emotions = None;
            for module in &entry.record.declared_modules {
                let source = module.source(bits);
                match registry.prototype(&module.name, &source, Frame::ZERO) {
                    Ok(PrototypeOutcome::Built(prototype)) => {
                        report.prototypes = report.prototypes.saturating_add(1);
                        built.push(prototype);
                        if kind_emotions.is_none()
                            && module.name.eq_ignore_ascii_case(TRACKER_MODULE)
                        {
                            kind_emotions = mint_sage_modules::emotion_tracker_update_from_fields(
                                &source,
                                Frame::ZERO,
                            )
                            .ok()
                            .map(|tracker| KindEmotions::build(&tracker, emotion_index));
                        }
                    }
                    Ok(PrototypeOutcome::Unregistered) => {
                        report.hand_rolled = report.hand_rolled.saturating_add(1);
                    }
                    Ok(PrototypeOutcome::Refused) | Err(_) => {
                        report.refused = report.refused.saturating_add(1);
                    }
                }
            }
            prototypes.push(built);
            emotions.push(kind_emotions);
            sets.push(kind_sets(
                &entry.record,
                combat,
                &entry.locomotor_sets,
                &kind_vocabulary,
            ));
        }
        Self {
            registry,
            prototypes,
            sets,
            emotions,
            state,
            collapse_state,
            spawn_state,
            expose_die_state,
            hole_state,
            refund_die_state,
            slaved_to,
            auto_ability_state,
            portal_state,
            gate_cycle_state,
            gate_trigger_state,
            wall_hub_state,
            replace_state,
            emotion_state,
            garrison_state,
            citadel_state,
            slaughter_state,
            tunnel_state,
            transport_state,
            dual_weapon_state,
            aim_state,
            group_bonus_state,
            hit_reaction_state,
            ability_hold,
            mount_state,
            spawn_point_state,
            production_state,
            production_doors,
            pick_up_state,
            pick_up_order,
            eating,
            ability_state,
            dynamic_portal_state,
            siege_state,
            ai_power_state,
            salvage_crate_state,
            attach_state,
            disguise_state,
            tdfx_state,
            report,
        }
    }

    pub(super) fn kind_emotions(&self, kind: Kind) -> Option<&KindEmotions> {
        self.emotions.get(usize::from(kind.0))?.as_ref()
    }

    pub(super) fn adopt_created(
        &self,
        world: &mut World,
        creator: Entity,
        requester: mint_sim::Requester,
        product: Entity,
        now: Frame,
    ) -> Result<()> {
        self.registry
            .adopt_created(world, creator, requester, product, now)
    }

    pub(super) fn product_died(&self, world: &mut World, product: Entity, now: Frame) {
        if let Some(link) = self.registry.created_by(world, product) {
            diag!(
                "spawn: entity {} of entity {} died at frame {}",
                product.index(),
                link.creator.index(),
                now.get()
            );
        }
        self.registry.product_died(world, product, now);
    }

    pub(super) fn spawners(&self, world: &World) -> Vec<Entity> {
        world
            .store(self.spawn_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default()
    }

    pub(super) fn reclaimable_spawner<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a SpawnBehavior> {
        first(world, self.spawn_state, entity).filter(|module| module.can_reclaim_orphans())
    }

    pub(super) fn created_by(
        &self,
        world: &World,
        product: Entity,
    ) -> Option<mint_sim::module::CreatedBy> {
        self.registry.created_by(world, product)
    }

    pub(crate) fn describe(&self) -> String {
        format!(
            "modules: {} upgrade- and power-family declaration(s) built into prototypes over {} \
             kind(s), {} refused, {} left to hand-rolled components",
            self.report.prototypes, self.report.kinds, self.report.refused, self.report.hand_rolled,
        )
    }

    pub(super) fn activate(
        &self,
        world: &mut World,
        entity: Entity,
        activation: &mint_sim::Activation<'_>,
        now: Frame,
        effects: Effects<'_>,
    ) -> bool {
        self.registry
            .activate(world, entity, activation, now, effects)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn collect_pending_approach_checks(&self, world: &World) -> Vec<(Entity, Entity)> {
        self.registry.collect_pending_approach_checks(world)
    }

    pub(super) fn end_other_activations(
        &self,
        world: &mut World,
        entity: Entity,
        new_power: &str,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        effects: Effects<'_>,
    ) {
        self.registry
            .end_other_activations(world, entity, new_power, now, rng, destroy, effects);
    }

    pub(super) fn broadcast_die(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        effects: Effects<'_>,
        death: mint_sim::module::Death<'_>,
    ) {
        self.registry
            .broadcast_die_into(world, entity, now, rng, destroy, effects, death);
    }

    pub(super) fn broadcast_capture(
        &self,
        world: &mut World,
        entity: Entity,
        (old, new): (mint_sim::PlayerId, mint_sim::PlayerId),
        reason: OwnerChange,
        now: Frame,
        effects: Effects<'_>,
    ) {
        self.registry
            .broadcast_capture(world, entity, old, new, reason, now, effects);
    }

    pub(super) fn broadcast_upgrade_removed(
        &self,
        world: &mut World,
        entity: Entity,
        removed: &UpgradeMask,
        effects: Effects<'_>,
    ) {
        self.registry
            .broadcast_upgrade_removed(world, entity, removed, effects);
    }

    pub(super) fn attach_for_death(
        &self,
        world: &mut World,
        entity: Entity,
        kind: Kind,
        now: Frame,
    ) -> Result<()> {
        for prototype in self
            .prototypes
            .get(usize::from(kind.0))
            .map_or(&[][..], Vec::as_slice)
        {
            self.registry
                .attach_prototype(world, entity, now, prototype)?;
        }
        Ok(())
    }

    pub(super) fn begin_collapse(
        &self,
        world: &mut World,
        entity: Entity,
        gravity_per_frame_squared: f32,
        geometry_height: f32,
    ) -> bool {
        let Some(column) = world.get_mut(self.collapse_state, entity) else {
            return false;
        };
        let mut started = false;
        for slot in column
            .0
            .iter_mut()
            .filter(|slot| slot.state.is_collapsing())
        {
            slot.state
                .set_physics(gravity_per_frame_squared, geometry_height);
            started = true;
        }
        started
    }

    pub(crate) fn attach_view<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a mint_sage_modules::AttachUpdate> {
        first(world, self.attach_state, entity)
    }

    pub(crate) fn collapse_view(
        &self,
        world: &World,
        entity: Entity,
    ) -> Option<(mint_sage_modules::CollapseState, f32, f32)> {
        let column = world.get(self.collapse_state, entity)?;
        column
            .0
            .iter()
            .find(|slot| slot.state.is_collapsing())
            .map(|slot| {
                (
                    slot.state.state(),
                    slot.state.sink(),
                    slot.state.max_shudder(),
                )
            })
    }

    pub(crate) fn disguise_fx<'a>(&self, world: &'a World, entity: Entity) -> Option<&'a str> {
        first(world, self.disguise_state, entity)?.disguise_fx()
    }

    pub(crate) fn disguise_as_template<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a str> {
        first(world, self.disguise_state, entity)?.disguise_as_template()
    }

    pub(crate) fn disguise_enemy_perspective_template<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a str> {
        first(world, self.disguise_state, entity)?.enemy_perspective_template()
    }

    pub(crate) fn disguise_opacity(&self, world: &World, entity: Entity) -> f32 {
        first(world, self.disguise_state, entity)
            .map_or(1.0, mint_sage_modules::SpecialDisguiseUpdate::opacity)
    }

    pub(crate) fn is_disguised(&self, world: &World, entity: Entity) -> bool {
        world
            .get(self.state, entity)
            .is_some_and(|state| state.has_model_condition(mint_sage_modules::DISGUISED))
    }

    pub(crate) fn cancels_disguise_on_dismount(&self, world: &World, entity: Entity) -> bool {
        first(world, self.mount_state, entity)
            .is_some_and(|mount| mount.cancel_disguise_when_dismounting())
    }

    pub(crate) fn tdfx_view<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a mint_sage_modules::TransitionDamageFX> {
        world
            .get(self.tdfx_state, entity)?
            .0
            .first()
            .map(|slot| &slot.state)
    }

    pub(crate) fn mount_opacity(&self, world: &World, entity: Entity) -> f32 {
        world.get(self.mount_state, entity).map_or(1.0, |column| {
            column
                .0
                .iter()
                .map(|slot| slot.state.opacity())
                .fold(1.0, f32::min)
        })
    }

    pub(crate) fn expose_die<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a RebuildHoleExposeDie> {
        world
            .get(self.expose_die_state, entity)?
            .0
            .first()
            .map(|slot| &slot.state)
    }

    pub(crate) fn rebuild_hole<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a RebuildHoleBehavior> {
        world
            .get(self.hole_state, entity)?
            .0
            .first()
            .map(|slot| &slot.state)
    }

    pub(crate) fn portal_entities(&self, world: &World) -> Vec<Entity> {
        world
            .store(self.portal_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default()
    }

    pub(crate) fn portal<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a FakePathfindPortalBehaviour> {
        world
            .get(self.portal_state, entity)?
            .0
            .first()
            .map(|slot| &slot.state)
    }

    pub(crate) fn gate_cycle<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a GateOpenAndCloseBehavior> {
        world
            .get(self.gate_cycle_state, entity)?
            .0
            .first()
            .map(|slot| &slot.state)
    }

    pub(crate) fn dynamic_portal_entities(&self, world: &World) -> Vec<Entity> {
        world
            .store(self.dynamic_portal_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default()
    }

    pub(crate) fn dynamic_portal<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a DynamicPortalBehaviour> {
        world
            .get(self.dynamic_portal_state, entity)?
            .0
            .first()
            .map(|slot| &slot.state)
    }

    pub(crate) fn gate_trigger(&self, world: &World, entity: Entity) -> Option<AIGateUpdate> {
        world
            .get(self.gate_trigger_state, entity)?
            .0
            .first()
            .map(|slot| slot.state)
    }

    pub(crate) fn wall_hubs<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> &'a [ModuleSlot<WallHubBehavior>] {
        world
            .get(self.wall_hub_state, entity)
            .map_or(&[], |column| column.0.as_slice())
    }

    pub(crate) fn transport_view<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a TransportContain> {
        first(world, self.transport_state, entity)
    }

    pub(crate) fn is_tunnel(&self, world: &World, entity: Entity) -> bool {
        first(world, self.tunnel_state, entity).is_some()
    }

    pub(crate) fn is_garrison(&self, world: &World, entity: Entity) -> bool {
        first(world, self.garrison_state, entity).is_some()
    }

    pub(crate) fn tunnels(&self, world: &World) -> Vec<Entity> {
        column_entities(world, self.tunnel_state)
    }

    pub(crate) fn transports(&self, world: &World) -> Vec<Entity> {
        column_entities(world, self.transport_state)
    }

    pub(crate) fn sieges(&self, world: &World) -> Vec<Entity> {
        column_entities(world, self.siege_state)
    }

    pub(crate) fn contain_view<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<super::contain::ContainView<'a>> {
        if let Some(module) = first(world, self.garrison_state, entity) {
            return Some(super::contain::ContainView {
                open: module.open(),
                capacity: module.open().contain_max(),
                cash_back: 0.0,
                ring: None,
                garrison_family: true,
            });
        }
        if let Some(module) = first(world, self.citadel_state, entity) {
            return Some(super::contain::ContainView {
                open: module.open(),
                capacity: module.open().contain_max(),
                cash_back: module.cash_back_percent(),
                ring: module.ring(),
                garrison_family: true,
            });
        }
        if let Some(module) = first(world, self.slaughter_state, entity) {
            return Some(super::contain::ContainView {
                open: module.open(),
                capacity: module.open().contain_max(),
                cash_back: module.cash_back_percent(),
                ring: None,
                garrison_family: true,
            });
        }
        if let Some(module) = first(world, self.tunnel_state, entity) {
            return Some(super::contain::ContainView {
                open: module.open(),
                capacity: module.open().contain_max(),
                cash_back: 0.0,
                ring: None,
                garrison_family: true,
            });
        }
        if let Some(module) = first(world, self.transport_state, entity) {
            return Some(super::contain::ContainView {
                open: module.open(),
                capacity: if module.open().contain_max() > 0 {
                    module.open().contain_max()
                } else {
                    module.slots()
                },
                cash_back: 0.0,
                ring: None,
                garrison_family: false,
            });
        }
        first(world, self.siege_state, entity).map(|module| super::contain::ContainView {
            open: module.open(),
            capacity: if module.open().contain_max() > 0 {
                module.open().contain_max()
            } else {
                module.crew_max()
            },
            cash_back: 0.0,
            ring: None,
            garrison_family: false,
        })
    }

    pub(crate) fn siege_contain<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a SiegeEngineContain> {
        first(world, self.siege_state, entity)
    }

    pub(crate) fn carries_status(&self, world: &World, entity: Entity, status: &str) -> bool {
        world.get(self.state, entity).is_some_and(|held| {
            held.status_bits
                .iter()
                .any(|bit| bit.eq_ignore_ascii_case(status))
        })
    }

    pub(crate) fn refund_die<'a>(&self, world: &'a World, entity: Entity) -> Option<&'a RefundDie> {
        world
            .get(self.refund_die_state, entity)?
            .0
            .first()
            .map(|slot| &slot.state)
    }

    pub(super) fn update(
        &self,
        world: &mut World,
        now: Frame,
        rng: &mut mint_sim::Pcg32,
        destroy: &mut mint_sim::module::DestroyRequests,
        effects: Effects<'_>,
        snapshots: &[&dyn std::any::Any],
    ) {
        self.registry.update_with_reads(
            world,
            now,
            rng,
            destroy,
            effects,
            mint_sim::module::Reads::from_snapshots(snapshots),
        );
    }
}

fn set_upgrade_menus(producer: &mut Producer, object: Option<u16>, player: Option<u16>) {
    producer.object_menu = object;
    producer.player_menu = player;
}

fn weapon_set_conditions(state: &UpgradeState) -> Vec<mint_sage_modules::effects::Name> {
    state
        .model_conditions
        .iter()
        .cloned()
        .chain(state.weapon_set_flags.iter().cloned())
        .collect()
}

fn armor_set_conditions(state: &UpgradeState) -> Vec<mint_sage_modules::effects::Name> {
    state
        .model_conditions
        .iter()
        .cloned()
        .chain(state.armor_set_flags.iter().cloned())
        .collect()
}

const STANDING_STILL: f32 = 0.05;

pub(super) fn is_moving(tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
    tick.state().contains(ids.path, entity)
        || tick
            .state()
            .get(ids.body, entity)
            .is_some_and(|body: &crate::unit::Body| body.0.speed.abs() > STANDING_STILL)
}

fn kind_sets(
    record: &mint_sage_modules::ObjectRecord,
    combat: &mint_sage_combat::CombatData,
    locomotor_sets: &[(String, crate::unit::Kinematics)],
    kind_vocabulary: &ConditionTable,
) -> KindSets {
    let mut sets = KindSets {
        locomotor_sets: locomotor_sets.to_vec(),
        kind_mask: record
            .kind_of
            .iter()
            .fold(ConditionMask::NONE, |mask, kind| {
                mask.union(kind_vocabulary.lookup(kind))
            }),
        ..KindSets::default()
    };
    for set in &record.weapon_sets {
        let names: Vec<&str> = set.conditions.iter().map(String::as_str).collect();
        let mask = sets.conditions.intern_all(&names);
        let mut slots = SlotSet::default();
        for declared in &set.slots {
            let Some(weapon) = combat.weapons.id(&declared.weapon) else {
                continue;
            };
            let kinds = |tokens: &[String]| {
                tokens.iter().fold(ConditionMask::NONE, |mask, kind| {
                    mask.union(kind_vocabulary.lookup(kind))
                })
            };
            let only_in_condition_tokens: Vec<&str> = declared
                .only_in_condition
                .iter()
                .map(String::as_str)
                .collect();
            let only_in_condition = sets.conditions.intern_all(&only_in_condition_tokens);
            if let Some(arm) = slots.arms.get_mut(declared.slot.index()) {
                *arm = Some(SlotArm {
                    weapon,
                    sources: declared.sources,
                    preferred_against: kinds(&declared.preferred_against),
                    only_against: kinds(&declared.only_against),
                    only_in_condition,
                });
            }
        }
        sets.weapons.push((mask, slots));
    }
    for set in &record.armor_sets {
        let names: Vec<&str> = set.conditions.iter().map(String::as_str).collect();
        let mask = sets.conditions.intern_all(&names);
        let armor = set.value.as_deref().and_then(|name| combat.armors.id(name));
        sets.armors.push((mask, armor));
    }
    sets
}

const USING_ABILITY_STATUS: &str = "IS_USING_ABILITY";

const MAX_EFFECT_NESTING: u16 = 6;

thread_local! {
    static EFFECT_DEPTH: std::cell::Cell<u16> = const { std::cell::Cell::new(0) };
    static EFFECT_CAP_REPORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct EffectNesting;

impl EffectNesting {
    fn enter() -> Option<Self> {
        let depth = EFFECT_DEPTH.with(std::cell::Cell::get);
        if depth >= MAX_EFFECT_NESTING {
            if !EFFECT_CAP_REPORTED.with(|seen| seen.replace(true)) {
                diag!(
                    "power: effects nest {MAX_EFFECT_NESTING} deep and the nested ones are dropped; an object creates itself"
                );
            }
            return None;
        }
        EFFECT_DEPTH.with(|d| d.set(depth.saturating_add(1)));
        Some(Self)
    }
}

impl Drop for EffectNesting {
    fn drop(&mut self) {
        let depth = EFFECT_DEPTH.with(|d| {
            let depth = d.get().saturating_sub(1);
            d.set(depth);
            depth
        });
        if depth == 0 {
            EFFECT_CAP_REPORTED.with(|seen| seen.set(false));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UsingAbility;

impl Component for UsingAbility {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AbilityLoop(pub(crate) std::sync::Arc<str>);

impl Component for AbilityLoop {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bytes(self.0.as_bytes());
    }
}

const NO_AUTO_ACQUIRE_STATUS: &str = "NO_AUTO_ACQUIRE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoAutoAcquire;

impl Component for NoAutoAcquire {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AbilityHold;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocomotorOverride(String);

impl Component for LocomotorOverride {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bytes(self.0.as_bytes());
    }
}

impl Component for AbilityHold {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

const HORDE_CONTAINER_EXCLUDED_MODULES: &[&str] = &[
    "HordeContain",
    "StancesBehavior",
    "HordeAIUpdate",
    "ProductionUpdate",
];

impl RotwkRules {
    pub(super) fn create_modules(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: Option<Kind>,
    ) -> Result<()> {
        let (Some(wiring), Some(kind)) = (self.modules.as_ref(), kind) else {
            return Ok(());
        };
        let Some(prototypes) = wiring.prototypes.get(usize::from(kind.0)) else {
            return Ok(());
        };
        if prototypes.is_empty() {
            return Ok(());
        }
        let now = tick.frame();
        for prototype in prototypes {
            wiring
                .registry
                .attach_prototype(tick.state_mut(), entity, now, prototype)?;
        }
        let mut log = EffectLog::new();
        wiring.registry.broadcast_create(
            tick.state_mut(),
            entity,
            now,
            Effects::into_sink(&mut log),
        );
        self.apply_effects(tick, ids, log.drain())?;
        self.offer_upgrades(tick, ids, entity)
    }

    pub(super) fn attach_horde_declared_modules(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: Entity,
        horde_kind: Kind,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let Some(entry) = self.kinds.get(horde_kind) else {
            return Ok(());
        };
        let now = tick.frame();
        let mut built: Vec<Prototype> = Vec::new();
        for module in &entry.record.declared_modules {
            if HORDE_CONTAINER_EXCLUDED_MODULES
                .iter()
                .any(|excluded| module.name.eq_ignore_ascii_case(excluded))
            {
                continue;
            }
            if let Ok(PrototypeOutcome::Built(prototype)) =
                wiring
                    .registry
                    .prototype(&module.name, &module.source(&self.upgrade_bits), now)
            {
                built.push(prototype);
            }
        }
        if built.is_empty() {
            return Ok(());
        }
        for prototype in &built {
            wiring
                .registry
                .attach_prototype(tick.state_mut(), horde, now, prototype)?;
        }
        let mut log = EffectLog::new();
        wiring.registry.broadcast_create(
            tick.state_mut(),
            horde,
            now,
            Effects::into_sink(&mut log),
        );
        self.apply_effects(tick, ids, log.drain())?;
        self.offer_upgrades(tick, ids, horde)
    }

    pub(super) fn offer_upgrades(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let held = self.effective_upgrades(tick, ids, entity);
        if held.is_empty() {
            return Ok(());
        }
        let mut log = EffectLog::new();
        let now = tick.frame();
        wiring.registry.broadcast_upgrade_into(
            tick.state_mut(),
            entity,
            &held,
            now,
            Effects::into_sink(&mut log),
        );
        self.apply_effects(tick, ids, log.drain())
    }

    pub(super) fn object_upgrade_completed(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        granted: &UpgradeMask,
    ) -> Result<()> {
        self.offer_upgrades(tick, ids, entity)?;
        let mut members: Vec<Entity> = tick
            .state()
            .get(ids.horde, entity)
            .map(|horde| horde.members.clone())
            .unwrap_or_default();
        members.sort_unstable_by_key(|member| member.index());
        for member in members {
            if !tick.state().is_live(member) {
                continue;
            }
            let mask = tick
                .state()
                .get(ids.object_upgrades, member)
                .map_or_else(UpgradeMask::default, |held| held.0.clone())
                .union(granted);
            tick.state_mut()
                .insert(ids.object_upgrades, member, ObjectUpgrades(mask))?;
            self.offer_upgrades(tick, ids, member)?;
        }
        Ok(())
    }

    pub(super) fn grant_object_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        bit: UpgradeBit,
    ) -> Result<()> {
        let mut granted = UpgradeMask::EMPTY;
        granted.set(bit);
        let granted = granted.union(&self.upgrade_bits.sub_mask(bit));
        let mask = tick
            .state()
            .get(ids.object_upgrades, entity)
            .map_or_else(UpgradeMask::default, |held| held.0.clone())
            .union(&granted);
        tick.state_mut()
            .insert(ids.object_upgrades, entity, ObjectUpgrades(mask))?;
        self.object_upgrade_completed(tick, ids, entity, &granted)
    }

    pub(super) fn grant_object_upgrades(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        granted: &UpgradeMask,
    ) -> Result<()> {
        if granted.is_empty() {
            return Ok(());
        }
        let mask = tick
            .state()
            .get(ids.object_upgrades, entity)
            .map_or_else(UpgradeMask::default, |held| held.0.clone())
            .union(granted);
        tick.state_mut()
            .insert(ids.object_upgrades, entity, ObjectUpgrades(mask))?;
        self.object_upgrade_completed(tick, ids, entity, granted)
    }

    fn split_removed_upgrades(&self, removed: &UpgradeMask) -> (UpgradeMask, UpgradeMask) {
        let mut player = UpgradeMask::EMPTY;
        let mut object = UpgradeMask::EMPTY;
        for (_, bit) in self.upgrade_bits.names() {
            if !removed.contains(bit) {
                continue;
            }
            if self.upgrade_bits.is_player(bit) {
                player.set(bit);
            } else {
                object.set(bit);
            }
        }
        (player, object)
    }

    pub(super) fn remove_player_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: mint_sim::PlayerId,
        bits: &UpgradeMask,
    ) -> Result<()> {
        let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        else {
            return Ok(());
        };
        let Some(mut mask) = tick
            .state()
            .get(ids.player_upgrades, player_entity)
            .map(|held| held.0.clone())
        else {
            return Ok(());
        };
        mask.clear_all(bits);
        tick.state_mut()
            .insert(ids.player_upgrades, player_entity, PlayerUpgrades(mask))?;
        Ok(())
    }

    fn remove_object_upgrades_from_all(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: mint_sim::PlayerId,
        bits: &UpgradeMask,
    ) -> Result<()> {
        let entities: Vec<Entity> = tick
            .state()
            .store(ids.owner)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, held)| held.0 == owner)
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in entities {
            if let Some(held) = tick.state_mut().get_mut(ids.object_upgrades, entity) {
                held.0.clear_all(bits);
            }
        }
        Ok(())
    }

    pub(super) fn apply_effects(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entries: Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let Some(_nesting) = EffectNesting::enter() else {
            return Ok(());
        };
        for (entity, effect) in entries {
            if self.apply_object_reading_effect(tick, ids, entity, &effect)? {
                continue;
            }
            if self.apply_instant_effect(tick, ids, wiring, entity, &effect)? {
                continue;
            }
            if UpgradeState::is_world_effect(&effect) {
                self.apply_power_effect(tick, ids, entity, &effect)?;
                continue;
            }
            if let Effect::RemoveUpgrades(removed, remove_from_all_player_objects) = &effect {
                self.apply_remove_upgrades_effect(
                    tick,
                    ids,
                    wiring,
                    entity,
                    removed,
                    *remove_from_all_player_objects,
                )?;
                continue;
            }
            self.apply_state_effect(tick, ids, wiring, entity, &effect)?;
        }
        Ok(())
    }

    fn apply_object_reading_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        effect: &Effect,
    ) -> Result<bool> {
        if let Effect::CastleUpgrade {
            upgrade,
            wall_radius,
        } = effect
        {
            self.castle_upgrade(tick, ids, entity, upgrade, *wall_radius)?;
            return Ok(true);
        }
        if let Effect::InheritUpgrade {
            upgrade,
            radius,
            filter,
        } = effect
        {
            self.inherit_upgrade(tick, ids, entity, upgrade, *radius, filter)?;
            return Ok(true);
        }
        if let Effect::ReplaceAndAdd {
            template,
            carry,
            attendants,
        } = effect
        {
            self.replace_and_add(tick, ids, entity, template, *carry, attendants)?;
            return Ok(true);
        }
        if let Effect::ReplenishHorde {
            melee_free,
            fx,
            nearby,
            all_nearby,
            upgrade_required,
        } = effect
        {
            self.replenish_hordes(
                tick,
                ids,
                entity,
                &super::replenish::Replenishment {
                    melee_free: *melee_free,
                    fx: fx.as_ref(),
                    nearby: *nearby,
                    all_nearby: *all_nearby,
                    upgrade_required,
                },
            )?;
            return Ok(true);
        }
        if let Effect::EmotionPulse {
            at,
            radius,
            kind,
            filter,
            lasts,
        } = effect
        {
            self.emotion_pulse(
                tick,
                ids,
                entity,
                super::emotions::Pulse {
                    at: *at,
                    radius: *radius,
                    kind: *kind,
                    filter,
                    lasts: *lasts,
                },
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    fn apply_instant_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        entity: Entity,
        effect: &Effect,
    ) -> Result<bool> {
        if let Effect::ToggleDisguise { force_mounted } = effect {
            self.apply_toggle_disguise(tick, ids, entity, *force_mounted)?;
            return Ok(true);
        }
        if let Effect::AiBusy(frames) = effect {
            Self::start_ai_busy(tick, ids, entity, *frames)?;
            return Ok(true);
        }
        if let Effect::HoldOrders(held) = effect {
            let held = *held;
            let frame = tick.frame().get();
            let at = tick
                .state()
                .get(ids.body, entity)
                .map_or(mint_movement::Vec2::ZERO, |body| body.0.position);
            if held {
                tick.state_mut()
                    .insert(wiring.ability_hold, entity, AbilityHold)?;
            } else {
                tick.state_mut().remove(wiring.ability_hold, entity);
            }
            let edge = if held { "held" } else { "released" };
            diag!(
                "hold: {entity:?} {edge} by its ability at frame {frame}, at ({:.1},{:.1})",
                at.x,
                at.y
            );
            return Ok(true);
        }
        if matches!(effect, Effect::ShareUpgradesWithSlaves) {
            self.share_upgrades_with_slaves(tick, ids, entity)?;
            return Ok(true);
        }
        if matches!(effect, Effect::EnterRampage) {
            self.enter_rampage(tick, ids, entity)?;
            return Ok(true);
        }
        if let Effect::TriggerReward {
            experience,
            skill_points,
        } = effect
        {
            self.reward_trigger(tick, ids, entity, *experience, *skill_points)?;
            return Ok(true);
        }
        if let Effect::PlayObjectSound(name) = effect {
            self.play_object_sound(tick, entity, name);
            self.emit_object_sound(tick, ids, entity, std::sync::Arc::clone(name));
            return Ok(true);
        }
        if let Effect::StartLoopSound(name) = effect {
            if !tick.state().contains(ids.ability_loop_sound, entity) {
                tick.state_mut().insert(
                    ids.ability_loop_sound,
                    entity,
                    AbilityLoop(std::sync::Arc::clone(name)),
                )?;
            }
            return Ok(true);
        }
        if matches!(effect, Effect::StopLoopSound) {
            tick.state_mut().remove(ids.ability_loop_sound, entity);
            return Ok(true);
        }
        if let Effect::CaptureFlash { target } = effect {
            self.apply_capture_flash(tick, ids, entity, *target)?;
            return Ok(true);
        }
        if matches!(effect, Effect::Die) {
            if !tick.state().contains(ids.doomed, entity)
                && !tick.state().contains(ids.lifetime, entity)
            {
                let dies_at = tick.frame();
                tick.state_mut().insert(
                    ids.lifetime,
                    entity,
                    crate::summons::Lifetime {
                        dies_at,
                        death: crate::combat::DeathNaming::Normal,
                        set_at: None,
                    },
                )?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn apply_remove_upgrades_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        entity: Entity,
        removed: &UpgradeMask,
        remove_from_all_player_objects: bool,
    ) -> Result<()> {
        let (player_bits, object_bits) = self.split_removed_upgrades(removed);
        let owner = tick.state().get(ids.owner, entity).map(|held| held.0);
        if !player_bits.is_empty() {
            if let Some(owner) = owner {
                self.remove_player_upgrade(tick, ids, owner, &player_bits)?;
            }
        }
        if !object_bits.is_empty() {
            if remove_from_all_player_objects {
                if let Some(owner) = owner {
                    self.remove_object_upgrades_from_all(tick, ids, owner, &object_bits)?;
                }
            } else if let Some(held) = tick.state_mut().get_mut(ids.object_upgrades, entity) {
                held.0.clear_all(&object_bits);
            }
        }
        let mut log = EffectLog::new();
        wiring.registry.broadcast_upgrade_removed(
            tick.state_mut(),
            entity,
            removed,
            Effects::into_sink(&mut log),
        );
        self.apply_effects(tick, ids, log.drain())?;
        if let Some(owner) = owner {
            self.cancel_heroes_missing_upgrade(tick, ids, owner)?;
        }

        Ok(())
    }

    fn apply_state_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        entity: Entity,
        effect: &Effect,
    ) -> Result<()> {
        if let Effect::LockWeaponSlot(slot) = effect {
            let armed = self
                .selected_set(tick, ids, entity)
                .and_then(|(_, slots)| slots.arms.get(usize::from(*slot)).copied().flatten());
            if armed.is_none() {
                return Ok(());
            }
        }
        let changed = match tick.state_mut().get_mut(wiring.state, entity) {
            Some(state) => state.apply(effect),
            None => {
                let mut state = UpgradeState::default();
                let changed = state.apply(effect);
                if changed != Changed::NONE {
                    tick.state_mut().insert(wiring.state, entity, state)?;
                }
                changed
            }
        };
        if let Effect::WeaponSetFlag(flag) | Effect::ClearWeaponSetFlag(flag) = effect {
            if let Some(swap) = mint_sage_modules::effects::weapon_set_toggle_swap_condition(flag) {
                self.apply_power_effect(
                    tick,
                    ids,
                    entity,
                    &Effect::TimedModelCondition {
                        flag: mint_sage_modules::effects::Name::from(swap),
                        lasts: mint_sage_modules::retail_duration_ticks(1000),
                        only_if_absent: false,
                    },
                )?;
            }
        }
        if let Effect::ToggleModelCondition(flag) = effect {
            if flag.eq_ignore_ascii_case("MOUNTED")
                && changed.intersects(Changed::LOOK)
                && !tick
                    .state()
                    .get(wiring.state, entity)
                    .is_some_and(|state| state.has_model_condition("MOUNTED"))
                && wiring.cancels_disguise_on_dismount(tick.state(), entity)
            {
                self.break_disguise(tick, ids, entity, false)?;
            }
        }
        if let Effect::StatusBits { set, clear } = effect {
            self.apply_status_bits_effect(tick, ids, entity, set, clear)?;
        }
        if changed.intersects(Changed::WEAPON_SET.union(Changed::LOOK)) {
            self.reselect_weapon(tick, ids, wiring, entity)?;
        }
        if changed.intersects(Changed::ARMOR_SET.union(Changed::LOOK)) {
            self.reselect_armour(tick, ids, wiring, entity)?;
        }
        if changed.intersects(Changed::LOOK) {
            self.reselect_locomotion(tick, ids, wiring, entity)?;
        }
        if changed.intersects(Changed::COMMAND_SET) {
            self.apply_command_set(tick, ids, wiring, entity)?;
        }
        if changed.intersects(Changed::GEOMETRY) {
            self.reshape_geometry(tick, ids, wiring, entity)?;
        }
        match effect {
            Effect::ModelConditions { add, .. } if changed.intersects(Changed::LOOK) => {
                self.fire_state_enter_events(tick, ids, entity, add)?;
            }
            Effect::LevelUp { levels, cap } => {
                self.gain_levels(tick, ids, entity, *levels, *cap)?;
            }
            Effect::AttributeModifier(name) if changed.intersects(Changed::ATTRIBUTES) => {
                self.attribute_modifier_gained(tick, ids, entity, name)?;
            }
            Effect::UnpausePower(power) if changed.intersects(Changed::POWERS) => {
                self.power_unpaused(tick, ids, entity, power)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_status_bits_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        set: &[mint_sage_modules::effects::Name],
        clear: &[mint_sage_modules::effects::Name],
    ) -> Result<()> {
        if set
            .iter()
            .any(|bit| bit.eq_ignore_ascii_case(USING_ABILITY_STATUS))
        {
            tick.state_mut()
                .insert(ids.using_ability, entity, UsingAbility)?;
            self.disengage_for_ability(tick, ids, entity, false)?;
        } else if clear
            .iter()
            .any(|bit| bit.eq_ignore_ascii_case(USING_ABILITY_STATUS))
        {
            tick.state_mut().remove(ids.using_ability, entity);
        }
        if set
            .iter()
            .any(|bit| bit.eq_ignore_ascii_case(NO_AUTO_ACQUIRE_STATUS))
        {
            tick.state_mut()
                .insert(ids.no_auto_acquire, entity, NoAutoAcquire)?;
        } else if clear
            .iter()
            .any(|bit| bit.eq_ignore_ascii_case(NO_AUTO_ACQUIRE_STATUS))
        {
            tick.state_mut().remove(ids.no_auto_acquire, entity);
            let has_newer_order = tick.state().contains(ids.player_order, entity)
                || tick.state().contains(ids.attack_order, entity);
            if !has_newer_order {
                if let Some(suspended) = tick.state_mut().remove(ids.suspended_order, entity) {
                    if let Some(order) = suspended.attack_order {
                        tick.state_mut().insert(ids.attack_order, entity, order)?;
                    }
                    if suspended.player_order {
                        tick.state_mut()
                            .insert(ids.player_order, entity, PlayerOrder)?;
                    }
                }
            } else {
                tick.state_mut().remove(ids.suspended_order, entity);
            }
        }
        Ok(())
    }

    fn power_unpaused(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        power: &str,
    ) -> Result<()> {
        let Some(kind) = tick.state().get(ids.kind, entity).copied() else {
            return Ok(());
        };
        let Some(slot) = self
            .kinds
            .get(kind)
            .and_then(|entry| entry.record.power(power))
        else {
            return Ok(());
        };
        if slot.obey_recharge_on_unpause && !slot.available_at_start {
            self.begin_recharge(tick, ids, entity, power)?;
        }
        Ok(())
    }

    fn replace_and_add(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        template: &str,
        carry: mint_sage_modules::ReplaceCarry,
        attendants: &mint_sage_modules::effects::Names,
    ) -> Result<()> {
        let placed = tick.state().get(ids.body, entity).map(|body| body.0);
        let owner = tick.state().get(ids.owner, entity).map(|owner| owner.0);
        let step = tick
            .state()
            .get(ids.kind, entity)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .map_or(0.0, |entry| {
                axis_extent(&crate::construction::footprint_of(&entry.record))
            });
        self.take_over_segments(tick, ids, entity, template, attendants)?;
        self.clear_for_conversion(tick, ids, entity, template, attendants);
        let carried = self.links_to_carry(tick.state(), ids, entity);
        let made = self.replace_object(tick, ids, entity, template, carry)?;
        if let (Some(new), Some(carried)) = (made, carried) {
            self.carry_links(tick, ids, new, carried)?;
        }
        let (Some(placed), Some(owner)) = (placed, owner) else {
            return Ok(());
        };
        if attendants.is_empty() {
            return Ok(());
        }
        if step <= 0.0 {
            diag!("replace: {template}'s attendants have no axis extent to stand along");
            return Ok(());
        }
        let angle = placed.facing.radians();
        let (sin, cos) = (mint_math::f32::sin(angle), mint_math::f32::cos(angle));
        for (index, name) in attendants.iter().enumerate() {
            let Some(kind) = self.kinds.kind(name) else {
                diag!("replace: {name} is not in this match's templates");
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let along = step * (index + 1) as f32;
            let at = mint_movement::Vec2::new(
                placed.position.x + along * cos,
                placed.position.y + along * sin,
            );
            self.spawn_kind_at(tick, ids, kind, at, placed.facing, owner)?;
        }
        Ok(())
    }

    fn kind_sets<'w>(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        wiring: &'w ModuleWiring,
        entity: Entity,
    ) -> Option<&'w KindSets> {
        let kind = tick.state().get(ids.kind, entity)?;
        wiring.sets.get(usize::from(kind.0))
    }

    fn reselect_weapon(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        entity: Entity,
    ) -> Result<()> {
        let Some(sets) = self.kind_sets(tick, ids, wiring, entity) else {
            return Ok(());
        };
        let flags = tick
            .state()
            .get(wiring.state, entity)
            .map(weapon_set_conditions)
            .unwrap_or_default();
        let masks: Vec<ConditionMask> = sets.weapons.iter().map(|(mask, _)| *mask).collect();
        let Some((at, slots)) = best_match(sets.active(&flags), &masks)
            .and_then(|at| Some((at, sets.weapons.get(at)?.1)))
        else {
            return Ok(());
        };
        let Some((slot, chosen)) = slots.first_weapon() else {
            return Ok(());
        };
        let Some(template) = self.combat.weapons.get(chosen) else {
            return Ok(());
        };
        let held = tick
            .state()
            .get(ids.weapon_slots, entity)
            .map(|held| usize::from(held.set));
        let same_set = held.is_some_and(|held| held == at)
            || (held.is_none()
                && tick
                    .state()
                    .get(ids.armed, entity)
                    .is_some_and(|armed| armed.weapon == chosen));
        let mut switched = false;
        if !same_set {
            if let Some(armed) = tick.state_mut().get_mut(ids.armed, entity) {
                *armed = Armed {
                    weapon: chosen,
                    state: WeaponState::new(template),
                    winding: false,
                    ..*armed
                };
                switched = true;
            }
        }
        if switched {
            tick.state_mut().remove(ids.weapon_slots, entity);
            if slots.arms.iter().filter(|arm| arm.is_some()).count() > 1 {
                let mut states = [None; SLOT_COUNT];
                for (index, arm) in slots.arms.iter().enumerate() {
                    if index == slot {
                        continue;
                    }
                    if let Some(template) = arm.and_then(|arm| self.combat.weapons.get(arm.weapon))
                    {
                        if let Some(entry) = states.get_mut(index) {
                            *entry = Some(WeaponState::new(template));
                        }
                    }
                }
                let Ok(set) = u8::try_from(at) else {
                    return Ok(());
                };
                let Ok(current) = u8::try_from(slot) else {
                    return Ok(());
                };
                tick.state_mut().insert(
                    ids.weapon_slots,
                    entity,
                    WeaponSlots {
                        set,
                        current,
                        states,
                    },
                )?;
            }
        }
        if switched {
            diag!(
                "upgrade: entity {} switched to WeaponSet {} of its template",
                entity.index(),
                at.saturating_add(1)
            );
        }
        Ok(())
    }

    pub(crate) fn armed_slots(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<(usize, SlotSet)> {
        self.selected_set(tick, ids, entity)
            .filter(|(_, slots)| slots.arms.iter().filter(|arm| arm.is_some()).count() > 1)
    }

    pub(crate) fn selected_set(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<(usize, SlotSet)> {
        let wiring = self.modules.as_ref()?;
        let sets = self.kind_sets(tick, ids, wiring, entity)?;
        let at = match tick.state().get(ids.weapon_slots, entity) {
            Some(held) => usize::from(held.set),
            None => {
                let flags = tick
                    .state()
                    .get(wiring.state, entity)
                    .map(weapon_set_conditions)
                    .unwrap_or_default();
                let masks: Vec<ConditionMask> =
                    sets.weapons.iter().map(|(mask, _)| *mask).collect();
                best_match(sets.active(&flags), &masks)?
            }
        };
        Some((at, sets.weapons.get(at)?.1))
    }

    pub(crate) fn aim_weapon(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        armed: &mut Armed,
        (at, slots): (usize, SlotSet),
        victim: Entity,
        source: CommandSource,
    ) -> Result<bool> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(false);
        };
        let held = tick.state().get(ids.weapon_slots, entity).copied();
        let current = held.map_or_else(
            || {
                slots
                    .arms
                    .iter()
                    .position(|arm| arm.is_some_and(|arm| arm.weapon == armed.weapon))
                    .unwrap_or(0)
            },
            |held| usize::from(held.current),
        );
        let victim_kinds = tick
            .state()
            .get(ids.kind, victim)
            .and_then(|kind| wiring.sets.get(usize::from(kind.0)))
            .map_or(ConditionMask::NONE, |sets| sets.kind_mask);
        let active_conditions =
            self.kind_sets(tick, ids, wiring, entity)
                .map_or(ConditionMask::NONE, |sets| {
                    let mut flags: Vec<mint_sage_modules::effects::Name> = tick
                        .state()
                        .get(wiring.state, entity)
                        .map(weapon_set_conditions)
                        .unwrap_or_default();
                    if is_moving(tick, ids, entity) {
                        flags.push(mint_sage_modules::effects::Name::from("MOVING"));
                    }
                    sets.active(&flags)
                });
        let armour = armour_of(
            &self.combat,
            tick.state().get(ids.armour, victim).copied(),
            &self.unarmoured,
        );
        let victim_kind = tick
            .state()
            .get(ids.kind, victim)
            .and_then(|kind| self.kinds.get(*kind));
        let airborne = self.airborne_category(tick.state(), ids, victim);
        let is_structure = victim_kind
            .is_some_and(|entry| entry.record.kind_of.iter().any(|kind| kind == "STRUCTURE"));
        let now = tick.frame();
        let distance = match (
            tick.state().get(ids.body, entity),
            tick.state().get(ids.body, victim),
        ) {
            (Some(me), Some(them)) => them.0.position.minus(me.0.position).length(),
            _ => f32::INFINITY,
        };
        let mut candidates: [Option<SlotCandidate>; SLOT_COUNT] = [None; SLOT_COUNT];
        for (index, arm) in slots.arms.iter().enumerate() {
            let Some(arm) = *arm else { continue };
            let Some(template) = self.combat.weapons.get(arm.weapon) else {
                continue;
            };
            let state = if index == current {
                Some(armed.state)
            } else {
                held.and_then(|held| held.states.get(index).copied().flatten())
                    .or_else(|| Some(WeaponState::new(template)))
            };
            let Some(mut state) = state else { continue };
            let status = state.status(now);
            let damage = self.combat.weapons.estimated_damage(arm.weapon, armour);
            let unresistable = damage <= 0.0 && self.unresistable(arm.weapon);
            if let Some(entry) = candidates.get_mut(index) {
                *entry = Some(SlotCandidate {
                    arm,
                    fitness: SlotFitness {
                        class_allowed: crate::combat::class_allowed(
                            template.anti_mask,
                            is_structure,
                            airborne,
                        ),
                        damage,
                        unresistable,
                        ready: matches!(
                            status,
                            mint_sage_combat::WeaponStatus::ReadyToFire
                                | mint_sage_combat::WeaponStatus::PreAttack
                        ) && distance >= template.minimum_attack_range,
                        inside_minimum: distance < template.minimum_attack_range,
                        out_of_ammo: status == mint_sage_combat::WeaponStatus::OutOfAmmo
                            && !template.auto_reloads_clip,
                    },
                });
            }
        }
        let locked = tick
            .state()
            .get(wiring.state, entity)
            .and_then(|state| state.weapon_lock)
            .map(usize::from)
            .filter(|slot| slots.arms.get(*slot).is_some_and(Option::is_some));
        let chosen = match locked {
            Some(slot) => slot,
            None => {
                let Some(chosen) = mint_sage_combat::choose_slot(
                    &candidates,
                    victim_kinds,
                    source,
                    active_conditions,
                ) else {
                    return Ok(false);
                };
                chosen.index()
            }
        };
        if !self.switch_slot(tick, ids, entity, armed, (at, slots), chosen)? {
            return Ok(false);
        }
        diag!(
            "weapon: entity {} switched to slot {} against entity {}",
            entity.index(),
            mint_sage_combat::WeaponSlot::from_index(chosen).map_or("?", |slot| slot.name()),
            victim.index()
        );
        Ok(true)
    }

    pub(crate) fn switch_slot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        armed: &mut Armed,
        (at, slots): (usize, SlotSet),
        chosen: usize,
    ) -> Result<bool> {
        let held = tick.state().get(ids.weapon_slots, entity).copied();
        let current = held.map_or_else(
            || {
                slots
                    .arms
                    .iter()
                    .position(|arm| arm.is_some_and(|arm| arm.weapon == armed.weapon))
                    .unwrap_or(0)
            },
            |held| usize::from(held.current),
        );
        if chosen == current {
            return Ok(false);
        }
        let Some(arm) = slots.arms.get(chosen).copied().flatten() else {
            return Ok(false);
        };
        let Some(template) = self.combat.weapons.get(arm.weapon) else {
            return Ok(false);
        };
        let mut states = held.map_or([None; SLOT_COUNT], |held| held.states);
        let mut taken = states
            .get_mut(chosen)
            .and_then(Option::take)
            .unwrap_or_else(|| WeaponState::new(template));
        if let Some(entry) = states.get_mut(current) {
            *entry = Some(armed.state);
        }
        armed.weapon = arm.weapon;
        armed.winding = taken.status(tick.frame()) == WeaponStatus::PreAttack;
        armed.state = taken;
        let (Ok(set), Ok(current)) = (u8::try_from(at), u8::try_from(chosen)) else {
            return Ok(false);
        };
        tick.state_mut().insert(
            ids.weapon_slots,
            entity,
            WeaponSlots {
                set,
                current,
                states,
            },
        )?;
        Ok(true)
    }

    fn unresistable(&self, weapon: WeaponId) -> bool {
        let Some(id) = self.combat.damage_types.lookup("UNRESISTABLE") else {
            return false;
        };
        self.combat
            .weapons
            .get(weapon)
            .is_some_and(|template| {
                template.nuggets.iter().any(|nugget| {
                    matches!(nugget, mint_sage_combat::Nugget::Damage(damage) if damage.damage_type == id)
                })
            })
    }

    fn reselect_armour(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        entity: Entity,
    ) -> Result<()> {
        let Some(sets) = self.kind_sets(tick, ids, wiring, entity) else {
            return Ok(());
        };
        if sets.armors.is_empty() {
            return Ok(());
        }
        let flags = tick
            .state()
            .get(wiring.state, entity)
            .map(armor_set_conditions)
            .unwrap_or_default();
        let masks: Vec<ConditionMask> = sets.armors.iter().map(|(mask, _)| *mask).collect();
        let chosen = best_match(sets.active(&flags), &masks)
            .and_then(|at| sets.armors.get(at))
            .and_then(|(_, armor)| *armor);
        match chosen {
            Some(armor) => {
                tick.state_mut().insert(ids.armour, entity, Armour(armor))?;
            }
            None => {
                tick.state_mut().remove(ids.armour, entity);
            }
        }
        Ok(())
    }

    fn reselect_locomotion(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        entity: Entity,
    ) -> Result<()> {
        let Some(sets) = self.kind_sets(tick, ids, wiring, entity) else {
            return Ok(());
        };
        if sets.locomotor_sets.is_empty() {
            return Ok(());
        }
        let requested = tick.state().get(ids.locomotor_override, entity).cloned();
        let chosen = match requested {
            Some(LocomotorOverride(name)) => sets
                .locomotor_sets
                .iter()
                .find(|(flag, _)| flag.eq_ignore_ascii_case(&name))
                .map(|(_, limits)| *limits),
            None => {
                let held = tick
                    .state()
                    .get(wiring.state, entity)
                    .map(|state| state.model_conditions.clone())
                    .unwrap_or_default();
                sets.locomotor_sets
                    .iter()
                    .find(|(flag, _)| held.iter().any(|name| name.eq_ignore_ascii_case(flag)))
                    .map(|(_, limits)| *limits)
            }
        };
        match chosen {
            Some(limits) => {
                tick.state_mut()
                    .insert(ids.locomotion, entity, crate::unit::Locomotion(limits))?;
            }
            None => {
                tick.state_mut().remove(ids.locomotion, entity);
            }
        }
        Ok(())
    }

    pub(super) fn apply_locomotor_override(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        name: Option<&str>,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let mut targets = vec![entity];
        if let Some(horde) = tick.state().get(ids.horde, entity) {
            targets.extend(horde.members.iter().copied());
        }
        for target in targets {
            match name {
                Some(name) => {
                    tick.state_mut().insert(
                        ids.locomotor_override,
                        target,
                        LocomotorOverride(name.to_owned()),
                    )?;
                }
                None => {
                    tick.state_mut().remove(ids.locomotor_override, target);
                }
            }
            self.reselect_locomotion(tick, ids, wiring, target)?;
        }
        Ok(())
    }

    fn start_producing(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: Kind,
        menu: u16,
    ) -> Result<()> {
        let Some(record) = self.kinds.get(kind).map(|entry| &entry.record) else {
            return Ok(());
        };
        let Some((position, facing)) = tick
            .state()
            .get(ids.body, entity)
            .map(|body| (body.0.position, body.0.facing))
        else {
            return Ok(());
        };
        if !record.produces {
            return Ok(());
        }
        if let Some(producer) = tick.state_mut().get_mut(ids.producer, entity) {
            producer.menu = Some(menu);
            return Ok(());
        }
        let producer = crate::economy::Producer {
            queue: mint_sage_economy::ProductionQueue::new(crate::templates::queue_depth(record)),
            menu: Some(menu),
            object_menu: None,
            player_menu: None,
            rally: crate::templates::exit_rally(record, position, facing),
            player_rally: None,
        };
        tick.state_mut().insert(ids.producer, entity, producer)?;
        Ok(())
    }

    fn apply_command_set(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        entity: Entity,
    ) -> Result<()> {
        let Some(kind) = tick.state().get(ids.kind, entity).copied() else {
            return Ok(());
        };
        let set = tick
            .state()
            .get(wiring.state, entity)
            .and_then(|state| state.command_set.clone());
        let index = usize::from(kind.0);
        let (unit_menu, object_menu, player_menu) = match set.as_deref() {
            Some(set) => {
                let Some((_, _, object, player)) = self
                    .upgrade_variant_menus
                    .iter()
                    .find(|(held, name, ..)| *held == kind && name.eq_ignore_ascii_case(set))
                else {
                    return Ok(());
                };
                (self.production.variant_menu(kind, set), *object, *player)
            }
            None => (
                self.production.menu_of.get(index).copied().flatten(),
                self.object_upgrade_menu_of.get(index).copied().flatten(),
                self.player_upgrade_menu_of.get(index).copied().flatten(),
            ),
        };
        if let (Some(menu), Some(producer)) =
            (unit_menu, tick.state_mut().get_mut(ids.producer, entity))
        {
            producer.menu = Some(menu);
        } else if let Some(menu) = unit_menu {
            self.start_producing(tick, ids, entity, kind, menu)?;
        }
        if let Some(producer) = tick.state_mut().get_mut(ids.producer, entity) {
            set_upgrade_menus(producer, object_menu, player_menu);
        }
        Ok(())
    }

    pub(super) fn affected_by_upgrade(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        bit: UpgradeBit,
    ) -> bool {
        let mut mask = self.effective_upgrades_in(world, ids, entity);
        mask.set(bit);
        self.affected_with_fit(world, ids, entity, &mask, true)
    }

    pub(crate) fn sub_upgrade_counted(
        &self,
        world: &World,
        ids: Ids,
        builder: Option<Entity>,
        bit: UpgradeBit,
    ) -> bool {
        let Some(builder) = builder else {
            return true;
        };
        let horde = if world.contains(ids.horde, builder) {
            Some(builder)
        } else {
            world.get(ids.member, builder).map(|member| member.horde)
        };
        let Some(horde) = horde.and_then(|horde| world.get(ids.horde, horde)) else {
            return true;
        };
        horde.members.is_empty()
            || horde
                .members
                .iter()
                .any(|member| self.affected_by_upgrade(world, ids, *member, bit))
    }

    pub(super) fn power_released(&self, world: &World, entity: Entity, power: &str) -> bool {
        self.modules
            .as_ref()
            .and_then(|wiring| world.get(wiring.state, entity))
            .is_some_and(|state| state.is_unpaused(power))
    }

    pub(super) fn upgrade_selection(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: mint_sim::PlayerId,
    ) -> Vec<Entity> {
        let mut out: Vec<Entity> = self
            .selection(tick, ids, player)
            .into_iter()
            .filter(|entity| !tick.state().contains(ids.doomed, *entity))
            .map(|entity| {
                tick.state()
                    .get(ids.member, entity)
                    .map(|member| member.horde)
                    .filter(|horde| tick.state().contains(ids.producer, *horde))
                    .unwrap_or(entity)
            })
            .collect();
        out.sort_unstable_by_key(|entity| entity.index());
        out.dedup();
        out
    }

    pub(super) fn cancel_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: mint_sim::PlayerId,
        button: u8,
        is_player: bool,
    ) -> Result<()> {
        let menus = if is_player {
            &self.player_upgrade_menus
        } else {
            &self.object_upgrade_menus
        };
        for structure in self.upgrade_selection(tick, ids, player) {
            if tick.state().get(ids.owner, structure).map(|owner| owner.0) != Some(player) {
                continue;
            }
            let Some(producer) = tick.state().get(ids.producer, structure) else {
                continue;
            };
            let menu = if is_player {
                producer.player_menu
            } else {
                producer.object_menu
            };
            let Some(item) = menu
                .and_then(|menu| menus.get(usize::from(menu)))
                .and_then(|menu| menu.get(usize::from(button)))
                .copied()
            else {
                continue;
            };
            let Some(entry) = producer
                .queue
                .entries()
                .iter()
                .find(|entry| {
                    matches!(entry.kind, mint_sage_economy::EntryKind::Upgrade)
                        && entry.item.index() == u32::from(item)
                })
                .map(|entry| entry.id)
            else {
                continue;
            };
            let Some(player_entity) =
                crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
            else {
                return Ok(());
            };
            let mut purse = tick
                .state()
                .get(ids.purse, player_entity)
                .map_or(mint_sage_economy::Money::new(0), |held| held.0);
            let cancelled = tick
                .state_mut()
                .get_mut(ids.producer, structure)
                .and_then(|producer| producer.queue.cancel(entry, &mut purse));
            if cancelled.is_none() {
                continue;
            }
            tick.state_mut()
                .insert(ids.purse, player_entity, crate::economy::Purse(purse))?;
            self.drop_player_upgrades_in_progress(tick, ids, player_entity, &[u32::from(item)])?;
            diag!(
                "upgrade: player {} cancelled button {} at entity {} - refunded in full",
                player.get(),
                u16::from(button).saturating_add(1),
                structure.index()
            );
            return Ok(());
        }
        diag!(
            "upgrade: player {} has nothing researching on button {} - cancel refused",
            player.get(),
            u16::from(button).saturating_add(1)
        );
        Ok(())
    }

    pub(crate) fn command_subject(&self, world: &World, entity: Entity) -> Entity {
        let Some(ids) = self.ids else {
            return entity;
        };
        world
            .get(ids.member, entity)
            .map(|member| member.horde)
            .filter(|horde| world.is_live(*horde) && world.contains(ids.kind, *horde))
            .unwrap_or(entity)
    }

    pub(super) fn command_subjects(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: mint_sim::PlayerId,
    ) -> Vec<Entity> {
        self.command_subjects_in(tick.state(), ids, player)
    }

    pub(super) fn command_subjects_in(
        &self,
        world: &World,
        ids: Ids,
        player: mint_sim::PlayerId,
    ) -> Vec<Entity> {
        let mut out: Vec<Entity> = world
            .store(ids.selected)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, mask)| mask.holds(player))
                    .map(|(entity, _)| self.command_subject(world, entity))
                    .collect()
            })
            .unwrap_or_default();
        out.sort_unstable_by_key(|entity| entity.index());
        out.dedup();
        out
    }

    pub(crate) fn research_commands(
        &self,
        world: &World,
        player: mint_sim::PlayerId,
        upgrade: &str,
    ) -> Vec<super::RotwkCommand> {
        let (Some(ids), Some(bit)) = (self.ids, self.upgrade_bits.bit(upgrade)) else {
            return Vec::new();
        };
        let Some(producers) = world.store(ids.producer) else {
            return Vec::new();
        };
        let button_in = |menu: Option<u16>, menus: &[Vec<u16>]| -> Option<u8> {
            menus
                .get(usize::from(menu?))?
                .iter()
                .position(|&index| {
                    self.upgrade_buildables
                        .get(usize::from(index))
                        .is_some_and(|buildable| buildable.bit == bit)
                })
                .and_then(|at| u8::try_from(at).ok())
        };
        let mut out = Vec::new();
        for (entity, producer) in producers.iter() {
            if world.get(ids.owner, entity).map(|owner| owner.0) != Some(player) {
                continue;
            }
            let routed = world
                .get(ids.member, entity)
                .is_some_and(|member| world.contains(ids.producer, member.horde));
            if routed {
                continue;
            }
            let target = world
                .get(ids.horde, entity)
                .and_then(|horde| horde.members.iter().copied().min_by_key(|m| m.index()))
                .unwrap_or(entity);
            let command = if let Some(button) =
                button_in(producer.object_menu, &self.object_upgrade_menus)
            {
                super::RotwkCommand::ObjectUpgrade { button }
            } else if let Some(button) = button_in(producer.player_menu, &self.player_upgrade_menus)
            {
                super::RotwkCommand::PlayerUpgrade { button }
            } else {
                continue;
            };
            let is_player = matches!(command, super::RotwkCommand::PlayerUpgrade { .. });
            out.push(super::RotwkCommand::SetSelection {
                replace: true,
                units: vec![target],
            });
            out.push(command);
            if is_player {
                break;
            }
        }
        out
    }

    pub(super) fn shooter_upgrades(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        weapon: WeaponId,
        shooter: Entity,
    ) -> UpgradeMask {
        if !self.combat.filters.filters_weapon(weapon.index()) {
            return UpgradeMask::EMPTY;
        }
        self.effective_upgrades(tick, ids, shooter)
    }

    pub(crate) fn command_set_override<'a>(
        &'a self,
        world: &'a World,
        entity: Entity,
    ) -> Option<&'a str> {
        if let Some(name) = self.cah_command_set(world, entity) {
            return Some(name);
        }
        let wiring = self.modules.as_ref()?;
        world.get(wiring.state, entity)?.command_set.as_deref()
    }

    pub(crate) fn upgrade_state_id(&self) -> Option<ComponentId<UpgradeState>> {
        self.modules.as_ref().map(|wiring| wiring.state)
    }
}

pub(super) fn axis_extent(records: &[mint_sage_passability::FootprintRecord]) -> f32 {
    records.iter().fold(0.0f32, |extent, record| {
        let half = match record.footprint {
            mint_sage_passability::Footprint::Box { major, .. } => major,
            mint_sage_passability::Footprint::Disc { radius } => radius,
        };
        extent.max(record.offset.x.abs() + half)
    }) * 2.0
}
