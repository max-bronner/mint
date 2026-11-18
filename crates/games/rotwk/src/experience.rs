use mint_sage_combat::{Attributes, ModifierId, ModifierStore};
use mint_sage_modules::{ExperienceTables, UpgradeBits};
use mint_sim::{Component, StateHasher, UpgradeMask};

use crate::templates::KindTable;
use crate::unit::Kind;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Level {
    pub(crate) name: String,
    pub(crate) rank: u16,
    pub(crate) required: u32,
    pub(crate) award: u32,
    pub(crate) own_guys: u32,
    pub(crate) decal: Option<mint_sage_modules::SelectionDecal>,
    pub(crate) modifiers: Vec<ModifierId>,
    pub(crate) granted: UpgradeMask,
    pub(crate) level_up_fx: Option<String>,
    pub(crate) emotion: Option<mint_sage_ini::EmotionType>,
    pub(crate) level_up_tint: Option<mint_sage_modules::LevelUpTint>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LevelTable {
    pub(crate) levels: Vec<Level>,
    pub(crate) trainable: bool,
    pub(crate) created_at: usize,
    pub(crate) scalars: Vec<f32>,
}

impl LevelTable {
    pub(crate) fn index_for(&self, points: u32) -> usize {
        self.levels
            .iter()
            .rposition(|level| level.required <= points)
            .unwrap_or(0)
    }

    pub(crate) fn with_unlocks(&self, unlocks: &[Option<mint_sim::UpgradeBit>]) -> Self {
        let mut table = self.clone();
        let mut granted = UpgradeMask::EMPTY;
        for (at, level) in table.levels.iter_mut().enumerate() {
            if let Some(bit) = unlocks.get(at).copied().flatten() {
                granted.set(bit);
            }
            level.granted = granted.clone();
        }
        table
    }

    pub(crate) fn index_at_rank(&self, rank: u16) -> Option<usize> {
        self.levels.iter().rposition(|level| level.rank <= rank)
    }

    pub(crate) fn level(&self, index: usize) -> Option<&Level> {
        self.levels.get(index)
    }

    pub(crate) fn top(&self) -> usize {
        self.levels.len().saturating_sub(1)
    }

    pub(crate) fn experience_scalar(&self, rank: u16, base_rank: u16) -> f32 {
        let Some(top) = self.scalars.len().checked_sub(1) else {
            return 1.0;
        };
        let offset = i32::from(rank) - i32::from(base_rank);
        #[allow(clippy::cast_sign_loss)]
        let index = offset.clamp(0, i32::try_from(top).unwrap_or(i32::MAX)) as usize;
        self.scalars.get(index).copied().unwrap_or(1.0)
    }

    pub(crate) fn readout(&self, index: usize, points: u32) -> Option<LevelReadout> {
        if self.levels.len() < 2 {
            return None;
        }
        let level = self.level(index)?;
        let progress = self.level(index.saturating_add(1)).map(|next| {
            let span = next.required.saturating_sub(level.required);
            let gained = points.saturating_sub(level.required).min(span);
            if span == 0 {
                1.0
            } else {
                #[allow(clippy::cast_possible_truncation)]
                let fraction = (f64::from(gained) / f64::from(span)) as f32;
                fraction
            }
        });
        Some(LevelReadout {
            rank: level.rank,
            progress,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct LevelReadout {
    pub(crate) rank: u16,
    pub(crate) progress: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RankDisplay {
    Level(LevelReadout),
    TimeRemaining(f32),
}

impl RankDisplay {
    const BAR_TOLERANCE: f32 = 0.1;

    pub(crate) fn bar(self) -> Option<f32> {
        match self {
            Self::Level(level) => level.progress,
            Self::TimeRemaining(left) => Some(left),
        }
    }

    pub(crate) fn agrees_with(self, other: Self) -> bool {
        let same_number = match (self, other) {
            (Self::Level(a), Self::Level(b)) => a.rank == b.rank,
            (Self::TimeRemaining(_), Self::TimeRemaining(_)) => true,
            _ => false,
        };
        same_number
            && match (self.bar(), other.bar()) {
                (Some(a), Some(b)) => (a - b).abs() < Self::BAR_TOLERANCE,
                (None, None) => true,
                _ => false,
            }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LevelReport {
    pub(crate) kinds: usize,
    pub(crate) levels: usize,
    pub(crate) unknown_modifiers: usize,
    pub(crate) timed_modifiers: usize,
    pub(crate) unknown_upgrades: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LevelTables {
    tables: Vec<Option<LevelTable>>,
    pub(crate) report: LevelReport,
    unknown_upgrade_names: Vec<String>,
}

impl LevelTables {
    pub(crate) fn build(
        kinds: &KindTable,
        experience: &ExperienceTables,
        scalars: &mint_sage_modules::ExperienceScalarTables,
        modifiers: &ModifierStore,
        bits: &UpgradeBits,
    ) -> Self {
        let mut out = Self::default();
        for index in 0..kinds.len() {
            let entry = u16::try_from(index).ok().and_then(|k| kinds.get(Kind(k)));
            let table = entry.and_then(|entry| {
                let records = experience.table(&entry.record.name)?;
                let mut granted = UpgradeMask::EMPTY;
                let mut levels = Vec::with_capacity(records.len());
                for record in records {
                    let mut ids = Vec::new();
                    for name in &record.attribute_modifiers {
                        match modifiers.id(name) {
                            Some(id) if modifiers.get(id).is_some_and(|l| l.is_permanent()) => {
                                ids.push(id);
                            }
                            Some(_) => {
                                out.report.timed_modifiers =
                                    out.report.timed_modifiers.saturating_add(1);
                            }
                            None => {
                                out.report.unknown_modifiers =
                                    out.report.unknown_modifiers.saturating_add(1);
                            }
                        }
                    }
                    for name in &record.upgrades {
                        match bits.bit(name) {
                            Some(bit) => granted.set(bit),
                            None => {
                                out.report.unknown_upgrades =
                                    out.report.unknown_upgrades.saturating_add(1);
                                if !out.unknown_upgrade_names.iter().any(|n| n == name) {
                                    out.unknown_upgrade_names.push(name.clone());
                                }
                            }
                        }
                    }
                    levels.push(Level {
                        name: record.block.clone(),
                        rank: record.rank,
                        required: record.required,
                        award: record.award,
                        own_guys: record.own_guys_award,
                        decal: record.decal.clone(),
                        modifiers: ids,
                        granted: granted.clone(),
                        level_up_fx: record.level_up_fx.clone(),
                        emotion: record
                            .emotion_type
                            .as_deref()
                            .and_then(mint_sage_ini::EmotionType::parse),
                        level_up_tint: record.level_up_tint,
                    });
                }
                if levels.is_empty() {
                    return None;
                }
                let created_at = entry
                    .record
                    .level_on_create
                    .and_then(|rank| levels.iter().rposition(|level| level.rank <= rank))
                    .unwrap_or(0);
                Some(LevelTable {
                    levels,
                    trainable: entry.record.is_trainable.unwrap_or(true),
                    created_at,
                    scalars: entry
                        .record
                        .experience_scalar_table
                        .as_deref()
                        .and_then(|name| scalars.table(name))
                        .map(<[f32]>::to_vec)
                        .unwrap_or_default(),
                })
            });
            if let Some(table) = &table {
                out.report.kinds = out.report.kinds.saturating_add(1);
                out.report.levels = out.report.levels.saturating_add(table.levels.len());
            }
            out.tables.push(table);
        }
        out
    }

    pub(crate) fn get(&self, kind: Kind) -> Option<&LevelTable> {
        self.tables.get(usize::from(kind.0))?.as_ref()
    }

    pub(crate) fn describe(&self) -> String {
        format!(
            "experience: {} kind(s) with a level table, {} level(s); {} modifier list name(s) \
             unknown, {} timed list(s) left out, {} upgrade name(s) unknown {:?}",
            self.report.kinds,
            self.report.levels,
            self.report.unknown_modifiers,
            self.report.timed_modifiers,
            self.report.unknown_upgrades,
            self.unknown_upgrade_names
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Experience {
    pub(crate) points: u32,
    pub(crate) level: u16,
    pub(crate) base_rank: u16,
    pub(crate) max_level: u16,
}

impl Component for Experience {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.points);
        hasher.write_u16(self.level);
        hasher.write_u16(self.base_rank);
        if self.max_level != 0 {
            hasher.write_u16(self.max_level);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bonuses(pub(crate) Attributes);

impl Component for Bonuses {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WeaponBonusConditions(pub(crate) mint_sage_combat::BonusConditions);

impl Component for WeaponBonusConditions {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.0 .0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TimedModifiers(pub(crate) Vec<(ModifierId, mint_sim::Frame)>);

impl Component for TimedModifiers {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (list, until) in &self.0 {
            hasher.write_u16(list.index());
            until.hash_into(hasher);
        }
    }
}

impl TimedModifiers {
    pub(crate) fn give(&mut self, list: ModifierId, until: mint_sim::Frame) -> bool {
        match self.0.iter_mut().find(|(held, _)| *held == list) {
            Some((_, end)) => {
                *end = (*end).max(until);
                false
            }
            None => {
                self.0.push((list, until));
                true
            }
        }
    }

    pub(crate) fn expire(&mut self, now: mint_sim::Frame) -> bool {
        let before = self.0.len();
        self.0.retain(|(_, until)| *until > now);
        self.0.len() != before
    }

    pub(crate) fn remove(&mut self, list: ModifierId) -> bool {
        let before = self.0.len();
        self.0.retain(|(held, _)| *held != list);
        self.0.len() != before
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TimedConditions(
    pub(crate) Vec<(mint_sage_modules::effects::Name, mint_sim::Frame)>,
);

impl Component for TimedConditions {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (flag, until) in &self.0 {
            hasher.write_bytes(flag.as_bytes());
            until.hash_into(hasher);
        }
    }
}

impl TimedConditions {
    pub(crate) fn give(
        &mut self,
        flag: mint_sage_modules::effects::Name,
        until: mint_sim::Frame,
    ) -> bool {
        match self.0.iter_mut().find(|(held, _)| *held == flag) {
            Some((_, end)) => {
                *end = (*end).max(until);
                false
            }
            None => {
                self.0.push((flag, until));
                true
            }
        }
    }

    pub(crate) fn expire(&mut self, now: mint_sim::Frame) -> Vec<mint_sage_modules::effects::Name> {
        let mut expired = Vec::new();
        self.0.retain(|(flag, until)| {
            if *until <= now {
                expired.push(mint_sage_modules::effects::Name::clone(flag));
                false
            } else {
                true
            }
        });
        expired
    }
}

pub(crate) const fn scores_kill(
    same_player: bool,
    hostile: bool,
    under_construction: bool,
) -> bool {
    !same_player && hostile && !under_construction
}

pub(crate) fn with_speed(
    limits: mint_movement::MotionLimits,
    bonuses: Option<&Bonuses>,
) -> mint_movement::MotionLimits {
    let Some(bonuses) = bonuses else {
        return limits;
    };
    let speed = limits.max_speed * bonuses.0.speed;
    if !speed.is_finite() || speed < 0.0 {
        return limits;
    }
    mint_movement::MotionLimits {
        max_speed: speed,
        ..limits
    }
}

pub(crate) fn production_time(frames: mint_sim::LogicFrames, speed: f32) -> mint_sim::LogicFrames {
    if !speed.is_finite() || speed <= 0.0 {
        return frames;
    }
    #[allow(clippy::cast_precision_loss)]
    let scaled = frames.get() as f32 / speed;
    if !scaled.is_finite() || scaled < 0.0 {
        return frames;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = scaled.min(u32::MAX as f32) as u32;
    mint_sim::LogicFrames::from_frames(whole)
}

pub(crate) fn recharge_time(frames: mint_sim::LogicFrames, factor: f32) -> mint_sim::LogicFrames {
    if !factor.is_finite() || factor <= 0.0 {
        return frames;
    }
    production_time(frames, 1.0 / factor)
}

pub(crate) fn scaled_frames(frames: u32, multiplier: f32) -> u32 {
    if !multiplier.is_finite() || multiplier < 0.0 {
        return frames;
    }
    #[allow(clippy::cast_precision_loss)]
    let product = frames as f32 * multiplier;
    if !product.is_finite() {
        return frames;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = product.min(u32::MAX as f32).max(0.0) as u32;
    whole
}

pub(crate) fn scaled_amount(amount: i64, multiplier: f32) -> Option<i64> {
    if !multiplier.is_finite() || multiplier < 0.0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let product = amount as f32 * multiplier;
    if !product.is_finite() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    let whole = product as i64;
    Some(whole)
}

pub(crate) fn scaled(amount: u32, scalar: f32) -> u32 {
    #[allow(clippy::cast_precision_loss)]
    let product = amount as f32 * scalar;
    if !product.is_finite() || product <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = product as u32;
    whole
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn experience_scalar_clamps_rank_minus_base_rank() {
        let mut levels = table(&[0, 50, 1000]);
        levels.scalars = vec![1.0, 0.5, 0.25];
        assert_eq!(levels.experience_scalar(1, 1), 1.0);
        assert_eq!(levels.experience_scalar(2, 1), 0.5);
        assert_eq!(levels.experience_scalar(3, 1), 0.25);
        assert_eq!(levels.experience_scalar(10, 1), 0.25, "clamped to the top");
        assert_eq!(
            levels.experience_scalar(1, 5),
            1.0,
            "clamped to zero below base"
        );
    }

    #[test]
    fn an_empty_scalar_table_is_always_1() {
        let levels = table(&[0, 50]);
        assert_eq!(levels.experience_scalar(5, 1), 1.0);
    }

    #[test]
    fn two_readouts_are_one_only_with_the_same_mode_rank_and_nearly_the_same_bar() {
        let level = |rank, progress| RankDisplay::Level(LevelReadout { rank, progress });
        let time = RankDisplay::TimeRemaining;
        assert!(level(2, Some(0.5)).agrees_with(level(2, Some(0.59))));
        assert!(!level(2, Some(0.5)).agrees_with(level(2, Some(0.61))));
        assert!(!level(2, Some(0.5)).agrees_with(level(3, Some(0.5))));
        assert!(level(4, None).agrees_with(level(4, None)));
        assert!(!level(4, None).agrees_with(level(4, Some(0.0))));
        assert!(time(0.5).agrees_with(time(0.41)));
        assert!(!time(0.5).agrees_with(time(0.39)));
        assert!(!time(0.5).agrees_with(level(1, Some(0.5))));
    }

    fn table(required: &[u32]) -> LevelTable {
        LevelTable {
            levels: required
                .iter()
                .enumerate()
                .map(|(at, required)| Level {
                    name: String::new(),
                    rank: u16::try_from(at).unwrap() + 1,
                    required: *required,
                    award: 0,
                    own_guys: 0,
                    decal: None,
                    modifiers: Vec::new(),
                    granted: UpgradeMask::EMPTY,
                    level_up_fx: None,
                    emotion: None,
                    level_up_tint: None,
                })
                .collect(),
            trainable: true,
            created_at: 0,
            scalars: Vec::new(),
        }
    }

    #[test]
    fn scaled_frames_multiplies_rather_than_divides() {
        assert_eq!(scaled_frames(100, 0.5), 50);
        assert_eq!(scaled_frames(100, 1.0), 100);
        assert_eq!(scaled_frames(100, 2.0), 200);
        assert_eq!(scaled_frames(100, -1.0), 100, "negative: unchanged");
        assert_eq!(scaled_frames(100, f32::NAN), 100, "non-finite: unchanged");
    }

    #[test]
    fn the_walk_finds_the_highest_level_met_and_can_skip_levels() {
        let t = table(&[1, 50, 100, 150, 200]);
        assert_eq!(t.index_for(0), 0, "the first level is the floor");
        assert_eq!(t.index_for(49), 0);
        assert_eq!(t.index_for(50), 1);
        assert_eq!(t.index_for(149), 2);
        assert_eq!(t.index_for(10_000), 4, "one award may skip levels");
        assert_eq!(t.top(), 4);
    }

    #[test]
    fn the_readout_fills_across_the_current_level_and_ends_at_the_last() {
        let t = table(&[1, 50, 100]);
        let shown = |index, points| t.readout(index, points).unwrap();
        assert_eq!(shown(0, 1).progress, Some(0.0), "a fresh object: empty");
        assert_eq!(shown(1, 75).progress, Some(0.5), "half of 50..100");
        assert_eq!((shown(2, 100).rank, shown(2, 100).progress), (3, None));
        assert_eq!(table(&[1]).readout(0, 1), None, "one level: nothing shown");
    }

    #[test]
    fn a_rank_maps_to_the_highest_level_at_or_below_it() {
        let t = table(&[1, 50, 100]);
        assert_eq!(t.index_at_rank(2), Some(1));
        assert_eq!(t.index_at_rank(9), Some(2));
        assert_eq!(t.index_at_rank(0), None);
    }

    #[test]
    fn only_an_enemys_finished_object_scores_a_kill() {
        for same in [false, true] {
            for hostile in [false, true] {
                for site in [false, true] {
                    assert_eq!(
                        scores_kill(same, hostile, site),
                        !same && hostile && !site,
                        "same {same} hostile {hostile} site {site}"
                    );
                }
            }
        }
        assert!(scores_kill(false, true, false));
    }

    #[test]
    fn production_speed_shortens_the_time_and_scales_income() {
        let frames = mint_sim::LogicFrames::from_frames(300);
        assert_eq!(production_time(frames, 1.25).get(), 240);
        assert_eq!(production_time(frames, 1.0).get(), 300);
        assert_eq!(
            production_time(frames, 0.0).get(),
            300,
            "a zero speed is refused"
        );
        assert_eq!(production_time(frames, f32::NAN).get(), 300);
        assert_eq!(scaled_amount(25, 1.5), Some(37));
        assert_eq!(scaled_amount(25, -1.0), None);
    }

    #[test]
    fn a_recharge_modifier_multiplies_the_reload() {
        let frames = mint_sim::LogicFrames::from_frames(300);
        assert_eq!(recharge_time(frames, 0.5).get(), 150);
        assert_eq!(recharge_time(frames, 2.0).get(), 600);
        assert_eq!(recharge_time(frames, 0.0).get(), 300);
    }

    #[test]
    fn a_timed_list_given_twice_is_held_once_until_the_later_end() {
        let mut timed = TimedModifiers::default();
        let list = ModifierId::from_index(3);
        assert!(timed.give(list, mint_sim::Frame::new(100)));
        assert!(!timed.give(list, mint_sim::Frame::new(50)));
        assert_eq!(timed.0, vec![(list, mint_sim::Frame::new(100))]);
        assert!(!timed.give(list, mint_sim::Frame::new(150)));
        assert_eq!(timed.0[0].1, mint_sim::Frame::new(150));
        assert!(!timed.expire(mint_sim::Frame::new(149)));
        assert!(
            timed.expire(mint_sim::Frame::new(150)),
            "gone on its end frame"
        );
        assert!(timed.0.is_empty());
    }

    #[test]
    fn experience_scales_and_truncates_once() {
        assert_eq!(scaled(3, 1.0), 3);
        assert_eq!(scaled(3, 2.0), 6);
        assert_eq!(scaled(5, 1.5), 7, "7.5 truncates");
        assert_eq!(scaled(5, f32::NAN), 0);
        assert_eq!(scaled(5, -1.0), 0);
    }
}
