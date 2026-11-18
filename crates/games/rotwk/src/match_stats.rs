use mint_sim::{Component, ComponentId, Entity, PlayerId, StateHasher, Tick};

use crate::game_data::ScoreWeights;

pub(crate) const PLAYER_SLOTS: usize = 20;

pub(crate) const SAMPLE_PERIOD: u32 = mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Sample {
    pub(crate) money: i32,
    pub(crate) score: f32,
    pub(crate) units: i16,
    pub(crate) structures: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpendKind {
    Units,
    Structures,
    Heroes,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct MatchStats {
    pub(crate) earned: i64,
    pub(crate) spent_on_units: i64,
    pub(crate) spent_on_structures: i64,
    pub(crate) spent_on_heroes: i64,
    spent_total: i64,
    pub(crate) heroes_vetted: u32,
    pub(crate) units_vetted: u32,
    pub(crate) power_points: u32,
    pub(crate) skill_points: f32,
    pub(crate) elimination_bonus: f32,
    pub(crate) last_enemy: Option<PlayerId>,
    units_destroyed: [u32; PLAYER_SLOTS],
    structures_destroyed: [u32; PLAYER_SLOTS],
    pub(crate) units_built: u32,
    pub(crate) units_lost: u32,
    pub(crate) structures_built: u32,
    pub(crate) structures_lost: u32,
    pub(crate) end_tick: Option<u32>,
    pub(crate) first_hero_tick: Option<u32>,
    pub(crate) fortress_ticks: Vec<u32>,
    built: Vec<(u16, u32)>,
    lost: Vec<(u16, u32)>,
    kills_of: Vec<(u16, u32)>,
    samples: Vec<Sample>,
    samples_digest: u64,
}

const DIGEST_PRIME: u64 = 0x0100_0000_01b3;

fn bump(map: &mut Vec<(u16, u32)>, template: u16) {
    match map.binary_search_by_key(&template, |(key, _)| *key) {
        Ok(at) => {
            if let Some((_, count)) = map.get_mut(at) {
                *count = count.saturating_add(1);
            }
        }
        Err(at) => map.insert(at, (template, 1)),
    }
}

fn unbump(map: &mut [(u16, u32)], template: u16) {
    if let Ok(at) = map.binary_search_by_key(&template, |(key, _)| *key) {
        if let Some((_, count)) = map.get_mut(at) {
            *count = count.saturating_sub(1);
        }
    }
}

fn count_where(map: &[(u16, u32)], matches: impl Fn(u16) -> bool) -> u32 {
    map.iter()
        .filter(|(template, _)| matches(*template))
        .fold(0, |total, (_, count)| total.saturating_add(*count))
}

impl MatchStats {
    pub(crate) fn spent(&self) -> i64 {
        self.spent_total
    }

    pub(crate) fn spend(&mut self, kind: SpendKind, cost: i64) {
        self.spent_total = self.spent_total.saturating_add(cost);
        self.category(kind, cost);
    }

    pub(crate) fn refund(&mut self, kind: SpendKind, amount: i64) {
        self.category(kind, amount.saturating_neg());
    }

    fn category(&mut self, kind: SpendKind, amount: i64) {
        let slot = match kind {
            SpendKind::Units => &mut self.spent_on_units,
            SpendKind::Structures => &mut self.spent_on_structures,
            SpendKind::Heroes => &mut self.spent_on_heroes,
        };
        *slot = slot.saturating_add(amount);
    }

    pub(crate) fn remove_structure_built(&mut self, template: u16) {
        self.structures_built = self.structures_built.saturating_sub(1);
        unbump(&mut self.built, template);
    }

    pub(crate) fn add_unit_built(&mut self, template: u16, members: u32) {
        self.units_built = self.units_built.saturating_add(members);
        bump(&mut self.built, template);
    }

    pub(crate) fn add_unit_lost(&mut self, template: u16) {
        self.units_lost = self.units_lost.saturating_add(1);
        bump(&mut self.lost, template);
    }

    pub(crate) fn add_structure_built(&mut self, template: u16) {
        self.structures_built = self.structures_built.saturating_add(1);
        bump(&mut self.built, template);
    }

    pub(crate) fn add_structure_lost(&mut self, template: u16) {
        self.structures_lost = self.structures_lost.saturating_add(1);
        bump(&mut self.lost, template);
    }

    pub(crate) fn add_kill(&mut self, own: PlayerId, victim: PlayerId, structure: bool) {
        if own == victim {
            return;
        }
        let table = if structure {
            &mut self.structures_destroyed
        } else {
            &mut self.units_destroyed
        };
        if let Some(count) = table.get_mut(usize::from(victim.get())) {
            *count = count.saturating_add(1);
        }
    }

    pub(crate) fn add_kill_of(&mut self, template: u16) {
        bump(&mut self.kills_of, template);
    }

    pub(crate) fn kills_of(&self, template: u16) -> u32 {
        count_where(&self.kills_of, |held| held == template)
    }

    pub(crate) fn units_killed(&self) -> u32 {
        self.units_destroyed
            .iter()
            .fold(0, |total, count| total.saturating_add(*count))
    }

    pub(crate) fn structures_destroyed(&self) -> u32 {
        self.structures_destroyed
            .iter()
            .fold(0, |total, count| total.saturating_add(*count))
    }

    pub(crate) fn built_where(&self, matches: impl Fn(u16) -> bool) -> u32 {
        count_where(&self.built, matches)
    }

    pub(crate) fn lost_where(&self, matches: impl Fn(u16) -> bool) -> u32 {
        count_where(&self.lost, matches)
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn score(&self, own: PlayerId, weights: &ScoreWeights) -> f32 {
        let mut score = self.skill_points * weights.skill_points;
        score += self.earned as f32 * weights.supplies_collected;
        score += self.structures_built as f32 * weights.structures_built;
        score += self.units_built as f32 * weights.units_built;
        let own = usize::from(own.get());
        for (index, (units, structures)) in self
            .units_destroyed
            .iter()
            .zip(&self.structures_destroyed)
            .enumerate()
        {
            if index != own {
                score += *units as f32 * weights.units_destroyed;
                score += *structures as f32 * weights.structures_destroyed;
            }
        }
        score += self.power_points as f32 * weights.power_points;
        score += self.heroes_vetted as f32 * weights.heroes_vetted;
        score += self.units_vetted as f32 * weights.units_vetted;
        score + self.elimination_bonus
    }

    pub(crate) fn built(&self) -> &[(u16, u32)] {
        &self.built
    }

    pub(crate) fn samples(&self) -> &[Sample] {
        &self.samples
    }

    pub(crate) fn write_sample(&mut self, index: usize, sample: Sample) {
        if self.samples.len() <= index {
            self.samples
                .resize(index.saturating_add(1), Sample::default());
        }
        if let Some(slot) = self.samples.get_mut(index) {
            *slot = sample;
        }
        let words = [
            u64::try_from(index).unwrap_or(u64::MAX),
            u64::from(sample.money.cast_unsigned()),
            u64::from(sample.score.to_bits()),
            u64::from(sample.units.cast_unsigned()),
            u64::from(sample.structures.cast_unsigned()),
        ];
        for word in words {
            self.samples_digest = (self.samples_digest ^ word).wrapping_mul(DIGEST_PRIME);
        }
    }
}

fn hash_map(hasher: &mut StateHasher, map: &[(u16, u32)]) {
    hasher.write_usize(map.len());
    for (template, count) in map {
        hasher.write_u16(*template);
        hasher.write_u32(*count);
    }
}

fn hash_tick(hasher: &mut StateHasher, tick: Option<u32>) {
    hasher.write_bool(tick.is_some());
    hasher.write_u32(tick.unwrap_or(0));
}

impl Component for MatchStats {
    fn hash_into(&self, hasher: &mut StateHasher) {
        for value in [
            self.earned,
            self.spent_on_units,
            self.spent_on_structures,
            self.spent_on_heroes,
            self.spent_total,
        ] {
            hasher.write_i64(value);
        }
        for count in [self.heroes_vetted, self.units_vetted, self.power_points] {
            hasher.write_u32(count);
        }
        hasher.write_u32(self.skill_points.to_bits());
        hasher.write_u32(self.elimination_bonus.to_bits());
        hasher.write_bool(self.last_enemy.is_some());
        hasher.write_u8(self.last_enemy.map_or(0, PlayerId::get));
        for count in self
            .units_destroyed
            .iter()
            .chain(&self.structures_destroyed)
        {
            hasher.write_u32(*count);
        }
        for count in [
            self.units_built,
            self.units_lost,
            self.structures_built,
            self.structures_lost,
        ] {
            hasher.write_u32(count);
        }
        hash_tick(hasher, self.end_tick);
        hash_tick(hasher, self.first_hero_tick);
        hasher.write_usize(self.fortress_ticks.len());
        for tick in &self.fortress_ticks {
            hasher.write_u32(*tick);
        }
        hash_map(hasher, &self.built);
        hash_map(hasher, &self.lost);
        if !self.kills_of.is_empty() {
            hash_map(hasher, &self.kills_of);
        }
        hasher.write_usize(self.samples.len());
        hasher.write_u64(self.samples_digest);
    }
}

pub(crate) fn record(
    tick: &mut Tick<'_>,
    id: ComponentId<MatchStats>,
    player_entity: Entity,
    update: impl FnOnce(&mut MatchStats),
) {
    if let Some(stats) = tick.state_mut().get_mut(id, player_entity) {
        update(stats);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndStatus {
    Victorious,
    Defeated,
    Undecided,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlayerFigures {
    pub(crate) player: PlayerId,
    pub(crate) side: Option<String>,
    pub(crate) status: EndStatus,
    pub(crate) end_tick: u32,
    pub(crate) structures_built: u32,
    pub(crate) structures_lost: u32,
    pub(crate) structures_destroyed: u32,
    pub(crate) fortresses: u32,
    pub(crate) units_built: u32,
    pub(crate) units_lost: u32,
    pub(crate) units_killed: u32,
    pub(crate) favourite: Option<String>,
    pub(crate) earned: i64,
    pub(crate) spent: i64,
    pub(crate) spent_on_structures: i64,
    pub(crate) spent_on_units: i64,
    pub(crate) spent_on_heroes: i64,
    pub(crate) first_hero_tick: Option<u32>,
    pub(crate) heroes_built: u32,
    pub(crate) heroes_lost: u32,
    pub(crate) fortress_ticks: Vec<u32>,
    pub(crate) samples: Vec<Sample>,
}
