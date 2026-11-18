use std::collections::HashMap;

use mint_sim::Entity;

#[derive(Debug, Default)]
pub struct Positions(HashMap<Entity, (f32, f32, f32, f32)>);

impl Positions {
    #[must_use]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Entity, (f32, f32, f32, f32))>) -> Self {
        Self(pairs.into_iter().collect())
    }

    #[must_use]
    pub fn get(&self, entity: Entity) -> Option<(f32, f32, f32, f32)> {
        self.0.get(&entity).copied()
    }
}

#[derive(Debug, Default)]
pub struct ModelConditions(HashMap<Entity, crate::effects::Names>);

impl ModelConditions {
    #[must_use]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Entity, crate::effects::Names)>) -> Self {
        Self(pairs.into_iter().collect())
    }

    #[must_use]
    pub fn holds(&self, entity: Entity, name: &str) -> bool {
        self.0
            .get(&entity)
            .is_some_and(|flags| flags.iter().any(|flag| flag.eq_ignore_ascii_case(name)))
    }
}

#[derive(Debug, Default)]
pub struct Movers(HashMap<Entity, Entity>);

impl Movers {
    #[must_use]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Entity, Entity)>) -> Self {
        Self(pairs.into_iter().collect())
    }

    #[must_use]
    pub fn resolve(&self, entity: Entity) -> Entity {
        self.0.get(&entity).copied().unwrap_or(entity)
    }
}

#[derive(Debug, Default)]
pub struct StillMoving(Vec<Entity>);

impl StillMoving {
    #[must_use]
    pub fn from_entities(movers: impl IntoIterator<Item = Entity>) -> Self {
        let mut movers: Vec<Entity> = movers.into_iter().collect();
        movers.sort_unstable();
        movers.dedup();
        Self(movers)
    }

    #[must_use]
    pub fn contains(&self, mover: Entity) -> bool {
        self.0.binary_search(&mover).is_ok()
    }
}

#[derive(Debug, Default)]
pub struct PlayerOrdered(Vec<Entity>);

impl PlayerOrdered {
    #[must_use]
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let mut entities: Vec<Entity> = entities.into_iter().collect();
        entities.sort_unstable();
        entities.dedup();
        Self(entities)
    }

    #[must_use]
    pub fn contains(&self, entity: Entity) -> bool {
        self.0.binary_search(&entity).is_ok()
    }
}

#[derive(Debug, Default)]
pub struct Structures(Vec<Entity>);

impl Structures {
    #[must_use]
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let mut entities: Vec<Entity> = entities.into_iter().collect();
        entities.sort_unstable();
        entities.dedup();
        Self(entities)
    }

    #[must_use]
    pub fn contains(&self, entity: Entity) -> bool {
        self.0.binary_search(&entity).is_ok()
    }
}

#[derive(Debug, Default)]
pub struct Immobile(Vec<Entity>);

impl Immobile {
    #[must_use]
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let mut entities: Vec<Entity> = entities.into_iter().collect();
        entities.sort_unstable();
        entities.dedup();
        Self(entities)
    }

    #[must_use]
    pub fn contains(&self, entity: Entity) -> bool {
        self.0.binary_search(&entity).is_ok()
    }
}

#[derive(Debug, Default)]
pub struct Hordes(Vec<Entity>);

impl Hordes {
    #[must_use]
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let mut entities: Vec<Entity> = entities.into_iter().collect();
        entities.sort_unstable();
        entities.dedup();
        Self(entities)
    }

    #[must_use]
    pub fn contains(&self, entity: Entity) -> bool {
        self.0.binary_search(&entity).is_ok()
    }
}

#[derive(Debug, Default)]
pub struct ViewBlocked(Vec<Entity>);

impl ViewBlocked {
    #[must_use]
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let mut entities: Vec<Entity> = entities.into_iter().collect();
        entities.sort_unstable();
        entities.dedup();
        Self(entities)
    }

    #[must_use]
    pub fn contains(&self, entity: Entity) -> bool {
        self.0.binary_search(&entity).is_ok()
    }
}

#[derive(Debug, Default)]
pub struct UnderConstruction(Vec<Entity>);

impl UnderConstruction {
    #[must_use]
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let mut entities: Vec<Entity> = entities.into_iter().collect();
        entities.sort_unstable();
        entities.dedup();
        Self(entities)
    }

    #[must_use]
    pub fn contains(&self, entity: Entity) -> bool {
        self.0.binary_search(&entity).is_ok()
    }
}

#[derive(Debug, Default)]
pub struct NeutralControlled(Vec<Entity>);

impl NeutralControlled {
    #[must_use]
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let mut entities: Vec<Entity> = entities.into_iter().collect();
        entities.sort_unstable();
        entities.dedup();
        Self(entities)
    }

    #[must_use]
    pub fn contains(&self, entity: Entity) -> bool {
        self.0.binary_search(&entity).is_ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlaveView {
    pub at: (f32, f32),
    pub idle: bool,
    pub ordered_goal: Option<(f32, f32)>,
    pub master: Option<MasterView>,
    pub allied: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MasterView {
    pub at: (f32, f32),
    pub facing: f32,
    pub victim: Option<(Entity, (f32, f32))>,
    pub owner: mint_sim::PlayerId,
}

#[derive(Debug, Default)]
pub struct MasterSlaves(Vec<(Entity, Entity)>);

impl MasterSlaves {
    #[must_use]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Entity, Entity)>) -> Self {
        let mut pairs: Vec<(Entity, Entity)> = pairs.into_iter().collect();
        pairs.sort_unstable();
        pairs.dedup();
        Self(pairs)
    }

    pub fn slaves_of(&self, master: Entity) -> impl Iterator<Item = Entity> + '_ {
        let start = self.0.partition_point(|(held, _)| *held < master);
        self.0
            .iter()
            .skip(start)
            .take_while(move |(held, _)| *held == master)
            .map(|(_, slave)| *slave)
    }
}

#[derive(Debug, Default)]
pub struct Slaves(Vec<(Entity, SlaveView)>);

impl Slaves {
    #[must_use]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Entity, SlaveView)>) -> Self {
        let mut pairs: Vec<(Entity, SlaveView)> = pairs.into_iter().collect();
        pairs.sort_unstable_by_key(|(slave, _)| *slave);
        pairs.dedup_by_key(|(slave, _)| *slave);
        Self(pairs)
    }

    #[must_use]
    pub fn get(&self, slave: Entity) -> Option<&SlaveView> {
        let at = self
            .0
            .binary_search_by_key(&slave, |(entity, _)| *entity)
            .ok()?;
        self.0.get(at).map(|(_, view)| view)
    }
}
