use std::collections::BTreeMap;

use mint_sim::{Component, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ScienceId(u16);

impl ScienceId {
    pub(crate) const fn get(self) -> u16 {
        self.0
    }

    pub(crate) const fn new(index: u16) -> Self {
        Self(index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Science {
    pub(crate) name: String,
    pub(crate) prerequisites: Vec<Vec<ScienceId>>,
    pub(crate) cost: u32,
    pub(crate) grantable: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ScienceReport {
    pub(crate) sciences: usize,
    pub(crate) purchasable: usize,
    pub(crate) alternatives: usize,
    pub(crate) alternatives_dropped: usize,
    pub(crate) unresolved_costs: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ScienceTable {
    sciences: Vec<Science>,
    by_name: BTreeMap<String, ScienceId>,
    pub(crate) report: ScienceReport,
}

impl ScienceTable {
    pub(crate) fn build(index: &mint_sage_ini::ScienceIndex, multiplayer: bool) -> Self {
        let mut table = Self::default();
        for (at, def) in index.iter().enumerate() {
            let Ok(id) = u16::try_from(at) else {
                break;
            };
            table
                .by_name
                .insert(def.name.to_ascii_lowercase(), ScienceId(id));
        }
        for def in index.iter() {
            let mut alternatives = Vec::new();
            table.report.alternatives_dropped = table
                .report
                .alternatives_dropped
                .saturating_add(def.empty_alternatives);
            for alternative in &def.prerequisites {
                table.report.alternatives = table.report.alternatives.saturating_add(1);
                let ids: Option<Vec<ScienceId>> =
                    alternative.iter().map(|name| table.id(name)).collect();
                match ids {
                    Some(mut ids) => {
                        ids.sort_unstable();
                        ids.dedup();
                        alternatives.push(ids);
                    }
                    None => {
                        table.report.alternatives_dropped =
                            table.report.alternatives_dropped.saturating_add(1);
                    }
                }
            }
            let cost = if multiplayer { def.cost_mp } else { def.cost };
            let cost = u32::try_from(cost.unwrap_or(0).max(0)).unwrap_or(0);
            table.report.unresolved_costs = table
                .report
                .unresolved_costs
                .saturating_add(def.unresolved_costs);
            if cost > 0 {
                table.report.purchasable = table.report.purchasable.saturating_add(1);
            }
            if alternatives.is_empty() && !def.prerequisites.is_empty() {
                alternatives.push(vec![ScienceId(u16::MAX)]);
            }
            table.sciences.push(Science {
                name: def.name.clone(),
                prerequisites: alternatives,
                cost,
                grantable: def.grantable.unwrap_or(true),
            });
        }
        table.report.sciences = table.sciences.len();
        table
    }

    pub(crate) fn id(&self, name: &str) -> Option<ScienceId> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }

    pub(crate) fn get(&self, id: ScienceId) -> Option<&Science> {
        self.sciences.get(usize::from(id.0))
    }

    pub(crate) fn len(&self) -> usize {
        self.sciences.len()
    }

    pub(crate) fn describe(&self) -> String {
        let r = self.report;
        format!(
            "sciences: {} read, {} purchasable, {} prerequisite alternative(s) ({} dropped), \
             {} cost field(s) unresolved",
            r.sciences, r.purchasable, r.alternatives, r.alternatives_dropped, r.unresolved_costs
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Ranks {
    needed: Vec<u32>,
    granted: Vec<u32>,
    sciences: Vec<Vec<ScienceId>>,
}

impl Ranks {
    pub(crate) fn build(
        ranks: &mint_sage_ini::RankTable,
        sciences: &ScienceTable,
        campaign: bool,
        side: Option<&str>,
    ) -> Self {
        let mut out = Self::default();
        let mut last = 0u32;
        let column = if campaign { Some("Campaign") } else { side };
        for def in ranks.iter() {
            let needed = column
                .and_then(|column| def.skill_points(column))
                .or_else(|| def.skill_points("Default"))
                .or_else(|| def.skill_points(""))
                .unwrap_or(last)
                .max(last);
            last = needed;
            out.needed.push(needed);
            out.granted.push(def.purchase_points_granted);
            out.sciences.push(
                def.sciences_granted
                    .iter()
                    .filter_map(|name| sciences.id(name))
                    .collect(),
            );
        }
        out
    }

    #[cfg(test)]
    pub(crate) fn from_columns(needed: Vec<u32>, granted: Vec<u32>) -> Self {
        let sciences = vec![Vec::new(); needed.len()];
        Self {
            needed,
            granted,
            sciences,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.needed.len()
    }

    fn index(rank: u16) -> Option<usize> {
        usize::from(rank).checked_sub(1)
    }

    pub(crate) fn needed(&self, rank: u16) -> Option<u32> {
        self.needed.get(Self::index(rank)?).copied()
    }

    pub(crate) fn granted_through(&self, rank: u16) -> u32 {
        (1..=rank).fold(0u32, |sum, r| sum.saturating_add(self.granted(r)))
    }

    fn granted(&self, rank: u16) -> u32 {
        Self::index(rank)
            .and_then(|at| self.granted.get(at))
            .copied()
            .unwrap_or(0)
    }

    fn sciences(&self, rank: u16) -> &[ScienceId] {
        Self::index(rank)
            .and_then(|at| self.sciences.get(at))
            .map_or(&[], Vec::as_slice)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PlayerScience {
    pub(crate) skill_points: u32,
    pub(crate) rank: u16,
    pub(crate) purchase_points: u32,
    pub(crate) sciences: Vec<ScienceId>,
    pub(crate) rank_cap: u16,
    pub(crate) book: Option<mint_sim::Entity>,
    pub(crate) unavailable: Vec<ScienceId>,
}

impl Component for PlayerScience {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.skill_points);
        hasher.write_u16(self.rank);
        hasher.write_u32(self.purchase_points);
        hasher.write_u16(self.rank_cap);
        hasher.write_bool(self.book.is_some());
        if let Some(book) = self.book {
            book.hash_into(hasher);
        }
        hasher.write_usize(self.sciences.len());
        for science in &self.sciences {
            hasher.write_u16(science.0);
        }
        if !self.unavailable.is_empty() {
            hasher.write_usize(self.unavailable.len());
            for science in &self.unavailable {
                hasher.write_u16(science.0);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    Unknown,
    Held,
    Unavailable,
    Prerequisites,
    NotPurchasable,
    Points { cost: u32, held: u32 },
}

impl Refusal {
    pub(crate) fn describe(self) -> String {
        match self {
            Self::Unknown => String::from("no such science"),
            Self::Held => String::from("already held"),
            Self::Unavailable => String::from("disabled or hidden by a script"),
            Self::Prerequisites => String::from("no prerequisite branch is held"),
            Self::NotPurchasable => String::from("not purchasable (cost 0)"),
            Self::Points { cost, held } => format!("costs {cost} power point(s), {held} held"),
        }
    }
}

impl PlayerScience {
    pub(crate) fn start(
        intrinsic: &[ScienceId],
        ranks: &Ranks,
        cap: Option<u32>,
        extra_purchase_points: u32,
    ) -> Self {
        let table_top = u16::try_from(ranks.len()).unwrap_or(u16::MAX).max(1);
        let rank_cap = cap
            .and_then(|cap| u16::try_from(cap).ok())
            .map_or(table_top, |cap| cap.clamp(1, table_top));
        let mut state = Self {
            skill_points: 0,
            rank: 1,
            purchase_points: ranks.granted(1).saturating_add(extra_purchase_points),
            sciences: Vec::new(),
            rank_cap,
            book: None,
            unavailable: Vec::new(),
        };
        for science in intrinsic.iter().chain(ranks.sciences(1)) {
            state.add(*science);
        }
        state
    }

    pub(crate) fn holds(&self, science: ScienceId) -> bool {
        self.sciences.binary_search(&science).is_ok()
    }

    pub(crate) fn set_unavailable(&mut self, science: ScienceId, unavailable: bool) {
        match (self.unavailable.binary_search(&science), unavailable) {
            (Err(at), true) => self.unavailable.insert(at, science),
            (Ok(at), false) => {
                self.unavailable.remove(at);
            }
            _ => {}
        }
    }

    pub(crate) fn add(&mut self, science: ScienceId) -> bool {
        match self.sciences.binary_search(&science) {
            Ok(_) => false,
            Err(at) => {
                self.sciences.insert(at, science);
                true
            }
        }
    }

    pub(crate) fn at_rank_cap(&self) -> bool {
        self.rank >= self.rank_cap
    }

    pub(crate) fn next_rank_at(&self, ranks: &Ranks) -> Option<u32> {
        if self.rank >= self.rank_cap {
            return None;
        }
        ranks.needed(self.rank.saturating_add(1))
    }

    pub(crate) fn add_skill_points(&mut self, delta: u32, ranks: &Ranks) -> Vec<ScienceId> {
        if delta == 0 {
            return Vec::new();
        }
        let cap_points = ranks.needed(self.rank_cap).unwrap_or(u32::MAX);
        self.skill_points = self.skill_points.saturating_add(delta).min(cap_points);
        let mut gained = Vec::new();
        while let Some(next) = self.next_rank_at(ranks) {
            if self.skill_points < next {
                break;
            }
            gained.extend(self.set_rank(self.rank.saturating_add(1), ranks));
        }
        gained
    }

    pub(crate) fn set_rank_level(
        &mut self,
        level: u16,
        ranks: &Ranks,
        extra_purchase_points: u32,
    ) -> Vec<ScienceId> {
        let top = u16::try_from(ranks.len()).unwrap_or(u16::MAX).max(1);
        let level = level.clamp(1, top).min(self.rank_cap.max(1));
        if level == self.rank {
            return Vec::new();
        }
        if level < self.rank {
            self.rank = 1;
            self.skill_points = 0;
            self.purchase_points = ranks.granted(1).saturating_add(extra_purchase_points);
        }
        self.set_rank(level, ranks)
    }

    fn set_rank(&mut self, rank: u16, ranks: &Ranks) -> Vec<ScienceId> {
        let rank = rank.min(self.rank_cap);
        let mut gained = Vec::new();
        while self.rank < rank {
            self.rank = self.rank.saturating_add(1);
            self.purchase_points = self
                .purchase_points
                .saturating_add(ranks.granted(self.rank));
            if let Some(needed) = ranks.needed(self.rank) {
                self.skill_points = self.skill_points.max(needed);
            }
            for science in ranks.sciences(self.rank) {
                if self.add(*science) {
                    gained.push(*science);
                }
            }
        }
        gained
    }

    pub(crate) fn check_purchase(
        &self,
        science: ScienceId,
        table: &ScienceTable,
    ) -> Result<u32, Refusal> {
        let def = table.get(science).ok_or(Refusal::Unknown)?;
        if self.holds(science) {
            return Err(Refusal::Held);
        }
        if self.unavailable.binary_search(&science).is_ok() {
            return Err(Refusal::Unavailable);
        }
        if !self.prerequisites_met(def) {
            return Err(Refusal::Prerequisites);
        }
        if def.cost == 0 {
            return Err(Refusal::NotPurchasable);
        }
        if def.cost > self.purchase_points {
            return Err(Refusal::Points {
                cost: def.cost,
                held: self.purchase_points,
            });
        }
        Ok(def.cost)
    }

    pub(crate) fn prerequisites_met(&self, def: &Science) -> bool {
        def.prerequisites.is_empty()
            || def
                .prerequisites
                .iter()
                .any(|alternative| alternative.iter().all(|id| self.holds(*id)))
    }

    pub(crate) fn purchase(&mut self, science: ScienceId, cost: u32) {
        self.purchase_points = self.purchase_points.saturating_sub(cost);
        self.add(science);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SpellPlan {
    pub(crate) spell_book: Option<crate::unit::Kind>,
    pub(crate) store: Vec<(String, ScienceId)>,
    pub(crate) intrinsic: Vec<ScienceId>,
    pub(crate) rank_cap: Option<u32>,
    pub(crate) ranks: Option<std::sync::Arc<Ranks>>,
    pub(crate) intrinsic_purchase_points: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SpellState {
    Owned,
    Available,
    CantAfford,
    Locked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpellEntry {
    pub(crate) science: String,
    pub(crate) id: u16,
    pub(crate) button: String,
    pub(crate) cost: u32,
    pub(crate) state: SpellState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct SpellBook {
    pub(crate) points: u32,
    pub(crate) rank: u16,
    pub(crate) skill_points: u32,
    pub(crate) next_rank_at: Option<u32>,
    pub(crate) entries: Vec<SpellEntry>,
    pub(crate) held: Vec<String>,
}

pub(crate) fn spell_state(
    state: &PlayerScience,
    table: &ScienceTable,
    science: ScienceId,
) -> SpellState {
    match state.check_purchase(science, table) {
        Ok(_) => SpellState::Available,
        Err(Refusal::Held) => SpellState::Owned,
        Err(Refusal::Points { .. }) => SpellState::CantAfford,
        Err(
            Refusal::Unknown
            | Refusal::Unavailable
            | Refusal::Prerequisites
            | Refusal::NotPurchasable,
        ) => SpellState::Locked,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, Document, MacroTable, ScienceIndex};

    const SCIENCES: &str = "\
Science SCIENCE_MEN\n  PrerequisiteSciences = None\n  SciencePurchasePointCost = 0\n  IsGrantable = No\nEnd\n\
Science SCIENCE_GOOD\n  PrerequisiteSciences = None\n  SciencePurchasePointCost = 0\nEnd\n\
Science SCIENCE_Heal\n  PrerequisiteSciences = SCIENCE_GOOD OR SCIENCE_MEN\n  \
    SciencePurchasePointCost = 5\n  SciencePurchasePointCostMP = 5\nEnd\n\
Science SCIENCE_Rally\n  PrerequisiteSciences = SCIENCE_MEN\n  SciencePurchasePointCostMP = 5\nEnd\n\
Science SCIENCE_Summon\n  PrerequisiteSciences = SCIENCE_GOOD SCIENCE_Heal OR SCIENCE_MEN SCIENCE_Rally\n  \
    SciencePurchasePointCost = 3\n  SciencePurchasePointCostMP = 10\nEnd\n\
Science SCIENCE_Old\n  PrerequisiteSciences = SCIENCE_MEN\n  SciencePurchasePointCost = 7\nEnd\n\
Science SCIENCE_Broken\n  PrerequisiteSciences = SCIENCE_NOWHERE\n  SciencePurchasePointCostMP = 1\nEnd\n";

    fn table_for(multiplayer: bool) -> ScienceTable {
        let doc = Document::parse("science.ini", SCIENCES, &BlockTable::corpus_path()).unwrap();
        let mut index = ScienceIndex::new();
        index.insert_document(&doc, &MacroTable::new());
        ScienceTable::build(&index, multiplayer)
    }

    fn table() -> ScienceTable {
        table_for(true)
    }

    fn id(table: &ScienceTable, name: &str) -> ScienceId {
        table.id(name).unwrap()
    }

    fn ranks() -> Ranks {
        Ranks::from_columns(vec![0, 60, 120, 180, 240, 300], vec![5, 1, 1, 1, 1, 1])
    }

    fn men(table: &ScienceTable) -> PlayerScience {
        PlayerScience::start(&[id(table, "SCIENCE_MEN")], &ranks(), Some(150), 0)
    }

    #[test]
    fn ids_are_alphabetical_and_the_price_is_the_modes_own_column_with_no_fallback() {
        let t = table();
        assert!(id(&t, "science_good") < id(&t, "SCIENCE_Heal"));
        let cost = |t: &ScienceTable, name: &str| t.get(id(t, name)).unwrap().cost;
        assert_eq!(cost(&t, "SCIENCE_Summon"), 10);
        assert_eq!(cost(&t, "SCIENCE_Old"), 0, "no MP cost: not purchasable");
        let campaign = table_for(false);
        assert_eq!(cost(&campaign, "SCIENCE_Summon"), 3);
        assert_eq!(cost(&campaign, "SCIENCE_Old"), 7);
        assert_eq!(cost(&campaign, "SCIENCE_Rally"), 0, "no campaign cost");
        assert_eq!(t.report.alternatives_dropped, 1, "Broken's only branch");
    }

    #[test]
    fn rank_thresholds_read_the_campaign_column_then_the_sides_then_default() {
        let doc = Document::parse(
            "rank.ini",
            "Rank 1\n  SkillPointsNeededDefault = 0\nEnd\n\
             Rank 2\n  SkillPointsNeededDefault = 60\n  SkillPointsNeededCampaign = 100\n  \
               SkillPointsNeededElves = 80\nEnd\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let ranks = mint_sage_ini::RankTable::build(&[doc], &MacroTable::new());
        let t = table();
        let needed = |campaign, side| Ranks::build(&ranks, &t, campaign, side).needed(2);
        assert_eq!(needed(true, Some("Elves")), Some(100));
        assert_eq!(needed(false, Some("Elves")), Some(80));
        assert_eq!(
            needed(false, Some("Men")),
            Some(60),
            "unset column: Default"
        );
        assert_eq!(needed(false, None), Some(60));
    }

    #[test]
    fn at_rank_cap_compares_against_the_cap_not_the_table() {
        let t = table();
        let mut state = PlayerScience::start(&[id(&t, "SCIENCE_MEN")], &ranks(), Some(2), 0);
        state.rank = 1;
        assert!(!state.at_rank_cap());
        state.rank = 2;
        assert!(state.at_rank_cap());
    }

    #[test]
    fn a_player_starts_at_rank_one_with_its_grant_and_its_roots() {
        let t = table();
        let state = men(&t);
        assert_eq!(
            (state.rank, state.skill_points, state.purchase_points),
            (1, 0, 5)
        );
        assert!(state.holds(id(&t, "SCIENCE_MEN")));
        assert!(!state.holds(id(&t, "SCIENCE_GOOD")));
        assert_eq!(state.rank_cap, 6, "150 is bounded by the six-rank table");
    }

    #[test]
    fn intrinsic_purchase_points_add_to_rank_ones_grant() {
        let t = table();
        let state = PlayerScience::start(&[id(&t, "SCIENCE_MEN")], &ranks(), Some(150), 3);
        assert_eq!(state.purchase_points, 8, "5 granted + 3 intrinsic");
    }

    #[test]
    fn setting_the_rank_level_grants_up_and_resets_down() {
        let t = table();
        let mut state = men(&t);
        state.set_rank_level(3, &ranks(), 0);
        assert_eq!(
            (state.rank, state.skill_points, state.purchase_points),
            (3, 120, 7)
        );
        state.set_rank_level(2, &ranks(), 0);
        assert_eq!(
            (state.rank, state.skill_points, state.purchase_points),
            (2, 60, 6),
            "reset to rank 1's grant, then rank 2's"
        );
        assert!(
            state.holds(id(&t, "SCIENCE_MEN")),
            "sciences survive a reset"
        );
        let mut capped = PlayerScience::start(&[], &ranks(), Some(4), 2);
        capped.set_rank_level(40, &ranks(), 2);
        assert_eq!((capped.rank, capped.purchase_points), (4, 10));
        capped.set_rank_level(0, &ranks(), 2);
        assert_eq!((capped.rank, capped.purchase_points), (1, 7));
    }

    #[test]
    fn skill_points_cross_several_ranks_at_once_and_stop_at_the_cap() {
        let t = table();
        let mut state = men(&t);
        state.add_skill_points(59, &ranks());
        assert_eq!((state.rank, state.purchase_points), (1, 5));
        state.add_skill_points(1, &ranks());
        assert_eq!((state.rank, state.purchase_points), (2, 6));
        state.add_skill_points(125, &ranks());
        assert_eq!(
            (state.rank, state.skill_points, state.purchase_points),
            (4, 185, 8)
        );
        state.add_skill_points(1_000_000, &ranks());
        assert_eq!(state.rank, 6);
        assert_eq!(
            state.skill_points, 300,
            "capped at the cap rank's threshold"
        );
        assert_eq!(state.next_rank_at(&ranks()), None);
    }

    #[test]
    fn a_lower_cap_stops_the_ranks_early() {
        let t = table();
        let mut state = PlayerScience::start(&[id(&t, "SCIENCE_MEN")], &ranks(), Some(3), 0);
        state.add_skill_points(10_000, &ranks());
        assert_eq!((state.rank, state.skill_points), (3, 120));
    }

    #[test]
    fn one_satisfied_prerequisite_branch_is_enough() {
        let t = table();
        let mut state = men(&t);
        state.purchase_points = 100;
        let summon = id(&t, "SCIENCE_Summon");
        assert_eq!(
            state.check_purchase(summon, &t),
            Err(Refusal::Prerequisites)
        );
        let rally = id(&t, "SCIENCE_Rally");
        let cost = state.check_purchase(rally, &t).unwrap();
        state.purchase(rally, cost);
        assert_eq!(state.check_purchase(summon, &t), Ok(10));
    }

    #[test]
    fn a_partly_held_branch_does_not_count() {
        let t = table();
        let mut state = men(&t);
        state.purchase_points = 100;
        let heal = id(&t, "SCIENCE_Heal");
        state.purchase(heal, 5);
        assert_eq!(
            state.check_purchase(id(&t, "SCIENCE_Summon"), &t),
            Err(Refusal::Prerequisites)
        );
    }

    #[test]
    fn every_refusal_in_the_sources_order() {
        let t = table();
        let mut state = men(&t);
        assert_eq!(
            state.check_purchase(ScienceId(999), &t),
            Err(Refusal::Unknown)
        );
        assert_eq!(
            state.check_purchase(id(&t, "SCIENCE_MEN"), &t),
            Err(Refusal::Held)
        );
        assert_eq!(
            state.check_purchase(id(&t, "SCIENCE_Broken"), &t),
            Err(Refusal::Prerequisites),
            "a branch naming nothing never opens"
        );
        state.purchase_points = 4;
        assert_eq!(
            state.check_purchase(id(&t, "SCIENCE_Heal"), &t),
            Err(Refusal::Points { cost: 5, held: 4 })
        );
        state.purchase_points = 5;
        let heal = id(&t, "SCIENCE_Heal");
        state.purchase(heal, state.check_purchase(heal, &t).unwrap());
        assert_eq!(state.purchase_points, 0);
        assert!(state.holds(heal));
        assert_eq!(state.check_purchase(heal, &t), Err(Refusal::Held));
        assert_eq!(
            spell_state(&state, &t, id(&t, "SCIENCE_Rally")),
            SpellState::CantAfford
        );
        assert_eq!(spell_state(&state, &t, heal), SpellState::Owned);
    }

    #[test]
    fn a_zero_cost_science_is_never_purchasable() {
        let t = table();
        let mut state = PlayerScience::start(&[], &ranks(), None, 0);
        state.purchase_points = 100;
        assert_eq!(
            state.check_purchase(id(&t, "SCIENCE_GOOD"), &t),
            Err(Refusal::NotPurchasable)
        );
    }

    #[test]
    fn ranks_grant_their_sciences_once() {
        let t = table();
        let rally = id(&t, "SCIENCE_Rally");
        let mut ranks = ranks();
        ranks.sciences[1] = vec![rally];
        let mut state = PlayerScience::start(&[], &ranks, None, 0);
        assert_eq!(state.add_skill_points(60, &ranks), vec![rally]);
        assert!(state.holds(rally));
        assert!(state.add_skill_points(60, &ranks).is_empty());
    }

    #[test]
    fn the_hash_sees_every_field() {
        let hash = |state: &PlayerScience| {
            let mut hasher = StateHasher::new();
            state.hash_into(&mut hasher);
            hasher.finish()
        };
        let t = table();
        let base = men(&t);
        let mut other = base.clone();
        other.purchase_points += 1;
        assert_ne!(hash(&base), hash(&other));
        let mut other = base.clone();
        other.add(id(&t, "SCIENCE_Heal"));
        assert_ne!(hash(&base), hash(&other));
    }
}
