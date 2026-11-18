use core::any::{Any, TypeId};
use core::cell::RefCell;

use mint_ecs::{Component, ComponentId, Entity, StateHasher, World};
use mint_math::rng::Pcg32;

use crate::command::PlayerId;
use crate::time::{Frame, LogicFrames};
use crate::{Error, Result};

pub use crate::upgrade::{UpgradeBit, UpgradeMask};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Phase {
    Initial,
    Physics,
    #[default]
    Normal,
    Final,
}

impl Phase {
    pub const ORDER: [Phase; 4] = [Phase::Initial, Phase::Physics, Phase::Normal, Phase::Final];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sleep {
    NextFrame,
    Frames(LogicFrames),
    Forever,
}

impl Sleep {
    #[must_use]
    pub fn wake_after(self, now: Frame) -> Frame {
        match self {
            Sleep::NextFrame => now.checked_add(LogicFrames::ONE).unwrap_or(Frame::FOREVER),
            Sleep::Frames(duration) => now.checked_add(duration).unwrap_or(Frame::FOREVER),
            Sleep::Forever => Frame::FOREVER,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Update,
    Die,
    Damage,
    Create,
    Collide,
    Body,
    Contain,
    Upgrade,
    SpecialPower,
    Destroy,
    Draw,
    ClientUpdate,
}

impl Capability {
    const fn bit(self) -> u16 {
        match self {
            Capability::Update => 0b0000_0000_0000_0001,
            Capability::Die => 0b0000_0000_0000_0010,
            Capability::Damage => 0b0000_0000_0000_0100,
            Capability::Create => 0b0000_0000_0000_1000,
            Capability::Collide => 0b0000_0000_0001_0000,
            Capability::Body => 0b0000_0000_0010_0000,
            Capability::Contain => 0b0000_0000_0100_0000,
            Capability::Upgrade => 0b0000_0000_1000_0000,
            Capability::SpecialPower => 0b0000_0001_0000_0000,
            Capability::Destroy => 0b0000_0010_0000_0000,
            Capability::Draw => 0b0000_0100_0000_0000,
            Capability::ClientUpdate => 0b0000_1000_0000_0000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapabilityMask(u16);

impl CapabilityMask {
    pub const NONE: Self = Self(0);

    #[must_use]
    pub const fn single(capability: Capability) -> Self {
        Self(capability.bit())
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub fn of(capabilities: &[Capability]) -> Self {
        let mut mask = Self::NONE;
        for &capability in capabilities {
            mask = mask.union(Self::single(capability));
        }
        mask
    }

    #[must_use]
    pub const fn contains(self, capability: Capability) -> bool {
        self.0 & capability.bit() != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleTypeId(u32);

impl ModuleTypeId {
    pub(crate) const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

const fn to_usize(value: u32) -> usize {
    value as usize
}

#[derive(Debug, Clone)]
pub struct ModuleSlot<T> {
    pub wake: Frame,
    pub state: T,
}

#[derive(Debug, Clone)]
pub struct ModuleColumn<T>(pub Vec<ModuleSlot<T>>);

impl<T> ModuleColumn<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }
}

impl<T> Default for ModuleColumn<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ModuleKind> Component for ModuleColumn<T> {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for slot in &self.0 {
            slot.wake.hash_into(hasher);
            slot.state.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModuleDeclarations(Vec<(ModuleTypeId, u32)>);

impl ModuleDeclarations {
    #[must_use]
    pub fn order(&self) -> &[(ModuleTypeId, u32)] {
        &self.0
    }

    fn push(&mut self, module: ModuleTypeId, slot: u32) {
        self.0.push((module, slot));
    }
}

impl Component for ModuleDeclarations {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (module, slot) in &self.0 {
            hasher.write_u32(module.index());
            hasher.write_u32(*slot);
        }
    }
}

pub trait ModuleKind: Component {
    const CAPABILITIES: CapabilityMask;

    const PHASE: Phase = Phase::Normal;

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let _ = ctx;
        Sleep::Forever
    }

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>)
    where
        Self: Sized,
    {
        let _ = (instances, ctx);
    }

    fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut UpgradeContext<'_>) {
        let _ = (active, ctx);
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        let _ = active;
        false
    }

    fn on_upgrade_removed(&mut self, removed: &UpgradeMask) {
        let _ = removed;
    }

    fn upgrade_removed_effects(&self, removed: &UpgradeMask, entity: Entity, effects: Effects<'_>) {
        let _ = (removed, entity, effects);
    }

    fn on_create(&mut self, ctx: &mut CreateContext<'_>) {
        let _ = ctx;
    }

    fn on_capture(&mut self, old: PlayerId, new: PlayerId, ctx: &mut CaptureContext<'_>) {
        let _ = (old, new, ctx);
    }

    fn is_for_power(&self, power: &str) -> bool {
        let _ = power;
        false
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        let _ = (activation, ctx);
    }

    fn end_other_activation(&mut self, new_power: &str, ctx: &mut UpdateContext<'_>) {
        let _ = (new_power, ctx);
    }

    fn pending_approach_check(&self) -> Option<Entity> {
        None
    }

    fn on_attach(&mut self, now: Frame) {
        let _ = now;
    }

    fn on_created(&mut self, product: Entity, now: Frame) {
        let _ = (product, now);
    }

    fn on_product_died(&mut self, product: Entity, now: Frame) {
        let _ = (product, now);
    }
}

pub trait ModuleFieldSource {
    fn field(&self, keyword: &str) -> Option<&str>;

    fn field_numeric(&self, keyword: &str) -> Option<f32> {
        self.field(keyword)?
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
    }

    fn field_percent(&self, keyword: &str) -> Option<f32> {
        let raw = self.field(keyword)?.trim();
        raw.strip_suffix('%')
            .unwrap_or(raw)
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|v| v / 100.0)
    }

    fn field_args(&self, keyword: &str) -> Vec<&str> {
        let _ = keyword;
        Vec::new()
    }

    fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
        let first = self.field_args(keyword);
        if first.is_empty() {
            Vec::new()
        } else {
            vec![first]
        }
    }

    fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
        let _ = keyword;
        UpgradeMask::EMPTY
    }

    fn field_upgrade_group_mask(&self, keyword: &str) -> UpgradeMask {
        let _ = keyword;
        UpgradeMask::EMPTY
    }

    fn nested(&self, keyword: &str, visit: &mut dyn FnMut(&dyn ModuleFieldSource)) {
        let _ = (keyword, visit);
    }

    fn header_args(&self) -> Vec<&str> {
        Vec::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError(String);

impl FieldError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl core::fmt::Display for FieldError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct UpdateContext<'a> {
    pub entity: Entity,
    pub requester: Requester,
    pub now: Frame,
    pub rng: &'a mut Pcg32,
    pub destroy: &'a mut DestroyRequests,
    pub effects: Effects<'a>,
    pub reads: Reads<'a>,
}

#[derive(Debug, Clone, Copy)]
pub struct Death<'a> {
    pub kind: &'a str,
    pub cause: &'a str,
    pub sub_cause: &'a str,
    pub status: &'a [&'a str],
    pub upgrades: Option<&'a UpgradeMask>,
    pub killer: Option<Entity>,
    pub killer_bearing: Option<f32>,
    pub amount: f32,
}

impl Death<'_> {
    pub const NONE: Self = Self {
        kind: "",
        cause: "",
        sub_cause: "",
        status: &[],
        upgrades: None,
        killer: None,
        killer_bearing: None,
        amount: 0.0,
    };
}

pub struct DieContext<'a> {
    pub entity: Entity,
    pub now: Frame,
    pub rng: &'a mut Pcg32,
    pub destroy: &'a mut DestroyRequests,
    pub effects: Effects<'a>,
    pub death: Death<'a>,
}

