#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub template: String,
    pub quantity: i32,
    pub experience: f32,
    pub level: i32,
    pub upgrades: Vec<String>,
    pub kills: u32,
    pub object: Option<u32>,
}

impl Member {
    #[must_use]
    pub fn new(template: &str, quantity: i32) -> Self {
        Self {
            template: template.to_owned(),
            quantity,
            experience: 0.0,
            level: 1,
            upgrades: Vec::new(),
            kills: 0,
            object: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Army {
    pub id: u32,
    pub named: bool,
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RankStep {
    pub threshold: f32,
    pub purchase_points_granted: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkillTracker {
    pub points: f32,
    pub purchase_points: i32,
    pub next_rank: usize,
    pub ranks: Vec<RankStep>,
    pub gain_scale: f32,
    pub base_rank: usize,
    pub scaling: Vec<f32>,
}

impl SkillTracker {
    pub fn add_purchase_points(&mut self, v: i32) {
        self.purchase_points = self.purchase_points.saturating_add(v).max(0);
    }

    pub(crate) fn add(&mut self, v: f32, scaled: bool) {
        let mut v = v * self.gain_scale;
        if scaled {
            if let (Some(depth), Some(last)) = (
                self.next_rank.checked_sub(self.base_rank),
                self.scaling.len().checked_sub(1),
            ) {
                v *= self.scaling.get(depth.min(last)).copied().unwrap_or(1.0);
            }
        }
        if v == 0.0 {
            return;
        }
        let cap = self.ranks.last().map_or(f32::MAX, |r| r.threshold);
        self.points = (self.points + v).min(cap);
        while let Some(step) = self.ranks.get(self.next_rank) {
            if self.points.floor() < step.threshold {
                break;
            }
            self.purchase_points = self
                .purchase_points
                .saturating_add(step.purchase_points_granted);
            self.next_rank = self.next_rank.saturating_add(1);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Participant {
    pub side: usize,
    pub weapon_multiplier: f32,
    pub armor_multiplier: f32,
    pub experience_multiplier: f32,
    pub skill: Option<SkillTracker>,
    pub legendary_bonus: i32,
    pub science_bonus: Vec<crate::tables::ScienceStep>,
    pub initial_units: Vec<Member>,
    pub armies: Vec<Army>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Options {
    pub tutorial: bool,
    pub round_cap: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Survivor {
    pub member: Member,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArmyResult {
    pub id: u32,
    pub touched: bool,
    pub survivors: Vec<Survivor>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParticipantResult {
    pub armies: Vec<ArmyResult>,
    pub skill_points: Option<f32>,
    pub purchase_points_delta: Option<i32>,
    pub lost: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kill {
    pub round: u32,
    pub attacker: usize,
    pub victim: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FallenHero {
    pub participant: usize,
    pub army: u32,
    pub member: Member,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundLog {
    pub round: u32,
    pub units: [usize; 2],
    pub hit_points: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticipantRound {
    pub units: usize,
    pub hit_points: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arrival {
    pub army: u32,
    pub round: u32,
    pub hit_points: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub winner: usize,
    pub rounds: u32,
    pub participants: Vec<ParticipantResult>,
    pub destroyed_objects: Vec<u32>,
    pub fallen_heroes: Vec<FallenHero>,
    pub kills: Vec<Kill>,
    pub log: Vec<RoundLog>,
    pub opening: RoundLog,
    pub fields: Vec<Vec<ParticipantRound>>,
    pub arrivals: Vec<Vec<Arrival>>,
    pub draws: u32,
}
