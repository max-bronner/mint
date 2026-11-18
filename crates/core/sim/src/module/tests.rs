use super::*;

#[test]
fn sleep_resolves_relative_to_the_frame_update_ran_on() {
    let now = Frame::new(100);
    assert_eq!(
        Sleep::NextFrame.wake_after(now),
        Frame::new(101),
        "next frame is exactly one frame later, not a fixed constant"
    );
    assert_eq!(
        Sleep::Frames(LogicFrames::from_frames(30)).wake_after(now),
        Frame::new(130)
    );
    assert_eq!(Sleep::Forever.wake_after(now), Frame::FOREVER);
}

#[test]
fn sleep_saturates_rather_than_overflows_at_the_end_of_the_timeline() {
    let almost_never = Frame::new(u64::MAX - 1);
    assert_eq!(Sleep::NextFrame.wake_after(almost_never), Frame::FOREVER);
}

#[test]
fn a_capability_mask_holds_exactly_what_it_was_given() {
    let mask = CapabilityMask::of(&[Capability::Update, Capability::Die]);
    assert!(mask.contains(Capability::Update));
    assert!(mask.contains(Capability::Die));
    assert!(!mask.contains(Capability::Damage));
    assert!(!CapabilityMask::NONE.contains(Capability::Update));
}

#[test]
fn every_capability_has_a_distinct_bit() {
    let all = [
        Capability::Update,
        Capability::Die,
        Capability::Damage,
        Capability::Create,
        Capability::Collide,
        Capability::Body,
        Capability::Contain,
        Capability::Upgrade,
        Capability::SpecialPower,
        Capability::Destroy,
        Capability::Draw,
        Capability::ClientUpdate,
    ];
    for (i, a) in all.iter().enumerate() {
        for (j, b) in all.iter().enumerate() {
            let mask = CapabilityMask::single(*a);
            assert_eq!(mask.contains(*b), i == j, "{a:?} vs {b:?}");
        }
    }
}

#[test]
fn phases_run_in_the_declared_order() {
    assert_eq!(
        Phase::ORDER,
        [Phase::Initial, Phase::Physics, Phase::Normal, Phase::Final]
    );
    assert!(Phase::Initial < Phase::Physics);
    assert!(Phase::Physics < Phase::Normal);
    assert!(Phase::Normal < Phase::Final);
}

#[derive(Debug, Clone, Copy)]
struct Countdown {
    expires: Frame,
    ticks: u32,
}

impl Component for Countdown {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.expires.hash_into(hasher);
        hasher.write_u32(self.ticks);
    }
}

impl ModuleKind for Countdown {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        self.ticks = self.ticks.saturating_add(1);
        if ctx.now >= self.expires {
            ctx.destroy.request(ctx.entity);
            return Sleep::Forever;
        }
        match self.expires.since(ctx.now) {
            Some(remaining) => Sleep::Frames(remaining),
            None => Sleep::Forever,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Recorder {
    seen: bool,
    calls: u32,
}

impl Component for Recorder {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(u8::from(self.seen));
        hasher.write_u32(self.calls);
    }
}

impl ModuleKind for Recorder {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Die);

    fn on_die(instances: &mut [ModuleSlot<Self>], _ctx: &mut DieContext<'_>) {
        for slot in instances {
            slot.state.seen = true;
            slot.state.calls = slot.state.calls.saturating_add(1);
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PhaseMarker(Option<Frame>);

impl Component for PhaseMarker {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self.0 {
            Some(frame) => frame.hash_into(hasher),
            None => hasher.write_u8(0xFF),
        }
    }
}

impl ModuleKind for PhaseMarker {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);
    const PHASE: Phase = Phase::Initial;

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        self.0 = Some(ctx.now);
        Sleep::Forever
    }
}

#[test]
fn a_type_registers_once_and_returns_a_stable_id() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    let first = registry.register::<Countdown>(&mut world, "Countdown");
    let second = registry.register::<Countdown>(&mut world, "Countdown (again)");
    assert_eq!(
        first, second,
        "registering the same type twice is idempotent"
    );
}

