#![allow(missing_docs)]

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegionId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerId(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArmyId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuildingId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TemplateId(pub u8);

impl RegionId {
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

impl PlayerId {
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerKind {
    Human,
    Computer,
    Inert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Planning,
    MoveArmies,
    ResolveBattles,
    FakePhaseOne,
    PlanRetreats,
    RetreatArmies,
    Complete,
}

impl Phase {
    #[must_use]
    pub fn next(self) -> Option<Self> {
        Some(match self {
            Self::Planning => Self::MoveArmies,
            Self::MoveArmies => Self::ResolveBattles,
            Self::ResolveBattles => Self::FakePhaseOne,
            Self::FakePhaseOne => Self::PlanRetreats,
            Self::PlanRetreats => Self::RetreatArmies,
            Self::RetreatArmies => Self::Complete,
            Self::Complete => return None,
        })
    }

    #[must_use]
    pub fn ordinal(self) -> u8 {
        self as u8
    }
}

pub const BONUS_FIELDS: usize = 14;

pub mod bonus {
    pub const RESOURCE: usize = 0;
    pub const ARMY: usize = 1;
    pub const LEGENDARY: usize = 2;
    pub const ATTACK: usize = 3;
    pub const DEFENSE: usize = 4;
    pub const EXPERIENCE: usize = 5;
    pub const FERTILE_TERRITORY: usize = 6;
    pub const EXTRA_START_RESOURCES: usize = 7;
    pub const BUILDING_DISCOUNT: usize = 8;
    pub const FREE_INN_UNITS: usize = 9;
    pub const DISCOUNTED_SIEGE_UNITS: usize = 10;
    pub const DISCOUNTED_HERO_UNITS: usize = 11;
    pub const FREE_BUILDER: usize = 12;
    pub const DISCOUNTED_BARRACKS_UNITS: usize = 13;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bonus(pub [i32; BONUS_FIELDS]);

impl Bonus {
    #[must_use]
    pub fn get(&self, field: usize) -> i32 {
        self.0.get(field).copied().unwrap_or(0)
    }

    pub fn set(&mut self, field: usize, value: i32) {
        if let Some(slot) = self.0.get_mut(field) {
            *slot = value;
        }
    }

    pub fn add(&mut self, other: &Bonus) {
        for (slot, value) in self.0.iter_mut().zip(other.0) {
            *slot = slot.saturating_add(value);
        }
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn scaled(&self, share: f32) -> Bonus {
        let mut out = Bonus::default();
        for (slot, value) in out.0.iter_mut().zip(self.0) {
            *slot = (value as f32 * share) as i32;
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub id: u32,
    pub unit: UnitId,
    pub level: u32,
    pub count: u32,
    pub experience: u32,
    pub battles: u32,
    pub kills: u32,
    pub kills_battle: u32,
    pub upgraded_at: Option<(RegionId, u8)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FallenHero {
    pub unit: UnitId,
    pub level: u32,
    pub experience: u32,
}

impl Member {
    #[must_use]
    pub fn is_upgraded(&self) -> bool {
        self.upgraded_at.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Army {
    pub id: ArmyId,
    pub player: PlayerId,
    pub hero: Option<UnitId>,
    pub roster: Vec<Member>,
    pub region: RegionId,
    pub prev_region: Option<RegionId>,
    pub turns_here: u32,
    pub order: Option<RegionId>,
    pub destroy_queued: bool,
}

impl Army {
    #[must_use]
    pub fn is_hero_army(&self) -> bool {
        self.hero.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlotBuilding {
    pub building: BuildingId,
    pub turns_left: i32,
    pub under_construction: bool,
    pub queue: Vec<u8>,
    pub queue_turns: u32,
    pub upgrades_used: i32,
}

impl PlotBuilding {
    #[must_use]
    pub fn new(building: BuildingId, turns_left: i32, under_construction: bool) -> Self {
        Self {
            building,
            turns_left,
            under_construction,
            queue: Vec::new(),
            queue_turns: 0,
            upgrades_used: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionState {
    pub owner: Option<PlayerId>,
    pub enabled: bool,
    pub changed_turn: u32,
    pub present: Vec<ArmyId>,
    pub plots: Vec<Option<PlotBuilding>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    pub id: PlayerId,
    pub template: TemplateId,
    pub kind: PlayerKind,
    pub team: i32,
    pub base_region: Option<RegionId>,
    pub resources: i32,
    pub reserved_cp: i32,
    pub ready: bool,
    pub defeated: bool,
    pub bonus_a: Bonus,
    pub territories: Vec<u16>,
    pub previous_territories: Vec<u16>,
    pub fallen_heroes: Vec<FallenHero>,
    pub skill: crate::ai::Skill,
    pub skill_points: f32,
    pub rank_points: i32,
    pub sciences: Vec<String>,
    pub upgrades: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerCarry {
    pub skill_points: f32,
    pub rank_points: i32,
    pub sciences: Vec<String>,
    pub upgrades: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Battle {
    pub id: u32,
    pub region: RegionId,
    pub players: Vec<PlayerId>,
    pub choices: Vec<(PlayerId, u8)>,
    pub withdrawn: Vec<PlayerId>,
    pub snapshot: Vec<(ArmyId, Vec<Member>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub player: PlayerId,
    pub region: RegionId,
    pub flags: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub region: RegionId,
    pub players: Vec<PlayerId>,
    pub flags: u8,
    pub resolved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleMode {
    Auto,
    RealTime,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BattleResult {
    pub winner: Option<u8>,
    pub rosters: Vec<(ArmyId, Vec<Member>)>,
    pub fallen_heroes: Vec<(PlayerId, FallenHero)>,
    pub fallen_records: Vec<(PlayerId, FallenHero)>,
    pub players: Vec<(PlayerId, PlayerCarry)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    EndPhase,
    MoveArmy {
        army: ArmyId,
        dest: RegionId,
    },
    CancelMove {
        army: ArmyId,
    },
    ChooseRetreat {
        army: ArmyId,
        dest: RegionId,
    },
    Build {
        region: RegionId,
        plot: u8,
        building: BuildingId,
    },
    Demolish {
        region: RegionId,
        plot: u8,
    },
    DisbandMember {
        army: ArmyId,
        member: u32,
    },
    DisbandAll {
        army: ArmyId,
    },
    SwapArmyMembers {
        a: ArmyId,
        b: ArmyId,
        a_members: Vec<u32>,
        b_members: Vec<u32>,
    },
    QueueArmy {
        region: RegionId,
        plot: u8,
        entry: u8,
    },
    CancelQueuedArmy {
        region: RegionId,
        plot: u8,
        index: u8,
    },
    ChooseSharedCapture {
        region: RegionId,
        player: PlayerId,
    },
    ResolveBattle {
        region: RegionId,
        mode: BattleMode,
    },
    ApplyBattleResult {
        region: RegionId,
        result: BattleResult,
    },
    LinkMemberForUpgrade {
        army: ArmyId,
        member: u32,
    },
    UnlinkMemberForUpgrade {
        army: ArmyId,
        member: u32,
    },
    ChooseBattleMode {
        battle: u32,
        method: u8,
    },
    SwapArmiesInBattle {
        battle: u32,
        side: u8,
        slot: u8,
        a: u8,
        b: u8,
    },
    ForceRealTime {
        battle: u32,
    },
    Surrender {
        player: PlayerId,
    },
    AutoResolveStep {
        battle: u32,
        round: u32,
    },
    AutoResolveSkip {
        battle: u32,
    },
    LeaveSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    SessionEnded,
    UnknownPlayer,
    PlayerDefeated,
    WrongPhase,
    AwaitingBattle,
    NoSuchArmy,
    NotYourArmy,
    NoSuchRegion,
    RegionDisabled,
    NotFriendly,
    OutOfRange,
    NoSuchMember,
    MemberIsHero,
    NoSuchPlot,
    PlotTaken,
    NotYourRegion,
    NotAvailable,
    RestrictedBuilding,
    CannotAfford,
    NoOrderPending,
    NoSuchBattle,
    NotInBattle,
    NoResolver,
    NoDecision,
    NotChooser,
    InvalidSwap,
    NoQueue,
    NoSuchEntry,
    QueueFull,
    HeroFielded,
    HeroQueued,
    InvalidMethod,
    NotUpgradeable,
    AlreadyUpgraded,
    NotUpgraded,
    NotYourself,
    NoDialog,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    PhaseChanged {
        turn: u32,
        phase: Phase,
        hash: u64,
    },
    OrderGiven {
        army: ArmyId,
        dest: RegionId,
    },
    OrderCancelled {
        army: ArmyId,
    },
    HeadOnLost {
        army: ArmyId,
    },
    BuildingStarted {
        region: RegionId,
        plot: u8,
    },
    BuildingCompleted {
        region: RegionId,
        plot: u8,
    },
    Income {
        player: PlayerId,
        region: RegionId,
        amount: i32,
    },
    BattlePending {
        battle: u32,
        region: RegionId,
    },
    BattleSpec(crate::battle::BattleSpec),
    BattleFinished {
        region: RegionId,
        winner: Option<u8>,
    },
    MemberCharged {
        army: ArmyId,
        member: u32,
        price: i32,
    },
    GarrisonKept {
        army: ArmyId,
        charged: i32,
    },
    RegionCaptured {
        region: RegionId,
        player: PlayerId,
    },
    ArmyDestroyed {
        army: ArmyId,
    },
    HeroKilled {
        army: ArmyId,
    },
    RetreatChosen {
        army: ArmyId,
        dest: RegionId,
    },
    ArmiesMerged {
        region: RegionId,
        into: ArmyId,
    },
    MemberDisbanded {
        army: ArmyId,
        member: u32,
        refund: i32,
    },
    ArmyQueued {
        region: RegionId,
        plot: u8,
    },
    QueueCancelled {
        region: RegionId,
        plot: u8,
        refund: i32,
    },
    ArmySpawned {
        army: ArmyId,
        region: RegionId,
    },
    TerritoryGained {
        player: PlayerId,
        territory: u16,
    },
    TerritoryLost {
        player: PlayerId,
        territory: u16,
    },
    PlayerDefeated {
        player: PlayerId,
    },
    TeamWon {
        team: i32,
    },
    SessionEnded {
        winning_team: Option<i32>,
    },
    MemberLinked {
        army: ArmyId,
        member: u32,
        region: RegionId,
        plot: u8,
    },
    MemberUnlinked {
        army: ArmyId,
        member: u32,
    },
    PlayerLeft {
        player: PlayerId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutoResolveDialog {
    pub battle: u32,
    pub rounds: Vec<(PlayerId, u32)>,
    pub skipped: Vec<PlayerId>,
}
