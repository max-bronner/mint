#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used
)]

use mint_sim::{
    first_divergence, is_ordered, Command, CommandKey, Component, ComponentId, Entity, Envelope,
    Error, Frame, LogicFrames, Pcg32, PlayerId, Replay, Result, Rules, Simulation, StateHasher,
    Tick,
};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Position {
    x: f32,
    y: f32,
}

impl Component for Position {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.x);
        hasher.write_f32(self.y);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Velocity {
    dx: f32,
    dy: f32,
}

impl Component for Velocity {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.dx);
        hasher.write_f32(self.dy);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Health {
    current: i32,
    regen_in: LogicFrames,
}

impl Component for Health {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.current);
        self.regen_in.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Owner(PlayerId);

impl Component for Owner {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Order {
    Recruit { x: f32, y: f32, dx: f32, dy: f32 },
    Damage { target: Entity, amount: i32 },
    Halt { target: Entity },
    Disband { target: Entity },
    Scatter,
}

impl Command for Order {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self {
            Self::Recruit { x, y, dx, dy } => {
                hasher.write_u8(0);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
                hasher.write_f32(*dx);
                hasher.write_f32(*dy);
            }
            Self::Damage { target, amount } => {
                hasher.write_u8(1);
                target.hash_into(hasher);
                hasher.write_i32(*amount);
            }
            Self::Halt { target } => {
                hasher.write_u8(2);
                target.hash_into(hasher);
            }
            Self::Disband { target } => {
                hasher.write_u8(3);
                target.hash_into(hasher);
            }
            Self::Scatter => hasher.write_u8(4),
        }
    }
}

const REGEN_DELAY: LogicFrames = LogicFrames::from_frames(15);

#[derive(Default)]
struct Battle {
    position: Option<ComponentId<Position>>,
    velocity: Option<ComponentId<Velocity>>,
    health: Option<ComponentId<Health>>,
    owner: Option<ComponentId<Owner>>,
}

struct Ids {
    position: ComponentId<Position>,
    velocity: ComponentId<Velocity>,
    health: ComponentId<Health>,
    owner: ComponentId<Owner>,
}

impl Battle {
    fn ids(&self) -> Option<Ids> {
        Some(Ids {
            position: self.position?,
            velocity: self.velocity?,
            health: self.health?,
            owner: self.owner?,
        })
    }
}

impl Rules for Battle {
    type Command = Order;

    fn setup(&mut self, tick: &mut Tick<'_>) -> Result<()> {
        self.position = Some(tick.state_mut().register::<Position>());
        self.velocity = Some(tick.state_mut().register::<Velocity>());
        self.health = Some(tick.state_mut().register::<Health>());
        self.owner = Some(tick.state_mut().register::<Owner>());

        let ids = self.ids().ok_or_else(unregistered)?;
        for index in 0..2u8 {
            let marker = tick.state_mut().spawn()?;
            let offset = f32::from(index);
            tick.state_mut().insert(
                ids.position,
                marker,
                Position {
                    x: offset,
                    y: -offset,
                },
            )?;
            tick.state_mut()
                .insert(ids.owner, marker, Owner(PlayerId::new(u8::MAX)))?;
        }
        Ok(())
    }

    fn apply(&self, tick: &mut Tick<'_>, key: CommandKey, command: &Order) -> Result<()> {
        let Some(ids) = self.ids() else {
            return Ok(());
        };
        match command {
            Order::Recruit { x, y, dx, dy } => {
                let unit = tick.state_mut().spawn()?;
                tick.state_mut()
                    .insert(ids.position, unit, Position { x: *x, y: *y })?;
                tick.state_mut()
                    .insert(ids.velocity, unit, Velocity { dx: *dx, dy: *dy })?;
                tick.state_mut().insert(
                    ids.health,
                    unit,
                    Health {
                        current: 100,
                        regen_in: LogicFrames::ZERO,
                    },
                )?;
                tick.state_mut()
                    .insert(ids.owner, unit, Owner(key.player))?;
            }
            Order::Damage { target, amount } => {
                let killed = match tick.state_mut().get_mut(ids.health, *target) {
                    Some(health) => {
                        health.current = health.current.saturating_sub(*amount);
                        health.regen_in = REGEN_DELAY;
                        health.current <= 0
                    }
                    None => false,
                };
                if killed {
                    tick.state_mut().despawn(*target);
                }
            }
            Order::Halt { target } => {
                if let Some(velocity) = tick.state_mut().get_mut(ids.velocity, *target) {
                    velocity.dx = 0.0;
                    velocity.dy = 0.0;
                }
            }
            Order::Disband { target } => {
                tick.state_mut().despawn(*target);
            }
            Order::Scatter => {
                let units: Vec<Entity> = tick
                    .state()
                    .store(ids.velocity)
                    .map(|store| store.entities().to_vec())
                    .unwrap_or_default();
                for unit in units {
                    let dx = tick.rng().next_f32();
                    let dy = tick.rng().next_f32();
                    if let Some(velocity) = tick.state_mut().get_mut(ids.velocity, unit) {
                        velocity.dx += (dx - 0.5) * 0.25;
                        velocity.dy += (dy - 0.5) * 0.25;
                    }
                }
            }
        }
        Ok(())
    }