pub struct UpgradeContext<'a> {
    pub entity: Entity,
    pub now: Frame,
    pub effects: Effects<'a>,
}

pub struct CreateContext<'a> {
    pub entity: Entity,
    pub now: Frame,
    pub effects: Effects<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerChange {
    Capture,
    Defect,
}

pub struct CaptureContext<'a> {
    pub entity: Entity,
    pub now: Frame,
    pub reason: OwnerChange,
    pub effects: Effects<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Activation<'a> {
    pub power: &'a str,
    pub target: Option<Entity>,
    pub at: Option<(f32, f32)>,
}

pub struct ActivateContext<'a> {
    pub entity: Entity,
    pub now: Frame,
    pub effects: Effects<'a>,
}

pub struct Effects<'a> {
    sink: Option<&'a mut dyn Any>,
}

impl<'a> Effects<'a> {
    #[must_use]
    pub const fn none() -> Self {
        Self { sink: None }
    }

    #[must_use]
    pub fn into_sink(sink: &'a mut dyn Any) -> Self {
        Self { sink: Some(sink) }
    }

    pub fn get<T: Any>(&mut self) -> Option<&mut T> {
        self.sink.as_deref_mut()?.downcast_mut::<T>()
    }

    pub fn reborrow(&mut self) -> Effects<'_> {
        Effects {
            sink: self.sink.as_deref_mut(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Reads<'a>(ReadsInner<'a>);

#[derive(Clone, Copy)]
enum ReadsInner<'a> {
    None,
    One(&'a dyn Any),
    Many(&'a [&'a dyn Any]),
}

impl<'a> Reads<'a> {
    #[must_use]
    pub const fn none() -> Self {
        Self(ReadsInner::None)
    }

    #[must_use]
    pub fn from_snapshot(snapshot: &'a dyn Any) -> Self {
        Self(ReadsInner::One(snapshot))
    }

    #[must_use]
    pub const fn from_snapshots(snapshots: &'a [&'a dyn Any]) -> Self {
        Self(ReadsInner::Many(snapshots))
    }

