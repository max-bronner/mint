use mint_sage_apt_player::Value;
use mint_sage_ini::living_world::players::BuildingType;

pub(super) const ARMIES: &str = "main.selectionDetails.tray.main.armies";
pub(super) const STRUCTURES: &str = "main.selectionDetails.tray.main.structures";
pub(super) const TERRITORY: &str = "main.selectionDetails.tray.main.territory";

pub(super) const TRAY: &str = "main.selectionDetails.tray";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tray {
    Region,
    BuildQueue,
    ArmyRetinue,
}

impl Tray {
    pub(super) fn movie(self) -> &'static str {
        match self {
            Tray::Region => "StrategicDetailsRegion.swf",
            Tray::BuildQueue => "StrategicDetailsBuildQueue.swf",
            Tray::ArmyRetinue => "StrategicDetailsArmyRetinue.swf",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct TrayView {
    pub(super) region: bool,
    pub(super) army_details: bool,
    pub(super) production_building: bool,
}

pub(super) fn tray_after(current: Option<Tray>, view: TrayView) -> Option<Tray> {
    if view.army_details {
        return Some(Tray::ArmyRetinue);
    }
    if view.production_building {
        return Some(Tray::BuildQueue);
    }
    if view.region && current != Some(Tray::Region) {
        return Some(Tray::Region);
    }
    current
}

pub(super) const TRAY_SHELL: &str = "main.selectionDetails";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum LockStep {
    Close,
    ToggleButton(&'static str),
    Flash { frame: usize, seconds: f32 },
}

#[derive(Debug, Default)]
pub(super) struct TrayLock {
    flashed: bool,
}

impl TrayLock {
    pub(super) fn update(&mut self, allowed: Option<bool>, open: bool) -> Vec<LockStep> {
        match allowed {
            None => Vec::new(),
            Some(false) => {
                let mut out = Vec::new();
                if open {
                    out.push(LockStep::Close);
                }
                out.push(LockStep::ToggleButton("_disabled"));
                out
            }
            Some(true) => {
                let mut out = vec![LockStep::ToggleButton("_enabled")];
                if !self.flashed {
                    self.flashed = true;
                    out.push(LockStep::Flash {
                        frame: 0,
                        seconds: 5.0,
                    });
                }
                out
            }
        }
    }
}

pub(super) fn slot_state(filled: bool, selected: bool) -> &'static str {
    match (filled, selected) {
        (false, _) => "_empty",
        (true, true) => "_selected",
        (true, false) => "_up",
    }
}

pub(super) fn visibility(shown: bool) -> &'static str {
    if shown {
        "_show"
    } else {
        "_hide"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Text {
    Plain(String),
    Formatted(&'static str, i64),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Bind {
    Call {
        function: &'static str,
        slot: Option<u32>,
        args: Vec<Value>,
    },
    Text {
        name: String,
        text: Text,
    },
    Picture {
        name: String,
        element: Option<String>,
    },
}

fn call(function: &'static str, args: Vec<Value>) -> Bind {
    Bind::Call {
        function,
        slot: None,
        args,
    }
}

fn slot_call(function: &'static str, slot: usize, label: &'static str) -> Bind {
    Bind::Call {
        function,
        slot: u32::try_from(slot).ok(),
        args: vec![Value::Number(slot as f64), Value::str(label)],
    }
}

fn plain(name: &str, text: String) -> Bind {
    Bind::Text {
        name: name.to_owned(),
        text: Text::Plain(text),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ArmySlot {
    pub(super) name: String,
    pub(super) hero: bool,
    pub(super) command_points: i32,
}

pub(super) fn armies(slots: &[ArmySlot], selected: Option<usize>) -> Vec<Bind> {
    let mut out = vec![call(
        "SetIconSlotCount",
        vec![Value::Number(slots.len() as f64)],
    )];
    for (i, slot) in slots.iter().enumerate() {
        out.push(slot_call(
            "SetIconSlotState",
            i,
            slot_state(true, selected == Some(i)),
        ));
        out.push(Bind::Picture {
            name: format!("IconSlotPortrait{i}"),
            element: None,
        });
        out.push(Bind::Picture {
            name: format!("IconSlotTypeImage{i}"),
            element: Some(
                if slot.hero {
                    "UnitHero"
                } else {
                    "UnitFortress"
                }
                .to_owned(),
            ),
        });
    }
    let picked = selected.and_then(|i| slots.get(i));
    out.push(plain(
        "ArmyName",
        picked.map(|s| s.name.clone()).unwrap_or_default(),
    ));
    out.push(plain(
        "CommandPoints",
        picked.map_or_else(String::new, |s| s.command_points.to_string()),
    ));
    out.push(call(
        "SetArmyNameState",
        vec![Value::str(visibility(picked.is_some()))],
    ));
    out.push(call(
        "SetCommandPointsState",
        vec![Value::str(visibility(picked.is_some()))],
    ));
    out
}

pub(super) fn retinue_layout(hero_army: bool) -> Vec<Bind> {
    let layout = if hero_army { "_hero" } else { "_garrison" };
    vec![call("SetLayout", vec![Value::str(layout)])]
}

pub(super) const RETINUE_READY: &str = "main.selectionDetails.tray.main.memberName";
pub(super) const QUEUE_READY: &str = "main.selectionDetails.tray.main.inProgressSlot";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemberSlot {
    pub(super) id: u32,
    pub(super) name: String,
    pub(super) quantity: u32,
    pub(super) level: u32,
    pub(super) upgraded: bool,
    pub(super) leader: bool,
    pub(super) command_points: i32,
}

fn rank(level: u32) -> Text {
    Text::Formatted("APT:RankLabel", i64::from(level))
}

pub(super) fn retinue_page(members: &[MemberSlot], selected: &[u32]) -> Vec<Bind> {
    let picked = selected
        .first()
        .and_then(|id| members.iter().find(|m| m.id == *id));
    let leader = members.iter().find(|m| m.leader);
    let mut out = vec![
        plain(
            "MemberName",
            picked.map(|m| m.name.clone()).unwrap_or_default(),
        ),
        Bind::Text {
            name: "MemberRank".to_owned(),
            text: picked.map_or_else(|| Text::Plain(String::new()), |m| rank(m.level)),
        },
        plain(
            "CommandPoints",
            members
                .iter()
                .map(|m| m.command_points)
                .sum::<i32>()
                .to_string(),
        ),
        plain(
            "LeaderQuantity",
            leader.map_or_else(String::new, |m| m.quantity.to_string()),
        ),
        Bind::Text {
            name: "LeaderRank".to_owned(),
            text: leader.map_or_else(|| Text::Plain(String::new()), |m| rank(m.level)),
        },
        Bind::Picture {
            name: "LeaderPortrait".to_owned(),
            element: None,
        },
        Bind::Picture {
            name: "LeaderTypeImage".to_owned(),
            element: leader.map(|_| "UnitHero".to_owned()),
        },
    ];
    out.push(call(
        "SetMemberNameState",
        vec![Value::str(visibility(picked.is_some()))],
    ));
    out.push(call(
        "SetMemberRankState",
        vec![Value::str(visibility(picked.is_some()))],
    ));
    out
}

pub(super) fn retinue_icon(member: &MemberSlot, picked: bool) -> Vec<Bind> {
    vec![
        plain("Quantity", member.quantity.to_string()),
        Bind::Picture {
            name: "Portrait".to_owned(),
            element: None,
        },
        Bind::Picture {
            name: "TypeImage".to_owned(),
            element: None,
        },
        call(
            "SetState",
            vec![Value::str(if picked { "_selected" } else { "_up" })],
        ),
        call(
            "SetUpgradeOverlayState",
            vec![Value::str(visibility(member.upgraded))],
        ),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct QueueSlot {
    pub(super) name: String,
    pub(super) quantity: u32,
    pub(super) build_time: i32,
    pub(super) turns_remaining: i32,
    pub(super) hero: bool,
    pub(super) command_points: i32,
}

pub(super) const QUEUED_SLOTS: usize = 6;

pub(super) fn build_queue(queue: &[QueueSlot]) -> Vec<Bind> {
    let head = queue.first();
    let type_image = |slot: &QueueSlot| {
        Some(
            if slot.hero {
                "UnitHero"
            } else {
                "UnitFortress"
            }
            .to_owned(),
        )
    };
    let mut out = vec![
        call(
            "SetInProgressIconSlotState",
            vec![Value::str(slot_state(head.is_some(), false))],
        ),
        plain(
            "InProgressIconSlotQuantity",
            head.map_or_else(String::new, |s| s.quantity.to_string()),
        ),
        plain(
            "InProgressIconSlotTurnsRemaining",
            head.map_or_else(String::new, |s| s.turns_remaining.to_string()),
        ),
        Bind::Picture {
            name: "InProgressIconSlotPortrait".to_owned(),
            element: None,
        },
        Bind::Picture {
            name: "InProgressIconSlotTypeImage".to_owned(),
            element: head.and_then(type_image),
        },
        plain("UnitName", head.map(|s| s.name.clone()).unwrap_or_default()),
        Bind::Text {
            name: "BuildTime".to_owned(),
            text: head.map_or_else(
                || Text::Plain(String::new()),
                |s| {
                    Text::Formatted(
                        if s.build_time == 1 {
                            "STRATEGICHUD:BuildTimeSingularLabel"
                        } else {
                            "STRATEGICHUD:BuildTimePluralLabel"
                        },
                        i64::from(s.build_time),
                    )
                },
            ),
        },
        plain(
            "CommandPoints",
            head.map_or_else(String::new, |s| s.command_points.to_string()),
        ),
        call(
            "SetUnitNameState",
            vec![Value::str(visibility(head.is_some()))],
        ),
        call(
            "SetBuildTimeState",
            vec![Value::str(visibility(head.is_some()))],
        ),
    ];
    for i in 0..QUEUED_SLOTS {
        let slot = queue.get(i.saturating_add(1));
        out.push(slot_call(
            "SetQueuedIconSlotState",
            i,
            slot_state(slot.is_some(), false),
        ));
        out.push(plain(
            &format!("QueuedIconSlotQuantity{i}"),
            slot.map_or_else(String::new, |s| s.quantity.to_string()),
        ));
        out.push(plain(
            &format!("QueuedIconSlotTurnsRemaining{i}"),
            slot.map_or_else(String::new, |s| s.turns_remaining.to_string()),
        ));
        out.push(Bind::Picture {
            name: format!("QueuedIconSlotPortrait{i}"),
            element: None,
        });
        out.push(Bind::Picture {
            name: format!("QueuedIconSlotTypeImage{i}"),
            element: slot.and_then(type_image),
        });
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlotSlot {
    pub(super) building: Option<String>,
    pub(super) kind: Option<BuildingType>,
    pub(super) turns_left: i32,
    pub(super) under_construction: bool,
}

pub(super) fn building_image(kind: Option<BuildingType>) -> Option<&'static str> {
    match kind? {
        BuildingType::Fortress => Some("BuildingFortress"),
        BuildingType::Armory => Some("BuildingArmory"),
        BuildingType::Resource => Some("BuildingResource"),
        BuildingType::Barracks => Some("BuildingBarracks"),
        BuildingType::None => None,
    }
}

pub(super) fn structures(plots: &[PlotSlot], selected: Option<usize>) -> Vec<Bind> {
    let mut out = vec![call(
        "SetIconSlotCount",
        vec![Value::Number(plots.len() as f64)],
    )];
    for (i, plot) in plots.iter().enumerate() {
        out.push(slot_call(
            "SetIconSlotState",
            i,
            slot_state(plot.building.is_some(), selected == Some(i)),
        ));
        out.push(Bind::Picture {
            name: format!("IconSlotPortrait{i}"),
            element: None,
        });
        out.push(Bind::Picture {
            name: format!("IconSlotTypeImage{i}"),
            element: building_image(plot.kind).map(str::to_owned),
        });
        let shown = plot.under_construction && plot.building.is_some();
        out.push(Bind::Call {
            function: "SetIconSlotProgressState",
            slot: u32::try_from(i).ok(),
            args: vec![Value::Number(i as f64), Value::str(visibility(shown))],
        });
        out.push(Bind::Text {
            name: format!("IconSlotTurnsRemaining{i}"),
            text: if shown {
                Text::Formatted(
                    if plot.turns_left == 1 {
                        "STRATEGICHUD:ConstructionOneTurnRemaining"
                    } else {
                        "STRATEGICHUD:ConstructionTurnsRemaining"
                    },
                    i64::from(plot.turns_left),
                )
            } else {
                Text::Plain(String::new())
            },
        });
    }
    let picked = selected
        .and_then(|i| plots.get(i))
        .and_then(|p| p.building.clone());
    out.push(call(
        "SetBuildingNameState",
        vec![Value::str(visibility(picked.is_some()))],
    ));
    out.push(plain("BuildingName", picked.unwrap_or_default()));
    out
}

pub(super) fn territory(name: &str, bonuses: &[(&'static str, i64)]) -> Vec<Bind> {
    let mut out = vec![plain("TerritoryName", name.to_owned())];
    for (i, (label, value)) in bonuses.iter().enumerate() {
        out.push(Bind::Text {
            name: format!("Bonus{i}"),
            text: Text::Formatted(label, *value),
        });
    }
    out
}
