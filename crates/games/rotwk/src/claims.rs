use mint_movement::Vec2;
use mint_sage_passability::claims::{ClaimGrid, ClaimLayer as Cells, Claimer};
use mint_sim::{Component, ComponentId, Entity, PlayerId, StateHasher, World};

use crate::construction::BuildTask;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Claimant {
    pub(crate) radius: f32,
    pub(crate) high_priority: bool,
    pub(crate) scales_income: bool,
    pub(crate) share: Option<f32>,
}

impl Component for Claimant {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.radius);
        hasher.write_bool(self.high_priority);
        hasher.write_bool(self.scales_income);
        hasher.write_bool(self.share.is_some());
        hasher.write_f32(self.share.unwrap_or(0.0));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Holder {
    Object(Entity),
    Plan { builder: Entity, task: BuildTask },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Registered {
    pub(crate) holder: Holder,
    pub(crate) owner: PlayerId,
    pub(crate) centre: Vec2,
    pub(crate) radius: f32,
    pub(crate) finished: bool,
    pub(crate) share: f32,
}

impl Registered {
    pub(crate) fn claimer(&self) -> Claimer {
        Claimer {
            key: key(&self.holder),
            player: u32::from(self.owner.get()),
            finished: self.finished,
        }
    }

    pub(crate) fn is_live(&self, world: &World, tasks: ComponentId<BuildTask>) -> bool {
        match &self.holder {
            Holder::Object(entity) => world.is_live(*entity),
            Holder::Plan { builder, task } => {
                world.is_live(*builder) && world.get(tasks, *builder) == Some(task)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClaimLayer {
    cells: Cells,
    registered: Vec<Registered>,
    max_radius: f32,
}

impl ClaimLayer {
    pub(crate) fn empty(grid: &ClaimGrid) -> Self {
        Self {
            cells: Cells::empty(grid),
            registered: Vec::new(),
            max_radius: 0.0,
        }
    }

    pub(crate) const fn cells(&self) -> &Cells {
        &self.cells
    }

    pub(crate) fn registered(&self) -> &[Registered] {
        &self.registered
    }

    pub(crate) const fn max_radius(&self) -> f32 {
        self.max_radius
    }

    pub(crate) fn register(&mut self, claimant: Registered, high_priority: bool) -> usize {
        if high_priority {
            self.registered.insert(0, claimant);
            0
        } else {
            self.registered.push(claimant);
            self.registered.len().saturating_sub(1)
        }
    }

    pub(crate) fn take(&mut self, at: usize, cells: &[u32], wanted: u64) -> Option<(u64, f32)> {
        let claimant = self.registered.get_mut(at)?;
        let taken = self.cells.take(cells, claimant.claimer());
        claimant.share = mint_sage_passability::claims::share(taken, wanted);
        self.max_radius = self.max_radius.max(claimant.radius);
        Some((taken, claimant.share))
    }

    pub(crate) fn finish(&mut self, at: usize) {
        if let Some(claimant) = self.registered.get_mut(at) {
            claimant.finished = true;
        }
    }

    pub(crate) fn remove(&mut self, at: usize) -> Option<Registered> {
        (at < self.registered.len()).then(|| {
            let claimant = self.registered.remove(at);
            self.cells.release(key(&claimant.holder));
            claimant
        })
    }

    pub(crate) fn holds_plan(&self, builder: Entity, task: &BuildTask) -> bool {
        self.registered.iter().any(|claimant| {
            matches!(&claimant.holder, Holder::Plan { builder: b, task: t } if *b == builder && t == task)
        })
    }

    pub(crate) fn object_of(&self, key_held: u32) -> Option<Entity> {
        self.registered
            .iter()
            .find(|claimant| key(&claimant.holder) == key_held)
            .and_then(|claimant| match claimant.holder {
                Holder::Object(entity) => Some(entity),
                Holder::Plan { .. } => None,
            })
    }

    pub(crate) fn hand_over(
        &mut self,
        builder: Entity,
        kind: crate::unit::Kind,
        site: Entity,
    ) -> Option<f32> {
        let claimant = self.registered.iter_mut().find(|claimant| {
            matches!(&claimant.holder, Holder::Plan { builder: b, task } if *b == builder && task.kind == kind)
        })?;
        let object = Holder::Object(site);
        self.cells.rekey(key(&claimant.holder), key(&object));
        claimant.holder = object;
        Some(claimant.share)
    }
}

fn key(holder: &Holder) -> u32 {
    let (entity, plan) = match holder {
        Holder::Object(entity) => (entity, 1),
        Holder::Plan { builder, .. } => (builder, 2),
    };
    entity
        .index()
        .checked_mul(2)
        .and_then(|key| key.checked_add(plan))
        .unwrap_or(0)
}

impl Component for ClaimLayer {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.registered.len());
        for claimant in &self.registered {
            match &claimant.holder {
                Holder::Object(entity) => {
                    hasher.write_bool(false);
                    entity.hash_into(hasher);
                }
                Holder::Plan { builder, task } => {
                    hasher.write_bool(true);
                    builder.hash_into(hasher);
                    task.hash_into(hasher);
                }
            }
            hasher.write_u8(claimant.owner.get());
            hasher.write_f32(claimant.centre.x);
            hasher.write_f32(claimant.centre.y);
            hasher.write_f32(claimant.radius);
            hasher.write_bool(claimant.finished);
            hasher.write_f32(claimant.share);
        }
        hasher.write_f32(self.max_radius);
    }
}

pub(crate) fn layer_of(world: &World, id: ComponentId<ClaimLayer>) -> Option<&ClaimLayer> {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
}