    #[must_use]
    pub fn get<T: Any>(&self) -> Option<&'a T> {
        match self.0 {
            ReadsInner::None => None,
            ReadsInner::One(snapshot) => snapshot.downcast_ref::<T>(),
            ReadsInner::Many(snapshots) => snapshots
                .iter()
                .find_map(|snapshot| snapshot.downcast_ref::<T>()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Requester {
    pub module: ModuleTypeId,
    pub slot: u32,
}

impl Requester {
    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u32(self.module.index());
        hasher.write_u32(self.slot);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreatedBy {
    pub creator: Entity,
    pub requester: Requester,
}

impl Component for CreatedBy {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.creator.hash_into(hasher);
        self.requester.hash_into(hasher);
    }
}

#[derive(Debug, Default)]
pub struct DestroyRequests(Vec<Entity>);

impl DestroyRequests {
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn request(&mut self, entity: Entity) {
        self.0.push(entity);
    }

    #[must_use]
    pub fn drain_sorted(&mut self) -> Vec<Entity> {
        self.0.sort_unstable();
        self.0.dedup();
        core::mem::take(&mut self.0)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StubModule;

impl Component for StubModule {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for StubModule {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StubHit {
    pub name: String,
    pub uses: u64,
}

trait ErasedModule {
    fn capabilities(&self) -> CapabilityMask;
    fn phase(&self) -> Phase;
    fn dispatch_update(
        &self,
        world: &mut World,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        effects: Effects<'_>,
        reads: Reads<'_>,
    );
    fn dispatch_die(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        effects: Effects<'_>,
        death: Death<'_>,
    );

    fn dispatch_upgrade_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        active: &UpgradeMask,
        now: Frame,
        effects: Effects<'_>,
    );

    fn would_upgrade_one(
        &self,
        world: &World,
        entity: Entity,
        slot: u32,
        active: &UpgradeMask,
    ) -> bool;

    fn dispatch_upgrade_removed_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        removed: &UpgradeMask,
        effects: Effects<'_>,
    );

    fn dispatch_create_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        now: Frame,
        effects: Effects<'_>,
    );

    fn dispatch_capture_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        change: (PlayerId, PlayerId),
        reason: OwnerChange,
        now: Frame,
        effects: Effects<'_>,
    );

    fn is_for_power_one(&self, world: &World, entity: Entity, slot: u32, power: &str) -> bool;

    fn activate_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        activation: &Activation<'_>,
        now: Frame,
        effects: Effects<'_>,
    );

    #[allow(clippy::too_many_arguments)]
    fn end_other_activation_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        new_power: &str,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        effects: Effects<'_>,
    );

    fn collect_pending_approach_checks(&self, world: &World, out: &mut Vec<(Entity, Entity)>);

    fn build(&self, fields: &dyn ModuleFieldSource, now: Frame) -> BuildOutcome;

    fn dispatch_created_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        product: Entity,
        now: Frame,
    );

    fn dispatch_product_died_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        product: Entity,
        now: Frame,
    );
}

enum BuildOutcome {
    NotLoadable,
    ConstructionFailed(FieldError),
    Built(Box<dyn ErasedPrototype>),
}

trait ErasedPrototype {
    fn attach(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        module: ModuleTypeId,
        declarations: ComponentId<ModuleDeclarations>,
    ) -> Result<u32>;
}

struct TypedPrototype<T: ModuleKind> {
    component: ComponentId<ModuleColumn<T>>,
    state: T,
}

impl<T: ModuleKind> ErasedPrototype for TypedPrototype<T> {
    fn attach(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        module: ModuleTypeId,
        declarations: ComponentId<ModuleDeclarations>,
    ) -> Result<u32> {
        push_and_declare(
            world,
            entity,
            now,
            module,
            self.component,
            declarations,
            self.state.clone(),
        )
    }
}

pub struct Prototype {
    module: ModuleTypeId,
    instance: Box<dyn ErasedPrototype>,
}

impl Prototype {
    #[must_use]
    pub const fn module(&self) -> ModuleTypeId {
        self.module
    }
}

impl core::fmt::Debug for Prototype {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Prototype")
            .field("module", &self.module)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum PrototypeOutcome {
    Built(Prototype),
    Unregistered,
    Refused,
}

type Loader<T> = Box<dyn Fn(&dyn ModuleFieldSource, Frame) -> core::result::Result<T, FieldError>>;

type NameLoader = Box<dyn Fn(&dyn ModuleFieldSource, Frame) -> BuildOutcome>;

fn name_loader<T: ModuleKind>(
    component: ComponentId<ModuleColumn<T>>,
    loader: Loader<T>,
) -> NameLoader {
    Box::new(move |fields, now| match loader(fields, now) {
        Ok(state) => BuildOutcome::Built(Box::new(TypedPrototype { component, state })),
        Err(err) => BuildOutcome::ConstructionFailed(err),
    })
}

struct Descriptor<T: ModuleKind> {
    id: ModuleTypeId,
    component: ComponentId<ModuleColumn<T>>,
    loader: Option<Loader<T>>,
}

impl<T: ModuleKind> ErasedModule for Descriptor<T> {
    fn capabilities(&self) -> CapabilityMask {
        T::CAPABILITIES
    }

    fn phase(&self) -> Phase {
        T::PHASE
    }