#[test]
fn capabilities_of_reports_by_name_and_none_for_the_unknown() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<Countdown>(&mut world, "Countdown");
    registry.register::<Recorder>(&mut world, "Recorder");
    assert_eq!(
        registry.capabilities_of("Countdown"),
        Some(CapabilityMask::single(Capability::Update))
    );
    assert_eq!(
        registry.capabilities_of("Recorder"),
        Some(CapabilityMask::single(Capability::Die))
    );
    assert_eq!(registry.capabilities_of("NeverRegistered"), None);
}

#[test]
fn attaching_an_unregistered_type_is_an_error_not_a_panic() {
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world);
    let entity = world.spawn().unwrap();
    assert_eq!(
        registry.attach(
            &mut world,
            entity,
            Frame::ZERO,
            Countdown {
                expires: Frame::ZERO,
                ticks: 0
            }
        ),
        Err(Error::ModuleTypeNotRegistered)
    );
}

#[test]
fn a_wake_frame_is_never_visited_before_it_arrives() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<Countdown>(&mut world, "Countdown");
    let entity = world.spawn().unwrap();
    registry
        .attach(
            &mut world,
            entity,
            Frame::ZERO,
            Countdown {
                expires: Frame::new(5),
                ticks: 0,
            },
        )
        .unwrap();

    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    for f in 0..5 {
        registry.update(&mut world, Frame::new(f), &mut rng, &mut destroy);
    }
    let id = component_id_for_countdown(&mut world);
    let ticks_before = world
        .store(id)
        .and_then(|store| store.get(entity))
        .map(|column| column.0[0].state.ticks);
    assert_eq!(
        ticks_before,
        Some(1),
        "must not have been called again before frame 5"
    );
    assert!(destroy.drain_sorted().is_empty());

    registry.update(&mut world, Frame::new(5), &mut rng, &mut destroy);
    let ticks_after = world
        .store(id)
        .and_then(|store| store.get(entity))
        .map(|column| column.0[0].state.ticks);
    assert_eq!(ticks_after, Some(2), "due exactly at its wake frame");
    assert_eq!(destroy.drain_sorted(), vec![entity]);
}

fn component_id_for_countdown(world: &mut World) -> ComponentId<ModuleColumn<Countdown>> {
    world.register::<ModuleColumn<Countdown>>()
}

#[test]
fn update_dispatch_honours_phase_before_registration_order() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<Countdown>(&mut world, "Countdown");
    registry.register::<PhaseMarker>(&mut world, "PhaseMarker");
    let entity = world.spawn().unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, PhaseMarker::default())
        .unwrap();

    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    registry.update(&mut world, Frame::new(3), &mut rng, &mut destroy);

    let id = world.register::<ModuleColumn<PhaseMarker>>();
    let column = world.store(id).unwrap();
    assert_eq!(
        column.get(entity).unwrap().0[0].state.0,
        Some(Frame::new(3))
    );
}

#[test]
fn event_dispatch_visits_a_group_exactly_once_regardless_of_slot_count() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<Recorder>(&mut world, "Recorder");
    let entity = world.spawn().unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, Recorder::default())
        .unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, Recorder::default())
        .unwrap();

    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    registry.broadcast_die(&mut world, entity, Frame::ZERO, &mut rng, &mut destroy);

    let id = world.register::<ModuleColumn<Recorder>>();
    let column = world.store(id).unwrap();
    let instances = &column.get(entity).unwrap().0;
    assert_eq!(instances.len(), 2);
    assert!(instances.iter().all(|slot| slot.state.seen));
    assert!(
        instances.iter().all(|slot| slot.state.calls == 1),
        "the group callback must run exactly once per broadcast, not once per slot: {:?}",
        instances
            .iter()
            .map(|slot| slot.state.calls)
            .collect::<Vec<_>>()
    );
}

#[derive(Debug, Clone, Default)]
struct UpgradeToyA {
    offered: Vec<UpgradeMask>,
}

impl Component for UpgradeToyA {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.offered.len());
    }
}

