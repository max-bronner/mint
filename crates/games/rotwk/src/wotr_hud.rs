use mint_sage_livingworld::{
    ArmyId, BuildingId, Command, LivingWorldState, PlayerId, RegionId, Rejected,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) region: Option<RegionId>,
    pub(crate) army: Option<ArmyId>,
    pub(crate) plot: Option<u8>,
    pub(crate) details: bool,
    pub(crate) members: Vec<u32>,
    pub(crate) leader_picked: bool,
    pub(crate) battle: Option<u32>,
    pub(crate) page: (u8, u8),
    pub(crate) award_row: Option<usize>,
}

impl Selection {
    pub(crate) fn pick_army_on_map(&mut self, army: Option<ArmyId>) -> bool {
        if army.is_some() && army == self.army {
            return false;
        }
        self.army = army;
        self.details = army.is_some();
        self.members.clear();
        self.leader_picked = false;
        self.plot = None;
        true
    }

    pub(crate) fn pick_every_member(&mut self, state: &LivingWorldState) {
        let Some(army) = self.army.and_then(|id| state.armies.get(&id)) else {
            return;
        };
        for m in &army.roster {
            if Some(m.unit) != army.hero && !self.members.contains(&m.id) {
                self.members.push(m.id);
            }
        }
        self.leader_picked = true;
    }

    pub(crate) fn leader_clicked(&mut self, state: &LivingWorldState) {
        if !self.leader_picked {
            self.pick_every_member(state);
            return;
        }
        let hero = self
            .army
            .and_then(|id| state.armies.get(&id))
            .and_then(|army| army.hero.map(|hero| (army, hero)));
        self.members.retain(|id| {
            hero.is_some_and(|(army, hero)| {
                army.roster.iter().any(|m| m.id == *id && m.unit == hero)
            })
        });
        self.leader_picked = false;
    }

    pub(crate) fn background_clicked(&mut self) {
        self.members.clear();
    }

    pub(crate) fn member_clicked(&mut self, member: u32, ctrl: bool) {
        let picked = self.members.contains(&member);
        if ctrl {
            if picked {
                self.members.retain(|id| *id != member);
            } else {
                self.members.push(member);
            }
        } else if picked && self.members.len() == 1 {
            self.members.clear();
        } else {
            self.members = vec![member];
        }
    }

    pub(crate) fn pick_army_in_tray(&mut self, army: Option<ArmyId>) {
        self.army = army;
        self.details = false;
        self.members.clear();
        self.plot = None;
    }

    pub(crate) fn pick_plot(&mut self, plot: Option<u8>) {
        self.plot = plot;
        self.army = None;
        self.details = false;
        self.members.clear();
    }
}

