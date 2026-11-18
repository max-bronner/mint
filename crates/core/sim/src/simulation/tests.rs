use super::*;
use mint_ecs::{Component, ComponentId, ComponentStore, Entity};

#[derive(Debug, PartialEq, Clone, Copy)]
struct Counter(i32);

impl Component for Counter {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Order {
    Spawn,
    Bump(i32),
    Roll,
    Fail,
}

impl crate::Command for Order {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self {
            Self::Spawn => hasher.write_u8(0),
            Self::Bump(amount) => {
                hasher.write_u8(1);
                hasher.write_i32(*amount);
            }
            Self::Roll => hasher.write_u8(2),
            Self::Fail => hasher.write_u8(3),
        }
    }
}

#[derive(Default)]
struct Toy {
    counter: Option<ComponentId<Counter>>,
    applied: std::cell::RefCell<Vec<CommandKey>>,
    steps: std::cell::Cell<u32>,
}

impl Toy {
    fn counter(&self) -> ComponentId<Counter> {
        self.counter.expect("setup registered it")
    }
}

impl Rules for Toy {
    type Command = Order;

    fn setup(&mut self, tick: &mut Tick<'_>) -> Result<()> {
        self.counter = Some(tick.state_mut().register::<Counter>());
        Ok(())
    }

    fn apply(&self, tick: &mut Tick<'_>, key: CommandKey, command: &Self::Command) -> Result<()> {
        self.applied.borrow_mut().push(key);
        let id = self.counter();
        match command {
            Order::Spawn => {
                let entity = tick.state_mut().spawn()?;
                tick.state_mut().insert(id, entity, Counter(0))?;
            }
            Order::Bump(amount) => {
                let targets: Vec<Entity> = tick
                    .state()
                    .store(id)
                    .map(|store| store.entities().to_vec())
                    .unwrap_or_default();
                for entity in targets {
                    if let Some(value) = tick.state_mut().get_mut(id, entity) {
                        value.0 = value.0.saturating_add(*amount);
                    }
                }
            }
            Order::Roll => {
                let _ = tick.rng().next_u32();
            }
            Order::Fail => {
                return Err(Error::Rules {
                    frame: tick.frame().get(),
                    reason: "asked to fail".to_owned(),
                })
            }
        }
        Ok(())
    }

    fn step(&self, _tick: &mut Tick<'_>) -> Result<()> {
        self.steps.set(self.steps.get().saturating_add(1));
        Ok(())
    }
}

fn sim() -> Simulation<Toy> {
    Simulation::new(7, Toy::default()).expect("setup cannot fail")
}

fn counters(simulation: &Simulation<Toy>) -> Vec<i32> {
    let id = simulation.rules().counter();
    simulation
        .state()
        .store(id)
        .map(|store| store.iter().map(|(_, value)| value.0).collect())
        .unwrap_or_default()
}

#[test]
fn a_fresh_simulation_is_at_frame_zero_with_the_seed_it_was_given() {
    let simulation = sim();
    assert_eq!(simulation.frame(), Frame::ZERO);
    assert_eq!(simulation.seed(), 7);
    assert_eq!(simulation.pending_commands(), 0);
    assert_eq!(
        simulation.recording().initial_hash(),
        simulation.state_hash()
    );
}

#[test]
fn a_tick_applies_this_frames_commands_and_then_steps_once() {
    let mut simulation = sim();
    let player = PlayerId::new(0);
    simulation.submit(player, Order::Spawn).unwrap();
    simulation.submit(player, Order::Bump(5)).unwrap();
    simulation
        .submit_after(LogicFrames::ONE, player, Order::Bump(100))
        .unwrap();

    let report = simulation.tick().unwrap();
    assert_eq!(report.frame, Frame::ZERO);
    assert_eq!(report.commands_applied, 2);
    assert_eq!(simulation.frame(), Frame::new(1));
    assert_eq!(counters(&simulation), vec![5]);
    assert_eq!(simulation.rules().steps.get(), 1);
    assert_eq!(simulation.pending_commands(), 1);

    simulation.tick().unwrap();
    assert_eq!(counters(&simulation), vec![105]);
    assert_eq!(simulation.rules().steps.get(), 2);
}