impl ModuleKind for UpgradeToyA {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade);

    fn attempt_upgrade(&mut self, active: &UpgradeMask, _ctx: &mut UpgradeContext) {
        self.offered.push(active.clone());
    }
}

#[test]
fn broadcast_upgrade_offers_every_instance_independently_not_once_per_type() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<UpgradeToyA>(&mut world, "UpgradeToyA");
    let entity = world.spawn().unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, UpgradeToyA::default())
        .unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, UpgradeToyA::default())
        .unwrap();

    let mut active = UpgradeMask::EMPTY;
    active.set(UpgradeBit::new(3));
    registry.broadcast_upgrade(&mut world, entity, &active, Frame::ZERO);

    let id = world.register::<ModuleColumn<UpgradeToyA>>();
    let column = world.store(id).unwrap();
    let instances = &column.get(entity).unwrap().0;
    assert_eq!(instances.len(), 2);
    assert!(
        instances.iter().all(|slot| slot.state.offered.len() == 1),
        "both instances must be offered independently, not grouped: {:?}",
        instances
            .iter()
            .map(|slot| slot.state.offered.len())
            .collect::<Vec<_>>()
    );
    assert!(instances[0].state.offered[0].contains(UpgradeBit::new(3)));
}

#[derive(Debug, Clone, Default)]
struct WouldToy {
    bit: u32,
    fired: bool,
}

impl Component for WouldToy {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.fired);
    }
}

impl ModuleKind for WouldToy {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade);

    fn attempt_upgrade(&mut self, active: &UpgradeMask, _ctx: &mut UpgradeContext) {
        self.fired |= active.contains(UpgradeBit::new(self.bit));
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        !self.fired && active.contains(UpgradeBit::new(self.bit))
    }

    fn on_upgrade_removed(&mut self, removed: &UpgradeMask) {
        if removed.contains(UpgradeBit::new(self.bit)) {
            self.fired = false;
        }
    }
}

#[test]
fn any_would_upgrade_asks_every_module_and_fires_none() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<WouldToy>(&mut world, "WouldToy");
    registry.register::<UpgradeToyA>(&mut world, "UpgradeToyA");
    let entity = world.spawn().unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, UpgradeToyA::default())
        .unwrap();
    registry
        .attach(
            &mut world,
            entity,
            Frame::ZERO,
            WouldToy {
                bit: 7,
                fired: false,
            },
        )
        .unwrap();
    let mut seven = UpgradeMask::EMPTY;
    seven.set(UpgradeBit::new(7));
    let mut two = UpgradeMask::EMPTY;
    two.set(UpgradeBit::new(2));
    let before = world.state_hash();
    assert!(registry.any_would_upgrade(&world, entity, &seven));
    assert!(!registry.any_would_upgrade(&world, entity, &two));
    assert_eq!(world.state_hash(), before, "asking must not fire anything");
    let other = world.spawn().unwrap();
    assert!(
        !registry.any_would_upgrade(&world, other, &seven),
        "no modules"
    );
    registry.broadcast_upgrade(&mut world, entity, &seven, Frame::ZERO);
    assert!(
        !registry.any_would_upgrade(&world, entity, &seven),
        "a fired module no longer would"
    );
}

#[test]
fn broadcast_upgrade_removed_re_arms_only_the_module_whose_bit_was_removed() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<WouldToy>(&mut world, "WouldToy");
    let entity = world.spawn().unwrap();
    registry
        .attach(
            &mut world,
            entity,
            Frame::ZERO,
            WouldToy {
                bit: 7,
                fired: false,
            },
        )
        .unwrap();
    registry
        .attach(
            &mut world,
            entity,
            Frame::ZERO,
            WouldToy {
                bit: 2,
                fired: false,
            },
        )
        .unwrap();
    let mut seven = UpgradeMask::EMPTY;
    seven.set(UpgradeBit::new(7));
    let mut two = UpgradeMask::EMPTY;
    two.set(UpgradeBit::new(2));

    registry.broadcast_upgrade(&mut world, entity, &seven, Frame::ZERO);
    registry.broadcast_upgrade(&mut world, entity, &two, Frame::ZERO);
    assert!(!registry.any_would_upgrade(&world, entity, &seven));
    assert!(!registry.any_would_upgrade(&world, entity, &two));

    registry.broadcast_upgrade_removed(&mut world, entity, &seven, Effects::none());
    assert!(
        registry.any_would_upgrade(&world, entity, &seven),
        "the bit-7 module re-armed"
    );
    assert!(
        !registry.any_would_upgrade(&world, entity, &two),
        "the bit-2 module was untouched"
    );

    registry.broadcast_upgrade(&mut world, entity, &seven, Frame::ZERO);
    assert!(
        !registry.any_would_upgrade(&world, entity, &seven),
        "fires a second time, then latches again"
    );
}