pub(crate) fn garrison_target(
    state: &LivingWorldState,
    army: ArmyId,
    region: RegionId,
) -> Option<ArmyId> {
    let owner = state.armies.get(&army)?.player;
    let garrison = state
        .armies_of(owner)
        .find(|a| !a.is_hero_army() && a.region == region)?;
    (garrison.id != army).then_some(garrison.id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClipEvent {
    pub(crate) clip: String,
    pub(crate) suffix: String,
    pub(crate) args: String,
}

impl ClipEvent {
    pub(crate) fn parse(qualified: &str, args: &str) -> Option<Self> {
        let at = qualified.rfind("_On")?;
        Some(Self {
            clip: qualified.get(..at)?.to_owned(),
            suffix: qualified.get(at..)?.to_owned(),
            args: args.to_owned(),
        })
    }

    pub(crate) fn name(&self) -> String {
        self.clip
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
    }

    pub(crate) fn index(&self) -> Option<usize> {
        let name = self.name();
        let digits = name.len() - name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        name.get(name.len() - digits..)?.parse().ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Intent {
    EndTurn,
    CancelArmyMove,
    DestroyBuilding,
    DisbandMembers,
    UpgradeMembers,
    BuildOnPlot(BuildingId),
    QueueArmy { entry: u8 },
    CancelQueued { index: u8 },
    BattleMode(u8),
    SwapInBattle { first: u8 },
    PickAwardRow(usize),
    PickArmy(usize),
    PickPlot(usize),
    AwardRegion,
    DialogHitDone,
    DialogChain,
    DialogReinforceDone,
    Opened,
    Closed,
    DialogSkip,
    ToggleDetails,
    LeaderIcon,
    IconListBackground,
    PickMember(u32),
    Presentation,
}

pub(crate) fn intent_of(event: &ClipEvent) -> Option<Intent> {
    let name = event.name();
    let suffix = event.suffix.as_str();
    Some(match suffix {
        "_OnAutoResolveButtonClicked" => Intent::BattleMode(1),
        "_OnRetreatButtonClicked" => Intent::BattleMode(2),
        "_OnRealTimeButtonClicked" => Intent::BattleMode(3),
        "_OnSwapButtonClicked" => Intent::SwapInBattle {
            first: event.args.trim().parse().ok().filter(|n| *n <= 2)?,
        },
        "_OnPlayerClicked" => Intent::PickAwardRow(event.index().unwrap_or(0)),
        "_OnIconSlotClicked" => {
            let path = event.clip.to_ascii_lowercase();
            match event.index() {
                Some(slot) if path.contains(".armies.") => Intent::PickArmy(slot),
                Some(slot) if path.contains(".structures.") => Intent::PickPlot(slot),
                _ => Intent::Presentation,
            }
        }
        "_OnSelectButtonClicked" => Intent::AwardRegion,
        "_OnSkipButtonClicked" => Intent::DialogSkip,
        "_OnHitAnimDone" => Intent::DialogHitDone,
        "_OnChainHitAnim" => Intent::DialogChain,
        "_OnReinforceAnimDone" => Intent::DialogReinforceDone,
        "_OnOpen" => Intent::Opened,
        "_OnClosed" => Intent::Closed,
        "_OnInProgressIconSlotClicked" => Intent::CancelQueued { index: 0 },
        "_OnQueuedIconSlotClicked" => Intent::CancelQueued {
            index: u8::try_from(event.index().unwrap_or(0).checked_add(1)?).ok()?,
        },
        "_OnLeaderIconClicked" => Intent::LeaderIcon,
        "_OnIconListBackgroundClicked" => Intent::IconListBackground,
        "_OnClicked" if event.clip.to_ascii_lowercase().contains(".slots.") => {
            let mut parts = event.clip.split('.');
            parts.find(|p| p.eq_ignore_ascii_case("slots"))?;
            Intent::PickMember(parts.next()?.parse().ok()?)
        }
        "_OnClicked" => match name.as_str() {
            "endturnbutton" => Intent::EndTurn,
            "cancelarmymovebutton" | "cancelarmymemberbutton" | "cancelarmymembermovebutton" => {
                Intent::CancelArmyMove
            }
            "destroybuildingbutton" | "cancelbuildingconstructionbutton" => Intent::DestroyBuilding,
            "toggleselectiondetailsbutton" => Intent::ToggleDetails,
            "disbandarmymemberbutton" => Intent::DisbandMembers,
            "upgradeunitbutton" => Intent::UpgradeMembers,
            _ => return None,
        },
        s if [
            "RollOver",
            "RollOut",
            "Loaded",
            "Open",
            "Closed",
            "Initialized",
            "TabClicked",
        ]
        .iter()
        .any(|w| s.contains(w)) =>
        {
            Intent::Presentation
        }
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unavailable {
    Army,
    Region,
    Plot,
    Member,
    Battle,
    AwardRow,
    Decision,
}

fn need<T>(value: Option<T>, why: Unavailable) -> Result<T, Unavailable> {
    value.ok_or(why)
}

pub(crate) fn commands(
    state: &LivingWorldState,
    sel: &Selection,
    intent: &Intent,
) -> Result<Vec<Command>, Unavailable> {
    let region = || need(sel.region, Unavailable::Region);
    let plot = || need(sel.plot, Unavailable::Plot);
    Ok(match intent {
        Intent::EndTurn => vec![Command::EndPhase],
        Intent::CancelArmyMove => vec![Command::CancelMove {
            army: need(sel.army, Unavailable::Army)?,
        }],
        Intent::DestroyBuilding => vec![Command::Demolish {
            region: region()?,
            plot: plot()?,
        }],
        Intent::DisbandMembers => {
            let army = need(sel.army, Unavailable::Army)?;
            if sel.members.is_empty() {
                return Err(Unavailable::Member);
            }
            sel.members
                .iter()
                .map(|&member| Command::DisbandMember { army, member })
                .collect()
        }
        Intent::UpgradeMembers => {
            let army = need(sel.army, Unavailable::Army)?;
            if sel.members.is_empty() {
                return Err(Unavailable::Member);
            }
            let held = |member: u32| {
                state
                    .armies
                    .get(&army)
                    .and_then(|a| a.roster.iter().find(|m| m.id == member))
                    .is_some_and(|m| m.is_upgraded())
            };
            let fresh: Vec<u32> = sel.members.iter().copied().filter(|m| !held(*m)).collect();
            if fresh.is_empty() {
                sel.members
                    .iter()
                    .map(|&member| Command::UnlinkMemberForUpgrade { army, member })
                    .collect()
            } else {
                fresh
                    .into_iter()
                    .map(|member| Command::LinkMemberForUpgrade { army, member })
                    .collect()
            }
        }
        Intent::BuildOnPlot(building) => vec![Command::Build {
            region: region()?,
            plot: plot()?,
            building: *building,
        }],
        Intent::QueueArmy { entry } => vec![Command::QueueArmy {
            region: region()?,
            plot: plot()?,
            entry: *entry,
        }],
        Intent::CancelQueued { index } => vec![Command::CancelQueuedArmy {
            region: region()?,
            plot: plot()?,
            index: *index,
        }],
        Intent::BattleMode(method) => vec![Command::ChooseBattleMode {
            battle: need(sel.battle, Unavailable::Battle)?,
            method: *method,
        }],
        Intent::SwapInBattle { first } => {
            let battle = need(sel.battle, Unavailable::Battle)?;
            let (a, b) = swapped_positions(state, battle, sel.page, *first);
            vec![Command::SwapArmiesInBattle {
                battle,
                side: sel.page.0,
                slot: sel.page.1,
                a,
                b,
            }]
        }
        Intent::AwardRegion => {
            let region = need(sel.region, Unavailable::Region)?;
            let row = need(sel.award_row, Unavailable::AwardRow)?;
            let decision = need(state.decisions.get(&region), Unavailable::Decision)?;
            let player = need(decision.players.get(row), Unavailable::AwardRow)?;
            vec![Command::ChooseSharedCapture {
                region,
                player: *player,
            }]
        }
        Intent::PickAwardRow(_)
        | Intent::PickArmy(_)
        | Intent::PickPlot(_)
        | Intent::DialogHitDone
        | Intent::DialogChain
        | Intent::DialogReinforceDone
        | Intent::Opened
        | Intent::Closed
        | Intent::DialogSkip
        | Intent::ToggleDetails
        | Intent::LeaderIcon
        | Intent::IconListBackground
        | Intent::PickMember(_)
        | Intent::Presentation => Vec::new(),
    })
}

fn page_armies(state: &LivingWorldState, battle: u32, page: (u8, u8)) -> Vec<ArmyId> {
    let Some(found) = state.battles.iter().find(|b| b.id == battle) else {
        return Vec::new();
    };
    let sides = mint_sage_livingworld::battle::battle_sides(state, found);
    let Some(owner) = sides
        .get(usize::from(page.0))
        .and_then(|s| s.get(usize::from(page.1)))
    else {
        return Vec::new();
    };
    state.region(found.region).map_or_else(Vec::new, |r| {
        r.present
            .iter()
            .copied()
            .filter(|id| state.armies.get(id).is_some_and(|a| a.player == *owner))
            .collect()
    })
}

pub(crate) fn page_heroes(state: &LivingWorldState, battle: u32, page: (u8, u8)) -> Vec<ArmyId> {
    page_armies(state, battle, page)
        .into_iter()
        .filter(|id| state.armies.get(id).is_some_and(|a| a.is_hero_army()))
        .collect()
}

fn swapped_positions(state: &LivingWorldState, battle: u32, page: (u8, u8), first: u8) -> (u8, u8) {
    let all = page_armies(state, battle, page);
    let heroes = page_heroes(state, battle, page);
    let position = |hero: u8| {
        heroes
            .get(usize::from(hero))
            .and_then(|id| all.iter().position(|a| a == id))
            .and_then(|i| u8::try_from(i).ok())
    };
    match (position(first), position(first.saturating_add(1))) {
        (Some(a), Some(b)) => (a, b),
        _ => (first, first.saturating_add(1)),
    }
}

pub(crate) fn intent_named(
    state: &LivingWorldState,
    word: &str,
    arg: &str,
) -> Result<Intent, String> {
    let number = || {
        arg.trim()
            .parse::<u8>()
            .map_err(|_| format!("{word}: wants a number, got {arg:?}"))
    };
    Ok(match word {
        "endturn" => Intent::EndTurn,
        "cancel-move" => Intent::CancelArmyMove,
        "destroy" => Intent::DestroyBuilding,
        "disband" => Intent::DisbandMembers,
        "upgrade" => Intent::UpgradeMembers,
        "build" => Intent::BuildOnPlot(
            state
                .rules
                .building_id(arg.trim())
                .ok_or_else(|| format!("build: no building {arg:?}"))?,
        ),
        "queue" => Intent::QueueArmy { entry: number()? },
        "dequeue" => Intent::CancelQueued { index: number()? },
        "mode" => Intent::BattleMode(match arg.trim() {
            "auto" => 1,
            "retreat" => 2,
            "real" => 3,
            other => return Err(format!("mode: auto, retreat or real, not {other:?}")),
        }),
        "swap" => Intent::SwapInBattle { first: number()? },
        "pick-army" => Intent::PickArmy(usize::from(number()?)),
        "pick-plot" => Intent::PickPlot(usize::from(number()?)),
        "pick-award" => Intent::PickAwardRow(usize::from(number()?)),
        "confirm-award" => Intent::AwardRegion,
        "skip" => Intent::DialogSkip,
        other => return Err(format!("no control named {other:?}")),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    Tutorial,
    Engine(Rejected),
}

impl From<Rejected> for Refused {
    fn from(reason: Rejected) -> Self {
        Self::Engine(reason)
    }
}

pub(crate) fn describe(
    player: PlayerId,
    command: &Command,
    result: &Result<(), Refused>,
) -> String {
    let verdict = match result {
        Ok(()) => "accepted".to_owned(),
        Err(Refused::Tutorial) => "refused by the tutorial".to_owned(),
        Err(Refused::Engine(reason)) => format!("rejected: {reason:?}"),
    };
    format!("hud: player {} {command:?} -> {verdict}", player.0)
}

#[cfg(test)]
mod tests;
