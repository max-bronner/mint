use mint_sage_apt_player::{Exposure, ImageKey, Value};
use mint_sage_str::{format_placeholder, Placeholder};
use mint_sim::{Entity, Frame, PlayerId, World};

use crate::button_flash::FlashView;
use crate::command_bar::{
    resolve_command_bar, Availability, CommandBar, Hero, ResolvedSlot, SlotAction, SlotContext,
    BAR_SLOTS, RING_SLOTS,
};
use crate::experience::RankDisplay;
use crate::game::{MatchState, LOCAL_PLAYER};
use crate::game_data::GameData;
use crate::radial::RadialAnchor;
use crate::render_state::UnitView;
use crate::rules::{Ids, RotwkRules, SpellBarReadiness};
use crate::spell_book::{SpellBookHoldings, SpellBookView, SpellSlotView, SpellState};
use crate::unit::Kind;

pub(crate) const PALANTIR_READY: &str = "OnInitialized";
#[cfg(test)]
pub(crate) const COMMAND_BAR_READY: &str = "OnAptInGameSideCommandBarLoaded";
pub(crate) const COMMAND_BAR_FADED_OUT: &str = "OnAptInGameSideCommandBarFadeOutComplete";
const RESOURCE_ICON: &str = "Resource_Icon";
const RANK_LABEL: &str = "APT:RankLabel";
const TIME_REMAINING_LABEL: &str = "APT:PalantirTimeRemaining";
const SIDE_CLIPS: [&str; 2] = [
    "PalantirButtons.Buttons.PlayerMagic",
    "PalantirButtons.Buttons.Objectives",
];
pub(crate) const IN_GAME: &str = "InGame";
pub(crate) const SIDE_BAR_CLIP: &str = "SideCommandBar";
pub(crate) const HERO_SELECT_CLIP: &str = "HeroSelectUI";
pub(crate) const HERO_SLOTS: usize = 16;
pub(crate) const RADIAL_SLOTS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Look {
    Normal,
    Grey,
    NotReady,
    Dim,
    ExtraDisabled,
    Passive,
    VisuallyEnabled,
}