#[derive(Debug, Clone)]
struct UpgradeLoggerA(std::rc::Rc<RefCell<Vec<&'static str>>>);
impl Component for UpgradeLoggerA {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}
impl ModuleKind for UpgradeLoggerA {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade);
    fn attempt_upgrade(&mut self, _active: &UpgradeMask, _ctx: &mut UpgradeContext) {
        self.0.borrow_mut().push("A");
    }
}

#[derive(Debug, Clone)]
struct UpgradeLoggerB(std::rc::Rc<RefCell<Vec<&'static str>>>);
impl Component for UpgradeLoggerB {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}
impl ModuleKind for UpgradeLoggerB {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade);
    fn attempt_upgrade(&mut self, _active: &UpgradeMask, _ctx: &mut UpgradeContext) {
        self.0.borrow_mut().push("B");
    }
}

#[test]
fn broadcast_upgrade_visits_the_entitys_own_declaration_order() {
    let log = std::rc::Rc::new(RefCell::new(Vec::new()));

    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<UpgradeLoggerA>(&mut world, "UpgradeLoggerA");
    registry.register::<UpgradeLoggerB>(&mut world, "UpgradeLoggerB");
    let entity = world.spawn().unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, UpgradeLoggerB(log.clone()))
        .unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, UpgradeLoggerA(log.clone()))
        .unwrap();

    registry.broadcast_upgrade(&mut world, entity, &UpgradeMask::EMPTY, Frame::ZERO);

    assert_eq!(
        *log.borrow(),
        vec!["B", "A"],
        "declaration order, not registration order"
    );
}

#[test]
fn broadcast_upgrade_is_a_no_op_for_a_module_with_no_upgrade_capability() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<Countdown>(&mut world, "Countdown");
    let entity = world.spawn().unwrap();
    registry
        .attach(
            &mut world,
            entity,
            Frame::ZERO,
            Countdown {
                expires: Frame::new(5),
                ticks: 0,
            },
        )
        .unwrap();

    registry.broadcast_upgrade(&mut world, entity, &UpgradeMask::EMPTY, Frame::ZERO);
}

#[test]
fn broadcast_upgrade_on_an_entity_with_no_declarations_at_all_does_nothing() {
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world);
    let entity = world.spawn().unwrap();
    registry.broadcast_upgrade(&mut world, entity, &UpgradeMask::EMPTY, Frame::ZERO);
}

#[test]
fn an_unregistered_name_attaches_as_an_inert_stub_and_is_tallied_once_per_name() {
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world);
    let entity = world.spawn().unwrap();

    registry
        .attach_stub(&mut world, entity, Frame::ZERO, "ForgottenPowerModule")
        .unwrap();
    registry
        .attach_stub(&mut world, entity, Frame::ZERO, "ForgottenPowerModule")
        .unwrap();
    registry
        .attach_stub(&mut world, entity, Frame::ZERO, "AnotherUnknownModule")
        .unwrap();

    let tally = registry.tally();
    assert_eq!(tally.len(), 2);
    let forgotten = tally
        .iter()
        .find(|hit| hit.name == "ForgottenPowerModule")
        .unwrap();
    assert_eq!(forgotten.uses, 2, "every use counted, not deduplicated");
}

