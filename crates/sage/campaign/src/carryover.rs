use mint_sage_save::{AsciiString, CarriedSub, CarryoverUnit, UpgradeList};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedUnit<'a> {
    pub name: &'a str,
    pub hero: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CapturedObject {
    pub template: String,
    pub hero: bool,
    pub horde: bool,
    pub respawn_name: Option<String>,
    pub horde_name: Option<String>,
    pub experience: f32,
    pub level: i32,
    pub upgrades: Vec<String>,
    pub missions: i32,
    pub kills_total: i32,
    pub kills_mission: i32,
}

impl CapturedObject {
    #[must_use]
    pub fn record_name(&self) -> &str {
        let given = if self.hero {
            self.respawn_name.as_deref()
        } else if self.horde {
            self.horde_name.as_deref()
        } else {
            None
        };
        given
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.template)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScienceFact {
    pub name: String,
    pub grantable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeFact {
    pub name: String,
    pub persists: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerCapture {
    pub money: f32,
    pub sciences: Vec<ScienceFact>,
    pub upgrades: Vec<UpgradeFact>,
    pub cost_multiplier: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Carryover {
    available: Vec<CarryoverUnit>,
    used: Vec<CarryoverUnit>,
    hero_record_count: i32,
    money: f32,
    sciences: Vec<String>,
    upgrades: Vec<String>,
    cost_multiplier: f32,
}

fn same(record: &CarryoverUnit, name: &str) -> bool {
    record
        .template_name
        .to_string_lossy()
        .eq_ignore_ascii_case(name)
}

fn upgrade_list(names: &[String]) -> UpgradeList {
    UpgradeList(
        names
            .iter()
            .map(|name| AsciiString::from(name.as_str()))
            .collect(),
    )
}

impl Carryover {
    pub fn seed<'a>(units: impl IntoIterator<Item = SeedUnit<'a>>) -> Self {
        let mut carry = Self::default();
        for unit in units {
            carry.available.push(CarryoverUnit {
                template_name: AsciiString::from(unit.name),
                count: 1,
                ..CarryoverUnit::default()
            });
            if unit.hero {
                carry.hero_record_count = carry.hero_record_count.saturating_add(1);
            }
        }
        carry
    }

    pub fn set_hero_record_count(&mut self, count: i32) {
        self.hero_record_count = count;
    }

    #[must_use]
    pub fn available(&self) -> &[CarryoverUnit] {
        &self.available
    }

    #[must_use]
    pub fn used(&self) -> &[CarryoverUnit] {
        &self.used
    }

    #[must_use]
    pub fn hero_record_count(&self) -> i32 {
        self.hero_record_count
    }

    #[must_use]
    pub fn money(&self) -> f32 {
        self.money
    }

    #[must_use]
    pub fn sciences(&self) -> &[String] {
        &self.sciences
    }

    #[must_use]
    pub fn upgrades(&self) -> &[String] {
        &self.upgrades
    }

    #[must_use]
    pub fn cost_multiplier(&self) -> f32 {
        self.cost_multiplier
    }

    #[must_use]
    pub fn has_delayed(&self, name: &str) -> bool {
        self.available.iter().any(|record| same(record, name))
    }

    pub fn restore(&mut self, delayed: &[String], mut spawn: impl FnMut(&CarryoverUnit) -> u32) {
        self.used.clear();
        let waiting = std::mem::take(&mut self.available);
        for record in waiting {
            let held_back = delayed.iter().any(|name| same(&record, name));
            if held_back || spawn(&record) == 0 {
                self.available.push(record);
            } else {
                self.used.push(record);
            }
        }
    }

    pub fn create_delayed(
        &mut self,
        name: &str,
        spawn: impl FnOnce(&CarryoverUnit) -> bool,
    ) -> bool {
        let Some(at) = self.available.iter().position(|record| same(record, name)) else {
            return false;
        };
        let Some(record) = self.available.get(at) else {
            return false;
        };
        let one = CarryoverUnit {
            count: 1,
            ..record.clone()
        };
        if !spawn(&one) {
            return false;
        }
        self.used.push(one);
        if self
            .available
            .get(at)
            .is_some_and(|record| record.count <= 1)
        {
            self.available.remove(at);
        } else if let Some(record) = self.available.get_mut(at) {
            record.count = record.count.saturating_sub(1);
        }
        true
    }

    pub fn capture(
        &mut self,
        objects: &[CapturedObject],
        revivals: &[CapturedObject],
        player: Option<&PlayerCapture>,
    ) {
        for object in objects.iter().chain(revivals) {
            self.available.push(CarryoverUnit {
                template_name: AsciiString::from(object.record_name()),
                count: 1,
                experience: object.experience,
                carried: CarriedSub {
                    missions: object.missions.saturating_add(1),
                    kills_total: object.kills_total,
                    kills_mission: object.kills_mission,
                    flag: true,
                    ..CarriedSub::default()
                },
                upgrades: upgrade_list(&object.upgrades),
                level: object.level,
            });
        }
        self.used.clear();
        if let Some(player) = player {
            self.money = player.money;
            self.sciences = player
                .sciences
                .iter()
                .filter(|science| science.grantable)
                .map(|science| science.name.clone())
                .collect();
            self.upgrades = player
                .upgrades
                .iter()
                .filter(|upgrade| upgrade.persists)
                .map(|upgrade| upgrade.name.clone())
                .collect();
            self.cost_multiplier = player.cost_multiplier;
        }
    }

    pub(crate) fn fill(&self, state: &mut mint_sage_save::CampaignState) {
        state.available.clone_from(&self.available);
        state.used.clone_from(&self.used);
        state.hero_record_count = self.hero_record_count;
        state.money = self.money;
        state.sciences = self
            .sciences
            .iter()
            .map(|name| AsciiString::from(name.as_str()))
            .collect();
        state.player_upgrades = upgrade_list(&self.upgrades);
        state.cost_multiplier_b0 = self.cost_multiplier;
    }

    pub(crate) fn from_state(state: &mint_sage_save::CampaignState) -> Self {
        Self {
            available: state.available.clone(),
            used: state.used.clone(),
            hero_record_count: state.hero_record_count,
            money: state.money,
            sciences: state
                .sciences
                .iter()
                .map(AsciiString::to_string_lossy)
                .collect(),
            upgrades: state
                .player_upgrades
                .0
                .iter()
                .map(AsciiString::to_string_lossy)
                .collect(),
            cost_multiplier: state.cost_multiplier_b0,
        }
    }
}