impl Look {
    pub(crate) fn of(slot: &ResolvedSlot) -> Self {
        match slot.availability {
            Availability::Disabled => Self::Grey,
            Availability::NotReady { .. } => Self::NotReady,
            Availability::CantAfford | Availability::Held => Self::Dim,
            Availability::VisuallyEnabled => Self::VisuallyEnabled,
            Availability::ExtraDisabled => Self::ExtraDisabled,
            Availability::Enabled | Availability::Hidden
                if matches!(slot.action, SlotAction::Passive) =>
            {
                Self::Passive
            }
            Availability::Enabled | Availability::Hidden => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ImageSource {
    MappedImage(String),
    Button(String),
    MapArt(String),
    MapPicture(String),
    Black,
    Hidden,
    List(std::sync::Arc<ListSpec>),
    Combo(std::sync::Arc<ComboSpec>),
    MapWindow(std::sync::Arc<MapWindowSpec>),
    Graph(std::sync::Arc<crate::post_match::GraphSpec>),
    Check(CheckSpec),
    Slider(SliderSpec),
    Entry(std::sync::Arc<EntrySpec>),
    HelpText(std::sync::Arc<Vec<crate::help_box::HelpLineSpec>>),
    Message(std::sync::Arc<Vec<crate::help_box::HelpLineSpec>>),
    Progress(u8),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct CheckSpec {
    pub(crate) checked: bool,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct EntrySpec {
    pub(crate) text: String,
    pub(crate) focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SliderSpec {
    pub(crate) value: i32,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct MapWindowSpec {
    pub(crate) backdrop: Option<String>,
    pub(crate) art: Option<String>,
    pub(crate) extent: [i32; 4],
    pub(crate) spots: Vec<([i32; 2], Option<String>)>,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct ComboSpec {
    pub(crate) items: Vec<ComboItemSpec>,
    pub(crate) selected: Option<usize>,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct ComboItemSpec {
    pub(crate) text: String,
    pub(crate) colour: Option<[u8; 4]>,
    pub(crate) swatch: Option<[u8; 4]>,
    pub(crate) image: Option<String>,
}

impl ComboItemSpec {
    pub(crate) fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct ListSpec {
    pub(crate) columns: Vec<(u8, bool)>,
    pub(crate) rows: Vec<Vec<CellSpec>>,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll_bar: Option<bool>,
    pub(crate) text_inset_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CellSpec {
    Text(String),
    Image {
        image: Option<String>,
        size: Option<[u16; 2]>,
    },
    ScaledImage {
        image: String,
        percent: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RadialHud {
    pub(crate) entity: Entity,
    pub(crate) anchor: Option<RadialAnchor>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct HudState {
    pub(crate) gold: i64,
    pub(crate) command_points: (i32, i32),
    pub(crate) evil: bool,
    pub(crate) faction_icon: Option<String>,
    pub(crate) selected: bool,
    pub(crate) portrait: Option<String>,
    pub(crate) command_bar: CommandBar,
    pub(crate) builders: Vec<Entity>,
    pub(crate) builder_image: Option<String>,
    pub(crate) side: Option<String>,
    pub(crate) fielded_heroes: Vec<FieldedHero>,
    pub(crate) minimap: Option<String>,
    pub(crate) radial: Option<RadialHud>,
    pub(crate) template: Option<String>,
    pub(crate) spell_book: SpellBookHoldings,
    pub(crate) spell_bar: Vec<SpellBarSlot>,
    pub(crate) rank: u16,
    pub(crate) rank_progress: f32,
    pub(crate) rank_ui: Option<RankDisplay>,
    pub(crate) level_text: Option<String>,
    pub(crate) stance_flyout: Option<u8>,
    pub(crate) stance: Option<mint_sage_ini::Stance>,
    pub(crate) flash: crate::button_flash::FlashView,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpellBarSlot {
    pub(crate) button: u8,
    pub(crate) command_button: String,
    pub(crate) readiness: SpellBarReadiness,
}

const SPELL_BAR_BOOK_SLOTS: usize = 33;

const SPELL_BAR_ROWS: usize = 24;

pub(crate) fn read_hud(
    state: &MatchState,
    data: &GameData,
    units: &[UnitView],
    viewport: (u32, u32),
) -> HudState {
    let world = state.simulation.state();
    let rules = state.simulation.rules();
    let ids = rules.ids();
    let economy = ids.map(RotwkRules::economy_ids);
    let purse = economy.map(|e| crate::economy::purse_of(world, e, LOCAL_PLAYER));
    let points = economy.map(|e| crate::economy::points_of(world, e, LOCAL_PLAYER));
    let counts = ids.map(|ids| rules.cost_modifier_counts(world, ids, LOCAL_PLAYER));
    let selected_entities: Vec<Entity> = units
        .iter()
        .filter(|u| u.selected)
        .map(|u| u.entity)
        .collect();
    let cost = |target: Kind| -> Option<i64> {
        counts
            .as_ref()
            .and_then(|counts| rules.build_price_from_counts(counts, LOCAL_PLAYER, target))
    };
    let afford = |target: Kind| -> bool {
        let affordable = purse.is_none_or(|held| held.can_afford(cost(target).unwrap_or(0)));
        let producer_free = economy.is_none_or(|e| {
            selected_entities.iter().any(|&entity| {
                world.get(e.producer, entity).is_none_or(|producer| {
                    producer.queue.entries().len()
                        < usize::try_from(producer.queue.max_entries()).unwrap_or(usize::MAX)
                })
            })
        });
        affordable && producer_free
    };
    let subject = selected_entities
        .first()
        .map(|&entity| state.simulation.rules().command_subject(world, entity));
    let selected_kind = subject
        .zip(ids)
        .and_then(|(entity, ids)| world.get(ids.kind, entity).copied())
        .or_else(|| crate::game::selected_kind_of(units));
    let selected_told_kind = subject
        .and_then(|entity| units.iter().find(|unit| unit.entity == entity))
        .and_then(|unit| unit.told_kind)
        .map(Kind)
        .or_else(|| crate::game::selected_told_kind_of(units));
    let fielded = |name: &str| {
        units.iter().any(|u| {
            u.owner == LOCAL_PLAYER
                && u.kind
                    .and_then(|k| state.kinds.get(Kind(k)))
                    .is_some_and(|e| e.record.name.eq_ignore_ascii_case(name))
        })
    };
    let page =
        state
            .simulation
            .rules()
            .hero_page(world, LOCAL_PLAYER, selected_entities.first().copied());
    let heroes: Vec<Hero> = state
        .local_heroes
        .iter()
        .enumerate()
        .map(|(at, name)| {
            let record = data.templates.get(name);
            let cost = record
                .and_then(|r| r.build_cost)
                .and_then(crate::templates::whole)
                .unwrap_or(0);
            let offer = page.get(at);
            let held_image = record
                .filter(|r| r.has_kind(crate::create_a_hero::CREATE_A_HERO_KIND))
                .and_then(|_| rules.cah_plan_of(LOCAL_PLAYER))
                .and_then(|plan| plan.button_image.as_deref());
            Hero {
                name: name.clone(),
                image: crate::heroes::page_image(
                    name,
                    held_image.or(record.and_then(|r| r.button_image.as_deref())),
                    |science| {
                        ids.is_some_and(|ids| {
                            rules.holds_science(world, ids, LOCAL_PLAYER, science)
                        })
                    },
                    |image| data.mapped_images.get(image).is_some(),
                ),
                fielded: offer.map_or_else(|| fielded(name), |offer| offer.fielded),
                affordable: offer.map_or_else(
                    || purse.is_none_or(|held| held.can_afford(cost)),
                    |offer| offer.affordable,
                ),
                allowed: offer.is_none_or(|offer| offer.allowed),
            }
        })
        .collect();
    for hero in &heroes {
        crate::diag::diag!(
            "hero: page {} fielded {} affordable {} allowed {}",
            hero.name,
            hero.fielded,
            hero.affordable,
            hero.allowed
        );
    }
    let upgrades: Vec<String> = subject
        .map(|entity| {
            state
                .simulation
                .rules()
                .effective_upgrade_names(world, entity)
        })
        .unwrap_or_default();
    let object_upgrades: Vec<String> = subject
        .map(|entity| state.simulation.rules().object_upgrade_names(world, entity))
        .unwrap_or_default();
    let player_upgrades: Vec<String> = subject
        .map(|entity| state.simulation.rules().player_upgrade_names(world, entity))
        .unwrap_or_default();
    let castle_upgrades: Vec<String> = subject
        .map(|entity| {
            state
                .simulation
                .rules()
                .castle_held_upgrade_names(world, entity)
        })
        .unwrap_or_default();
    let first = subject;
    let released_powers: Vec<String> = first
        .zip(rules.upgrade_state_id())
        .and_then(|(entity, id)| world.get(id, entity))
        .map(|upgraded| {
            upgraded
                .unpaused_powers
                .iter()
                .map(|power| power.to_string())
                .collect()
        })
        .unwrap_or_default();
    let command_set = first.and_then(|entity| rules.command_set_override(world, entity));
    let sell_refusal = first
        .zip(ids)
        .and_then(|(entity, ids)| rules.sell_refusal(world, ids, LOCAL_PLAYER, entity));
    let self_repair = first.zip(ids).map(|(entity, ids)| {
        rules.self_repair(world, ids, LOCAL_PLAYER, entity, state.simulation.frame())
    });
    let gate = first
        .zip(ids)
        .and_then(|(entity, ids)| rules.gate_buttons(world, ids, entity));
    let under_construction = first
        .zip(ids)
        .is_some_and(|(entity, ids)| world.contains(ids.construction, entity));
    let cancel_refusal = first
        .zip(ids)
        .and_then(|(entity, ids)| rules.cancel_refusal(world, ids, LOCAL_PLAYER, entity));
    let researching: Vec<String> = subject
        .map(|entity| {
            state
                .simulation
                .rules()
                .upgrade_names_in_production(world, entity)
        })
        .unwrap_or_default();
    let upgrade_afford = |name: &str| -> bool {
        subject.is_none_or(|entity| rules.upgrade_affordable(world, entity, name))
    };
    let upgrade_affected = |name: &str| -> bool {
        subject.is_none_or(|entity| rules.object_upgrade_affected(world, entity, name))
    };
    let now = state.simulation.frame();
    let power_clock = |power: &str| {
        first
            .zip(ids)
            .and_then(|(entity, ids)| rules.power_clock(world, ids, entity, power, now))
    };
    let auto_cast_holder = first
        .zip(ids)
        .and_then(|(entity, ids)| rules.autocast_holder(world, ids, entity));
    let auto_cast_on = |power: &str| {
        auto_cast_holder
            .as_deref()
            .is_some_and(|held| held.eq_ignore_ascii_case(power))
    };
    let weapon_set_raised = |flags: &[String]| {
        first
            .zip(rules.upgrade_state_id())
            .and_then(|(entity, id)| world.get(id, entity))
            .is_some_and(|upgraded| flags.iter().all(|flag| upgraded.has_weapon_set_flag(flag)))
    };
    let passenger_list: Vec<mint_sim::Entity> = first
        .zip(ids)
        .and_then(|(entity, ids)| world.get(ids.passengers, entity))
        .map(|held| held.0.clone())
        .unwrap_or_default();
    let passengers: Vec<Option<String>> = passenger_list
        .iter()
        .map(|&passenger| {
            let judged = ids
                .and_then(|ids| world.get(ids.horde, passenger))
                .and_then(|horde| horde.members.first().copied())
                .unwrap_or(passenger);
            ids.and_then(|ids| world.get(ids.kind, judged))
                .and_then(|kind| state.kinds.get(*kind))
                .and_then(|entry| entry.record.button_image.clone())
        })
        .collect();
    let capacity = first
        .and_then(|entity| rules.contain_view(world, entity))
        .map(|view| view.capacity);
    let foreign_owner = first
        .zip(ids)
        .and_then(|(entity, ids)| world.get(ids.owner, entity))
        .is_some_and(|owner| owner.0 != LOCAL_PLAYER);
    let allowed_to_build = |target: Kind| -> bool {
        ids.is_none_or(|ids| {
            let is_structure = state
                .kinds
                .get(target)
                .is_some_and(|entry| entry.record.has_kind("STRUCTURE"));
            rules.allowed_to_build(world, ids, LOCAL_PLAYER, target, is_structure)
        })
    };
    let ctx = SlotContext {
        afford: &afford,
        allowed_to_build: &allowed_to_build,
        cost: &cost,
        upgrades: &upgrades,
        object_upgrades: &object_upgrades,
        player_upgrades: &player_upgrades,
        castle_upgrades: &castle_upgrades,
        researching: &researching,
        upgrade_afford: &upgrade_afford,
        upgrade_affected: &upgrade_affected,
        heroes: &heroes,
        released_powers: &released_powers,
        command_set,
        sell_refusal,
        self_repair,
        gate,
        cancel_refusal,
        under_construction,
        power_clock: &power_clock,
        auto_cast_on: &auto_cast_on,
        weapon_set_raised: &weapon_set_raised,
        passengers: &passengers,
        passenger_entities: &passenger_list,
        capacity,
        foreign_owner,
    };
    let held_set =
        command_set.and_then(|name| rules.cah_set_named(name).map(|def| (name, def.clone())));
    let held_sets = held_set.map(|(name, def)| {
        let mut index = mint_sage_ini::CommandSetIndex::new();
        index.insert(name, def);
        index
    });
    let set_name = command_set.or_else(|| {
        selected_kind
            .and_then(|kind| state.kinds.get(kind))
            .and_then(|entry| entry.record.command_set.as_deref())
    });
    let base_sets = held_sets.as_ref().unwrap_or(&data.command_sets);
    let patched_sets = set_name.and_then(|name| {
        let added: Vec<_> = state
            .scene
            .script_view
            .hud
            .added_buttons
            .iter()
            .filter(|added| added.set.eq_ignore_ascii_case(name))
            .collect();
        if added.is_empty() {
            return None;
        }
        let mut def = base_sets.get(name)?.clone();
        for added in added {
            def.slots.insert(added.slot, added.button.clone());
        }
        let mut index = mint_sage_ini::CommandSetIndex::new();
        index.insert(name, def);
        Some(index)
    });
    let mut command_bar = resolve_command_bar(
        &state.kinds,
        patched_sets.as_ref().unwrap_or(base_sets),
        &data.command_buttons,
        selected_kind,
        &ctx,
    );
    if let Some(set_name) = set_name {
        let removed: Vec<&str> = state
            .scene
            .script_view
            .hud
            .removed_buttons
            .iter()
            .filter(|(set, _)| set == set_name)
            .map(|(_, button)| button.as_str())
            .collect();
        crate::command_bar::remove_buttons(&mut command_bar, &removed);
    }
    let stance = ids.and_then(|ids| {
        selected_entities
            .iter()
            .find_map(|&entity| rules.stance_of(world, ids, entity))
    });
    if let Some(stance) = stance {
        crate::command_bar::show_stance_face(&mut command_bar, &data.command_buttons, stance);
    }
    if let Some(structure) = first {
        fill_progress(&mut command_bar, |slot| {
            let def = data.command_buttons.get(&slot.command_button);
            let count = if shows_production_count(def) {
                production_count(rules, world, structure, &slot.action)
            } else {
                0
            };
            (
                production_progress(rules, world, structure, &slot.action),
                count,
            )
        });
    }
    let radial = radial_structure(state, units, viewport);
    let local = |u: &&UnitView| u.owner == LOCAL_PLAYER;
    let record_of = |u: &UnitView| {
        u.kind
            .and_then(|k| state.kinds.get(Kind(k)))
            .map(|e| &e.record)
    };
    let builder_image = units
        .iter()
        .filter(local)
        .filter(|u| u.kind_of.contains(mint_sage_asset::KindOf::DOZER))
        .find_map(|u| record_of(u).and_then(|r| r.button_image.clone()));
    let mut fielded: Vec<(i32, FieldedHero)> = units
        .iter()
        .filter(local)
        .filter(|u| !u.dying)
        .filter_map(|u| {
            let record = record_of(u)?;
            let order = hero_row_order(u.kind_of, record)?;
            let hero = FieldedHero {
                entity: u.entity,
                template: record.name.clone(),
                image: state_image(state, u.entity, |s| &s.button_image)
                    .or_else(|| record.button_image.clone()),
                rank: rules.rank_of(world, u.entity).unwrap_or(1),
                health: u.health.filter(|f| f.is_finite()).unwrap_or(1.0),
                hit_points: ids
                    .and_then(|ids| world.get(ids.health, u.entity))
                    .map_or(0.0, |health| health.0.current()),
                selected: u.selected,
                rank_progress: rules
                    .level_readout(world, u.entity)
                    .map_or(1.0, |shown| shown.progress.unwrap_or(1.0)),
            };
            Some((order, hero))
        })
        .collect();
    fielded.sort_by_key(|(order, _)| *order);
    let fielded_heroes: Vec<FieldedHero> = fielded.into_iter().map(|(_, hero)| hero).collect();
    let spell_bar: Vec<SpellBarSlot> = (0..SPELL_BAR_BOOK_SLOTS)
        .filter_map(|button| {
            #[allow(clippy::cast_possible_truncation)]
            let button = button as u8;
            let (readiness, command_button) =
                rules.spell_bar_state(world, LOCAL_PLAYER, button, now)?;
            Some(SpellBarSlot {
                button,
                command_button,
                readiness,
            })
        })
        .take(SPELL_BAR_ROWS)
        .collect();
    let book = rules.spell_book(world, LOCAL_PLAYER);
    let rank_ui = rules.rank_display(world, &selected_entities, now);
    let level_text = rank_ui.map(|shown| match shown {
        RankDisplay::Level(level) => state.strings.get(RANK_LABEL).map_or_else(
            || level.rank.to_string(),
            |label| format_placeholder(label, Placeholder::Int(i64::from(level.rank))),
        ),
        RankDisplay::TimeRemaining(_) => state
            .strings
            .get(TIME_REMAINING_LABEL)
            .map(ToString::to_string)
            .unwrap_or_default(),
    });
    let stance_flyout = state.stance_flyout.filter(|&position| {
        !command_bar.stance_flyout.is_empty()
            && command_bar
                .ring
                .get(usize::from(position))
                .and_then(Option::as_ref)
                .is_some_and(|slot| matches!(slot.action, SlotAction::ToggleStance(_)))
    });
    HudState {
        gold: purse.map_or(0, mint_sage_economy::Money::amount),
        command_points: points.map_or((0, 0), |p| (p.used(), p.limit())),
        evil: state.local_evil,
        faction_icon: state
            .local_faction_icon
            .clone()
            .filter(|name| data.mapped_images.get(name).is_some()),
        selected: selected_kind.is_some(),
        portrait: subject
            .and_then(|entity| state_image(state, entity, |s| &s.portrait_image))
            .or_else(|| {
                selected_told_kind
                    .and_then(|kind| state.kinds.get(kind))
                    .and_then(|entry| entry.record.select_portrait.clone())
            }),
        command_bar,
        builders: ids.map_or_else(Vec::new, |ids| available_builders(world, ids, LOCAL_PLAYER)),
        builder_image,
        side: state.local_side.clone(),
        fielded_heroes,
        minimap: minimap_image_path(&state.scene.map_path),
        radial,
        template: state.local_template.clone(),
        spell_book: crate::spell_book::holdings_of(book.as_ref()),
        spell_bar,
        rank: book.as_ref().map_or(1, |b| b.rank),
        rank_progress: rules.rank_progress(world, LOCAL_PLAYER),
        rank_ui,
        level_text,
        stance_flyout,
        stance,
        flash: crate::button_flash::FlashView::default(),
    }
}

fn state_image(
    state: &MatchState,
    entity: Entity,
    image: impl Fn(&mint_sage_ini::ModelStateDef) -> &Option<String>,
) -> Option<String> {
    state
        .units
        .active_states(entity.index())
        .find_map(|active| image(active).clone())
}

fn is_radial_subject(unit: &UnitView) -> bool {
    unit.owner == LOCAL_PLAYER && unit.kind_of.contains(mint_sage_asset::KindOf::STRUCTURE)
}

fn radial_structure(
    state: &MatchState,
    units: &[UnitView],
    viewport: (u32, u32),
) -> Option<RadialHud> {
    let mut selected = units.iter().filter(|u| u.selected);
    let (Some(unit), None) = (selected.next(), selected.next()) else {
        return None;
    };
    if !is_radial_subject(unit) {
        return None;
    }
    let record = unit
        .kind
        .and_then(|k| state.kinds.get(Kind(k)))
        .map(|e| &e.record);
    let anchor = crate::radial::anchor(
        &state.camera.camera(),
        viewport,
        [unit.position.x, unit.position.y],
        unit.ground,
        unit.facing,
        unit.height,
        record
            .and_then(|r| r.geometry.major_radius)
            .unwrap_or(unit.radius),
        record.and_then(|r| r.menu_point),
    );
    Some(RadialHud {
        entity: unit.entity,
        anchor,
    })
}

pub(crate) fn minimap_image_path(map_path: &str) -> Option<String> {
    map_path
        .strip_suffix(".map")
        .map(|stem| format!("{stem}_art.tga"))
}

pub(crate) fn minimap_image_fallback(art_path: &str) -> Option<String> {
    art_path
        .strip_suffix("_art.tga")
        .map(|stem| format!("{stem}.tga"))
}

pub(crate) fn available_builders(world: &World, ids: Ids, player: PlayerId) -> Vec<Entity> {
    let Some(owners) = world.store(ids.owner) else {
        return Vec::new();
    };
    owners
        .iter()
        .filter(|(entity, owner)| {
            world.is_live(*entity)
                && world.get(ids.shape, *entity).is_some_and(|shape| {
                    builder_available(
                        owner.0 == player,
                        world.get(ids.build_task, *entity).is_some(),
                        world.get(ids.concealed, *entity).is_some(),
                        shape.kind_of,
                    )
                })
        })
        .map(|(entity, _)| entity)
        .collect()
}

pub(crate) fn builder_available(
    owned: bool,
    has_build_task: bool,
    concealed: bool,
    kind_of: mint_sage_asset::KindOf,
) -> bool {
    owned
        && !has_build_task
        && !concealed
        && kind_of.contains(mint_sage_asset::KindOf::DOZER)
        && kind_of.contains(mint_sage_asset::KindOf::SELECTABLE)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct BuilderCycle {
    origin: [f32; 2],
    visited: Vec<Entity>,
    last: Option<std::time::Duration>,
    mark: u64,
}

impl BuilderCycle {
    pub(crate) fn click(
        &mut self,
        available: &[(Entity, [f32; 2])],
        here: [f32; 2],
        now: std::time::Duration,
        mark: u64,
        timeout: std::time::Duration,
    ) -> Option<Entity> {
        let fresh = self
            .last
            .is_none_or(|last| mark != self.mark || now.saturating_sub(last) > timeout);
        if fresh {
            self.origin = here;
            self.visited.clear();
        }
        let [ox, oy] = self.origin;
        let distance = |p: [f32; 2]| (p[0] - ox) * (p[0] - ox) + (p[1] - oy) * (p[1] - oy);
        let nearest = |skip: &[Entity]| {
            available
                .iter()
                .filter(|(e, _)| !skip.contains(e))
                .min_by(|a, b| {
                    distance(a.1)
                        .total_cmp(&distance(b.1))
                        .then(a.0.index().cmp(&b.0.index()))
                })
                .map(|(e, _)| *e)
        };
        let chosen = match nearest(&self.visited) {
            Some(e) => e,
            None => {
                self.visited.clear();
                nearest(&[])?
            }
        };
        self.visited.push(chosen);
        self.last = Some(now);
        Some(chosen)
    }

    pub(crate) fn settle(&mut self, mark: u64) {
        self.mark = mark;
    }
}

pub(crate) fn on_screen(pixel: Option<(f32, f32, f32)>, viewport: (u32, u32)) -> bool {
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (viewport.0 as f32, viewport.1 as f32);
    pixel.is_some_and(|(x, y, _)| x >= 0.0 && x < w && y >= 0.0 && y < h)
}

pub(crate) const COMMAND_BUTTON: &str = "CommandButton";

fn fill_progress(bar: &mut CommandBar, reading: impl Fn(&ResolvedSlot) -> (Option<u8>, u16)) {
    let slots = bar
        .ring
        .iter_mut()
        .flatten()
        .chain(bar.bar.iter_mut())
        .chain(bar.radial.iter_mut().map(|(_, slot)| slot));
    for slot in slots {
        let (progress, count) = reading(slot);
        if progress.is_some() {
            slot.progress = progress;
        }
        slot.count = count;
    }
}

fn shows_production_count(def: Option<&mint_sage_ini::CommandButtonDef>) -> bool {
    def.is_some_and(|def| def.show_production_count)
}

pub(crate) fn production_count(
    rules: &RotwkRules,
    world: &World,
    structure: Entity,
    action: &SlotAction,
) -> u16 {
    let queued = match *action {
        SlotAction::Build(button) => rules.build_count(world, structure, button),
        _ => 0,
    };
    u16::try_from(queued).unwrap_or(u16::MAX)
}

fn production_progress(
    rules: &RotwkRules,
    world: &World,
    structure: Entity,
    action: &SlotAction,
) -> Option<u8> {
    let fraction = match *action {
        SlotAction::Build(button) => rules.build_progress(world, structure, button),
        SlotAction::ObjectUpgrade(button) => {
            rules.object_upgrade_progress(world, structure, button)
        }
        SlotAction::PlayerUpgrade(button) => {
            rules.player_upgrade_progress(world, structure, button)
        }
        SlotAction::Revive(button) => rules.hero_progress(world, structure, button),
        _ => None,
    }?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((fraction.clamp(0.0, 1.0) * 100.0).round() as u8)
}

pub(crate) fn button_state(slot: &ResolvedSlot, hovered: bool) -> &'static str {
    match Look::of(slot) {
        Look::Grey => "_disabled",
        Look::NotReady => "_notReady",
        Look::Dim => "_cantAfford",
        Look::ExtraDisabled => "_extraDisabled",
        Look::Passive => "_static",
        Look::VisuallyEnabled => "_visuallyEnabled",
        Look::Normal if hovered => "_over",
        Look::Normal => "_up",
    }
}

fn command_slot(
    ex: &mut Exposure<ImageSource>,
    frame: &str,
    print: &str,
    slot: Option<&ResolvedSlot>,
    hovered: bool,
    flash: &FlashView,
) {
    let content = format!("{frame}.{COMMAND_BUTTON}");
    let key = format!("{print}.{COMMAND_BUTTON}");
    if let Some(on) = flash.cameo(slot.map_or("", |slot| slot.command_button.as_str())) {
        ex.call(
            &format!("flash {frame}"),
            &format!("{frame}.SetFlashEffectState"),
            vec![Value::str(flash_state(on))],
        );
    }
    match slot {
        Some(slot) => {
            ex.call_on(
                &format!("content {frame}"),
                frame,
                &format!("{frame}.CreateContent"),
                vec![Value::str(COMMAND_BUTTON), Value::str(COMMAND_BUTTON)],
            );
            ex.call_on(
                &format!("state {frame}"),
                &content,
                &format!("{content}.SetState"),
                vec![Value::str(button_state(slot, hovered))],
            );
            let source = match &slot.image {
                Some(image) => ImageSource::MappedImage(image.clone()),
                None => ImageSource::Button(slot.command_button.clone()),
            };
            ex.image(ImageKey::Name(format!("{key}_Image")), Some(source));
            let timer = slot
                .progress
                .map_or(ImageSource::Hidden, ImageSource::Progress);
            ex.image(ImageKey::Name(format!("{key}_Timer")), Some(timer));
        }
        None => {
            ex.call_on(
                &format!("content {frame}"),
                frame,
                &format!("{frame}.DeleteContent"),
                Vec::new(),
            );
            ex.image(
                ImageKey::Name(format!("{key}_Image")),
                Some(ImageSource::Hidden),
            );
            ex.image(
                ImageKey::Name(format!("{key}_Timer")),
                Some(ImageSource::Hidden),
            );
        }
    }
    let count = slot
        .map(|slot| slot.count)
        .filter(|count| *count > 0)
        .map_or_else(String::new, |count| count.to_string());
    ex.string(&format!("{key}_ProductionCount"), Some(count));
}

fn flash_state(on: bool) -> &'static str {
    if on {
        "_show"
    } else {
        "_hide"
    }
}

pub(crate) fn printed(path: &str) -> String {
    format!("_level0.{path}")
}

pub(crate) const GLASS_ACTIVE: &str = "_active";

pub(crate) const GLASS_PASSIVE: &str = "_passive";

pub(crate) fn glass_state(slot: Option<&ResolvedSlot>) -> &'static str {
    match slot.map(Look::of) {
        Some(Look::Passive) => GLASS_PASSIVE,
        _ => GLASS_ACTIVE,
    }
}

pub(crate) const FLYOUT_SLOTS: usize = 4;

pub(crate) fn flyout_frame(position: u8, button: usize) -> String {
    format!("CommandButtons.subMenu{position}.trays.{button}.rot.bttnFrame")
}

pub(crate) fn declare_palantir(ex: &mut Exposure<ImageSource>, hud: &HudState) {
    ex.global(IN_GAME, Value::Bool(true));
    ex.set_extern(IN_GAME, Value::Bool(true));
    let state = if hud.evil { "_evil" } else { "_good" };
    ex.call(
        "frame_state",
        "SetPalantirFrameState",
        vec![Value::str(state)],
    );
    let side = if hud.evil { "_ring" } else { "_evenstar" };
    for clip in SIDE_CLIPS {
        ex.call_on(
            &format!("side {clip}"),
            clip,
            &format!("{clip}.gotoAndPlay"),
            vec![Value::str(side)],
        );
    }
    let command_ui = if hud.selected {
        "ShowCommandInterface"
    } else {
        "HideCommandInterface"
    };
    ex.call("command_ui", command_ui, Vec::new());
    ex.call_on(
        "player_magic_progress",
        "PalantirButtons.Buttons.PlayerMagic.ProgressBar",
        "SetPlayerMagicProgress",
        vec![percent(hud.rank_progress)],
    );
    ex.string("PlayerRank", Some(hud.spell_book.points.to_string()));
    if let Some(on) = hud.flash.player_magic {
        ex.call(
            "player_magic_flash",
            "SetPlayerMagicButtonFlashEffectState",
            vec![Value::str(flash_state(on))],
        );
    }
    let interface = match hud.rank_ui {
        Some(_) => "ShowRankInterface",
        None => "HideRankInterface",
    };
    ex.call_on("rank_interface", "CommandUI.RankUI", interface, Vec::new());
    if let Some(progress) = hud.rank_ui.map(RankDisplay::bar) {
        let shown = match progress {
            Some(_) => "ShowRankProgress",
            None => "HideRankProgress",
        };
        ex.call_on(
            "rank_progress",
            "CommandUI.RankUI.Progress",
            shown,
            Vec::new(),
        );
        if let Some(fraction) = progress {
            ex.call_on(
                "rank_progress_bar",
                "CommandUI.RankUI.Progress.Bar",
                "SetRankProgressBar",
                vec![Value::Number(f64::from(rank_bar_frame(fraction)))],
            );
        }
    }
    ex.string("HeroRank", hud.level_text.clone());
    for (p, slot) in hud.command_bar.ring.iter().enumerate().take(RING_SLOTS) {
        ex.call(
            &format!("glass{p}"),
            &format!("CommandButtons.{p}.SetGlassState"),
            vec![Value::str(glass_state(slot.as_ref()))],
        );
        let frame = format!("CommandButtons.{p}");
        command_slot(
            ex,
            &frame,
            &printed(&frame),
            slot.as_ref(),
            false,
            &hud.flash,
        );
        let auto_cast_state = if slot.as_ref().is_some_and(|slot| slot.auto_cast) {
            "_show"
        } else {
            "_hide"
        };
        ex.call_on(
            &format!("auto{p}"),
            &format!("AutoAbilityOverlays.{p}"),
            &format!("CommandButtons.{p}.SetAutoAbilityOverlayState"),
            vec![Value::str(auto_cast_state)],
        );
    }
    for position in 0..FLYOUT_SLOTS {
        let open = hud
            .stance_flyout
            .is_some_and(|at| usize::from(at) == position)
            .then_some(hud.command_bar.stance_flyout.as_slice())
            .filter(|buttons| !buttons.is_empty());
        let position = u8::try_from(position).unwrap_or(u8::MAX);
        let clip = format!("CommandButtons.subMenu{position}");
        match open {
            Some(buttons) => {
                ex.call_on(
                    &format!("flyout {position}"),
                    &clip,
                    &format!("{clip}.Show"),
                    vec![Value::Number(f64::from(
                        u32::try_from(buttons.len()).unwrap_or(u32::MAX),
                    ))],
                );
                for (button, slot) in buttons.iter().enumerate() {
                    let frame = flyout_frame(position, button);
                    command_slot(ex, &frame, &printed(&frame), Some(slot), false, &hud.flash);
                }
            }
            None => ex.call_on(
                &format!("flyout {position}"),
                &clip,
                &format!("{clip}.Hide"),
                Vec::new(),
            ),
        }
    }
    let (used, limit) = hud.command_points;
    ex.string("PalantirResources", Some(hud.gold.max(0).to_string()));
    ex.string("PalantirResourceMultiplier", Some(String::new()));
    ex.string(
        "PalantirCommandPoints",
        Some(format!("{}/{}", used.max(0), limit.max(0))),
    );
    ex.image(
        ImageKey::Name("RenderFactionIcon".to_owned()),
        Some(
            hud.faction_icon
                .clone()
                .map_or(ImageSource::Hidden, ImageSource::MappedImage),
        ),
    );
    ex.call(
        "resource_icon",
        "SetResourceIconState",
        vec![Value::str("_show")],
    );
    ex.image(
        ImageKey::Path("ResourceBar.ResourceIcon.~Enabled".to_owned()),
        Some(ImageSource::MappedImage(RESOURCE_ICON.to_owned())),
    );
    ex.image(
        ImageKey::Name("AptPalantir::RenderRadar".to_owned()),
        hud.minimap.clone().map(ImageSource::MapArt),
    );
    for hidden in [
        "AptPalantir::ClipRadar",
        "AptPalantir::RenderRadarViewBox",
        "AptPalantir::RenderMovie",
    ] {
        ex.image(ImageKey::Name(hidden.to_owned()), Some(ImageSource::Hidden));
    }
    ex.image(
        ImageKey::Path("CommandUI.Portrait".to_owned()),
        Some(
            hud.portrait
                .clone()
                .map_or(ImageSource::Black, ImageSource::MappedImage),
        ),
    );
}

pub(crate) fn rank_bar_frame(fraction: f32) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let percent = (fraction.clamp(0.0, 1.0) * 100.0) as i32;
    percent.saturating_add(1).clamp(1, 100)
}

fn percent(fraction: f32) -> Value {
    if fraction >= 1.0 {
        return Value::Number(1.0);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let value = (fraction.clamp(0.0, 1.0) * 100.0) as i32;
    Value::Number(f64::from(value.clamp(1, 100)))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SideBarContent {
    shown: Vec<ResolvedSlot>,
}

impl SideBarContent {
    pub(crate) fn update(&mut self, bar: &[ResolvedSlot]) {
        if !bar.is_empty() {
            bar.clone_into(&mut self.shown);
        }
    }

    pub(crate) fn on_event(&mut self, command: &str, bar: &[ResolvedSlot]) {
        if command == COMMAND_BAR_FADED_OUT && bar.is_empty() {
            self.shown.clear();
        }
    }

    pub(crate) fn shown(&self) -> &[ResolvedSlot] {
        &self.shown
    }
}

pub(crate) fn under(root: &str, path: &str) -> String {
    if root.is_empty() {
        path.to_owned()
    } else {
        format!("{root}.{path}")
    }
}

pub(crate) fn declare_command_bar(
    ex: &mut Exposure<ImageSource>,
    hud: &HudState,
    content: &SideBarContent,
    root: &str,
) {
    ex.global(IN_GAME, Value::Bool(true));
    let fade = if hud.command_bar.bar.is_empty() {
        "FadeOut"
    } else {
        "FadeIn"
    };
    ex.call(&under(root, "fade"), &under(root, fade), Vec::new());
    let bar = content.shown();
    for i in 0..BAR_SLOTS {
        let slot = bar.get(i);
        #[allow(clippy::cast_precision_loss)]
        let index = Value::Number(i as f64);
        let state = Value::str(if slot.is_some() { "_show" } else { "_hide" });
        ex.call(
            &under(root, &format!("slot{i}")),
            &under(root, "SetButtonState"),
            vec![index, state],
        );
        let frame = under(root, &format!("ButtonSet.Button{i}.Button"));
        command_slot(ex, &frame, &printed(&frame), slot, false, &hud.flash);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FieldedHero {
    pub(crate) entity: Entity,
    pub(crate) template: String,
    pub(crate) image: Option<String>,
    pub(crate) rank: u16,
    pub(crate) health: f32,
    pub(crate) hit_points: f32,
    pub(crate) selected: bool,
    pub(crate) rank_progress: f32,
}

pub(crate) fn declare_hero_select(
    ex: &mut Exposure<ImageSource>,
    hud: &HudState,
    root: &str,
    root_path: &str,
) {
    ex.call(&under(root, "show"), &under(root, "Show"), Vec::new());
    if let Some(side) = &hud.side {
        ex.call(
            &under(root, "faction"),
            &under(root, "SetFaction"),
            vec![Value::str(side)],
        );
    }
    let crystal = if hud.fielded_heroes.is_empty() {
        "_unused"
    } else {
        "_up"
    };
    ex.call_on(
        &under(root, "all"),
        &under(root, "SelectAllHeroesBttn"),
        &under(root, "SetSelectAllHeroesButtonState"),
        vec![Value::str(crystal)],
    );
    for n in 1..=HERO_SLOTS {
        let (state, image, rank, health) = hero_slot(hud, n);
        let hero = hero_at_slot(hud, n);
        #[allow(clippy::cast_precision_loss)]
        let index = Value::Number(n as f64);
        ex.call_on(
            &under(root, &format!("hero{n}")),
            &under(root, &format!("Hero{n}")),
            &under(root, "SetButtonState"),
            vec![index.clone(), Value::str(state)],
        );
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let health_percent = (health.clamp(0.0, 1.0) * 100.0).round() as i32;
        ex.call_on(
            &under(root, &format!("health{n}")),
            &under(root, &format!("Hero{n}.HealthBar")),
            &under(root, "SetButtonHealthBar"),
            vec![index.clone(), Value::Number(f64::from(health_percent))],
        );
        let builder = if n == 1 { hud.flash.builder } else { None };
        let flashing = match (builder, hud.flash.hero(hero.map(|hero| hero.entity))) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(false) || b.unwrap_or(false)),
        };
        if let Some(on) = flashing {
            ex.call_on(
                &under(root, &format!("flash{n}")),
                &under(root, &format!("FlashEffect{n}")),
                &under(root, "SetButtonFlashEffectState"),
                vec![index.clone(), Value::str(flash_state(on))],
            );
        }
        if let Some(hero) = hero {
            let highlight = if hero.selected { "_show" } else { "_hide" };
            ex.call_on(
                &under(root, &format!("selected{n}")),
                &under(root, &format!("Hero{n}.SelectedHighlight")),
                &under(root, "SetButtonSelectedHighlightState"),
                vec![index.clone(), Value::str(highlight)],
            );
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let progress = (hero.rank_progress.clamp(0.0, 1.0) * 100.0).round() as i32;
            ex.call_on(
                &under(root, &format!("progress{n}")),
                &under(root, &format!("Hero{n}.RankProgress")),
                &under(root, "SetButtonRankProgress"),
                vec![index, Value::Number(f64::from(progress))],
            );
        }
        ex.image(
            ImageKey::Name(format!("{root_path}_Hero{n}Image")),
            image.map(ImageSource::MappedImage),
        );
        ex.string(&format!("{root_path}_Hero{n}Rank"), rank);
    }
}

pub(crate) const SPELL_BOOK_CLIP: &str = "SpellBookUI";

pub(crate) fn declare_spell_row(ex: &mut Exposure<ImageSource>, hud: &HudState, root: &str) {
    ex.call(
        &under(root, "show"),
        &under(root, "gotoAndPlay"),
        vec![Value::str("_show")],
    );
    for n in 1..=SPELL_BAR_ROWS {
        let slot = hud.spell_bar.get(n - 1);
        let (state, percent) = match slot.map(|s| s.readiness) {
            Some(SpellBarReadiness::Ready) => ("_up", None),
            Some(SpellBarReadiness::OnCooldown { percent }) => ("_notReady", Some(percent)),
            Some(SpellBarReadiness::Static) => ("_static", None),
            Some(SpellBarReadiness::Disabled) => ("_disabled", None),
            None => ("_unused", None),
        };
        #[allow(clippy::cast_precision_loss)]
        let index = Value::Number(n as f64);
        let clip = format!("Spell{n}");
        ex.call_on(
            &under(root, &format!("spell{n}")),
            &under(root, &clip),
            &under(root, "SetButtonState"),
            vec![index.clone(), Value::str(state)],
        );
        if let Some(on) = hud
            .flash
            .cameo(slot.map_or("", |slot| slot.command_button.as_str()))
        {
            ex.call_on(
                &under(root, &format!("flash{n}")),
                &under(root, &format!("{clip}.FlashEffect")),
                &under(root, "SetButtonFlashEffectState"),
                vec![index, Value::str(flash_state(on))],
            );
        }
        let image = slot.map(|s| ImageSource::Button(s.command_button.clone()));
        ex.image(ImageKey::Name(format!("InGameSpellBook{clip}Image")), image);
        let timer = percent.map_or(ImageSource::Hidden, ImageSource::Progress);
        ex.image(
            ImageKey::Name(format!("InGameSpellBook{clip}Timer")),
            Some(timer),
        );
    }
}

pub(crate) const SPELL_BAR_PRESSED: &str = "OnAptInGameSpellBookButtonPressed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeroButton {
    Slot(usize),
    All,
}

pub(crate) fn hero_button(command: &str) -> Option<HeroButton> {
    if command.ends_with("_OnBttnSelectAllHeroes") {
        return Some(HeroButton::All);
    }
    let clip = command.strip_suffix("_OnBttnHeroSelect")?;
    let name = clip.rsplit('.').next()?;
    let n = name
        .get(..4)
        .filter(|p| p.eq_ignore_ascii_case("hero"))
        .and_then(|_| name.get(4..))?
        .parse::<usize>()
        .ok()?;
    (1..=HERO_SLOTS).contains(&n).then_some(HeroButton::Slot(n))
}

pub(crate) const HERO_DOUBLE_CLICK_MS: u32 = 500;

pub(crate) fn hero_double_click(
    last: Option<(HeroButton, std::time::Duration)>,
    target: HeroButton,
    now: std::time::Duration,
    window_ms: u32,
) -> bool {
    last.is_some_and(|(button, at)| {
        button == target
            && now.saturating_sub(at) <= std::time::Duration::from_millis(u64::from(window_ms))
    })
}

pub(crate) fn hero_slot(
    hud: &HudState,
    n: usize,
) -> (&'static str, Option<String>, Option<String>, f32) {
    if n == 1 && !hud.builders.is_empty() {
        return (
            "_up",
            hud.builder_image.clone(),
            Some(hud.builders.len().to_string()),
            1.0,
        );
    }
    match hero_at_slot(hud, n) {
        Some(hero) => (
            "_up",
            hero.image.clone(),
            Some(hero.rank.to_string()),
            hero.health,
        ),
        None => ("_unused", None, None, 1.0),
    }
}

pub(crate) fn hero_row_order(
    kind_of: mint_sage_asset::KindOf,
    record: &mint_sage_modules::ObjectRecord,
) -> Option<i32> {
    (kind_of.contains(mint_sage_asset::KindOf::HERO) && record.button_image.is_some())
        .then(|| record.hero_sort_order.unwrap_or(i32::MAX))
}

pub(crate) fn hero_at_slot(hud: &HudState, n: usize) -> Option<&FieldedHero> {
    let first_hero_slot = if hud.builders.is_empty() { 1 } else { 2 };
    hud.fielded_heroes.get(n.checked_sub(first_hero_slot)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeroEffect {
    Attacked(usize),
    LevelUp(usize),
    Kill(usize),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct HeroRowMemory {
    slots: Vec<Option<(Entity, u16, f32)>>,
}

impl HeroRowMemory {
    pub(crate) fn effects(&mut self, hud: &HudState) -> Vec<HeroEffect> {
        let mut effects = Vec::new();
        let now: Vec<Option<(Entity, u16, f32)>> = (1..=HERO_SLOTS)
            .map(|n| hero_at_slot(hud, n).map(|hero| (hero.entity, hero.rank, hero.hit_points)))
            .collect();
        for (at, shown) in now.iter().enumerate() {
            let n = at.saturating_add(1);
            let before = self.slots.get(at).copied().flatten();
            match (before, *shown) {
                (Some((was, rank_was, hp_was)), Some((hero, rank, hp))) if was == hero => {
                    if hp < hp_was {
                        effects.push(HeroEffect::Attacked(n));
                    }
                    if rank > rank_was {
                        effects.push(HeroEffect::LevelUp(n));
                    }
                }
                (Some(_), _) => effects.push(HeroEffect::Kill(n)),
                (None, _) => {}
            }
        }
        self.slots = now;
        effects
    }
}

pub(crate) fn declare_hero_effects(
    ex: &mut Exposure<ImageSource>,
    root: &str,
    effects: &[HeroEffect],
) {
    if !ex.landed(&under(root, "show")) {
        return;
    }
    for effect in effects {
        let (function, n) = match *effect {
            HeroEffect::Attacked(n) => ("PlayButtonAttackedEffect", n),
            HeroEffect::LevelUp(n) => ("PlayButtonLevelUpEffect", n),
            HeroEffect::Kill(n) => ("KillButtonEffects", n),
        };
        #[allow(clippy::cast_precision_loss)]
        let index = Value::Number(n as f64);
        ex.fire(&under(root, function), vec![index]);
    }
}

pub(crate) fn declare_radial(
    ex: &mut Exposure<ImageSource>,
    buttons: &[&ResolvedSlot],
    rects: &[[f32; 4]],
    hovered: Option<usize>,
    flash: &FlashView,
) {
    ex.global(IN_GAME, Value::Bool(true));
    for id in 0..RADIAL_SLOTS {
        #[allow(clippy::cast_precision_loss)]
        let number = Value::Number(id as f64);
        let life = format!("life{id:02}");
        let frame = format!("{id}.bttnFrame");
        match (buttons.get(id), rects.get(id)) {
            (Some(button), Some(&[x, y, w, h])) => {
                ex.call(&life, "CreateButton", vec![number.clone()]);
                ex.call_on(
                    &format!("move{id:02}"),
                    &id.to_string(),
                    "MoveButton",
                    vec![
                        number,
                        Value::Number(f64::from(x)),
                        Value::Number(f64::from(y)),
                        Value::Number(f64::from(w)),
                        Value::Number(f64::from(h)),
                    ],
                );
                let state = if !button.enabled {
                    "_disabled"
                } else if hovered == Some(id) {
                    "_over"
                } else {
                    "_up"
                };
                ex.call_on(
                    &format!("state{id:02}"),
                    &frame,
                    &format!("{frame}.GotoState"),
                    vec![Value::str(state)],
                );
                command_slot(
                    ex,
                    &frame,
                    &printed(&frame),
                    Some(button),
                    hovered == Some(id),
                    flash,
                );
            }
            _ => {
                ex.call(&life, "DestroyButton", vec![number]);
                command_slot(ex, &frame, &printed(&frame), None, false, flash);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HelpBoxState {
    Sample,
    Hidden,
    Shown {
        height: f32,
        lines: std::sync::Arc<Vec<crate::help_box::HelpLineSpec>>,
    },
}

pub(crate) fn declare_help_box(
    ex: &mut Exposure<ImageSource>,
    content_key: &str,
    state: &HelpBoxState,
) {
    let (function, args, content) = match state {
        HelpBoxState::Sample => (
            "helpBox.SampleContentWidth",
            Vec::new(),
            ImageSource::Hidden,
        ),
        HelpBoxState::Hidden => ("helpBox.Hide", Vec::new(), ImageSource::Hidden),
        HelpBoxState::Shown { height, lines } => (
            "helpBox.Show",
            vec![Value::Number(f64::from(*height))],
            ImageSource::HelpText(std::sync::Arc::clone(lines)),
        ),
    };
    ex.call("help_box", function, args);
    ex.image(ImageKey::Name(content_key.to_owned()), Some(content));
}

const PLAYER_MAGIC_BUTTON: &str = "PalantirButtons.Buttons.PlayerMagic.ButtonClip";

pub(crate) fn declare_player_magic(ex: &mut Exposure<ImageSource>, enabled: bool) {
    ex.call_on(
        "player_magic",
        PLAYER_MAGIC_BUTTON,
        "EnablePlayerMagicButton",
        vec![Value::str(if enabled { "1" } else { "0" })],
    );
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PalantirMemory {
    frame_evil: Option<bool>,
    rank: Option<u16>,
    holdings: Option<SpellBookHoldings>,
    purchasable: Vec<bool>,
    highlighted: bool,
}

impl PalantirMemory {
    pub(crate) fn update(&mut self, hud: &HudState, store_open: bool, enabled: bool) -> bool {
        if self.frame_evil != Some(hud.evil) {
            self.frame_evil = Some(hud.evil);
            self.rank = None;
        }
        let level_up = self.rank.is_some_and(|was| hud.rank > was);
        self.rank = Some(hud.rank);
        if self.holdings.as_ref() != Some(&hud.spell_book) {
            let now: Vec<bool> = hud
                .spell_book
                .states
                .iter()
                .map(|(_, state)| *state == SpellState::Available)
                .collect();
            let newly_purchasable = now
                .iter()
                .enumerate()
                .any(|(i, &p)| p && !self.purchasable.get(i).copied().unwrap_or(false));
            if newly_purchasable {
                self.highlighted = true;
            }
            self.purchasable = now;
            self.holdings = Some(hud.spell_book.clone());
        }
        if store_open || (enabled && !self.purchasable.iter().any(|&p| p)) {
            self.highlighted = false;
        }
        level_up
    }

    pub(crate) fn highlighted(&self) -> bool {
        self.highlighted
    }
}

pub(crate) fn declare_palantir_rank_feedback(
    ex: &mut Exposure<ImageSource>,
    level_up: bool,
    highlighted: bool,
) {
    if level_up {
        println!("palantir: level up");
        ex.fire("PlayPlayerLevelUpEffect", Vec::new());
    }
    ex.call_on(
        "player_magic_highlight",
        PLAYER_MAGIC_BUTTON,
        "HighlightPlayerMagicButton",
        vec![Value::str(if highlighted { "1" } else { "0" })],
    );
}

pub(crate) fn provisional_holdings(
    state: &MatchState,
    provisional: &[String],
) -> SpellBookHoldings {
    let book = state.simulation.rules().provisional_spell_book(
        state.simulation.state(),
        LOCAL_PLAYER,
        provisional,
    );
    crate::spell_book::holdings_of(book.as_ref())
}

pub(crate) const SPELL_STORE_READY: &str = "OnInitialized";
pub(crate) const SPELL_STORE_SLOTS: usize = 12;
const SPELL_STORE_INPUT: &str = "AptSpellStore::InputEnabled";
const SPELL_STORE_LAYOUT: &str = "_multiplayer";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SpellStoreSession {
    pub(crate) owned_at_open: Vec<String>,
    pub(crate) hovered: Option<usize>,
    pub(crate) closing: bool,
    pub(crate) provisional: Vec<String>,
}

impl SpellStoreSession {
    pub(crate) fn open(view: &SpellBookView) -> Self {
        Self {
            owned_at_open: view
                .slots
                .iter()
                .flatten()
                .filter(|slot| slot.state == SpellState::Owned)
                .map(|slot| slot.science.clone())
                .collect(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SpellStoreEvent {
    Nothing,
    Closed(Vec<String>),
}

impl SpellStoreSession {
    pub(crate) fn on_event(
        &mut self,
        view: &SpellBookView,
        name: &str,
        args: &str,
    ) -> SpellStoreEvent {
        let slot = spell_slot(args);
        match name {
            "OnBttnClose" => self.closing = true,
            "OnClosed" => return SpellStoreEvent::Closed(std::mem::take(&mut self.provisional)),
            "OnBttnReset" => self.provisional.clear(),
            "OnRollOverBttnSpell" => self.hovered = slot,
            "OnRollOutBttnSpell" if self.hovered == slot => self.hovered = None,
            "OnBttnSpell" => {
                let available = slot
                    .and_then(|n| view.slots.get(n.wrapping_sub(1)))
                    .and_then(Option::as_ref)
                    .filter(|s| s.state == SpellState::Available);
                if let Some(slot) = available {
                    self.provisional.push(slot.science.clone());
                }
            }
            _ => {}
        }
        SpellStoreEvent::Nothing
    }
}

pub(crate) fn spell_slot(name: &str) -> Option<usize> {
    let n = name
        .get(..5)
        .filter(|p| p.eq_ignore_ascii_case("spell"))
        .and_then(|_| name.get(5..))?
        .parse::<usize>()
        .ok()?;
    (1..=SPELL_STORE_SLOTS).contains(&n).then_some(n)
}

pub(crate) fn spell_button_state(
    slot: Option<&SpellSlotView>,
    owned_at_open: bool,
) -> &'static str {
    match slot.map(|s| s.state) {
        None => "_unused",
        Some(SpellState::Owned) if owned_at_open => "_purchased",
        Some(SpellState::Owned) => "_already_purchased",
        Some(SpellState::Available) => "_active",
        Some(SpellState::CantAfford) => "_disabled",
        Some(SpellState::Locked) => "_disabled_level",
    }
}

pub(crate) fn declare_spell_store(
    ex: &mut Exposure<ImageSource>,
    view: &SpellBookView,
    session: &SpellStoreSession,
) {
    ex.global(IN_GAME, Value::Bool(true));
    ex.set_extern(IN_GAME, Value::Bool(true));
    ex.set_extern(SPELL_STORE_INPUT, Value::Bool(true));
    ex.call_on(
        "layout",
        "SpellStore",
        "SetLayout",
        vec![Value::str(SPELL_STORE_LAYOUT)],
    );
    ex.string("SpellStoreSpellPoints", Some(view.points.to_string()));
    let slot_at = |n: usize| view.slots.get(n.wrapping_sub(1)).and_then(Option::as_ref);
    for n in 1..=SPELL_STORE_SLOTS {
        let slot = slot_at(n);
        let owned_at_open = slot.is_some_and(|s| {
            session
                .owned_at_open
                .iter()
                .any(|o| o.eq_ignore_ascii_case(&s.science))
        });
        let clip = format!("SpellStore.Buttons.Spell{n}");
        #[allow(clippy::cast_precision_loss)]
        let index = Value::Number(n as f64);
        ex.call_on(
            &format!("spell{n}"),
            &clip,
            "SetSpellButtonState",
            vec![index, Value::str(spell_button_state(slot, owned_at_open))],
        );
        let image = slot
            .and_then(|s| s.image.clone())
            .map(ImageSource::MappedImage);
        for surface in ["~Enabled", "~Disabled"] {
            ex.image(ImageKey::Path(format!("{clip}.{surface}")), image.clone());
        }
        ex.string(&format!("Spell{n}Cost"), slot.map(|s| s.cost.to_string()));
    }
    let hovered = session.hovered.and_then(slot_at);
    ex.string("SpellHelpText", hovered.map(|s| s.name.clone()));
    ex.string("SpellDescription", hovered.map(|s| s.description.clone()));
    let help = if hovered.is_some() { "_on" } else { "_off" };
    ex.call("help", "ShowSpellHelpText", vec![Value::str(help)]);
    if session.closing {
        ex.call("close", "Close", Vec::new());
    }
}

fn command_center_ring_cells(bounding_radius: f32, map_width: f32) -> f32 {
    use crate::apt_overlay::RADAR_CELLS;
    if map_width <= 0.0 {
        return 2.0;
    }
    let cells = (bounding_radius * RADAR_CELLS / map_width + 0.5).floor();
    cells.max(2.0).clamp(0.0, RADAR_CELLS)
}

fn radar_mark(
    has_kind: impl Fn(&str) -> bool,
    radius: f32,
    facing: f32,
    major_radius: Option<f32>,
    minor_radius: Option<f32>,
    map_width: f32,
) -> crate::apt_overlay::RadarMark {
    use crate::apt_overlay::RadarMark;
    if has_kind("HERO") {
        RadarMark::Hero
    } else if has_kind("WALL_SEGMENT") {
        let scale = if map_width > 0.0 {
            crate::apt_overlay::RADAR_CELLS / map_width
        } else {
            0.0
        };
        RadarMark::Wall {
            facing,
            half_major: major_radius.unwrap_or(0.0),
            half_minor: minor_radius.unwrap_or(0.0),
            bounding_radius_cells: radius * scale,
            scale,
        }
    } else if has_kind("COMMANDCENTER") {
        RadarMark::Ring(command_center_ring_cells(radius, map_width))
    } else {
        RadarMark::Blip
    }
}

fn radar_color(color: [f32; 4]) -> [f32; 4] {
    let [red, green, blue, alpha] = color;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    if max <= 0.0 || delta <= 0.0 {
        return color;
    }
    let saturation = (delta / max * 1.6).min(1.0);
    let hue = if max == red {
        ((green - blue) / delta).rem_euclid(6.0)
    } else if max == green {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    };
    let chroma = max * saturation;
    let low = max - chroma;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    #[allow(clippy::cast_possible_truncation)]
    let sector = hue.floor() as i32;
    let (r, g, b) = match sector {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let quantise = |c: f32| (c * 255.0).trunc() / 255.0;
    [
        quantise(r + low),
        quantise(g + low),
        quantise(b + low),
        alpha,
    ]
}

fn stealth_alpha(
    stealthed_from_viewer: bool,
    own_hero: bool,
    hidden: bool,
    frame: u64,
) -> Option<f32> {
    if stealthed_from_viewer {
        None
    } else if own_hero && hidden {
        Some(f32::from(hero_pulse_alpha(frame)) / 255.0)
    } else {
        Some(1.0)
    }
}

fn hero_pulse_alpha(frame: u64) -> u8 {
    const HALF: u64 = 30;
    let t = frame % (2 * HALF);
    let value = if t < HALF {
        0x40 + t * 0xBF / HALF
    } else {
        0xFF - (t - HALF) * 0xBF / HALF
    };
    u8::try_from(value).unwrap_or(u8::MAX)
}

pub(crate) fn minimap_dots(
    units: &[UnitView],
    state: &MatchState,
) -> Vec<crate::apt_overlay::MinimapDot> {
    use crate::apt_overlay::{MinimapDot, RadarMark};
    use mint_sage_modules::RadarPriority;
    use mint_sage_shroud::ObjectVisibility;
    let map = state.scene.terrain.active_boundary_bounds();
    let map_width = map.max().x - map.min().x;
    let rules = state.simulation.rules();
    let world = state.simulation.state();
    let frame = state.simulation.frame().get();
    let snapshot = &state.snapshot;
    let mut dots: Vec<MinimapDot> = units
        .iter()
        .filter_map(|unit| {
            let record = &state.kinds.get(Kind(unit.kind?))?.record;
            let priority = record.radar_priority;
            if !priority.is_visible()
                || (priority == RadarPriority::LocalUnitOnly && unit.owner != state.shroud_viewer)
            {
                return None;
            }
            if matches!(
                unit.visibility,
                ObjectVisibility::Fogged | ObjectVisibility::Shrouded
            ) {
                return None;
            }
            let uv = crate::apt_overlay::minimap_uv_of(&map, unit.position.x, unit.position.y)?;
            if !(0.0..=1.0).contains(&uv[0]) || !(0.0..=1.0).contains(&uv[1]) {
                return None;
            }
            let mark = radar_mark(
                |token| record.has_kind(token),
                unit.radius,
                unit.facing,
                record.geometry.major_radius,
                record.geometry.minor_radius,
                map_width,
            );
            let own = unit.owner == state.shroud_viewer;
            let mut color = radar_color(state.player_colors.of(unit.owner));
            let viewer = state.shroud_viewer;
            let stealth = snapshot
                .facts(unit.entity)
                .filter(|_| snapshot.sees_for(Frame::new(frame), viewer))
                .map(|facts| (facts.stealth_hides, facts.hidden))
                .or_else(|| {
                    rules.ids().map(|ids| {
                        (
                            rules.stealth_look_hides(world, ids, viewer, unit.entity),
                            rules.hidden_from(world, ids, viewer, unit.entity),
                        )
                    })
                });
            if let Some((hides, hidden)) = stealth {
                let alpha = stealth_alpha(hides, own && mark == RadarMark::Hero, hidden, frame)?;
                color[3] *= alpha;
            }
            Some(MinimapDot {
                uv,
                color,
                mark,
                layer: (
                    own,
                    match priority {
                        RadarPriority::LocalUnitOnly => 2,
                        RadarPriority::Unit => 1,
                        _ => 0,
                    },
                ),
            })
        })
        .collect();
    dots.sort_by_key(|dot| dot.layer);
    dots
}

#[cfg(test)]
mod tests;