#[test]
fn a_stub_is_inert_it_never_runs_update_or_die() {
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world);
    let entity = world.spawn().unwrap();
    registry
        .attach_stub(&mut world, entity, Frame::ZERO, "SomeUnknownModule")
        .unwrap();

    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    registry.update(&mut world, Frame::new(1), &mut rng, &mut destroy);
    registry.broadcast_die(&mut world, entity, Frame::new(1), &mut rng, &mut destroy);
    assert!(destroy.drain_sorted().is_empty());

    let declarations_id = component_id_for_declarations(&mut world);
    let declarations = world.get(declarations_id, entity).unwrap();
    assert_eq!(declarations.order().len(), 1);
}

fn component_id_for_declarations(world: &mut World) -> ComponentId<ModuleDeclarations> {
    world.register::<ModuleDeclarations>()
}

#[test]
fn strict_mode_refuses_an_unregistered_name_instead_of_stubbing_it() {
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world).with_strict(true);
    let entity = world.spawn().unwrap();
    assert_eq!(
        registry.attach_stub(&mut world, entity, Frame::ZERO, "SomeUnknownModule"),
        Err(Error::UnregisteredModule {
            name: "SomeUnknownModule".to_owned()
        })
    );
    assert!(registry.tally().is_empty(), "a refused name is not tallied");
}

#[test]
fn destroy_requests_drain_sorted_and_deduplicated() {
    let mut world = World::new();
    let a = world.spawn().unwrap();
    let b = world.spawn().unwrap();
    let mut destroy = DestroyRequests::new();
    destroy.request(b);
    destroy.request(a);
    destroy.request(b);
    assert_eq!(destroy.drain_sorted(), vec![a, b]);
    assert!(destroy.drain_sorted().is_empty(), "the queue is left empty");
}

struct TestFields(std::collections::BTreeMap<&'static str, &'static str>);

impl ModuleFieldSource for TestFields {
    fn field(&self, keyword: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
            .map(|(_, v)| *v)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Weighted(u32);

impl Component for Weighted {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.0);
    }
}

impl ModuleKind for Weighted {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);
}

fn weighted_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> core::result::Result<Weighted, FieldError> {
    let raw = fields
        .field("Weight")
        .ok_or_else(|| FieldError::new("missing Weight"))?;
    let value: u32 = raw
        .parse()
        .map_err(|_| FieldError::new(format!("Weight '{raw}' is not an integer")))?;
    Ok(Weighted(value))
}

#[test]
fn attach_named_builds_a_registered_loadable_type_from_fields() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Weighted", weighted_from_fields);
    let entity = world.spawn().unwrap();
    let fields = TestFields(std::collections::BTreeMap::from([("Weight", "7")]));

    registry
        .attach_named(&mut world, entity, Frame::ZERO, "Weighted", &fields)
        .unwrap();

    let id = world.register::<ModuleColumn<Weighted>>();
    let column = world.store(id).unwrap();
    assert_eq!(column.get(entity).unwrap().0[0].state, Weighted(7));
    assert!(
        registry.tally().is_empty(),
        "a successful build is not a stub and must not be tallied"
    );
}

#[test]
fn attach_named_falls_back_to_the_stub_for_a_name_nobody_registered() {
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world);
    let entity = world.spawn().unwrap();
    let fields = TestFields(std::collections::BTreeMap::new());

    registry
        .attach_named(&mut world, entity, Frame::ZERO, "NeverRegistered", &fields)
        .unwrap();

    assert_eq!(registry.tally().len(), 1);
    assert_eq!(registry.tally()[0].name, "NeverRegistered");
}

#[test]
fn attach_named_falls_back_to_the_stub_for_a_type_registered_without_a_constructor() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<Countdown>(&mut world, "Countdown");
    let entity = world.spawn().unwrap();
    let fields = TestFields(std::collections::BTreeMap::new());

    registry
        .attach_named(&mut world, entity, Frame::ZERO, "Countdown", &fields)
        .unwrap();

    assert_eq!(
        registry.tally().len(),
        1,
        "a registered-but-not-loadable name still falls back to the stub and is tallied"
    );
}