    fn dispatch_update(
        &self,
        world: &mut World,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        mut effects: Effects<'_>,
        reads: Reads<'_>,
    ) {
        if !T::CAPABILITIES.contains(Capability::Update) {
            return;
        }
        let Some(store) = world.store_mut(self.component) else {
            return;
        };
        let due = |column: &ModuleColumn<T>| column.0.iter().any(|slot| slot.wake <= now);
        for (entity, column) in store.iter_mut_where(due) {
            for (index, slot) in column.0.iter_mut().enumerate() {
                if slot.wake > now {
                    continue;
                }
                let mut ctx = UpdateContext {
                    entity,
                    requester: Requester {
                        module: self.id,
                        slot: u32::try_from(index).unwrap_or(u32::MAX),
                    },
                    now,
                    rng,
                    destroy,
                    effects: effects.reborrow(),
                    reads,
                };
                let sleep = slot.state.update(&mut ctx);
                slot.wake = sleep.wake_after(now);
            }
        }
    }

    fn dispatch_die(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        effects: Effects<'_>,
        death: Death<'_>,
    ) {
        if !T::CAPABILITIES.contains(Capability::Die) {
            return;
        }
        let Some(column) = world.get_mut(self.component, entity) else {
            return;
        };
        let mut ctx = DieContext {
            entity,
            now,
            rng,
            destroy,
            effects,
            death,
        };
        T::on_die(&mut column.0, &mut ctx);
    }

    fn dispatch_upgrade_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        active: &UpgradeMask,
        now: Frame,
        effects: Effects<'_>,
    ) {
        if !T::CAPABILITIES.contains(Capability::Upgrade) {
            return;
        }
        let Some(module_slot) = self.slot_mut(world, entity, slot) else {
            return;
        };
        let mut ctx = UpgradeContext {
            entity,
            now,
            effects,
        };
        module_slot.state.attempt_upgrade(active, &mut ctx);
        if T::CAPABILITIES.contains(Capability::Update) {
            module_slot.wake = now;
        }
    }

    fn would_upgrade_one(
        &self,
        world: &World,
        entity: Entity,
        slot: u32,
        active: &UpgradeMask,
    ) -> bool {
        if !T::CAPABILITIES.contains(Capability::Upgrade) {
            return false;
        }
        world
            .get(self.component, entity)
            .and_then(|column| column.0.get(to_usize(slot)))
            .is_some_and(|module_slot| module_slot.state.would_upgrade(active))
    }

    fn dispatch_upgrade_removed_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        removed: &UpgradeMask,
        effects: Effects<'_>,
    ) {
        if !T::CAPABILITIES.contains(Capability::Upgrade) {
            return;
        }
        let Some(module_slot) = self.slot_mut(world, entity, slot) else {
            return;
        };
        module_slot
            .state
            .upgrade_removed_effects(removed, entity, effects);
        module_slot.state.on_upgrade_removed(removed);
    }

    fn dispatch_create_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        now: Frame,
        effects: Effects<'_>,
    ) {
        if !T::CAPABILITIES.contains(Capability::Create) {
            return;
        }
        let Some(module_slot) = self.slot_mut(world, entity, slot) else {
            return;
        };
        let mut ctx = CreateContext {
            entity,
            now,
            effects,
        };
        module_slot.state.on_create(&mut ctx);
    }

    fn dispatch_capture_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        (old, new): (PlayerId, PlayerId),
        reason: OwnerChange,
        now: Frame,
        effects: Effects<'_>,
    ) {
        let Some(module_slot) = self.slot_mut(world, entity, slot) else {
            return;
        };
        let mut ctx = CaptureContext {
            entity,
            now,
            reason,
            effects,
        };
        module_slot.state.on_capture(old, new, &mut ctx);
    }

    fn is_for_power_one(&self, world: &World, entity: Entity, slot: u32, power: &str) -> bool {
        if !T::CAPABILITIES.contains(Capability::SpecialPower) {
            return false;
        }
        world
            .get(self.component, entity)
            .and_then(|column| column.0.get(to_usize(slot)))
            .is_some_and(|module_slot| module_slot.state.is_for_power(power))
    }

    fn activate_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        activation: &Activation<'_>,
        now: Frame,
        effects: Effects<'_>,
    ) {
        if !T::CAPABILITIES.contains(Capability::SpecialPower) {
            return;
        }
        let Some(module_slot) = self.slot_mut(world, entity, slot) else {
            return;
        };
        let mut ctx = ActivateContext {
            entity,
            now,
            effects,
        };
        module_slot.state.activate(activation, &mut ctx);
        if T::CAPABILITIES.contains(Capability::Update) {
            module_slot.wake = now;
        }
    }

    fn end_other_activation_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        new_power: &str,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        effects: Effects<'_>,
    ) {
        if !T::CAPABILITIES.contains(Capability::SpecialPower) {
            return;
        }
        let Some(module_slot) = self.slot_mut(world, entity, slot) else {
            return;
        };
        let mut ctx = UpdateContext {
            entity,
            requester: Requester {
                module: self.id,
                slot,
            },
            now,
            rng,
            destroy,
            effects,
            reads: Reads::none(),
        };
        module_slot.state.end_other_activation(new_power, &mut ctx);
    }

    fn collect_pending_approach_checks(&self, world: &World, out: &mut Vec<(Entity, Entity)>) {
        if !T::CAPABILITIES.contains(Capability::SpecialPower) {
            return;
        }
        let Some(store) = world.store(self.component) else {
            return;
        };
        for (entity, column) in store.iter() {
            for slot in &column.0 {
                if let Some(target) = slot.state.pending_approach_check() {
                    out.push((entity, target));
                }
            }
        }
    }

    fn build(&self, fields: &dyn ModuleFieldSource, now: Frame) -> BuildOutcome {
        let Some(loader) = &self.loader else {
            return BuildOutcome::NotLoadable;
        };
        match loader(fields, now) {
            Ok(state) => BuildOutcome::Built(Box::new(TypedPrototype {
                component: self.component,
                state,
            })),
            Err(err) => BuildOutcome::ConstructionFailed(err),
        }
    }

    fn dispatch_created_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        product: Entity,
        now: Frame,
    ) {
        if let Some(module_slot) = self.slot_mut(world, entity, slot) {
            module_slot.state.on_created(product, now);
        }
    }

    fn dispatch_product_died_one(
        &self,
        world: &mut World,
        entity: Entity,
        slot: u32,
        product: Entity,
        now: Frame,
    ) {
        if let Some(module_slot) = self.slot_mut(world, entity, slot) {
            module_slot.state.on_product_died(product, now);
        }
    }
}