    fn step(&self, tick: &mut Tick<'_>) -> Result<()> {
        let Some(ids) = self.ids() else {
            return Ok(());
        };

        let motion: Vec<(Entity, Velocity)> = tick
            .state()
            .store(ids.velocity)
            .map(|store| store.iter().map(|(e, v)| (e, *v)).collect())
            .unwrap_or_default();
        for (entity, velocity) in motion {
            if let Some(position) = tick.state_mut().get_mut(ids.position, entity) {
                position.x += velocity.dx;
                position.y += velocity.dy;
            }
        }

        let injured: Vec<Entity> = tick
            .state()
            .store(ids.health)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for entity in injured {
            if let Some(health) = tick.state_mut().get_mut(ids.health, entity) {
                health.regen_in = health.regen_in.saturating_sub(LogicFrames::ONE);
                if health.regen_in.is_zero() && health.current < 100 {
                    health.current = health.current.saturating_add(1);
                }
            }
        }
        Ok(())
    }
}

const FRAMES: u32 = 600;
const PLAYERS: u8 = 4;
const DRIVER_SEED: u64 = 0x5EED_1234_ABCD_0001;
const SIM_SEED: u64 = 0x0BAD_C0DE_1234_5678;

fn record() -> Replay<Order> {
    let mut simulation = Simulation::new(SIM_SEED, Battle::default()).expect("setup");
    let mut driver = Pcg32::new(DRIVER_SEED);

    for _ in 0..FRAMES {
        let orders = driver.below(8);
        for _ in 0..orders {
            let player = PlayerId::new(u8::try_from(driver.below(u32::from(PLAYERS))).unwrap_or(0));
            let delay = LogicFrames::from_frames(driver.below(4));
            let live: Vec<Entity> = simulation.state().iter_entities().collect();
            let order = match driver.below(10) {
                0..=3 => Order::Recruit {
                    x: driver.next_f32() * 100.0,
                    y: driver.next_f32() * 100.0,
                    dx: (driver.next_f32() - 0.5) * 2.0,
                    dy: (driver.next_f32() - 0.5) * 2.0,
                },
                4..=6 => match pick(&mut driver, &live) {
                    Some(target) => Order::Damage {
                        target,
                        amount: i32::try_from(driver.below(60)).unwrap_or(1),
                    },
                    None => Order::Scatter,
                },
                7 => match pick(&mut driver, &live) {
                    Some(target) => Order::Halt { target },
                    None => Order::Scatter,
                },
                8 => match pick(&mut driver, &live) {
                    Some(target) => Order::Disband { target },
                    None => Order::Scatter,
                },
                _ => Order::Scatter,
            };
            simulation
                .submit_after(delay, player, order)
                .expect("the queue is far from full");
        }
        simulation.tick().expect("the ruleset never fails");
    }

    simulation.into_recording()
}

fn pick(driver: &mut Pcg32, live: &[Entity]) -> Option<Entity> {
    let count = u32::try_from(live.len()).ok()?;
    if count == 0 {
        return None;
    }
    let index = usize::try_from(driver.below(count)).ok()?;
    live.get(index).copied()
}