#[test]
fn attach_named_falls_back_to_the_stub_when_the_constructor_refuses_the_fields() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Weighted", weighted_from_fields);
    let entity = world.spawn().unwrap();
    let fields = TestFields(std::collections::BTreeMap::new());

    registry
        .attach_named(&mut world, entity, Frame::ZERO, "Weighted", &fields)
        .unwrap();

    assert_eq!(registry.tally().len(), 1);
    assert_eq!(registry.tally()[0].name, "Weighted");
    let id = world.register::<ModuleColumn<Weighted>>();
    let column = world.store(id).unwrap();
    assert!(column.get(entity).is_none());
}

#[test]
fn attach_named_strict_refuses_a_construction_failure_the_same_as_an_unregistered_name() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world).with_strict(true);
    registry.register_loadable(&mut world, "Weighted", weighted_from_fields);
    let entity = world.spawn().unwrap();
    let fields = TestFields(std::collections::BTreeMap::new());

    assert_eq!(
        registry.attach_named(&mut world, entity, Frame::ZERO, "Weighted", &fields),
        Err(Error::UnregisteredModule {
            name: "Weighted".to_owned()
        })
    );
}

#[test]
fn register_loadable_is_idempotent_and_keeps_the_first_constructor() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    let first = registry.register_loadable(&mut world, "Weighted", weighted_from_fields);
    let second =
        registry.register_loadable(&mut world, "Weighted (again)", |_, _| Ok(Weighted(999)));
    assert_eq!(first, second, "the same compile-time type keeps its id");

    let entity = world.spawn().unwrap();
    let fields = TestFields(std::collections::BTreeMap::from([("Weight", "3")]));
    registry
        .attach_named(&mut world, entity, Frame::ZERO, "Weighted", &fields)
        .unwrap();
    let id = world.register::<ModuleColumn<Weighted>>();
    let column = world.store(id).unwrap();
    assert_eq!(
        column.get(entity).unwrap().0[0].state,
        Weighted(3),
        "the second registration's constructor must not have replaced the first"
    );
}

#[test]
fn a_second_name_for_one_module_type_is_an_alias_rather_than_a_stub() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Weighted", weighted_from_fields);
    registry.register_loadable(&mut world, "Weighted (again)", weighted_from_fields);

    let entity = world.spawn().unwrap();
    let fields = TestFields(std::collections::BTreeMap::from([("Weight", "3")]));
    registry
        .attach_named(&mut world, entity, Frame::ZERO, "Weighted (again)", &fields)
        .unwrap();
    let id = world.register::<ModuleColumn<Weighted>>();
    let column = world.store(id).unwrap();
    assert_eq!(column.get(entity).unwrap().0[0].state, Weighted(3));
}

#[test]
fn a_second_name_with_its_own_constructor_builds_through_it_not_the_first_names() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Weighted", weighted_from_fields);
    registry.register_loadable(&mut world, "Weighted (again)", |_, _| Ok(Weighted(999)));

    let entity = world.spawn().unwrap();
    let fields = TestFields(std::collections::BTreeMap::from([("Weight", "3")]));
    registry
        .attach_named(&mut world, entity, Frame::ZERO, "Weighted (again)", &fields)
        .unwrap();
    let id = world.register::<ModuleColumn<Weighted>>();
    let column = world.store(id).unwrap();
    assert_eq!(
        column.get(entity).unwrap().0[0].state,
        Weighted(999),
        "the second name's own constructor must run, not the first name's"
    );
}

#[derive(Debug, Clone, Copy)]
struct Reporter(&'static str);
impl Component for Reporter {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}
impl ModuleKind for Reporter {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade)
        .union(CapabilityMask::single(Capability::Create));
    fn attempt_upgrade(&mut self, _active: &UpgradeMask, ctx: &mut UpgradeContext<'_>) {
        if let Some(log) = ctx.effects.get::<Vec<&'static str>>() {
            log.push(self.0);
        }
    }
    fn on_create(&mut self, ctx: &mut CreateContext<'_>) {
        if let Some(log) = ctx.effects.get::<Vec<&'static str>>() {
            log.push(self.0);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct OtherReporter(&'static str);
impl Component for OtherReporter {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}
impl ModuleKind for OtherReporter {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade);
    fn attempt_upgrade(&mut self, _active: &UpgradeMask, ctx: &mut UpgradeContext<'_>) {
        if let Some(log) = ctx.effects.get::<Vec<&'static str>>() {
            log.push(self.0);
        }
    }
    fn on_create(&mut self, ctx: &mut CreateContext<'_>) {
        if let Some(log) = ctx.effects.get::<Vec<&'static str>>() {
            log.push("must never run: not create-capable");
        }
    }
}

