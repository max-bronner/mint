use super::gates::GateAction;
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RotwkCommand {
    SetSelection {
        replace: bool,
        units: Vec<Entity>,
    },
    ClearSelection,
    SetStance {
        stance: u8,
    },
    ToggleWeaponSet {
        button: String,
        lead: Option<Entity>,
    },
    MoveTo {
        x: f32,
        y: f32,
        queue: bool,
    },
    AttackObject {
        target: Entity,
    },
    AttackMove {
        x: f32,
        y: f32,
        queue: bool,
    },
    ForceAttackObject {
        target: Entity,
    },
    Stop,
    ToggleHordeFormation,
    AttackGround {
        slot: u8,
        x: f32,
        y: f32,
    },
    WakeAutoPickUp {
        option: u8,
    },
    SetMovementModes(Modes),
    QueueUnit {
        button: u8,
    },
    CancelProduction,
    CancelBuild {
        button: u8,
    },
    CancelHero {
        button: u8,
    },
    ObjectUpgrade {
        button: u8,
    },
    PlayerUpgrade {
        button: u8,
    },
    FireSpecialPower {
        button: u8,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
    },
    RecruitHero {
        button: u8,
    },
    PurchaseScience {
        science: u16,
    },
    CastSpell {
        button: u8,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
    },
    CaptureObject {
        target: Entity,
    },
    Enter {
        target: Entity,
    },
    Exit {
        passenger: Option<Entity>,
    },
    Sell,
    StartSelfRepair,
    CancelUpgrade {
        button: u8,
        player: bool,
    },
    DebugSkillPoints {
        points: u32,
    },
    DebugSpellPoints {
        points: u32,
    },
    DebugExperience {
        points: u32,
    },
    DebugMoney {
        amount: u32,
    },
    DebugKill,
    DebugOath,
    ResetShroud {
        use_shroud: bool,
    },
    Construct {
        kind: u16,
        x: f32,
        y: f32,
        angle: f32,
    },
    FoundationConstruct {
        button: u8,
    },
    WallSpan {
        option: u8,
        x: f32,
        y: f32,
    },
    CancelConstruction,
    DebugSpawnHorde {
        recipe: u16,
        x: f32,
        y: f32,
        facing: f32,
    },
    DebugSpawnUnit {
        buildable: u16,
        x: f32,
        y: f32,
        facing: f32,
    },
    Debug(DebugCommand),
    CahPreview(super::cah_preview::CahPreview),
    DebugSpawnStructure {
        kind: u16,
        x: f32,
        y: f32,
        angle: f32,
    },
    MoveToFormation {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        attack: bool,
    },
    ToggleAutoCast {
        button: u8,
    },
    SetRallyPoint {
        structures: Vec<Entity>,
        x: f32,
        y: f32,
        global: bool,
        target: Option<Entity>,
    },
    GateCommand {
        action: GateAction,
    },
    SetUiPanelOpen {
        panel: UiPanel,
        open: bool,
    },
    CameraScrolled {
        distance: f32,
    },
    CameraReset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum UiPanel {
    SpellStore,
    Objectives,
    StanceSubMenu,
}

impl UiPanel {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::SpellStore => 0,
            Self::Objectives => 1,
            Self::StanceSubMenu => 2,
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        [Self::SpellStore, Self::Objectives, Self::StanceSubMenu]
            .into_iter()
            .find(|panel| format!("{panel:?}").eq_ignore_ascii_case(name))
    }
}