#[test]
fn a_recorded_run_replays_to_an_identical_hash_sequence() {
    let recording = record();

    assert_eq!(recording.hashes().len(), frames());
    assert!(
        recording.commands().len() > 1_500,
        "only {} commands recorded; the test is meant to be hard to pass",
        recording.commands().len()
    );
    let distinct = {
        let mut sorted = recording.hashes().to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        sorted.len()
    };
    assert!(
        distinct > 500,
        "only {distinct} distinct frame hashes in {FRAMES} frames; \
         a simulation that stopped changing would pass vacuously"
    );
    assert!(is_ordered(recording.commands()), "the log is in key order");

    let verified = recording
        .verify(Battle::default())
        .expect("the replay must reproduce the recording exactly");
    assert_eq!(verified.frames, frames());
    assert_eq!(verified.commands, recording.commands().len());
    assert_eq!(
        Some(&verified.final_hash),
        recording.hashes().last(),
        "the final hash is the last recorded one"
    );

    let counts = command_counts(recording.commands());
    println!(
        "recorded {} frames, {} commands ({} recruit, {} damage, {} disband, {} scatter),          {distinct} distinct hashes, peak {} live entities, digest {:#018x}",
        recording.hashes().len(),
        recording.commands().len(),
        counts.0,
        counts.1,
        counts.2,
        counts.3,
        peak_population(&recording),
        recording.digest(),
    );
    assert!(counts.0 > 600, "recruits: {}", counts.0);
    assert!(counts.1 > 400, "damage: {}", counts.1);
    assert!(counts.2 > 150, "disbands: {}", counts.2);
    assert!(counts.3 > 150, "scatters: {}", counts.3);
    assert!(
        peak_population(&recording) > 500,
        "the world must be big enough that an ordering mistake has room to show"
    );
}

#[test]
fn two_recordings_of_the_same_script_are_byte_for_byte_the_same_log() {
    let first = record();
    let second = record();
    assert_eq!(first.digest(), second.digest());
    assert_eq!(first.hashes(), second.hashes());
    assert_eq!(first.commands(), second.commands());
    assert_eq!(first.initial_hash(), second.initial_hash());
}

#[test]
fn a_shuffled_arrival_order_produces_the_same_hashes() {
    let recording = record();

    let mut shuffled: Vec<Envelope<Order>> = recording.commands().to_vec();
    let mut shuffler = Pcg32::new(0xD15C_0FFE);
    for index in (1..shuffled.len()).rev() {
        let swap =
            usize::try_from(shuffler.below(u32::try_from(index).unwrap_or(0) + 1)).unwrap_or(0);
        shuffled.swap(index, swap);
    }
    assert!(
        !is_ordered(&shuffled),
        "the shuffle must actually disorder the log"
    );

    let mut simulation = Simulation::new(recording.seed(), Battle::default()).expect("setup");
    for envelope in &shuffled {
        let (key, order) = envelope.clone().into_parts();
        simulation.submit_with_key(key, order).expect("in range");
    }
    let mut observed = Vec::with_capacity(recording.hashes().len());
    for _ in 0..recording.hashes().len() {
        observed.push(simulation.tick().expect("no ruleset failure").hash);
    }

    assert_eq!(
        first_divergence(recording.hashes(), &observed),
        None,
        "arrival order changed the outcome"
    );
}

#[test]
fn a_corrupted_command_is_reported_with_the_frame_it_diverged_on() {
    let recording = record();

    let victim = recording
        .commands()
        .iter()
        .position(|envelope| {
            envelope.key().frame > Frame::new(100)
                && matches!(envelope.command(), Order::Damage { .. })
        })
        .expect("the script issues damage after frame 100");
    let mut commands = recording.commands().to_vec();
    let corrupted_frame = commands
        .get(victim)
        .map(|envelope| envelope.key().frame.get())
        .expect("index came from `position`");
    if let Some(envelope) = commands.get_mut(victim) {
        let (key, order) = envelope.clone().into_parts();
        let changed = match order {
            Order::Damage { target, amount } => Order::Damage {
                target,
                amount: amount.saturating_add(1),
            },
            other => other,
        };
        *envelope = Envelope::new(key, changed);
    }

    let tampered = rebuild(&recording, commands);
    match tampered.verify(Battle::default()) {
        Err(Error::Divergence { frame, .. }) => assert_eq!(
            frame, corrupted_frame,
            "the divergence must be reported on the frame the changed command ran"
        ),
        other => panic!("expected a divergence at frame {corrupted_frame}, got {other:?}"),
    }
}

#[test]
fn a_dropped_command_diverges_on_the_frame_it_was_dropped_from() {
    let recording = record();
    let victim = recording
        .commands()
        .iter()
        .position(|envelope| envelope.key().frame > Frame::new(50))
        .expect("the script issues commands after frame 50");
    let dropped_frame = recording
        .commands()
        .get(victim)
        .map(|envelope| envelope.key().frame.get())
        .expect("index came from `position`");
    let mut commands = recording.commands().to_vec();
    commands.remove(victim);

    let tampered = rebuild(&recording, commands);
    match tampered.verify(Battle::default()) {
        Err(Error::Divergence { frame, .. }) => assert_eq!(frame, dropped_frame),
        other => panic!("expected a divergence at frame {dropped_frame}, got {other:?}"),
    }
}

