#![allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;

use mint_sim::module::{
    Capability, CapabilityMask, DieContext, ModuleKind, ModuleRegistry, ModuleSlot, UpdateContext,
};
use mint_sim::{
    Command, CommandKey, Component, Entity, LogicFrames, PlayerId, Result, Rules, Simulation,
    StateHasher, Tick,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Ticker {
    count: u32,
}

impl Component for Ticker {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.count);
    }
}

impl ModuleKind for Ticker {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);

    fn update(&mut self, _ctx: &mut UpdateContext<'_>) -> mint_sim::Sleep {
        self.count = self.count.saturating_add(1);
        mint_sim::Sleep::Frames(LogicFrames::from_frames(3))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Weighted {
    weight: u32,
    selected: bool,
}

impl Weighted {
    fn new(weight: u32) -> Self {
        Self {
            weight: weight.max(1),
            selected: false,
        }
    }
}

impl Component for Weighted {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.weight);
        hasher.write_u8(u8::from(self.selected));
    }
}

impl ModuleKind for Weighted {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Die);

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>) {
        let total: u32 = instances
            .iter()
            .fold(0u32, |sum, slot| sum.saturating_add(slot.state.weight));
        if total == 0 {
            return;
        }
        let roll = ctx.rng.below(total).saturating_add(1);
        let mut remaining = roll;
        let mut winner_chosen = false;
        for slot in instances.iter_mut() {
            slot.state.selected = false;
            if winner_chosen {
                continue;
            }
            remaining = remaining.saturating_sub(slot.state.weight);
            if remaining == 0 {
                slot.state.selected = true;
                winner_chosen = true;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Order {
    Spawn(Vec<u32>),
    KillAll,
}

impl Command for Order {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self {
            Order::Spawn(weights) => {
                hasher.write_u8(0);
                hasher.write_usize(weights.len());
                for weight in weights {
                    hasher.write_u32(*weight);
                }
            }
            Order::KillAll => hasher.write_u8(1),
        }
    }
}

struct Battlefield {
    ticker_first: bool,
    registry: Option<ModuleRegistry>,
    ticker_id: Option<mint_sim::ComponentId<mint_sim::module::ModuleColumn<Ticker>>>,
    weighted_id: Option<mint_sim::ComponentId<mint_sim::module::ModuleColumn<Weighted>>>,
    destroy: RefCell<mint_sim::module::DestroyRequests>,
}

impl Battlefield {
    fn new(ticker_first: bool) -> Self {
        Self {
            ticker_first,
            registry: None,
            ticker_id: None,
            weighted_id: None,
            destroy: RefCell::new(mint_sim::module::DestroyRequests::new()),
        }
    }

    fn registry(&self) -> &ModuleRegistry {
        self.registry
            .as_ref()
            .expect("setup registers the module registry before anything else runs")
    }

    fn ticker_id(&self) -> mint_sim::ComponentId<mint_sim::module::ModuleColumn<Ticker>> {
        self.ticker_id
            .expect("setup registers Ticker's column before anything else runs")
    }

    fn weighted_id(&self) -> mint_sim::ComponentId<mint_sim::module::ModuleColumn<Weighted>> {
        self.weighted_id
            .expect("setup registers Weighted's column before anything else runs")
    }
}

impl Rules for Battlefield {
    type Command = Order;

    fn setup(&mut self, tick: &mut Tick<'_>) -> Result<()> {
        let world = tick.state_mut();
        let mut registry = ModuleRegistry::new(world);
        if self.ticker_first {
            registry.register::<Ticker>(world, "Ticker");
            registry.register::<Weighted>(world, "Weighted");
        } else {
            registry.register::<Weighted>(world, "Weighted");
            registry.register::<Ticker>(world, "Ticker");
        }
        self.ticker_id = Some(world.register());
        self.weighted_id = Some(world.register());
        self.registry = Some(registry);
        Ok(())
    }

    fn apply(&self, tick: &mut Tick<'_>, _key: CommandKey, command: &Order) -> Result<()> {
        let registry = self.registry();
        match command {
            Order::Spawn(weights) => {
                let now = tick.frame();
                let world = tick.state_mut();
                let entity = world.spawn()?;
                registry.attach(world, entity, now, Ticker::default())?;
                for &weight in weights {
                    registry.attach(world, entity, now, Weighted::new(weight))?;
                }
            }
            Order::KillAll => {
                let now = tick.frame();
                let targets: Vec<Entity> = tick.state().iter_entities().collect();
                let (world, rng) = tick.state_and_rng_mut();
                let mut destroy = self.destroy.borrow_mut();
                for entity in targets {
                    registry.broadcast_die(world, entity, now, rng, &mut destroy);
                }
            }
        }
        Ok(())
    }

    fn step(&self, tick: &mut Tick<'_>) -> Result<()> {
        let registry = self.registry();
        let now = tick.frame();
        let (world, rng) = tick.state_and_rng_mut();
        let mut destroy = self.destroy.borrow_mut();
        registry.update(world, now, rng, &mut destroy);
        for entity in destroy.drain_sorted() {
            world.despawn(entity);
        }
        Ok(())
    }
}

fn run_script(ticker_first: bool, seed: u64) -> Vec<u64> {
    let mut sim = Simulation::new(seed, Battlefield::new(ticker_first)).unwrap();
    let player = PlayerId::new(0);
    sim.submit(player, Order::Spawn(vec![3, 2, 5])).unwrap();
    sim.submit(player, Order::Spawn(vec![1, 1, 1, 6])).unwrap();
    sim.run(LogicFrames::from_frames(10)).unwrap();
    sim.submit(player, Order::KillAll).unwrap();
    sim.run(LogicFrames::from_frames(5)).unwrap();
    sim.into_recording().hashes().to_vec()
}

fn read_ticker_counts(sim: &Simulation<Battlefield>) -> Vec<u32> {
    let id = sim.rules().ticker_id();
    sim.state()
        .store(id)
        .map(|store| {
            store
                .iter()
                .filter_map(|(_, column)| column.0.first().map(|slot| slot.state.count))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn registration_order_changes_the_hash_but_not_the_outcome() {
    let mut ticker_first = Simulation::new(0xA11CE, Battlefield::new(true)).unwrap();
    let mut weighted_first = Simulation::new(0xA11CE, Battlefield::new(false)).unwrap();
    let player = PlayerId::new(0);
    for sim in [&mut ticker_first, &mut weighted_first] {
        sim.submit(player, Order::Spawn(vec![3, 2, 5])).unwrap();
        sim.submit(player, Order::Spawn(vec![1, 1, 1, 6])).unwrap();
        sim.run(LogicFrames::from_frames(10)).unwrap();
    }

    let outcome_a = read_ticker_counts(&ticker_first);
    let outcome_b = read_ticker_counts(&weighted_first);
    assert_eq!(
        outcome_a, outcome_b,
        "the same commands must tick every Ticker the same number of times \
         regardless of which type this ruleset registered first"
    );
    assert!(
        outcome_a.iter().any(|&count| count > 0),
        "the script must have actually ticked something"
    );

    assert_ne!(
        ticker_first.world_hash(),
        weighted_first.world_hash(),
        "registration order is part of what the hash covers by design - see \
         this test's own doc comment"
    );
}

#[test]
fn declaration_order_changes_which_weight_wins() {
    fn run(weights: Vec<u32>) -> u32 {
        let mut sim = Simulation::new(1, Battlefield::new(true)).unwrap();
        let player = PlayerId::new(0);
        sim.submit(player, Order::Spawn(weights)).unwrap();
        sim.submit(player, Order::KillAll).unwrap();
        sim.run(LogicFrames::from_frames(2)).unwrap();

        let id = sim.rules().weighted_id();
        let world = sim.state();
        let entity = world
            .iter_entities()
            .next()
            .expect("the spawned entity is still live");
        let column = world.store(id).and_then(|store| store.get(entity));
        column
            .and_then(|column| column.0.iter().find(|slot| slot.state.selected))
            .map(|slot| slot.state.weight)
            .expect("exactly one instance wins")
    }

    let forward = run(vec![3, 2, 5]);
    let reversed = run(vec![5, 2, 3]);
    assert_ne!(
        forward, reversed,
        "reordering the same weights for the same seed must change which weight wins"
    );
}

#[test]
fn the_same_script_reproduces_the_same_hash_sequence() {
    let first = run_script(true, 7);
    let second = run_script(true, 7);
    assert_eq!(first, second);
}