impl Command for RotwkCommand {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self {
            Self::SetSelection { replace, units } => {
                hasher.write_u8(0);
                hasher.write_bool(*replace);
                hasher.write_usize(units.len());
                for unit in units {
                    unit.hash_into(hasher);
                }
            }
            Self::ClearSelection => hasher.write_u8(1),
            Self::MoveTo { x, y, queue } => {
                hasher.write_u8(2);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
                hasher.write_bool(*queue);
            }
            Self::AttackObject { target } => {
                hasher.write_u8(4);
                target.hash_into(hasher);
            }
            Self::AttackMove { x, y, queue } => {
                hasher.write_u8(12);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
                hasher.write_bool(*queue);
            }
            Self::AttackGround { slot, x, y } => {
                hasher.write_u8(38);
                hasher.write_u8(*slot);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
            }
            Self::WakeAutoPickUp { option } => {
                hasher.write_u8(39);
                hasher.write_u8(*option);
            }
            Self::SetMovementModes(modes) => {
                hasher.write_u8(3);
                crate::unit::hash_modes(*modes, hasher);
            }
            Self::QueueUnit { button } => {
                hasher.write_u8(5);
                hasher.write_u8(*button);
            }
            Self::CancelProduction => hasher.write_u8(6),
            Self::ResetShroud { use_shroud } => {
                hasher.write_u8(7);
                hasher.write_bool(*use_shroud);
            }
            Self::Construct { kind, x, y, angle } => {
                hasher.write_u8(8);
                hasher.write_u16(*kind);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
                hasher.write_f32(*angle);
            }
            Self::CancelConstruction => hasher.write_u8(9),
            Self::DebugSpawnHorde {
                recipe,
                x,
                y,
                facing,
            } => {
                hasher.write_u8(10);
                hasher.write_u16(*recipe);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
                hasher.write_f32(*facing);
            }
            Self::DebugSpawnUnit {
                buildable,
                x,
                y,
                facing,
            } => {
                hasher.write_u8(11);
                hasher.write_u16(*buildable);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
                hasher.write_f32(*facing);
            }
            Self::ObjectUpgrade { button } => {
                hasher.write_u8(13);
                hasher.write_u8(*button);
            }
            Self::PlayerUpgrade { button } => {
                hasher.write_u8(14);
                hasher.write_u8(*button);
            }
            Self::RecruitHero { button } => {
                hasher.write_u8(16);
                hasher.write_u8(*button);
            }
            Self::FireSpecialPower { button, target, at } => {
                hasher.write_u8(15);
                hash_power_target(hasher, *button, *target, *at);
            }
            Self::PurchaseScience { science } => {
                hasher.write_u8(17);
                hasher.write_u16(*science);
            }
            Self::CastSpell { button, target, at } => {
                hasher.write_u8(18);
                hash_power_target(hasher, *button, *target, *at);
            }
            Self::Sell => hasher.write_u8(19),
            Self::StartSelfRepair => hasher.write_u8(32),
            Self::CancelUpgrade { button, player } => {
                hasher.write_u8(20);
                hasher.write_u8(*button);
                hasher.write_bool(*player);
            }
            Self::DebugSkillPoints { points } => {
                hasher.write_u8(21);
                hasher.write_u32(*points);
            }
            Self::DebugSpellPoints { points } => {
                hasher.write_u8(46);
                hasher.write_u32(*points);
            }
            Self::DebugExperience { points } => {
                hasher.write_u8(22);
                hasher.write_u32(*points);
            }
            Self::DebugKill => hasher.write_u8(23),
            Self::DebugOath => hasher.write_u8(44),
            Self::CancelBuild { button } => {
                hasher.write_u8(24);
                hasher.write_u8(*button);
            }
            Self::CancelHero { button } => {
                hasher.write_u8(25);
                hasher.write_u8(*button);
            }
            Self::DebugMoney { amount } => {
                hasher.write_u8(26);
                hasher.write_u32(*amount);
            }
            Self::FoundationConstruct { button } => {
                hasher.write_u8(27);
                hasher.write_u8(*button);
            }
            Self::Debug(command) => {
                hasher.write_u8(28);
                command.hash_into(hasher);
            }
            Self::CahPreview(command) => {
                hasher.write_u8(47);
                command.hash_into(hasher);
            }
            Self::CaptureObject { target } => {
                hasher.write_u8(31);
                target.hash_into(hasher);
            }
            Self::DebugSpawnStructure { kind, x, y, angle } => {
                hasher.write_u8(30);
                hasher.write_u16(*kind);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
                hasher.write_f32(*angle);
            }
            Self::ToggleAutoCast { button } => {
                hasher.write_u8(33);
                hasher.write_u8(*button);
            }
            Self::Enter { target } => {
                hasher.write_u8(35);
                target.hash_into(hasher);
            }
            Self::Exit { passenger } => {
                hasher.write_u8(36);
                hasher.write_bool(passenger.is_some());
                if let Some(passenger) = passenger {
                    passenger.hash_into(hasher);
                }
            }
            Self::SetStance { stance } => {
                hasher.write_u8(37);
                hasher.write_u8(*stance);
            }
            Self::ToggleWeaponSet { button, lead } => {
                hasher.write_u8(44);
                hasher.write_usize(button.len());
                for byte in button.bytes() {
                    hasher.write_u8(byte);
                }
                hasher.write_bool(lead.is_some());
                if let Some(lead) = lead {
                    lead.hash_into(hasher);
                }
            }
            Self::WallSpan { option, x, y } => {
                hasher.write_u8(34);
                hasher.write_u8(*option);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
            }
            Self::ForceAttackObject { target } => {
                hasher.write_u8(29);
                target.hash_into(hasher);
            }
            Self::Stop => hasher.write_u8(40),
            Self::ToggleHordeFormation => hasher.write_u8(41),
            Self::MoveToFormation {
                x,
                y,
                dx,
                dy,
                attack,
            } => {
                hasher.write_u8(42);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
                hasher.write_f32(*dx);
                hasher.write_f32(*dy);
                hasher.write_bool(*attack);
            }
            Self::SetRallyPoint {
                structures,
                x,
                y,
                global,
                target,
            } => {
                hasher.write_u8(43);
                hasher.write_usize(structures.len());
                for structure in structures {
                    structure.hash_into(hasher);
                }
                hasher.write_f32(*x);
                hasher.write_f32(*y);
                hasher.write_bool(*global);
                hasher.write_bool(target.is_some());
                if let Some(target) = target {
                    target.hash_into(hasher);
                }
            }
            Self::GateCommand { action } => {
                hasher.write_u8(45);
                hasher.write_u8(match action {
                    GateAction::Open => 0,
                    GateAction::Close => 1,
                    GateAction::Toggle => 2,
                });
            }
            Self::SetUiPanelOpen { panel, open } => {
                hasher.write_u8(47);
                hasher.write_u8(panel.code());
                hasher.write_bool(*open);
            }
            Self::CameraScrolled { distance } => {
                hasher.write_u8(48);
                hasher.write_f32(*distance);
            }
            Self::CameraReset => hasher.write_u8(49),
        }
    }
}

pub(super) fn hash_power_target(
    hasher: &mut StateHasher,
    button: u8,
    target: Option<Entity>,
    at: Option<(f32, f32)>,
) {
    hasher.write_u8(button);
    hasher.write_bool(target.is_some());
    if let Some(target) = target {
        target.hash_into(hasher);
    }
    hasher.write_bool(at.is_some());
    if let Some((x, y)) = at {
        hasher.write_f32(x);
        hasher.write_f32(y);
    }
}