#[test]
fn a_command_cannot_be_added_to_a_frame_that_has_run() {
    let mut simulation = sim();
    simulation.tick().unwrap();
    assert_eq!(
        simulation.submit_at(Frame::ZERO, PlayerId::new(0), Order::Spawn),
        Err(Error::CommandTooLate { target: 0, next: 1 })
    );
}

#[test]
fn an_outside_authority_can_stamp_any_future_frame_on_a_command() {
    let mut simulation = sim();
    let player = PlayerId::new(2);
    for frame in [9_u64, 3, 7] {
        let key = CommandKey::new(Frame::new(frame), player, 500);
        simulation
            .submit_with_key(
                CommandKey {
                    sequence: 500,
                    ..key
                },
                Order::Spawn,
            )
            .unwrap();
    }
    assert_eq!(simulation.pending_commands(), 3);
    simulation.run(LogicFrames::from_frames(4)).unwrap();
    assert_eq!(simulation.state().len(), 1, "only frame 3 has run");
    simulation.run(LogicFrames::from_frames(6)).unwrap();
    assert_eq!(simulation.state().len(), 3);
    let applied: Vec<u64> = simulation
        .rules()
        .applied
        .borrow()
        .iter()
        .map(|key| key.frame.get())
        .collect();
    assert_eq!(applied, vec![3, 7, 9]);
}

#[test]
fn commands_are_applied_in_key_order_whatever_order_they_arrived_in() {
    let mut simulation = sim();
    let late = CommandKey::new(Frame::ZERO, PlayerId::new(3), 0);
    let early = CommandKey::new(Frame::ZERO, PlayerId::new(1), 0);
    let middle = CommandKey::new(Frame::ZERO, PlayerId::new(1), 4);
    simulation.submit_with_key(late, Order::Spawn).unwrap();
    simulation.submit_with_key(early, Order::Spawn).unwrap();
    simulation.submit_with_key(middle, Order::Spawn).unwrap();

    simulation.tick().unwrap();
    assert_eq!(
        *simulation.rules().applied.borrow(),
        vec![early, middle, late]
    );
}

#[test]
fn the_frame_hash_moves_when_the_generator_does_and_the_world_does_not() {
    let mut simulation = sim();
    let before_world = simulation.world_hash();
    let before_state = simulation.state_hash();
    simulation.submit(PlayerId::new(0), Order::Roll).unwrap();
    simulation.tick().unwrap();

    assert_eq!(
        simulation.world_hash(),
        before_world,
        "drawing a random number changes no component"
    );
    assert_ne!(
        simulation.state_hash(),
        before_state,
        "but it is simulation state and the frame hash must see it"
    );
}

#[test]
fn a_ruleset_error_poisons_the_simulation_rather_than_continuing() {
    let mut simulation = sim();
    simulation.submit(PlayerId::new(0), Order::Fail).unwrap();
    assert_eq!(
        simulation.tick(),
        Err(Error::in_rule(
            "command 0/player 0#0",
            Error::Rules {
                frame: 0,
                reason: "asked to fail".to_owned(),
            }
        ))
    );
    assert!(simulation.is_poisoned());
    assert_eq!(simulation.tick(), Err(Error::Poisoned));
    assert_eq!(simulation.frame(), Frame::ZERO);
    assert!(simulation.recording().hashes().is_empty());
}

#[test]
fn running_a_duration_runs_that_many_frames() {
    let mut simulation = sim();
    simulation.run(LogicFrames::from_frames(10)).unwrap();
    assert_eq!(simulation.frame(), Frame::new(10));
    assert_eq!(simulation.recording().hashes().len(), 10);
    assert_eq!(simulation.rules().steps.get(), 10);
    simulation.run(LogicFrames::ZERO).unwrap();
    assert_eq!(simulation.frame(), Frame::new(10));
}

#[test]
fn each_tick_records_one_hash_and_its_commands() {
    let mut simulation = sim();
    let player = PlayerId::new(0);
    simulation.submit(player, Order::Spawn).unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    simulation.submit(player, Order::Bump(1)).unwrap();
    simulation.tick().unwrap();

    let recording = simulation.recording();
    assert_eq!(recording.hashes().len(), 3);
    assert_eq!(recording.commands().len(), 2);
    assert_eq!(
        recording
            .commands()
            .first()
            .map(Envelope::key)
            .map(|k| k.frame),
        Some(Frame::ZERO)
    );
    assert_eq!(
        recording
            .commands()
            .get(1)
            .map(Envelope::key)
            .map(|k| k.frame),
        Some(Frame::new(2))
    );
}