impl<T: ModuleKind> Descriptor<T> {
    fn slot_mut<'w>(
        &self,
        world: &'w mut World,
        entity: Entity,
        slot: u32,
    ) -> Option<&'w mut ModuleSlot<T>> {
        world
            .store_mut(self.component)?
            .get_mut(entity)?
            .0
            .get_mut(to_usize(slot))
    }
}

fn push_and_declare<T: ModuleKind>(
    world: &mut World,
    entity: Entity,
    now: Frame,
    module_id: ModuleTypeId,
    component: ComponentId<ModuleColumn<T>>,
    declarations: ComponentId<ModuleDeclarations>,
    mut state: T,
) -> Result<u32> {
    if !world
        .store(component)
        .is_some_and(|store| store.contains(entity))
    {
        world.insert(component, entity, ModuleColumn::new())?;
    }
    let store = world
        .store_mut(component)
        .ok_or(Error::ModuleTypeNotRegistered)?;
    let Some(column) = store.get_mut(entity) else {
        return Err(Error::ModuleTypeNotRegistered);
    };
    let slot = u32::try_from(column.0.len()).unwrap_or(u32::MAX);
    state.on_attach(now);
    column.0.push(ModuleSlot { wake: now, state });

    if let Some(decls) = world.get_mut(declarations, entity) {
        decls.push(module_id, slot);
    } else {
        let mut decls = ModuleDeclarations::default();
        decls.push(module_id, slot);
        world.insert(declarations, entity, decls)?;
    }
    Ok(slot)
}

pub struct ModuleRegistry {
    by_name: Vec<(String, ModuleTypeId, Option<NameLoader>)>,
    type_ids: Vec<(TypeId, ModuleTypeId)>,
    descriptors: Vec<Box<dyn ErasedModule>>,
    declarations: ComponentId<ModuleDeclarations>,
    created_by: ComponentId<CreatedBy>,
    stub: ModuleTypeId,
    strict: bool,
    tally: RefCell<Vec<StubHit>>,
}

impl ModuleRegistry {
    #[must_use]
    pub fn new(world: &mut World) -> Self {
        let mut registry = Self {
            by_name: Vec::new(),
            type_ids: Vec::new(),
            descriptors: Vec::new(),
            declarations: world.register::<ModuleDeclarations>(),
            created_by: world.register::<CreatedBy>(),
            stub: ModuleTypeId::new(0),
            strict: false,
            tally: RefCell::new(Vec::new()),
        };
        registry.stub = registry.register::<StubModule>(world, "<stub>");
        registry
    }

    pub fn adopt_created(
        &self,
        world: &mut World,
        creator: Entity,
        requester: Requester,
        product: Entity,
        now: Frame,
    ) -> Result<()> {
        world.insert(self.created_by, product, CreatedBy { creator, requester })?;
        if let Some(descriptor) = self.descriptors.get(to_usize(requester.module.index())) {
            descriptor.dispatch_created_one(world, creator, requester.slot, product, now);
        }
        Ok(())
    }

    pub fn product_died(&self, world: &mut World, product: Entity, now: Frame) {
        let Some(link) = world.get(self.created_by, product).copied() else {
            return;
        };
        if let Some(descriptor) = self
            .descriptors
            .get(to_usize(link.requester.module.index()))
        {
            descriptor.dispatch_product_died_one(
                world,
                link.creator,
                link.requester.slot,
                product,
                now,
            );
        }
    }

    #[must_use]
    pub fn created_by(&self, world: &World, product: Entity) -> Option<CreatedBy> {
        world.get(self.created_by, product).copied()
    }

    #[must_use]
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    pub fn register<T: ModuleKind>(&mut self, world: &mut World, name: &str) -> ModuleTypeId {
        self.register_impl::<T>(world, name, None)
    }