fn reporters() -> (World, ModuleRegistry, Entity) {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<Reporter>(&mut world, "Reporter");
    registry.register::<OtherReporter>(&mut world, "OtherReporter");
    let entity = world.spawn().unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, OtherReporter("first"))
        .unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, Reporter("second"))
        .unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, OtherReporter("third"))
        .unwrap();
    (world, registry, entity)
}

#[test]
fn effects_reach_the_callers_sink_in_declaration_order() {
    let (mut world, registry, entity) = reporters();
    let mut log: Vec<&'static str> = Vec::new();
    registry.broadcast_upgrade_into(
        &mut world,
        entity,
        &UpgradeMask::EMPTY,
        Frame::ZERO,
        Effects::into_sink(&mut log),
    );
    assert_eq!(log, ["first", "second", "third"]);
}

#[test]
fn a_sink_of_another_type_is_invisible_to_a_module() {
    let (mut world, registry, entity) = reporters();
    let mut wrong: Vec<u32> = Vec::new();
    registry.broadcast_upgrade_into(
        &mut world,
        entity,
        &UpgradeMask::EMPTY,
        Frame::ZERO,
        Effects::into_sink(&mut wrong),
    );
    assert!(wrong.is_empty());
    assert!(Effects::none().get::<Vec<&'static str>>().is_none());
}

#[test]
fn broadcast_create_runs_only_create_capable_instances_in_declaration_order() {
    let (mut world, registry, entity) = reporters();
    let mut log: Vec<&'static str> = Vec::new();
    registry.broadcast_create(
        &mut world,
        entity,
        Frame::ZERO,
        Effects::into_sink(&mut log),
    );
    assert_eq!(log, ["second"]);
}

#[derive(Debug, Clone)]
struct PowerToy {
    power: &'static str,
    tag: &'static str,
}
impl Component for PowerToy {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}
impl ModuleKind for PowerToy {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);
    fn is_for_power(&self, power: &str) -> bool {
        self.power == power
    }
    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        if let Some(log) = ctx.effects.get::<Vec<(&'static str, Option<(f32, f32)>)>>() {
            log.push((self.tag, activation.at));
        }
    }
}

#[derive(Debug, Clone)]
struct PowerlessToy;
impl Component for PowerlessToy {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}
impl ModuleKind for PowerlessToy {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade);
    fn is_for_power(&self, _power: &str) -> bool {
        true
    }
}

#[test]
fn an_activation_reaches_every_module_for_its_power() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<PowerlessToy>(&mut world, "PowerlessToy");
    registry.register::<PowerToy>(&mut world, "PowerToy");
    let entity = world.spawn().unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, PowerlessToy)
        .unwrap();
    for (power, tag) in [("Heal", "heal"), ("Summon", "first"), ("Summon", "second")] {
        registry
            .attach(&mut world, entity, Frame::ZERO, PowerToy { power, tag })
            .unwrap();
    }
    let mut log: Vec<(&'static str, Option<(f32, f32)>)> = Vec::new();
    let at = Some((10.0, 20.0));
    let activation = Activation {
        power: "Summon",
        target: None,
        at,
    };
    assert!(registry.activate(
        &mut world,
        entity,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));
    assert_eq!(log, [("first", at), ("second", at)]);
    assert!(registry.answers_power(&world, entity, "Heal"));
    assert!(!registry.answers_power(&world, entity, "Missing"));
    let missing = Activation {
        power: "Missing",
        ..activation
    };
    assert!(!registry.activate(
        &mut world,
        entity,
        &missing,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));
    assert_eq!(log.len(), 2, "nothing answered, so the log did not grow");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamped {
    weight: u32,
    attached_at: Frame,
}
impl Component for Stamped {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.weight);
        self.attached_at.hash_into(hasher);
    }
}
impl ModuleKind for Stamped {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);
    fn on_attach(&mut self, now: Frame) {
        self.attached_at = now;
    }
}
fn stamped_from_fields(
    fields: &dyn ModuleFieldSource,
    now: Frame,
) -> core::result::Result<Stamped, FieldError> {
    let weight = weighted_from_fields(fields, now)?.0;
    Ok(Stamped {
        weight,
        attached_at: Frame::ZERO,
    })
}