#[test]
fn state_is_readable_but_the_type_system_offers_no_way_to_write_it() {
    let mut simulation = sim();
    simulation.submit(PlayerId::new(0), Order::Spawn).unwrap();
    simulation.tick().unwrap();
    let world: &World = simulation.state();
    assert_eq!(world.len(), 1);
    let id = simulation.rules().counter();
    assert_eq!(world.store(id).map(ComponentStore::len), Some(1));
}

#[derive(Debug, Clone)]
struct Leaky(std::cell::Cell<i32>);

impl Component for Leaky {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.0.get());
    }
}

struct LeakyRules {
    id: Option<ComponentId<Leaky>>,
    bypass: bool,
}

impl Rules for LeakyRules {
    type Command = Order;

    fn setup(&mut self, tick: &mut Tick<'_>) -> Result<()> {
        let id = tick.state_mut().register::<Leaky>();
        let entity = tick.state_mut().spawn()?;
        tick.state_mut()
            .insert(id, entity, Leaky(std::cell::Cell::new(0)))?;
        self.id = Some(id);
        Ok(())
    }

    fn apply(&self, _tick: &mut Tick<'_>, _key: CommandKey, _command: &Order) -> Result<()> {
        Ok(())
    }

    fn step(&self, tick: &mut Tick<'_>) -> Result<()> {
        let id = self.id.expect("setup registered it");
        let entity = tick
            .state()
            .iter_entities()
            .next()
            .expect("setup spawned it");
        if self.bypass {
            let value = &tick.state().get(id, entity).expect("inserted").0;
            value.set(value.get().saturating_add(1));
        } else if let Some(value) = tick.state_mut().get_mut(id, entity) {
            value.0.set(value.0.get().saturating_add(1));
        }
        Ok(())
    }
}

fn leaky(bypass: bool, frame_hash: FrameHash) -> Simulation<LeakyRules> {
    let mut simulation = Simulation::new(1, LeakyRules { id: None, bypass }).unwrap();
    simulation.set_frame_hash(frame_hash).unwrap();
    simulation
}

#[test]
fn verified_hashing_catches_a_write_the_store_did_not_see() {
    let mut marked = leaky(false, FrameHash::Verified);
    let before = marked.state_hash();
    let report = marked.tick().unwrap();
    assert_ne!(report.hash, before);

    let mut unverified = leaky(true, FrameHash::Incremental);
    let before = unverified.state_hash();
    assert_eq!(
        unverified.tick().unwrap().hash,
        before,
        "unverified, the bypassing write is invisible - the reason the check exists"
    );

    let mut verified = leaky(true, FrameHash::Verified);
    assert!(matches!(
        verified.tick(),
        Err(Error::StaleHashCache { frame: 0, .. })
    ));
    assert!(verified.is_poisoned());
}

#[test]
fn every_frame_hash_mode_reports_its_own_definition() {
    let hashes = |frame_hash| {
        let mut simulation = sim();
        simulation.set_frame_hash(frame_hash).unwrap();
        simulation.submit(PlayerId::new(0), Order::Spawn).unwrap();
        simulation.tick().unwrap();
        simulation.submit(PlayerId::new(0), Order::Bump(3)).unwrap();
        simulation.tick().unwrap();
        let world = simulation.state();
        let world_hash = match frame_hash {
            FrameHash::Legacy => world.legacy_state_hash(),
            _ => world.state_hash(),
        };
        (
            simulation.recording().hashes().to_vec(),
            world_hash,
            simulation.world_hash(),
        )
    };
    let incremental = hashes(FrameHash::Incremental);
    let verified = hashes(FrameHash::Verified);
    let legacy = hashes(FrameHash::Legacy);
    assert_eq!(incremental, verified);
    assert_ne!(incremental.0, legacy.0);
    assert_eq!(legacy.1, legacy.2);
    assert_eq!(incremental.1, incremental.2);
}