    pub fn register_loadable<T: ModuleKind>(
        &mut self,
        world: &mut World,
        name: &str,
        from_fields: impl Fn(&dyn ModuleFieldSource, Frame) -> core::result::Result<T, FieldError>
            + 'static,
    ) -> ModuleTypeId {
        self.register_impl(world, name, Some(Box::new(from_fields)))
    }

    fn register_impl<T: ModuleKind>(
        &mut self,
        world: &mut World,
        name: &str,
        loader: Option<Loader<T>>,
    ) -> ModuleTypeId {
        let type_id = TypeId::of::<T>();
        if let Some(&(_, id)) = self.type_ids.iter().find(|(t, _)| *t == type_id) {
            if !self.by_name.iter().any(|(n, _, _)| n == name) {
                let component = world.register::<ModuleColumn<T>>();
                let built = loader.map(|loader| name_loader(component, loader));
                self.by_name.push((name.to_owned(), id, built));
            }
            return id;
        }
        let id = ModuleTypeId::new(u32::try_from(self.descriptors.len()).unwrap_or(u32::MAX));
        let component = world.register::<ModuleColumn<T>>();
        self.descriptors.push(Box::new(Descriptor::<T> {
            id,
            component,
            loader,
        }));
        self.by_name.push((name.to_owned(), id, None));
        self.type_ids.push((type_id, id));
        id
    }

    #[must_use]
    pub fn names_with(&self, capability: Capability) -> Vec<&str> {
        self.by_name
            .iter()
            .filter(|(name, _, _)| {
                self.capabilities_of(name)
                    .is_some_and(|mask| mask.contains(capability))
            })
            .map(|(name, _, _)| name.as_str())
            .collect()
    }

    #[must_use]
    pub fn is_registered(&self, name: &str) -> bool {
        self.by_name.iter().any(|(n, _, _)| n == name)
    }

    #[must_use]
    pub fn capabilities_of(&self, name: &str) -> Option<CapabilityMask> {
        let (_, id, _) = self.by_name.iter().find(|(n, _, _)| n == name)?;
        self.descriptors
            .get(to_usize(id.index()))
            .map(|descriptor| descriptor.capabilities())
    }

    pub fn attach<T: ModuleKind>(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        state: T,
    ) -> Result<u32> {
        let type_id = TypeId::of::<T>();
        let module_id = self
            .type_ids
            .iter()
            .find(|(t, _)| *t == type_id)
            .map(|(_, id)| *id)
            .ok_or(Error::ModuleTypeNotRegistered)?;
        let component = world.register::<ModuleColumn<T>>();
        self.attach_state(world, entity, now, module_id, component, state)
    }

    pub fn attach_stub(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        name: &str,
    ) -> Result<u32> {
        if self.strict {
            return Err(Error::UnregisteredModule {
                name: name.to_owned(),
            });
        }
        if self.tally_first_seen(name) {
            eprintln!("mint-sim: module '{name}' is not registered; loaded as an inert stub");
        }
        self.attach_stub_unlogged(world, entity, now)
    }

    fn tally_first_seen(&self, name: &str) -> bool {
        let mut tally = self.tally.borrow_mut();
        match tally.iter_mut().find(|hit| hit.name == name) {
            Some(hit) => {
                hit.uses = hit.uses.saturating_add(1);
                false
            }
            None => {
                tally.push(StubHit {
                    name: name.to_owned(),
                    uses: 1,
                });
                true
            }
        }
    }

    fn attach_stub_unlogged(&self, world: &mut World, entity: Entity, now: Frame) -> Result<u32> {
        let component = world.register::<ModuleColumn<StubModule>>();
        self.attach_state(world, entity, now, self.stub, component, StubModule)
    }

    fn attach_state<T: ModuleKind>(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        module_id: ModuleTypeId,
        component: ComponentId<ModuleColumn<T>>,
        state: T,
    ) -> Result<u32> {
        push_and_declare(
            world,
            entity,
            now,
            module_id,
            component,
            self.declarations,
            state,
        )
    }

    pub fn attach_named(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        name: &str,
        fields: &dyn ModuleFieldSource,
    ) -> Result<u32> {
        let Some((_, module_id, _)) = self.by_name.iter().find(|(n, _, _)| n == name) else {
            return self.attach_stub(world, entity, now, name);
        };
        let module_id = *module_id;
        let Some(outcome) = self.build_named(name, fields, now) else {
            return self.attach_stub(world, entity, now, name);
        };
        match outcome {
            BuildOutcome::Built(instance) => {
                instance.attach(world, entity, now, module_id, self.declarations)
            }
            outcome => {
                self.refuse(name, &outcome)?;
                self.attach_stub_unlogged(world, entity, now)
            }
        }
    }

    fn build_named(
        &self,
        name: &str,
        fields: &dyn ModuleFieldSource,
        now: Frame,
    ) -> Option<BuildOutcome> {
        let (_, module_id, named) = self.by_name.iter().find(|(n, _, _)| n == name)?;
        if let Some(named) = named {
            return Some(named(fields, now));
        }
        self.descriptors
            .get(to_usize(module_id.index()))
            .map(|descriptor| descriptor.build(fields, now))
    }