fn built(outcome: PrototypeOutcome) -> Prototype {
    match outcome {
        PrototypeOutcome::Built(prototype) => prototype,
        other => panic!("expected a prototype, got {other:?}"),
    }
}

#[test]
fn a_prototype_attaches_by_clone_and_on_attach_sees_each_attach_frame() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Stamped", stamped_from_fields);
    let fields = TestFields(std::collections::BTreeMap::from([("Weight", "5")]));
    let prototype = built(registry.prototype("Stamped", &fields, Frame::ZERO).unwrap());

    let early = world.spawn().unwrap();
    let late = world.spawn().unwrap();
    registry
        .attach_prototype(&mut world, early, Frame::new(3), &prototype)
        .unwrap();
    registry
        .attach_prototype(&mut world, late, Frame::new(40), &prototype)
        .unwrap();

    let id = world.register::<ModuleColumn<Stamped>>();
    let column = world.store(id).unwrap();
    assert_eq!(
        column.get(early).unwrap().0[0].state,
        Stamped {
            weight: 5,
            attached_at: Frame::new(3)
        }
    );
    assert_eq!(
        column.get(late).unwrap().0[0].state,
        Stamped {
            weight: 5,
            attached_at: Frame::new(40)
        }
    );
}

#[test]
fn a_prototype_attach_hashes_the_same_as_building_from_fields() {
    let fields = TestFields(std::collections::BTreeMap::from([("Weight", "9")]));

    let mut named = World::new();
    let mut named_registry = ModuleRegistry::new(&mut named);
    named_registry.register_loadable(&mut named, "Stamped", stamped_from_fields);
    let entity = named.spawn().unwrap();
    named_registry
        .attach_named(&mut named, entity, Frame::new(7), "Stamped", &fields)
        .unwrap();

    let mut cloned = World::new();
    let mut cloned_registry = ModuleRegistry::new(&mut cloned);
    cloned_registry.register_loadable(&mut cloned, "Stamped", stamped_from_fields);
    let prototype = built(
        cloned_registry
            .prototype("Stamped", &fields, Frame::ZERO)
            .unwrap(),
    );
    let entity = cloned.spawn().unwrap();
    cloned_registry
        .attach_prototype(&mut cloned, entity, Frame::new(7), &prototype)
        .unwrap();

    assert_eq!(named.state_hash(), cloned.state_hash());
}

#[test]
fn an_unregistered_prototype_is_reported_and_not_tallied() {
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world).with_strict(true);
    let fields = TestFields(std::collections::BTreeMap::new());
    assert!(matches!(
        registry.prototype("Nobody", &fields, Frame::ZERO),
        Ok(PrototypeOutcome::Unregistered)
    ));
    assert!(registry.tally().is_empty());
}

#[test]
fn a_refused_prototype_is_tallied_once_per_use_and_strict_mode_refuses_it() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Stamped", stamped_from_fields);
    let empty = TestFields(std::collections::BTreeMap::new());
    for _ in 0..3 {
        assert!(matches!(
            registry.prototype("Stamped", &empty, Frame::ZERO),
            Ok(PrototypeOutcome::Refused)
        ));
    }
    assert_eq!(
        registry.tally(),
        vec![StubHit {
            name: "Stamped".to_owned(),
            uses: 3
        }]
    );

    let strict = registry.with_strict(true);
    assert_eq!(
        strict.prototype("Stamped", &empty, Frame::ZERO).err(),
        Some(Error::UnregisteredModule {
            name: "Stamped".to_owned()
        })
    );
}
