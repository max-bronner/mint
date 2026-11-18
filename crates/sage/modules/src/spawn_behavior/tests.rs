use super::*;
use mint_sim::module::{DestroyRequests, ModuleRegistry};
use mint_sim::{Effects, World};

struct TestFields(Vec<(&'static str, Vec<&'static str>)>);

impl ModuleFieldSource for TestFields {
    fn field(&self, keyword: &str) -> Option<&str> {
        self.field_args(keyword).first().copied()
    }
    fn field_numeric(&self, keyword: &str) -> Option<f32> {
        self.field(keyword)?.parse().ok()
    }
    fn field_args(&self, keyword: &str) -> Vec<&str> {
        self.0
            .iter()
            .rev()
            .find(|(k, _)| *k == keyword)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
    fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
        self.0
            .iter()
            .filter(|(k, _)| *k == keyword)
            .map(|(_, v)| v.clone())
            .collect()
    }
}

fn spawner(pairs: &[(&'static str, &'static str)]) -> SpawnBehavior {
    let fields = TestFields(
        pairs
            .iter()
            .map(|(k, v)| (*k, v.split_whitespace().collect()))
            .collect(),
    );
    spawn_behavior_from_fields(&fields, Frame::ZERO).unwrap()
}

struct Lair {
    world: World,
    registry: ModuleRegistry,
    created: Vec<(u64, String, Entity)>,
}

impl Lair {
    fn new(module: SpawnBehavior) -> Self {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<SpawnBehavior>(&mut world, "SpawnBehavior");
        let lair = world.spawn().unwrap();
        registry
            .attach(&mut world, lair, Frame::ZERO, module)
            .unwrap();
        Self {
            world,
            registry,
            created: Vec::new(),
        }
    }

    fn run(&mut self, frames: core::ops::Range<u64>, kills: &[(u64, usize)]) {
        let mut rng = mint_sim::Pcg32::new(1);
        for frame in frames {
            let now = Frame::new(frame);
            for (at, index) in kills {
                if *at == frame {
                    let (_, _, spawn) = self.created[*index];
                    self.registry.product_died(&mut self.world, spawn, now);
                }
            }
            let mut log = EffectLog::new();
            self.registry.update_into(
                &mut self.world,
                now,
                &mut rng,
                &mut DestroyRequests::new(),
                Effects::into_sink(&mut log),
            );
            for (source, effect) in log.drain() {
                let Effect::Spawn {
                    name, requester, ..
                } = effect
                else {
                    continue;
                };
                let product = self.world.spawn().unwrap();
                self.registry
                    .adopt_created(&mut self.world, source, requester, product, now)
                    .unwrap();
                self.created.push((frame, name.to_string(), product));
            }
        }
    }

    fn names(&self) -> Vec<&str> {
        self.created
            .iter()
            .map(|(_, name, _)| name.as_str())
            .collect()
    }
}

#[test]
fn a_spawner_creates_its_names_in_turn_and_keeps_turning_for_replacements() {
    let mut lair = Lair::new(spawner(&[
        ("SpawnNumber", "4"),
        ("InitialBurst", "4"),
        ("SpawnTemplateName", "ZAlpha ZBeta"),
        ("SpawnTemplateName", "ZGamma"),
        ("SpawnReplaceDelay", "1000"),
    ]));
    lair.run(0..200, &[(40, 0), (41, 1)]);
    assert_eq!(
        lair.names(),
        ["ZAlpha", "ZBeta", "ZGamma", "ZAlpha", "ZBeta", "ZGamma"]
    );
}

#[test]
fn a_dead_spawn_is_replaced_on_the_first_pass_after_the_replace_delay() {
    let mut lair = Lair::new(spawner(&[
        ("SpawnNumber", "2"),
        ("SpawnTemplateName", "ZWolf"),
        ("SpawnReplaceDelay", "2000"),
    ]));
    lair.run(0..200, &[(20, 1)]);
    let frames: Vec<u64> = lair.created.iter().map(|(frame, _, _)| *frame).collect();
    assert_eq!(frames, [15, 15, 90]);
}