#[test]
fn a_recording_made_under_another_seed_does_not_verify() {
    let recording = record();
    let different = Replay::from_parts(
        SIM_SEED ^ 1,
        recording.initial_hash(),
        recording.commands().to_vec(),
        recording.hashes().to_vec(),
    );
    assert!(matches!(
        different.verify(Battle::default()),
        Err(Error::SetupDivergence { .. })
    ));
}

#[test]
fn a_different_registration_order_fails_before_frame_zero() {
    #[derive(Default)]
    struct Reordered(Battle);

    impl Rules for Reordered {
        type Command = Order;

        fn setup(&mut self, tick: &mut Tick<'_>) -> Result<()> {
            self.0.health = Some(tick.state_mut().register::<Health>());
            self.0.position = Some(tick.state_mut().register::<Position>());
            self.0.velocity = Some(tick.state_mut().register::<Velocity>());
            self.0.owner = Some(tick.state_mut().register::<Owner>());
            let ids = self.0.ids().ok_or_else(unregistered)?;
            for index in 0..2u8 {
                let marker = tick.state_mut().spawn()?;
                let offset = f32::from(index);
                tick.state_mut().insert(
                    ids.position,
                    marker,
                    Position {
                        x: offset,
                        y: -offset,
                    },
                )?;
                tick.state_mut()
                    .insert(ids.owner, marker, Owner(PlayerId::new(u8::MAX)))?;
            }
            Ok(())
        }

        fn apply(&self, tick: &mut Tick<'_>, key: CommandKey, command: &Order) -> Result<()> {
            self.0.apply(tick, key, command)
        }

        fn step(&self, tick: &mut Tick<'_>) -> Result<()> {
            self.0.step(tick)
        }
    }

    let recording = record();
    assert!(matches!(
        recording.verify(Reordered::default()),
        Err(Error::SetupDivergence { .. })
    ));
}

#[test]
fn a_log_with_commands_past_its_own_end_is_reported_as_malformed() {
    let recording = record();
    let mut commands = recording.commands().to_vec();
    commands.push(Envelope::new(
        CommandKey::new(
            Frame::new(u64::from(FRAMES).saturating_add(10)),
            PlayerId::new(0),
            u32::MAX,
        ),
        Order::Scatter,
    ));
    let tampered = rebuild(&recording, commands);
    assert!(matches!(
        tampered.verify(Battle::default()),
        Err(Error::UnconsumedCommands { remaining: 1, .. })
    ));
}

fn peak_population(recording: &Replay<Order>) -> usize {
    let mut simulation = Simulation::new(recording.seed(), Battle::default()).expect("setup");
    let mut peak = simulation.state().len();
    let mut cursor = 0usize;
    for index in 0..recording.hashes().len() {
        let frame = Frame::new(u64::try_from(index).unwrap_or(0));
        while let Some(envelope) = recording.commands().get(cursor) {
            if envelope.key().frame > frame {
                break;
            }
            let (key, order) = envelope.clone().into_parts();
            simulation.submit_with_key(key, order).expect("in range");
            cursor += 1;
        }
        simulation.tick().expect("no failure");
        peak = peak.max(simulation.state().len());
    }
    peak
}

fn frames() -> usize {
    usize::try_from(FRAMES).expect("240 fits in a usize")
}

fn unregistered() -> Error {
    Error::Rules {
        frame: 0,
        reason: "components are not registered".to_owned(),
    }
}

fn command_counts(commands: &[Envelope<Order>]) -> (usize, usize, usize, usize) {
    let mut counts = (0usize, 0usize, 0usize, 0usize);
    for envelope in commands {
        match envelope.command() {
            Order::Recruit { .. } => counts.0 += 1,
            Order::Damage { .. } => counts.1 += 1,
            Order::Disband { .. } => counts.2 += 1,
            Order::Scatter => counts.3 += 1,
            Order::Halt { .. } => {}
        }
    }
    counts
}

fn rebuild(original: &Replay<Order>, commands: Vec<Envelope<Order>>) -> Replay<Order> {
    Replay::from_parts(
        original.seed(),
        original.initial_hash(),
        commands,
        original.hashes().to_vec(),
    )
}
