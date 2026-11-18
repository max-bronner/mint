use mint_sage_ini::{CommandButtonDef, CommandButtonIndex, CommandSetIndex, Stance};
use mint_sage_modules::ObjectRecord;

use crate::templates::{KindTable, TargetKind};
use crate::unit::Kind;

pub(crate) const RING_SLOTS: usize = 6;
pub(crate) const BAR_SLOTS: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SlotAction {
    Build(u8),
    RadialPush {
        start: u32,
        count: u32,
    },
    RadialPop,
    ObjectUpgrade(u8),
    PlayerUpgrade(u8),
    FireSpecialPower {
        button: u8,
        target_kind: TargetKind,
        name: String,
    },
    FireWeaponAt {
        slot: u8,
        name: String,
    },
    WakeAutoPickUp(u8),
    Revive(u8),
    Sell,
    Exit(Option<mint_sim::Entity>),
    EmptySlot,
    Passive,
    StartSelfRepair,
    FoundationConstruct(u8),
    SetStance(Stance),
    ToggleStance(Vec<Stance>),
    CancelConstruction,
    BeginWallSpan {
        option: u8,
    },
    ToggleWeaponSet(String),
    Stop,
    ToggleHordeFormation,
    ArmAttackMove,
    GateCommand(crate::rules::GateAction),
    Unimplemented(&'static str),
}

fn stances(button: &CommandButtonDef) -> Vec<Stance> {
    button
        .stances
        .iter()
        .filter_map(|name| Stance::from_name(name))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Availability {
    Hidden,
    Disabled,
    NotReady { percent: u8 },
    CantAfford,
    Held,
    ExtraDisabled,
    Enabled,
    VisuallyEnabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedSlot {
    pub(crate) action: SlotAction,
    pub(crate) label: String,
    pub(crate) command_button: String,
    pub(crate) enabled: bool,
    pub(crate) availability: Availability,
    pub(crate) image: Option<String>,
    pub(crate) cost: Option<i64>,
    pub(crate) progress: Option<u8>,
    pub(crate) count: u16,
    pub(crate) auto_cast: bool,
    pub(crate) hero: Option<SlotHero>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlotHero {
    pub(crate) display_name: Option<String>,
    pub(crate) literal_name: Option<String>,
    pub(crate) revive: bool,
    pub(crate) hotkey: Option<String>,
    pub(crate) revive_text: Option<String>,
    pub(crate) recruit_text: Option<String>,
    pub(crate) level: u16,
    pub(crate) command_points: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CommandBar {
    pub(crate) ring: [Option<ResolvedSlot>; RING_SLOTS],
    pub(crate) bar: Vec<ResolvedSlot>,
    pub(crate) radial: Vec<(u32, ResolvedSlot)>,
    pub(crate) radial_root: Option<u32>,
    pub(crate) stance_flyout: Vec<ResolvedSlot>,
}

impl CommandBar {
    pub(crate) fn ring_count(&self) -> usize {
        self.ring.iter().flatten().count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hero {
    pub(crate) name: String,
    pub(crate) image: Option<String>,
    pub(crate) fielded: bool,
    pub(crate) affordable: bool,
    pub(crate) allowed: bool,
    pub(crate) display_name: Option<String>,
    pub(crate) literal_name: Option<String>,
    pub(crate) revive: bool,
    pub(crate) hotkey: Option<String>,
    pub(crate) revive_text: Option<String>,
    pub(crate) recruit_text: Option<String>,
    pub(crate) level: u16,
    pub(crate) command_points: i64,
}

pub(crate) struct SlotContext<'a> {
    pub(crate) afford: &'a dyn Fn(Kind) -> bool,
    pub(crate) cost: &'a dyn Fn(Kind) -> Option<i64>,
    pub(crate) allowed_to_build: &'a dyn Fn(Kind) -> bool,
    pub(crate) upgrades: &'a [String],
    pub(crate) object_upgrades: &'a [String],
    pub(crate) player_upgrades: &'a [String],
    pub(crate) castle_upgrades: &'a [String],
    pub(crate) researching: &'a [String],
    pub(crate) upgrade_afford: &'a dyn Fn(&str) -> bool,
    pub(crate) upgrade_affected: &'a dyn Fn(&str) -> bool,
    pub(crate) heroes: &'a [Hero],
    pub(crate) released_powers: &'a [String],
    pub(crate) command_set: Option<&'a str>,
    pub(crate) sell_refusal: Option<&'a str>,
    pub(crate) self_repair: Option<crate::rules::SelfRepair>,
    pub(crate) gate: Option<crate::rules::GateButtons>,
    pub(crate) cancel_refusal: Option<&'a str>,
    pub(crate) under_construction: bool,
    pub(crate) power_clock: &'a dyn Fn(&str) -> Option<u8>,
    pub(crate) auto_cast_on: &'a dyn Fn(&str) -> bool,
    pub(crate) weapon_set_raised: &'a dyn Fn(&[String]) -> bool,
    pub(crate) model_condition_gate: &'a dyn Fn(&[String], &[String]) -> bool,
    pub(crate) passengers: &'a [Option<String>],
    pub(crate) passenger_entities: &'a [mint_sim::Entity],
    pub(crate) capacity: Option<u32>,
    pub(crate) foreign_owner: bool,
}

const CANCEL_CONSTRUCTION_BUTTON: &str = "Command_CancelConstruction";

fn under_construction_bar(
    record: &ObjectRecord,
    command_buttons: &CommandButtonIndex,
    ctx: &SlotContext<'_>,
) -> CommandBar {
    let mut out = CommandBar::default();
    if record.has_kind("DONT_USE_CANCEL_BUILD_BUTTON") {
        return out;
    }
    let Some(button) = command_buttons.get(CANCEL_CONSTRUCTION_BUTTON) else {
        return out;
    };
    let availability = if ctx.cancel_refusal.is_some() {
        Availability::Disabled
    } else {
        Availability::Enabled
    };
    let slot = ResolvedSlot {
        action: SlotAction::CancelConstruction,
        label: button
            .text_label
            .clone()
            .unwrap_or_else(|| CANCEL_CONSTRUCTION_BUTTON.to_owned()),
        command_button: CANCEL_CONSTRUCTION_BUTTON.to_owned(),
        enabled: availability == Availability::Enabled,
        availability,
        image: button.button_image.clone(),
        cost: None,
        progress: None,
        count: 0,
        auto_cast: false,
        hero: None,
    };
    if button.radial == Some(true) {
        out.radial.push((1, slot.clone()));
    }
    if let Some(first) = out.ring.first_mut() {
        *first = Some(slot);
    }
    out
}

fn unimplemented_label(command: &str) -> &'static str {
    match command.to_ascii_uppercase().as_str() {
        "OBJECT_UPGRADE" | "PLAYER_UPGRADE" => "an upgrade",
        "SPECIAL_POWER" | "SPECIAL_POWER_TOGGLE" => "a special power",
        "TOGGLE_STANCE" | "SET_STANCE" => "a stance",
        _ => "an unimplemented command",
    }
}

fn toggled_image(button: &CommandButtonDef, ctx: &SlotContext<'_>) -> Option<String> {
    let follows = button
        .options
        .iter()
        .any(|option| option.eq_ignore_ascii_case("TOGGLE_IMAGE_ON_WEAPONSET"));
    let toggles = is(button.command.as_deref(), "TOGGLE_WEAPONSET")
        && !button.flags_used_for_toggle.is_empty();
    if !(follows && toggles && (ctx.weapon_set_raised)(&button.flags_used_for_toggle)) {
        return None;
    }
    button.toggle_face(true)?.button_image.clone()
}

fn is_passive(button: &CommandButtonDef) -> bool {
    button
        .options
        .iter()
        .any(|option| option.eq_ignore_ascii_case("NONPRESSABLE"))
}

fn is(command: Option<&str>, name: &str) -> bool {
    command.is_some_and(|c| c.eq_ignore_ascii_case(name))
}

fn module_option(button: &CommandButtonDef) -> u8 {
    button
        .options
        .iter()
        .find_map(|option| mint_sage_modules::ModuleOption::parse(option))
        .map_or(0, mint_sage_modules::ModuleOption::index)
}

fn inventory_state(ctx: &SlotContext<'_>, button: &CommandButtonDef, slot: usize) -> Availability {
    if ctx
        .capacity
        .is_some_and(|max| u64::try_from(slot).unwrap_or(u64::MAX) >= u64::from(max))
    {
        return Availability::Hidden;
    }
    let any_container = button.requires_valid_container && ctx.capacity.is_some();
    if slot < ctx.passengers.len() || any_container {
        Availability::Enabled
    } else {
        Availability::Disabled
    }
}

fn has_option(button: &CommandButtonDef, option: &str) -> bool {
    button
        .options
        .iter()
        .any(|o| o.eq_ignore_ascii_case(option))
}

pub(crate) fn availability(
    button: &CommandButtonDef,
    record: &ObjectRecord,
    ctx: &SlotContext<'_>,
    build: Option<bool>,
    hero: Option<&Hero>,
    allowed: Option<bool>,
) -> Availability {
    let command = button.command.as_deref();
    let held = |upgrade: &str| ctx.upgrades.iter().any(|u| u.eq_ignore_ascii_case(upgrade));
    let object_held = |upgrade: &str| {
        ctx.object_upgrades
            .iter()
            .any(|u| u.eq_ignore_ascii_case(upgrade))
    };
    let player_held = |upgrade: &str| {
        ctx.player_upgrades
            .iter()
            .any(|u| u.eq_ignore_ascii_case(upgrade))
    };
    let castle_held = |upgrade: &str| {
        ctx.castle_upgrades
            .iter()
            .any(|u| u.eq_ignore_ascii_case(upgrade))
    };
    let researching = |upgrade: &str| {
        ctx.researching
            .iter()
            .any(|u| u.eq_ignore_ascii_case(upgrade))
    };
    let state = if !(ctx.model_condition_gate)(
        &button.enable_on_model_condition,
        &button.disable_on_model_condition,
    ) {
        Availability::Disabled
    } else if is(command, "REVIVE") && hero.is_none() {
        return Availability::Hidden;
    } else if !button.needs_met(held) {
        Availability::Disabled
    } else if allowed == Some(false) {
        Availability::ExtraDisabled
    } else if let Some(affordable) = build {
        if affordable {
            Availability::Enabled
        } else {
            Availability::CantAfford
        }
    } else if is(command, "SPECIAL_POWER") || is(command, "SPECIAL_POWER_TOGGLE") {
        let recharging = button.special_power.as_deref().and_then(ctx.power_clock);
        let paused = button.special_power.as_deref().is_some_and(|power| {
            record.power(power).is_some_and(|slot| slot.starts_paused)
                && !ctx
                    .released_powers
                    .iter()
                    .any(|released| released.eq_ignore_ascii_case(power))
        });
        if let Some(percent) = recharging {
            Availability::NotReady { percent }
        } else if paused {
            Availability::Disabled
        } else {
            Availability::Enabled
        }
    } else if let Some(hero) = hero {
        if hero.fielded {
            return Availability::Hidden;
        } else if !hero.allowed {
            Availability::ExtraDisabled
        } else if hero.affordable {
            Availability::Enabled
        } else {
            Availability::CantAfford
        }
    } else if is(command, "OBJECT_UPGRADE")
        && button
            .upgrade
            .as_deref()
            .is_some_and(|u| object_held(u) || researching(u))
    {
        Availability::Held
    } else if is(command, "PLAYER_UPGRADE")
        && button
            .upgrade
            .as_deref()
            .is_some_and(|u| player_held(u) || researching(u))
    {
        Availability::Held
    } else if is(command, "CASTLE_UPGRADE") && button.upgrade.as_deref().is_some_and(castle_held) {
        Availability::Held
    } else if is(command, "OBJECT_UPGRADE")
        && button
            .upgrade
            .as_deref()
            .is_some_and(|u| !(ctx.upgrade_affected)(u))
    {
        Availability::Disabled
    } else if (is(command, "OBJECT_UPGRADE")
        || is(command, "PLAYER_UPGRADE")
        || is(command, "CASTLE_UPGRADE"))
        && button
            .upgrade
            .as_deref()
            .is_some_and(|u| !(ctx.upgrade_afford)(u))
    {
        Availability::CantAfford
    } else if is(command, "SELL") && ctx.sell_refusal.is_some() {
        Availability::Disabled
    } else if is(command, "START_SELF_REPAIR") {
        match ctx.self_repair {
            Some(crate::rules::SelfRepair::Rebuild | crate::rules::SelfRepair::Repair) => {
                Availability::Enabled
            }
            Some(crate::rules::SelfRepair::Repairing) => Availability::Held,
            Some(crate::rules::SelfRepair::Hidden) => return Availability::Hidden,
            Some(crate::rules::SelfRepair::Refused(_)) | None => Availability::Disabled,
        }
    } else if is(command, "TOGGLE_GATE") || is(command, "OPEN_GATE") || is(command, "CLOSE_GATE") {
        match ctx.gate {
            Some(gate)
                if gate.may_operate
                    && (is(command, "TOGGLE_GATE")
                        || (is(command, "OPEN_GATE") && !gate.open)
                        || (is(command, "CLOSE_GATE") && gate.open)) =>
            {
                Availability::Enabled
            }
            _ => Availability::Disabled,
        }
    } else {
        Availability::Enabled
    };
    let state = if state == Availability::Disabled && has_option(button, "HIDE_WHILE_DISABLED") {
        Availability::Hidden
    } else {
        state
    };
    if ctx.foreign_owner && state == Availability::Enabled {
        Availability::VisuallyEnabled
    } else {
        state
    }
}

pub(crate) fn resolve_command_bar(
    kinds: &KindTable,
    command_sets: &CommandSetIndex,
    command_buttons: &CommandButtonIndex,
    selected: Option<Kind>,
    ctx: &SlotContext<'_>,
) -> CommandBar {
    let mut out = CommandBar::default();
    let Some(entry) = selected.and_then(|kind| kinds.get(kind)) else {
        return out;
    };
    if ctx.under_construction {
        return under_construction_bar(&entry.record, command_buttons, ctx);
    }
    let Some(set) = ctx
        .command_set
        .or(entry.record.command_set.as_deref())
        .and_then(|name| command_sets.get(name))
    else {
        return out;
    };
    let build_command = if entry.record.dozer {
        "DOZER_CONSTRUCT"
    } else {
        "UNIT_BUILD"
    };
    let structure = entry.record.has_kind("STRUCTURE");
    out.radial_root = set.initial_visible.and_then(|v| u32::try_from(v).ok());
    let mut build_index: u16 = 0;
    let mut object_upgrade_index: u8 = 0;
    let mut player_upgrade_index: u8 = 0;
    let mut power_index: u8 = 0;
    let mut revive_index: u8 = 0;
    let mut foundation_index: u8 = 0;
    let mut exit_index: usize = 0;
    let mut heroes = ctx.heroes.iter();
    let mut toggle_order: Option<Vec<Stance>> = None;
    for (&slot_number, name) in &set.slots {
        let Some(button) = command_buttons.get(name) else {
            continue;
        };
        let command = button.command.as_deref();
        if toggle_order.is_none() && is(command, "TOGGLE_STANCE") {
            toggle_order = Some(stances(button));
        }
        let is_build = is(command, build_command) && button.object.is_some();
        let is_foundation = is(command, "FOUNDATION_CONSTRUCT") && button.object.is_some();
        let hero = if is(command, "REVIVE") {
            heroes.next()
        } else {
            None
        };
        let inventory = is(command, "EXIT_CONTAINER").then(|| {
            let slot = exit_index;
            exit_index = exit_index.saturating_add(1);
            slot
        });
        let (action, label, build, cost, allowed) = if is_build || is_foundation {
            let (Some(target), Some(action)) = (
                kinds.kind(button.object.as_deref().unwrap_or_default()),
                if is_build {
                    let index = build_index;
                    build_index = build_index.saturating_add(1);
                    u8::try_from(index).ok().map(SlotAction::Build)
                } else {
                    let index = foundation_index;
                    foundation_index = foundation_index.saturating_add(1);
                    Some(SlotAction::FoundationConstruct(index))
                },
            ) else {
                continue;
            };
            let record = kinds.get(target);
            let label = record.map_or_else(|| String::from("?"), |e| e.record.name.clone());
            let cost = (ctx.cost)(target);
            (
                action,
                label,
                Some((ctx.afford)(target)),
                cost,
                Some((ctx.allowed_to_build)(target)),
            )
        } else {
            let upper = command.map(str::to_ascii_uppercase);
            let action = match (
                upper.as_deref(),
                button.command_range_start,
                button.command_range_count,
            ) {
                (Some("PUSH_VISIBLE_COMMAND_RANGE"), Some(start), Some(count)) => {
                    SlotAction::RadialPush { start, count }
                }
                (Some("POP_VISIBLE_COMMAND_RANGE"), _, _) => SlotAction::RadialPop,
                (Some("OBJECT_UPGRADE" | "CASTLE_UPGRADE"), _, _) if button.upgrade.is_some() => {
                    let index = object_upgrade_index;
                    object_upgrade_index = object_upgrade_index.saturating_add(1);
                    SlotAction::ObjectUpgrade(index)
                }
                (Some("PLAYER_UPGRADE"), _, _) if button.upgrade.is_some() => {
                    let index = player_upgrade_index;
                    player_upgrade_index = player_upgrade_index.saturating_add(1);
                    SlotAction::PlayerUpgrade(index)
                }
                (Some("REVIVE"), _, _) => {
                    let index = revive_index;
                    revive_index = revive_index.saturating_add(1);
                    SlotAction::Revive(index)
                }
                (Some("DOZER_CONSTRUCT"), _, _) => SlotAction::BeginWallSpan {
                    option: module_option(button),
                },
                (Some("SET_STANCE"), _, _) if !stances(button).is_empty() => {
                    stances(button).first().copied().map_or(
                        SlotAction::Unimplemented("SET_STANCE"),
                        SlotAction::SetStance,
                    )
                }
                (Some("TOGGLE_STANCE"), _, _) if !stances(button).is_empty() => {
                    SlotAction::ToggleStance(stances(button))
                }
                (Some("TOGGLE_WEAPONSET"), _, _) if !button.flags_used_for_toggle.is_empty() => {
                    SlotAction::ToggleWeaponSet(name.clone())
                }
                (Some("SELL"), _, _) => SlotAction::Sell,
                (Some("EXIT_CONTAINER" | "EVACUATE"), _, _) => SlotAction::Exit(
                    inventory.and_then(|slot| ctx.passenger_entities.get(slot).copied()),
                ),
                (Some("START_SELF_REPAIR"), _, _) => SlotAction::StartSelfRepair,
                (Some("STOP"), _, _) => SlotAction::Stop,
                (Some("TOGGLE_GATE"), _, _) => {
                    SlotAction::GateCommand(crate::rules::GateAction::Toggle)
                }
                (Some("OPEN_GATE"), _, _) => {
                    SlotAction::GateCommand(crate::rules::GateAction::Open)
                }
                (Some("CLOSE_GATE"), _, _) => {
                    SlotAction::GateCommand(crate::rules::GateAction::Close)
                }
                (Some("HORDE_TOGGLE_FORMATION"), _, _) => SlotAction::ToggleHordeFormation,
                (Some("ATTACK_MOVE"), _, _) => SlotAction::ArmAttackMove,
                (Some("WAKE_AUTO_PICKUP"), _, _) => {
                    SlotAction::WakeAutoPickUp(module_option(button).saturating_add(1))
                }
                (Some("FIRE_WEAPON"), _, _)
                    if crate::templates::target_kind(&button.options) == TargetKind::Position =>
                {
                    button.weapon_slot.map_or(
                        SlotAction::Unimplemented("a weapon button with no WeaponSlot"),
                        |slot| SlotAction::FireWeaponAt {
                            slot,
                            name: name.clone(),
                        },
                    )
                }
                (Some("SPECIAL_POWER" | "SPECIAL_POWER_TOGGLE"), _, _)
                    if button.special_power.is_some() =>
                {
                    let index = power_index;
                    power_index = power_index.saturating_add(1);
                    SlotAction::FireSpecialPower {
                        button: index,
                        target_kind: crate::templates::target_kind(&button.options),
                        name: name.clone(),
                    }
                }
                _ => SlotAction::Unimplemented(
                    command.map_or("an unrecognised command", unimplemented_label),
                ),
            };
            let label = hero.map_or_else(
                || button.text_label.clone().unwrap_or_else(|| name.clone()),
                |h| h.name.clone(),
            );
            (action, label, None, None, None)
        };
        let action = match inventory {
            Some(slot) if slot >= ctx.passengers.len() => SlotAction::EmptySlot,
            _ if is_passive(button) => SlotAction::Passive,
            _ => action,
        };
        let state = match inventory {
            Some(slot) => inventory_state(ctx, button, slot),
            None if is(command, "EVACUATE")
                && !button.requires_valid_container
                && ctx.capacity.is_some()
                && ctx.passengers.is_empty() =>
            {
                Availability::Disabled
            }
            None => availability(button, &entry.record, ctx, build, hero, allowed),
        };
        if state == Availability::Hidden {
            continue;
        }
        let progress = match state {
            Availability::NotReady { percent } => Some(percent),
            _ => None,
        };
        let auto_cast = button.auto_ability == Some(true)
            && button
                .special_power
                .as_deref()
                .is_some_and(ctx.auto_cast_on);
        let slot = ResolvedSlot {
            action,
            label,
            command_button: name.clone(),
            enabled: state == Availability::Enabled,
            availability: state,
            cost,
            image: inventory
                .and_then(|slot| ctx.passengers.get(slot).cloned().flatten())
                .or_else(|| hero.and_then(|h| h.image.clone()))
                .or_else(|| toggled_image(button, ctx)),
            progress,
            count: 0,
            auto_cast,
            hero: hero.map(|h| SlotHero {
                display_name: h.display_name.clone(),
                literal_name: h.literal_name.clone(),
                revive: h.revive,
                hotkey: h.hotkey.clone(),
                revive_text: h.revive_text.clone(),
                recruit_text: h.recruit_text.clone(),
                level: h.level,
                command_points: h.command_points,
            }),
        };
        if matches!(slot.action, SlotAction::SetStance(_)) {
            out.stance_flyout.push(slot.clone());
        }
        let ring_position = usize::try_from(slot_number)
            .ok()
            .and_then(|n| n.checked_sub(1))
            .filter(|&p| p < RING_SLOTS);
        if let (Some(p), Some(true)) = (ring_position, button.in_palantir) {
            if let Some(cell) = out.ring.get_mut(p) {
                *cell = Some(slot.clone());
            }
        }
        if button.radial == Some(true) {
            if structure {
                out.radial.push((slot_number, slot));
            } else if out.bar.len() < BAR_SLOTS {
                out.bar.push(slot);
            }
        }
    }
    if let Some(order) = toggle_order {
        out.stance_flyout.sort_by_key(|slot| match slot.action {
            SlotAction::SetStance(stance) => order
                .iter()
                .position(|&listed| listed == stance)
                .unwrap_or(usize::MAX),
            _ => usize::MAX,
        });
    }
    out
}

pub(crate) fn remove_buttons(bar: &mut CommandBar, removed: &[&str]) {
    if removed.is_empty() {
        return;
    }
    let gone = |slot: &ResolvedSlot| removed.contains(&slot.command_button.as_str());
    for cell in &mut bar.ring {
        if cell.as_ref().is_some_and(gone) {
            *cell = None;
        }
    }
    bar.bar.retain(|slot| !gone(slot));
    bar.radial.retain(|(_, slot)| !gone(slot));
    bar.stance_flyout.retain(|slot| !gone(slot));
}

pub(crate) fn show_stance_face(
    bar: &mut CommandBar,
    command_buttons: &CommandButtonIndex,
    stance: Stance,
) {
    let slots = bar
        .ring
        .iter_mut()
        .flatten()
        .chain(bar.bar.iter_mut())
        .chain(bar.radial.iter_mut().map(|(_, slot)| slot));
    for slot in slots {
        if !matches!(slot.action, SlotAction::ToggleStance(_)) {
            continue;
        }
        let Some(face) = command_buttons
            .get(&slot.command_button)
            .and_then(|button| button.stance_face(stance))
        else {
            continue;
        };
        if let Some(label) = &face.text_label {
            slot.label.clone_from(label);
        }
        if face.button_image.is_some() {
            slot.image.clone_from(&face.button_image);
        }
    }
}

pub(crate) fn hotkey_letter(strings: &mint_sage_str::StringTable, label_key: &str) -> Option<char> {
    let text = strings.get(label_key)?;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            return chars.next().and_then(|next| next.to_lowercase().next());
        }
    }
    None
}

pub(crate) fn armed_hotkey_action(
    bar: &CommandBar,
    strings: &mint_sage_str::StringTable,
    key: char,
) -> Option<SlotAction> {
    bar.ring
        .iter()
        .flatten()
        .chain(bar.bar.iter())
        .chain(bar.radial.iter().map(|(_, slot)| slot))
        .find(|slot| slot.enabled && hotkey_letter(strings, &slot.label) == Some(key))
        .map(|slot| slot.action.clone())
}

#[cfg(test)]
mod tests;