    fn refuse(&self, name: &str, outcome: &BuildOutcome) -> Result<()> {
        if self.strict {
            return Err(Error::UnregisteredModule {
                name: name.to_owned(),
            });
        }
        if self.tally_first_seen(name) {
            match outcome {
                BuildOutcome::ConstructionFailed(err) => eprintln!(
                    "mint-sim: module '{name}' is registered but its fields did not parse \
                     ({err}); loaded as an inert stub"
                ),
                _ => eprintln!(
                    "mint-sim: module '{name}' is registered but has no data constructor; \
                     loaded as an inert stub"
                ),
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn check(
        &self,
        name: &str,
        fields: &dyn ModuleFieldSource,
        now: Frame,
    ) -> Option<FieldError> {
        match self.build_named(name, fields, now)? {
            BuildOutcome::ConstructionFailed(err) => Some(err),
            BuildOutcome::Built(_) | BuildOutcome::NotLoadable => None,
        }
    }

    pub fn prototype(
        &self,
        name: &str,
        fields: &dyn ModuleFieldSource,
        now: Frame,
    ) -> Result<PrototypeOutcome> {
        let Some(&(_, module, _)) = self.by_name.iter().find(|(n, _, _)| n == name) else {
            return Ok(PrototypeOutcome::Unregistered);
        };
        let Some(outcome) = self.build_named(name, fields, now) else {
            return Ok(PrototypeOutcome::Unregistered);
        };
        match outcome {
            BuildOutcome::Built(instance) => {
                Ok(PrototypeOutcome::Built(Prototype { module, instance }))
            }
            outcome => {
                self.refuse(name, &outcome)?;
                Ok(PrototypeOutcome::Refused)
            }
        }
    }

    pub fn attach_prototype(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        prototype: &Prototype,
    ) -> Result<u32> {
        prototype
            .instance
            .attach(world, entity, now, prototype.module, self.declarations)
    }

    pub fn update(
        &self,
        world: &mut World,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
    ) {
        self.update_into(world, now, rng, destroy, Effects::none());
    }

    pub fn update_into(
        &self,
        world: &mut World,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        effects: Effects<'_>,
    ) {
        self.update_with_reads(world, now, rng, destroy, effects, Reads::none());
    }

    pub fn update_with_reads(
        &self,
        world: &mut World,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        mut effects: Effects<'_>,
        reads: Reads<'_>,
    ) {
        for phase in Phase::ORDER {
            for descriptor in &self.descriptors {
                if descriptor.phase() != phase {
                    continue;
                }
                descriptor.dispatch_update(world, now, rng, destroy, effects.reborrow(), reads);
            }
        }
    }

    pub fn broadcast_die(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
    ) {
        self.broadcast_die_into(
            world,
            entity,
            now,
            rng,
            destroy,
            Effects::none(),
            Death::NONE,
        );
    }

    pub fn broadcast_die_into(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        mut effects: Effects<'_>,
        death: Death<'_>,
    ) {
        let Some(declarations) = world.get(self.declarations, entity) else {
            return;
        };
        let order = declarations.order().to_vec();
        let mut dispatched: Vec<ModuleTypeId> = Vec::new();
        for (module_id, _slot) in order {
            if dispatched.contains(&module_id) {
                continue;
            }
            dispatched.push(module_id);
            let Some(descriptor) = self.descriptors.get(to_usize(module_id.index())) else {
                continue;
            };
            descriptor.dispatch_die(world, entity, now, rng, destroy, effects.reborrow(), death);
        }
    }

    pub fn broadcast_upgrade(
        &self,
        world: &mut World,
        entity: Entity,
        active: &UpgradeMask,
        now: Frame,
    ) {
        self.broadcast_upgrade_into(world, entity, active, now, Effects::none());
    }

    pub fn broadcast_upgrade_into(
        &self,
        world: &mut World,
        entity: Entity,
        active: &UpgradeMask,
        now: Frame,
        mut effects: Effects<'_>,
    ) {
        let Some(declarations) = world.get(self.declarations, entity) else {
            return;
        };
        let order = declarations.order().to_vec();
        for (module_id, slot) in order {
            let Some(descriptor) = self.descriptors.get(to_usize(module_id.index())) else {
                continue;
            };
            descriptor.dispatch_upgrade_one(world, entity, slot, active, now, effects.reborrow());
        }
    }

    pub fn broadcast_upgrade_removed(
        &self,
        world: &mut World,
        entity: Entity,
        removed: &UpgradeMask,
        mut effects: Effects<'_>,
    ) {
        let Some(declarations) = world.get(self.declarations, entity) else {
            return;
        };
        let order = declarations.order().to_vec();
        for (module_id, slot) in order {
            let Some(descriptor) = self.descriptors.get(to_usize(module_id.index())) else {
                continue;
            };
            descriptor.dispatch_upgrade_removed_one(
                world,
                entity,
                slot,
                removed,
                effects.reborrow(),
            );
        }
    }

    #[must_use]
    pub fn any_would_upgrade(&self, world: &World, entity: Entity, active: &UpgradeMask) -> bool {
        let Some(declarations) = world.get(self.declarations, entity) else {
            return false;
        };
        declarations.order().iter().any(|(module_id, slot)| {
            self.descriptors
                .get(to_usize(module_id.index()))
                .is_some_and(|descriptor| {
                    descriptor.would_upgrade_one(world, entity, *slot, active)
                })
        })
    }

    #[must_use]
    pub fn any_would_upgrade_except(
        &self,
        world: &World,
        entity: Entity,
        active: &UpgradeMask,
        discount: &dyn Fn(ModuleTypeId, u32) -> bool,
    ) -> bool {
        let Some(declarations) = world.get(self.declarations, entity) else {
            return false;
        };
        declarations.order().iter().any(|(module_id, slot)| {
            self.descriptors
                .get(to_usize(module_id.index()))
                .is_some_and(|descriptor| {
                    descriptor.would_upgrade_one(world, entity, *slot, active)
                })
                && !discount(*module_id, *slot)
        })
    }

    #[must_use]
    pub fn type_id_of<T: ModuleKind>(&self) -> Option<ModuleTypeId> {
        let type_id = TypeId::of::<T>();
        self.type_ids
            .iter()
            .find(|(t, _)| *t == type_id)
            .map(|(_, id)| *id)
    }

    pub fn broadcast_create(
        &self,
        world: &mut World,
        entity: Entity,
        now: Frame,
        mut effects: Effects<'_>,
    ) {
        let Some(declarations) = world.get(self.declarations, entity) else {
            return;
        };
        let order = declarations.order().to_vec();
        for (module_id, slot) in order {
            let Some(descriptor) = self.descriptors.get(to_usize(module_id.index())) else {
                continue;
            };
            descriptor.dispatch_create_one(world, entity, slot, now, effects.reborrow());
        }
    }

    pub fn broadcast_capture(
        &self,
        world: &mut World,
        entity: Entity,
        old: PlayerId,
        new: PlayerId,
        reason: OwnerChange,
        now: Frame,
        mut effects: Effects<'_>,
    ) {
        let Some(declarations) = world.get(self.declarations, entity) else {
            return;
        };
        let order = declarations.order().to_vec();
        for (module_id, slot) in order {
            let Some(descriptor) = self.descriptors.get(to_usize(module_id.index())) else {
                continue;
            };
            descriptor.dispatch_capture_one(
                world,
                entity,
                slot,
                (old, new),
                reason,
                now,
                effects.reborrow(),
            );
        }
    }

    fn power_module(
        &self,
        world: &World,
        entity: Entity,
        power: &str,
    ) -> Option<(ModuleTypeId, u32)> {
        let declarations = world.get(self.declarations, entity)?;
        declarations
            .order()
            .iter()
            .copied()
            .find(|(module_id, slot)| {
                self.descriptors
                    .get(to_usize(module_id.index()))
                    .is_some_and(|descriptor| {
                        descriptor.is_for_power_one(world, entity, *slot, power)
                    })
            })
    }

    #[must_use]
    pub fn answers_power(&self, world: &World, entity: Entity, power: &str) -> bool {
        self.power_module(world, entity, power).is_some()
    }

    pub fn activate(
        &self,
        world: &mut World,
        entity: Entity,
        activation: &Activation<'_>,
        now: Frame,
        mut effects: Effects<'_>,
    ) -> bool {
        let Some(declarations) = world.get(self.declarations, entity) else {
            return false;
        };
        let order = declarations.order().to_vec();
        let mut answered = false;
        for (module_id, slot) in order {
            let Some(descriptor) = self.descriptors.get(to_usize(module_id.index())) else {
                continue;
            };
            if !descriptor.is_for_power_one(world, entity, slot, activation.power) {
                continue;
            }
            descriptor.activate_one(world, entity, slot, activation, now, effects.reborrow());
            answered = true;
        }
        answered
    }

    pub fn end_other_activations(
        &self,
        world: &mut World,
        entity: Entity,
        new_power: &str,
        now: Frame,
        rng: &mut Pcg32,
        destroy: &mut DestroyRequests,
        mut effects: Effects<'_>,
    ) {
        let Some(declarations) = world.get(self.declarations, entity) else {
            return;
        };
        let order = declarations.order().to_vec();
        for (module_id, slot) in order {
            let Some(descriptor) = self.descriptors.get(to_usize(module_id.index())) else {
                continue;
            };
            descriptor.end_other_activation_one(
                world,
                entity,
                slot,
                new_power,
                now,
                rng,
                destroy,
                effects.reborrow(),
            );
        }
    }

    #[must_use]
    pub fn collect_pending_approach_checks(&self, world: &World) -> Vec<(Entity, Entity)> {
        let mut out = Vec::new();
        for descriptor in &self.descriptors {
            descriptor.collect_pending_approach_checks(world, &mut out);
        }
        out
    }

    #[must_use]
    pub fn tally(&self) -> Vec<StubHit> {
        self.tally.borrow().clone()
    }
}

#[cfg(test)]
mod tests;
