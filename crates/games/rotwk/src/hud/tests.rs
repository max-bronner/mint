#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::collections::HashMap;
use std::sync::Arc;

use mint_sage_apt_player::expose::resolve_path;
use mint_sage_apt_player::synth::{op, place, Asm, Frames, Item, Synth};
use mint_sage_apt_player::{
    Exposure, ImageChange, InstanceKind, Library, Limits, Player, Readiness, RenderCommand,
    RenderList, RenderOptions, Value,
};
use mint_sim::Entity;

use super::{
    builder_available, declare_command_bar, declare_hero_select, declare_palantir, declare_radial,
    declare_spell_row, fill_progress, hero_at_slot, hero_button, hero_double_click, hero_row_order,
    hero_slot, on_screen, rank_bar_frame, shows_production_count, BuilderCycle, FieldedHero,
    HeroButton, HeroEffect, HeroRowMemory, HudState, ImageSource, Look, SideBarContent,
    SpellBarSlot, COMMAND_BAR_READY, PALANTIR_READY,
};
use crate::command_bar::{Availability, CommandBar, ResolvedSlot, SlotAction, BAR_SLOTS};
use crate::experience::{LevelReadout, RankDisplay};
use crate::rules::SpellBarReadiness;

const LANDING_FRAMES: usize = 3;

fn player(s: &Synth, root: &Frames) -> Player {
    let (c, apt) = s.build(root);
    let mut lib = Library::new();
    let id = lib.add("test", &c, apt).expect("synthetic movie parses");
    lib.link();
    Player::new(Arc::new(lib), id, Limits::default()).expect("player starts")
}

fn stop(s: &mut Synth) -> u32 {
    let mut a = Asm::new();
    a.op(op::STOP).end();
    s.code(&mut a)
}

fn set_block(s: &mut Synth, name: &str, value: &str) -> u32 {
    let mut a = Asm::new();
    a.push_str(name).push_str(value).op(op::SET_VARIABLE).end();
    s.code(&mut a)
}

fn goto(a: &mut Asm, target: &str, push_arg: impl FnOnce(&mut Asm)) {
    push_arg(a);
    a.push_byte(1)
        .get_var(target)
        .push_str("gotoAndPlay")
        .op(op::CALL_METHOD)
        .op(op::POP);
}

fn in_segment(p: &Player, path: &str, label: &str) -> bool {
    let Some(id) = resolve_path(p, path) else {
        return false;
    };
    let Some(InstanceKind::Clip(clip)) = p.instance(id).map(|i| i.kind().clone()) else {
        return false;
    };
    let Some(timeline) = p.library().timeline(clip.timeline) else {
        return false;
    };
    let mut last = None;
    for frame in 1..=clip.current_frame {
        if let Some(l) = timeline.labels_at(frame).next() {
            last = Some(l.to_owned());
        }
    }
    last.as_deref() == Some(label)
}

fn at_label(p: &Player, path: &str, label: &str) -> bool {
    let Some(id) = resolve_path(p, path) else {
        return false;
    };
    let Some(InstanceKind::Clip(clip)) = p.instance(id).map(|i| i.kind().clone()) else {
        return false;
    };
    p.library()
        .timeline(clip.timeline)
        .is_some_and(|t| t.labels_at(clip.current_frame).any(|l| l == label))
}

fn slot(name: &str) -> ResolvedSlot {
    ResolvedSlot {
        action: SlotAction::Build(0),
        label: name.to_owned(),
        command_button: name.to_owned(),
        enabled: true,
        availability: Availability::Enabled,
        image: None,
        cost: None,
        progress: None,
        count: 0,
        auto_cast: false,
    }
}

fn palantir_movie() -> Player {
    let mut s = Synth::new();
    let stop = stop(&mut s);
    let square = s.shape_box([0.0, 0.0, 100.0, 100.0]);

    let radar_type = set_block(&mut s, "_type", "AptPalantir::RenderRadar");
    let radar_surface = s.sprite(vec![vec![
        Item::Place(place(1, square)),
        Item::Action(radar_type),
    ]]);
    let radar = s.sprite(vec![
        vec![Item::Label("_hide"), Item::Action(stop)],
        vec![
            Item::Label("_show"),
            Item::Place(place(1, radar_surface)),
            Item::Action(stop),
        ],
    ]);
    let frame = s.sprite(vec![
        vec![Item::Label("_good"), Item::Action(stop)],
        vec![Item::Label("_evil"), Item::Action(stop)],
    ]);
    let field = s.bound_text("999999", "stringName");
    let placeholder = set_block(&mut s, "stringName", "$PalantirResources");
    let resources = s.sprite(vec![vec![
        Item::Place(place(1, field)),
        Item::Action(placeholder),
    ]]);
    let resource_bar = s.sprite(vec![vec![
        Item::Place(place(1, resources).name("Resources")),
        Item::Action(stop),
    ]]);
    let portrait_type = set_block(&mut s, "_type", "RenderImage");
    let portrait = s.sprite(vec![vec![
        Item::Place(place(1, square)),
        Item::Action(portrait_type),
    ]]);
    let command_ui = s.sprite(vec![
        vec![Item::Label("_hide"), Item::Action(stop)],
        vec![
            Item::Label("_show"),
            Item::Place(place(1, portrait).name("Portrait")),
            Item::Action(stop),
        ],
    ]);
    let glass = s.sprite(vec![
        vec![Item::Label("_active")],
        vec![Item::Action(stop)],
        vec![Item::Label("_passive")],
        vec![Item::Action(stop)],
    ]);
    let mut define_glass = Asm::new();
    define_glass.op(op::PUSH_THIS).push_str("SetGlassState");
    define_glass.function("", &["state"], |b| {
        goto(b, "_parent.glass0", |b| {
            b.get_var("state");
        })
    });
    define_glass.op(op::SET_MEMBER).end();
    let define_glass = s.code(&mut define_glass);
    let slot0 = s.sprite(vec![vec![Item::Action(define_glass)]]);
    let command_buttons = s.sprite(vec![vec![
        Item::Place(place(1, slot0).name("0")),
        Item::Place(place(2, glass).name("glass0")),
        Item::Action(stop),
    ]]);
    let side_art = s.sprite(vec![
        vec![Item::Label("_blank"), Item::Action(stop)],
        vec![Item::Label("_evenstar"), Item::Action(stop)],
        vec![Item::Label("_ring"), Item::Action(stop)],
    ]);
    let buttons = s.sprite(vec![vec![
        Item::Place(place(1, side_art).name("PlayerMagic")),
        Item::Place(place(2, side_art).name("Objectives")),
        Item::Action(stop),
    ]]);
    let palantir_buttons = s.sprite(vec![vec![
        Item::Place(place(1, buttons).name("Buttons")),
        Item::Action(stop),
    ]]);

    let mut root_code = Asm::new();
    root_code.function("SetPalantirFrameState", &["state"], |b| {
        goto(b, "Radar", |b| {
            b.push_str("_show");
        });
        goto(b, "PalantirFrame", |b| {
            b.get_var("state");
        });
    });
    root_code.function("SetPlayerFaction", &["faction"], |_| {});
    root_code.function("ShowCommandInterface", &[], |b| {
        goto(b, "CommandUI", |b| {
            b.push_str("_show");
        });
    });
    root_code.function("HideCommandInterface", &[], |b| {
        goto(b, "CommandUI", |b| {
            b.push_str("_hide");
        });
    });
    root_code
        .get_url("FSCommand:AptPalantir::OnInitialized", "")
        .op(op::STOP)
        .end();
    let root_code = s.code(&mut root_code);
    let root = vec![vec![
        Item::Place(place(1, radar).name("Radar")),
        Item::Place(place(2, frame).name("PalantirFrame")),
        Item::Place(place(3, resource_bar).name("ResourceBar")),
        Item::Place(place(4, command_ui).name("CommandUI")),
        Item::Place(place(5, command_buttons).name("CommandButtons")),
        Item::Place(place(6, palantir_buttons).name("PalantirButtons")),
        Item::Action(root_code),
    ]];
    player(&s, &root)
}

fn evil_hud() -> HudState {
    let mut hud = HudState {
        gold: 1250,
        command_points: (60, 300),
        evil: true,
        selected: true,
        minimap: Some("maps/test/test_art.tga".to_owned()),
        ..HudState::default()
    };
    hud.command_bar.ring[0] = Some(slot("Build_Soldier"));
    hud
}

#[test]
fn a_host_call_before_the_first_tick_finds_nothing() {
    let mut p = palantir_movie();
    let root = p.root();
    let called = p
        .call_defined(root, "SetPalantirFrameState", vec![Value::str("_evil")])
        .unwrap();
    assert_eq!(called, None, "nothing is defined before the first tick");
    for _ in 0..LANDING_FRAMES {
        p.tick().unwrap();
    }
    assert!(at_label(&p, "Radar", "_hide"));
}

#[test]
fn the_palantir_lands_in_the_live_order() {
    let mut p = palantir_movie();
    let mut ex = Exposure::new(Readiness::Event(PALANTIR_READY.to_owned()));
    let hud = evil_hud();
    let mut strings = HashMap::new();
    let mut images = Vec::new();
    for _ in 0..LANDING_FRAMES {
        declare_palantir(&mut ex, &hud);
        ex.frame(&mut p, 1);
        for (key, value) in ex.take_string_changes() {
            match value {
                Some(v) => strings.insert(key, v),
                None => strings.remove(&key),
            };
        }
        images.extend(ex.take_image_changes());
    }
    assert!(ex.stats().ready);
    assert!(at_label(&p, "PalantirFrame", "_evil"), "faction art");
    assert!(at_label(&p, "Radar", "_show"), "the minimap's frame");
    assert!(at_label(&p, "CommandUI", "_show"), "the command interface");
    assert!(
        in_segment(&p, "CommandButtons.glass0", "_active"),
        "a usable slot's glass is not the gold ring"
    );
    for clip in ["PlayerMagic", "Objectives"] {
        assert!(
            at_label(&p, &format!("PalantirButtons.Buttons.{clip}"), "_ring"),
            "{clip} shows the evil side's art"
        );
    }

    let field = resolve_path(&p, "ResourceBar.Resources.#1").unwrap();
    let raw = p.text(field).unwrap();
    assert_eq!(
        raw, "$PalantirResources",
        "the movie's own binding is untouched"
    );
    let shown =
        mint_sage_apt_viewer::localise_with(&raw, &strings, &mint_sage_str::StringTable::new());
    assert_eq!(shown, "1250");
    assert_eq!(
        strings.get("PalantirCommandPoints").map(String::as_str),
        Some("60/300")
    );

    let mut list = RenderList::default();
    p.render_with(&mut list, RenderOptions { surfaces: true });
    let radar_drawn = list.commands.iter().any(|c| {
        matches!(c, RenderCommand::Surface { instance, .. }
            if p.own_string(*instance, "_type").as_deref() == Some("AptPalantir::RenderRadar"))
    });
    assert!(radar_drawn, "the RenderRadar surface is in the render list");
    assert!(images.contains(&ImageChange::Name {
        key: "AptPalantir::RenderRadar".to_owned(),
        content: Some(ImageSource::MapArt("maps/test/test_art.tga".to_owned())),
    }));
}

fn rank_movie() -> Player {
    let mut s = Synth::new();
    let stop = stop(&mut s);
    let bar = s.sprite(
        (0..101)
            .map(|at| {
                if at == 0 {
                    vec![Item::Action(stop)]
                } else {
                    Vec::new()
                }
            })
            .collect(),
    );
    let mut state_clip = |child: u32, name: &'static str| {
        s.sprite(vec![
            vec![Item::Label("_hide"), Item::Action(stop)],
            vec![
                Item::Label("_show"),
                Item::Place(place(1, child).name(name)),
                Item::Action(stop),
            ],
        ])
    };
    let progress = state_clip(bar, "Bar");
    let rank_ui = state_clip(progress, "Progress");
    let command_ui = state_clip(rank_ui, "RankUI");
    let mut code = Asm::new();
    for (function, target, label) in [
        ("ShowCommandInterface", "CommandUI", "_show"),
        ("HideCommandInterface", "CommandUI", "_hide"),
        ("ShowRankInterface", "CommandUI.RankUI", "_show"),
        ("HideRankInterface", "CommandUI.RankUI", "_hide"),
        ("ShowRankProgress", "CommandUI.RankUI.Progress", "_show"),
        ("HideRankProgress", "CommandUI.RankUI.Progress", "_hide"),
    ] {
        code.function(function, &[], |b| {
            goto(b, target, |b| {
                b.push_str(label);
            });
        });
    }
    code.function("SetRankProgressBar", &["value"], |b| {
        b.get_var("value")
            .push_byte(1)
            .get_var("CommandUI.RankUI.Progress.Bar")
            .push_str("gotoAndStop")
            .op(op::CALL_METHOD)
            .op(op::POP);
    });
    code.function("SetPlayerMagicProgress", &["value"], |_| {});
    code.get_url("FSCommand:AptPalantir::OnInitialized", "")
        .op(op::STOP)
        .end();
    let code = s.code(&mut code);
    let root = vec![vec![
        Item::Place(place(1, command_ui).name("CommandUI")),
        Item::Action(code),
    ]];
    player(&s, &root)
}

#[test]
fn the_level_readout_follows_the_selection_through_a_hidden_command_interface() {
    let mut p = rank_movie();
    let mut ex = Exposure::new(Readiness::Event(PALANTIR_READY.to_owned()));
    let mut run = |p: &mut Player, hud: &HudState| {
        for _ in 0..=LANDING_FRAMES {
            declare_palantir(&mut ex, hud);
            ex.frame(p, 1);
        }
    };
    let bar_frame = |p: &Player| {
        let id = resolve_path(p, "CommandUI.RankUI.Progress.Bar")?;
        match p.instance(id).map(|i| i.kind().clone()) {
            Some(InstanceKind::Clip(clip)) => Some(clip.current_frame),
            _ => None,
        }
    };
    let levelled = |progress| HudState {
        selected: true,
        rank_ui: Some(RankDisplay::Level(LevelReadout { rank: 2, progress })),
        ..HudState::default()
    };
    run(&mut p, &levelled(Some(0.5)));
    assert!(at_label(&p, "CommandUI.RankUI", "_show"), "the level line");
    assert!(
        at_label(&p, "CommandUI.RankUI.Progress", "_show"),
        "its bar"
    );
    assert_eq!(bar_frame(&p), Some(51), "half way to the next level");
    run(&mut p, &HudState::default());
    run(&mut p, &levelled(Some(0.5)));
    assert!(
        at_label(&p, "CommandUI.RankUI", "_show"),
        "RankUI re-created by CommandUI is shown again"
    );
    assert_eq!(bar_frame(&p), Some(51), "and its bar set again");
    run(&mut p, &levelled(None));
    assert!(
        at_label(&p, "CommandUI.RankUI.Progress", "_hide"),
        "last level"
    );
    run(
        &mut p,
        &HudState {
            selected: true,
            ..HudState::default()
        },
    );
    assert!(at_label(&p, "CommandUI.RankUI", "_hide"), "no levels");
}

#[test]
fn a_timed_selection_drains_the_rank_bar() {
    let mut p = rank_movie();
    let mut ex = Exposure::new(Readiness::Event(PALANTIR_READY.to_owned()));
    let mut bar_at = |left: f32| {
        let hud = HudState {
            selected: true,
            rank_ui: Some(RankDisplay::TimeRemaining(left)),
            ..HudState::default()
        };
        for _ in 0..=LANDING_FRAMES {
            declare_palantir(&mut ex, &hud);
            ex.frame(&mut p, 1);
        }
        assert!(at_label(&p, "CommandUI.RankUI", "_show"), "the readout");
        assert!(
            at_label(&p, "CommandUI.RankUI.Progress", "_show"),
            "its bar"
        );
        let id = resolve_path(&p, "CommandUI.RankUI.Progress.Bar").unwrap();
        match p.instance(id).map(|i| i.kind().clone()) {
            Some(InstanceKind::Clip(clip)) => clip.current_frame,
            _ => panic!("the bar is a clip"),
        }
    };
    let full = bar_at(1.0);
    let quarter = bar_at(0.25);
    let spent = bar_at(0.0);
    assert_eq!(full - quarter, 74, "frame 100 down to frame 26");
    assert_eq!(quarter - spent, 25, "and to frame 1");
}

#[test]
fn the_spell_row_packs_bought_spells_and_a_new_buy_shifts_the_rest_up() {
    let book = [
        "SpellTest_A5",
        "SpellTest_B5",
        "SpellTest_C10",
        "SpellTest_D10",
    ];
    let rows_after = |bought: &[u8]| {
        let mut p = palantir_movie();
        let mut ex = Exposure::new(Readiness::Immediate);
        let hud = HudState {
            spell_bar: bought
                .iter()
                .map(|&button| SpellBarSlot {
                    button,
                    command_button: book[usize::from(button)].to_owned(),
                    readiness: SpellBarReadiness::Ready,
                })
                .collect(),
            ..HudState::default()
        };
        declare_spell_row(&mut ex, &hud, "SpellBook");
        ex.frame(&mut p, 1);
        let images = ex.take_image_changes();
        (1..=4)
            .map(|n| {
                images.iter().find_map(|change| match change {
                    ImageChange::Name {
                        key,
                        content: Some(ImageSource::Button(name)),
                    } if key == &format!("InGameSpellBookSpell{n}Image") => Some(name.clone()),
                    _ => None,
                })
            })
            .collect::<Vec<_>>()
    };
    let named = |names: [Option<&str>; 4]| names.map(|n| n.map(str::to_owned)).to_vec();
    assert_eq!(
        rows_after(&[0]),
        named([Some("SpellTest_A5"), None, None, None])
    );
    assert_eq!(
        rows_after(&[0, 2]),
        named([Some("SpellTest_A5"), Some("SpellTest_C10"), None, None])
    );
    assert_eq!(
        rows_after(&[0, 1, 2]),
        named([
            Some("SpellTest_A5"),
            Some("SpellTest_B5"),
            Some("SpellTest_C10"),
            None
        ])
    );
}

#[test]
fn the_rank_bar_frame_is_the_whole_percent_plus_one() {
    for (fill, frame) in [
        (0.0, 1),
        (0.004, 1),
        (0.25, 26),
        (0.5, 51),
        (0.999, 100),
        (1.0, 100),
        (-1.0, 1),
        (2.0, 100),
    ] {
        assert_eq!(rank_bar_frame(fill), frame, "fill {fill}");
    }
}

fn command_bar_movie() -> Player {
    let mut s = Synth::new();
    let stop = stop(&mut s);
    let button = s.sprite(vec![
        vec![Item::Label("_init")],
        vec![Item::Label("_show"), Item::Action(stop)],
        vec![Item::Label("_hide"), Item::Action(stop)],
    ]);
    let names: Vec<String> = (0..BAR_SLOTS).map(|i| format!("Button{i}")).collect();
    let mut buttons = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let depth = i32::try_from(i).unwrap() + 1;
        buttons.push(Item::Place(place(depth, button).name(name)));
    }
    buttons.push(Item::Action(stop));
    let button_set = s.sprite(vec![buttons]);
    let mut code = Asm::new();
    code.function("FadeIn", &[], |b| {
        b.push_str("_fadeIn")
            .push_byte(1)
            .push_str("gotoAndPlay")
            .op(op::CALL_FUNCTION)
            .op(op::POP);
    });
    code.function("FadeOut", &[], |b| {
        b.push_str("_fadeOut")
            .push_byte(1)
            .push_str("gotoAndPlay")
            .op(op::CALL_FUNCTION)
            .op(op::POP);
    });
    code.function("SetButtonState", &["index", "state"], |b| {
        b.get_var("state").push_byte(1).get_var("ButtonSet");
        b.push_str("Button")
            .get_var("index")
            .op(0x47)
            .op(op::GET_MEMBER);
        b.push_str("gotoAndPlay").op(op::CALL_METHOD).op(op::POP);
    });
    code.get_url("FSCommand:OnAptInGameSideCommandBarLoaded", "")
        .op(op::STOP)
        .end();
    let code = s.code(&mut code);
    let mut faded = Asm::new();
    faded
        .get_url("FSCommand:OnAptInGameSideCommandBarFadeOutComplete", "")
        .op(op::STOP)
        .end();
    let faded = s.code(&mut faded);
    let root = vec![
        vec![
            Item::Place(place(1, button_set).name("ButtonSet")),
            Item::Action(code),
        ],
        vec![Item::Label("_fadeIn")],
        vec![Item::Label("_shown"), Item::Action(stop)],
        vec![Item::Label("_fadeOut")],
        vec![],
        vec![],
        vec![Item::Label("_hidden"), Item::Action(faded)],
    ];
    player(&s, &root)
}

fn bar_frame(
    p: &mut Player,
    ex: &mut Exposure<ImageSource>,
    content: &mut SideBarContent,
    hud: &HudState,
) {
    content.update(&hud.command_bar.bar);
    declare_command_bar(ex, hud, content, "");
    ex.frame(p, 1);
    for event in ex.take_events() {
        if let Some(command) = event.command() {
            content.on_event(command, &hud.command_bar.bar);
        }
    }
}

#[test]
fn the_side_bar_follows_the_button_count_and_slides_back() {
    let mut p = command_bar_movie();
    let mut ex = Exposure::new(Readiness::Event(COMMAND_BAR_READY.to_owned()));
    let mut content = SideBarContent::default();
    let mut hud = HudState::default();
    hud.command_bar.bar = vec![slot("Build_A"), slot("Build_B")];
    for _ in 0..LANDING_FRAMES {
        bar_frame(&mut p, &mut ex, &mut content, &hud);
    }
    assert!(at_label(&p, "", "_shown"), "slid in");
    for i in 0..BAR_SLOTS {
        let expected = if i < 2 { "_show" } else { "_hide" };
        assert!(
            at_label(&p, &format!("ButtonSet.Button{i}"), expected),
            "Button{i} should rest on {expected}"
        );
    }
    hud.command_bar.bar.clear();
    bar_frame(&mut p, &mut ex, &mut content, &hud);
    assert!(!at_label(&p, "", "_hidden"), "still sliding out");
    assert!(
        at_label(&p, "ButtonSet.Button0", "_show"),
        "the buttons ride the slide out"
    );
    for _ in 0..=LANDING_FRAMES {
        bar_frame(&mut p, &mut ex, &mut content, &hud);
    }
    assert!(at_label(&p, "", "_hidden"), "slid back out");
    assert!(
        at_label(&p, "ButtonSet.Button0", "_hide"),
        "cleared once done"
    );
}

#[test]
fn a_builder_is_available_only_without_a_build_order() {
    use mint_sage_asset::KindOf;
    let dozer = KindOf::DOZER.union(KindOf::SELECTABLE);
    assert!(
        builder_available(true, false, false, dozer),
        "idle or merely moving"
    );
    assert!(
        !builder_available(true, true, false, dozer),
        "walking to a site"
    );
    assert!(!builder_available(true, true, true, dozer), "inside one");
    assert!(
        !builder_available(false, false, false, dozer),
        "someone else's"
    );
    assert!(
        !builder_available(true, false, false, KindOf::SELECTABLE),
        "not a builder"
    );
    assert!(
        !builder_available(true, false, false, KindOf::DOZER),
        "a construction-site worker: KindOf DOZER with no SELECTABLE"
    );
}

fn entities(n: u32) -> Vec<Entity> {
    let mut world = mint_sim::World::new();
    (0..n).map(|_| world.spawn().unwrap()).collect()
}

const TIMEOUT: std::time::Duration = std::time::Duration::from_millis(3500);

fn ms(n: u64) -> std::time::Duration {
    std::time::Duration::from_millis(n)
}

#[test]
fn the_builder_button_goes_nearest_first_from_where_the_cycle_started() {
    let e = entities(3);
    let available = [
        (e[0], [100.0, 0.0]),
        (e[1], [10.0, 0.0]),
        (e[2], [50.0, 0.0]),
    ];
    let mut cycle = BuilderCycle::default();
    let mut picks = Vec::new();
    for click in 0..4u64 {
        let here = if click == 0 { [0.0, 0.0] } else { [100.0, 0.0] };
        picks.push(cycle.click(&available, here, ms(click * 500), 1, TIMEOUT));
        cycle.settle(1);
    }
    assert_eq!(picks, [Some(e[1]), Some(e[2]), Some(e[0]), Some(e[1])]);
    assert_eq!(
        BuilderCycle::default().click(&[], [0.0; 2], ms(0), 0, TIMEOUT),
        None
    );
}

#[test]
fn the_builder_cycle_resets_after_the_timeout_or_another_action() {
    let e = entities(2);
    let available = [(e[0], [10.0, 0.0]), (e[1], [90.0, 0.0])];
    let mut cycle = BuilderCycle::default();
    assert_eq!(
        cycle.click(&available, [0.0; 2], ms(0), 5, TIMEOUT),
        Some(e[0])
    );
    cycle.settle(6);
    assert_eq!(
        cycle.click(&available, [100.0, 0.0], ms(3500), 6, TIMEOUT),
        Some(e[1]),
        "within the timeout: the next of the same cycle"
    );
    cycle.settle(7);
    assert_eq!(
        cycle.click(&available, [100.0, 0.0], ms(7001), 7, TIMEOUT),
        Some(e[1]),
        "timed out: nearest to the new centre"
    );
    cycle.settle(8);
    assert_eq!(
        cycle.click(&available, [0.0, 0.0], ms(7100), 9, TIMEOUT),
        Some(e[0]),
        "another action in between: nearest to the new centre"
    );
    let e = entities(3);
    let three = [
        (e[0], [10.0, 0.0]),
        (e[1], [90.0, 0.0]),
        (e[2], [50.0, 0.0]),
    ];
    let mut cycle = BuilderCycle::default();
    assert_eq!(cycle.click(&three, [0.0; 2], ms(0), 1, TIMEOUT), Some(e[0]));
    cycle.settle(2);
    assert_eq!(
        cycle.click(&three, [0.0; 2], ms(100), 5, TIMEOUT),
        Some(e[0]),
        "an action in between starts over, not on to the next"
    );
}

#[test]
fn a_hero_button_double_click_is_the_same_button_within_the_window() {
    let a = HeroButton::Slot(2);
    let b = HeroButton::Slot(3);
    assert!(!hero_double_click(None, a, ms(0), 500));
    assert!(hero_double_click(Some((a, ms(0))), a, ms(499), 500));
    assert!(hero_double_click(Some((a, ms(0))), a, ms(500), 500));
    assert!(
        !hero_double_click(Some((a, ms(0))), a, ms(501), 500),
        "past the window: not a double click"
    );
    assert!(
        !hero_double_click(Some((a, ms(0))), b, ms(100), 500),
        "a different button: not a double click"
    );
}

#[test]
fn a_builder_on_screen_does_not_move_the_camera() {
    assert!(on_screen(Some((10.0, 10.0, 5.0)), (800, 600)));
    assert!(
        !on_screen(Some((800.0, 10.0, 5.0)), (800, 600)),
        "right edge"
    );
    assert!(!on_screen(Some((-1.0, 10.0, 5.0)), (800, 600)));
    assert!(!on_screen(None, (800, 600)), "behind the camera");
}

fn command_slot_parts(s: &mut Synth) -> u32 {
    let stop = stop(s);
    let mut define_frame = Asm::new();
    define_frame.op(op::PUSH_THIS).push_str("GotoState");
    define_frame.function("", &["state"], |b| {
        goto(b, "_parent", |b| {
            b.get_var("state");
        });
    });
    define_frame.op(op::SET_MEMBER);
    define_frame.op(op::PUSH_THIS).push_str("CreateContent");
    define_frame.function("", &["contentType", "contentName"], |b| {
        b.push_byte(1)
            .get_var("contentName")
            .get_var("contentType")
            .push_byte(3)
            .op(op::PUSH_THIS)
            .push_str("attachMovie")
            .op(op::CALL_METHOD)
            .op(op::POP);
    });
    define_frame.op(op::SET_MEMBER);
    define_frame.op(op::PUSH_THIS).push_str("DeleteContent");
    define_frame.function("", &[], |b| {
        b.push_byte(0)
            .op(op::PUSH_THIS)
            .push_str("CommandButton")
            .op(op::GET_MEMBER)
            .push_str("removeMovieClip")
            .op(op::CALL_METHOD)
            .op(op::POP);
    });
    define_frame.op(op::SET_MEMBER).op(op::STOP).end();
    let define_frame = s.code(&mut define_frame);
    let frame = s.sprite(vec![vec![Item::Action(define_frame)]]);

    let mut define_button = Asm::new();
    define_button
        .push_str("me")
        .op(op::PUSH_THIS)
        .op(0x4B)
        .op(op::SET_VARIABLE);
    define_button.op(op::PUSH_THIS).push_str("SetState");
    define_button.function("", &["state"], |b| {
        b.get_var("state")
            .push_byte(1)
            .push_str("gotoAndPlay")
            .op(op::CALL_FUNCTION)
            .op(op::POP);
    });
    define_button.op(op::SET_MEMBER).op(op::STOP).end();
    let define_button = s.code(&mut define_button);
    let button = s.sprite(vec![
        vec![Item::Action(define_button)],
        vec![Item::Label("_up"), Item::Action(stop)],
        vec![Item::Label("_over"), Item::Action(stop)],
        vec![Item::Label("_disabled"), Item::Action(stop)],
        vec![Item::Label("_cantAfford"), Item::Action(stop)],
    ]);
    s.export(super::COMMAND_BUTTON, button);
    frame
}

fn radial_movie() -> Player {
    let mut s = Synth::new();
    let stop = stop(&mut s);
    let frame = command_slot_parts(&mut s);
    let mut read_global = Asm::new();
    read_global.push_str("inGame").get_var("_global.InGame");
    read_global.op(op::SET_VARIABLE).end();
    let read_global = s.code(&mut read_global);
    let shell = s.sprite(vec![
        vec![
            Item::Label("_up"),
            Item::Place(place(1, frame).name("bttnFrame")),
            Item::Action(read_global),
        ],
        vec![Item::Action(stop)],
        vec![Item::Label("_over")],
        vec![Item::Action(stop)],
        vec![Item::Label("_disabled")],
        vec![Item::Action(stop)],
    ]);
    s.export("RadialButtonShell", shell);
    let mut code = Asm::new();
    code.function("CreateButton", &["id"], |b| {
        b.get_var("id")
            .get_var("id")
            .op(0x4B)
            .push_str("RadialButtonShell");
        b.push_byte(3)
            .op(op::PUSH_THIS)
            .push_str("attachMovie")
            .op(op::CALL_METHOD)
            .op(op::POP);
    });
    code.function("MoveButton", &["id", "x", "y", "width", "height"], |b| {
        for (member, arg) in [("px", "x"), ("py", "y")] {
            b.op(op::PUSH_THIS).get_var("id").op(op::GET_MEMBER);
            b.push_str(member).get_var(arg).op(op::SET_MEMBER);
        }
    });
    code.function("DestroyButton", &["id"], |b| {
        b.push_byte(0)
            .op(op::PUSH_THIS)
            .get_var("id")
            .op(op::GET_MEMBER);
        b.push_str("removeMovieClip")
            .op(op::CALL_METHOD)
            .op(op::POP);
    });
    code.op(op::STOP).end();
    let code = s.code(&mut code);
    player(&s, &vec![vec![Item::Action(code)]])
}

fn dimmed(name: &str) -> ResolvedSlot {
    ResolvedSlot {
        enabled: false,
        availability: Availability::Disabled,
        ..slot(name)
    }
}

fn radial_frames(
    p: &mut Player,
    ex: &mut Exposure<ImageSource>,
    buttons: &[&ResolvedSlot],
    rects: &[[f32; 4]],
    hovered: Option<usize>,
) -> Vec<ImageChange<ImageSource>> {
    let mut images = Vec::new();
    for _ in 0..LANDING_FRAMES {
        declare_radial(
            ex,
            buttons,
            rects,
            hovered,
            &crate::button_flash::FlashView::default(),
        );
        ex.frame(p, 1);
        images.extend(ex.take_image_changes());
    }
    images
}

#[test]
fn the_radial_menu_lands_through_the_movies_own_api() {
    let mut p = radial_movie();
    let mut ex = Exposure::new(Readiness::Immediate);
    let (a, b) = (slot("Build_A"), dimmed("Build_B"));
    let rects = [[10.0, 20.0, 48.0, 48.0], [100.0, 20.0, 48.0, 48.0]];
    let images = radial_frames(&mut p, &mut ex, &[&a, &b], &rects, None);
    let root = p.root();
    assert_eq!(p.variable(root, "0.px"), Value::Number(10.0));
    assert_eq!(p.variable(root, "1.px"), Value::Number(100.0));
    assert_eq!(
        p.variable(root, "0.inGame"),
        Value::Bool(true),
        "InGame first"
    );
    assert!(in_segment(&p, "0", "_up"));
    assert!(in_segment(&p, "1", "_disabled"), "a dimmed button");
    let button0 = "0.bttnFrame.CommandButton";
    assert!(in_segment(&p, button0, "_up"));
    assert!(
        in_segment(&p, "1.bttnFrame.CommandButton", "_disabled"),
        "its RenderImage greys itself"
    );
    let me = p.variable(root, &format!("{button0}.me"));
    assert_eq!(me, Value::str("_level0.0.bttnFrame.CommandButton"));
    assert!(images.contains(&ImageChange::Name {
        key: "_level0.0.bttnFrame.CommandButton_Image".to_owned(),
        content: Some(ImageSource::Button("Build_A".to_owned())),
    }));
    assert!(images.contains(&ImageChange::Name {
        key: "_level0.1.bttnFrame.CommandButton_Image".to_owned(),
        content: Some(ImageSource::Button("Build_B".to_owned())),
    }));

    let _ = radial_frames(&mut p, &mut ex, &[&a], &rects[..1], Some(0));
    assert!(resolve_path(&p, "1").is_none(), "left the page: destroyed");
    assert!(in_segment(&p, "0", "_over"), "hovered");
    assert!(
        in_segment(&p, button0, "_over"),
        "the button shows the pointer too"
    );

    let _ = radial_frames(&mut p, &mut ex, &[&a, &b], &rects, None);
    assert_eq!(
        p.variable(root, "1.px"),
        Value::Number(100.0),
        "re-created and moved again"
    );
}

#[test]
fn a_slot_leaving_the_page_hides_its_icon_rather_than_unbinding_it() {
    let mut p = radial_movie();
    let mut ex = Exposure::new(Readiness::Immediate);
    let (a, b) = (slot("Build_A"), slot("Build_B"));
    let rects = [[10.0, 20.0, 48.0, 48.0], [100.0, 20.0, 48.0, 48.0]];
    let _ = radial_frames(&mut p, &mut ex, &[&a, &b], &rects, None);

    let images = radial_frames(&mut p, &mut ex, &[&a], &rects[..1], None);
    let key = "_level0.1.bttnFrame.CommandButton_Image".to_owned();
    assert!(
        images.contains(&ImageChange::Name {
            key: key.clone(),
            content: Some(ImageSource::Hidden),
        }),
        "a slot leaving the page must hide its icon, not just unbind it: {images:?}"
    );
    assert!(
        !images.iter().any(|change| matches!(
            change,
            ImageChange::Name { key: k, content: None } if *k == key
        )),
        "bare None reverts to the engine-surface placeholder, not nothing: {images:?}"
    );
}

#[test]
fn the_queued_count_is_gated_on_show_production_count() {
    let shown = mint_sage_ini::CommandButtonDef {
        show_production_count: true,
        ..Default::default()
    };
    assert!(shows_production_count(Some(&shown)));
    let hidden = mint_sage_ini::CommandButtonDef::default();
    assert!(!shows_production_count(Some(&hidden)));
    assert!(
        !shows_production_count(None),
        "a button the index does not carry shows no count"
    );
}

#[test]
fn every_region_takes_the_production_clock() {
    let mut bar = CommandBar::default();
    bar.ring[0] = Some(slot("Ring"));
    bar.bar.push(slot("Bar"));
    bar.radial.push((1, slot("Radial")));
    fill_progress(&mut bar, |_| (Some(40), 2));
    assert_eq!(bar.ring[0].as_ref().and_then(|s| s.progress), Some(40));
    assert_eq!(bar.bar[0].progress, Some(40));
    assert_eq!(bar.radial[0].1.progress, Some(40));
    assert_eq!(bar.radial[0].1.count, 2);
}

#[test]
fn a_button_clock_reads_production_or_recharge_through_one_channel() {
    let mut bar = CommandBar::default();
    let mut building = slot("Building");
    building.progress = None;
    bar.ring[0] = Some(building);
    let mut recharging = slot("Recharging");
    recharging.action = SlotAction::Unimplemented("a special power");
    recharging.availability = Availability::NotReady { percent: 65 };
    recharging.progress = Some(65);
    bar.ring[1] = Some(recharging);
    fill_progress(&mut bar, |slot| {
        if slot.action == SlotAction::Build(0) {
            (Some(40), 2)
        } else {
            (None, 0)
        }
    });
    assert_eq!(bar.ring[0].as_ref().and_then(|s| s.progress), Some(40));
    assert_eq!(
        bar.ring[1].as_ref().and_then(|s| s.progress),
        Some(65),
        "the power's own clock is not clobbered by a pass with nothing to report"
    );
}

#[test]
fn every_hud_movie_is_told_it_is_in_the_game() {
    let mut p = palantir_movie();
    let mut ex = Exposure::new(Readiness::Event(PALANTIR_READY.to_owned()));
    declare_palantir(&mut ex, &evil_hud());
    ex.sync(&mut p);
    let root = p.root();
    assert_eq!(p.variable(root, "_global.InGame"), Value::Bool(true));
    assert_eq!(p.variable(root, "extern.InGame"), Value::Bool(true));
    let mut p = command_bar_movie();
    let mut ex = Exposure::new(Readiness::Event(COMMAND_BAR_READY.to_owned()));
    declare_command_bar(
        &mut ex,
        &HudState::default(),
        &SideBarContent::default(),
        "",
    );
    ex.sync(&mut p);
    let root = p.root();
    assert_eq!(p.variable(root, "_global.InGame"), Value::Bool(true));
}

#[test]
fn a_command_buttons_state_is_its_look() {
    let with = |availability| ResolvedSlot {
        availability,
        ..slot("A")
    };
    use super::button_state;
    assert_eq!(button_state(&with(Availability::Enabled), false), "_up");
    assert_eq!(button_state(&with(Availability::Enabled), true), "_over");
    assert_eq!(
        button_state(&with(Availability::Disabled), true),
        "_disabled"
    );
    assert_eq!(
        button_state(&with(Availability::CantAfford), false),
        "_cantAfford"
    );
}

#[test]
fn a_buttons_look_follows_why_it_cannot_be_used() {
    let with = |availability| ResolvedSlot {
        availability,
        ..slot("A")
    };
    assert_eq!(Look::of(&with(Availability::Enabled)), Look::Normal);
    assert_eq!(Look::of(&with(Availability::Disabled)), Look::Grey);
    assert_eq!(Look::of(&with(Availability::CantAfford)), Look::Dim);
}

#[test]
fn a_passive_ability_rests_static_on_the_gold_glass() {
    use super::{button_state, glass_state};
    let with = |availability| ResolvedSlot {
        availability,
        action: SlotAction::Passive,
        ..slot("A")
    };
    assert_eq!(button_state(&with(Availability::Enabled), false), "_static");
    assert_eq!(button_state(&with(Availability::Enabled), true), "_static");
    assert_eq!(glass_state(Some(&with(Availability::Enabled))), "_passive");
    assert_eq!(
        button_state(&with(Availability::Disabled), false),
        "_disabled"
    );
    assert_eq!(glass_state(Some(&with(Availability::Disabled))), "_active");
    assert_eq!(glass_state(Some(&slot("B"))), "_active");
    assert_eq!(glass_state(None), "_active");
}

#[test]
fn a_hero_row_click_names_its_button() {
    assert_eq!(
        hero_button("_level0.HeroSelectUI.Hero1_OnBttnHeroSelect"),
        Some(HeroButton::Slot(1))
    );
    assert_eq!(
        hero_button("_level0.HeroSelectUI.Hero16_OnBttnHeroSelect"),
        Some(HeroButton::Slot(16))
    );
    assert_eq!(
        hero_button("_level0.HeroSelectUI_OnBttnSelectAllHeroes"),
        Some(HeroButton::All)
    );
    for other in [
        "_level0.HeroSelectUI.Hero0_OnBttnHeroSelect",
        "_level0.HeroSelectUI.Hero17_OnBttnHeroSelect",
        "_level0.HeroSelectUI.Villain2_OnBttnHeroSelect",
        "_level0.HeroSelectUI.Hero2",
        "OnInitialized",
        "_level0.HeroSelectUI_OnBttnHeroSelect",
    ] {
        assert_eq!(hero_button(other), None, "{other}");
    }
}

#[test]
fn the_hero_row_takes_every_hero_kind_with_a_portrait_in_sort_order() {
    use mint_sage_asset::KindOf;
    let record = |image: Option<&str>, order: Option<i32>| mint_sage_modules::ObjectRecord {
        name: "SummonedGust".to_owned(),
        button_image: image.map(str::to_owned),
        hero_sort_order: order,
        ..mint_sage_modules::ObjectRecord::default()
    };
    let hero = KindOf::HERO.union(KindOf::SELECTABLE);
    assert_eq!(
        hero_row_order(hero, &record(Some("GustIcon"), None)),
        Some(i32::MAX)
    );
    assert_eq!(
        hero_row_order(hero, &record(Some("GustIcon"), Some(30))),
        Some(30)
    );
    assert_eq!(hero_row_order(hero, &record(None, Some(30))), None);
    assert_eq!(
        hero_row_order(KindOf::SELECTABLE, &record(Some("GustIcon"), Some(30))),
        None
    );
}

#[test]
fn the_hero_row_is_the_builder_then_the_fielded_heroes() {
    let e = entities(3);
    let mut hud = HudState::default();
    assert_eq!(
        hero_slot(&hud, 1),
        ("_unused", None, None, 1.0),
        "no builder"
    );
    hud.builder_image = Some("BuilderPortrait".to_owned());
    assert_eq!(
        hero_slot(&hud, 1),
        ("_unused", None, None, 1.0),
        "every builder busy - gone, not disabled"
    );
    hud.builders = vec![e[0], e[1]];
    assert_eq!(
        hero_slot(&hud, 1),
        (
            "_up",
            Some("BuilderPortrait".to_owned()),
            Some("2".to_owned()),
            1.0,
        ),
        "the builder slot's bar is always full"
    );
    hud.fielded_heroes = vec![hero(e[2], 3, 0.4, 400.0, false)];
    assert_eq!(
        hero_slot(&hud, 2),
        (
            "_up",
            Some("HeroPortrait".to_owned()),
            Some("3".to_owned()),
            0.4,
        ),
        "a fielded hero's own health fraction"
    );
    assert_eq!(hero_slot(&hud, 3), ("_unused", None, None, 1.0));
}

#[test]
fn the_hero_row_slides_heroes_left_when_no_builder_is_free() {
    let e = entities(2);
    let mut hud = HudState {
        builder_image: Some("BuilderPortrait".to_owned()),
        fielded_heroes: vec![
            hero(e[0], 3, 0.4, 400.0, false),
            hero(e[1], 5, 1.0, 900.0, false),
        ],
        ..Default::default()
    };

    assert_eq!(
        hero_slot(&hud, 1),
        (
            "_up",
            Some("HeroPortrait".to_owned()),
            Some("3".to_owned()),
            0.4,
        ),
        "the first hero slides into the builder's own slot"
    );
    assert_eq!(hero_at_slot(&hud, 1).map(|h| h.entity), Some(e[0]));
    assert_eq!(hero_at_slot(&hud, 2).map(|h| h.entity), Some(e[1]));
    assert_eq!(hero_at_slot(&hud, 3), None);

    hud.builders = vec![e[0]];
    assert_eq!(
        hero_slot(&hud, 1),
        (
            "_up",
            Some("BuilderPortrait".to_owned()),
            Some("1".to_owned()),
            1.0,
        ),
        "a free builder displaces the slid-in hero"
    );
    assert_eq!(hero_at_slot(&hud, 1), None, "slot 1 is the builder again");
    assert_eq!(hero_at_slot(&hud, 2).map(|h| h.entity), Some(e[0]));
    assert_eq!(hero_at_slot(&hud, 3).map(|h| h.entity), Some(e[1]));
}

fn hero(entity: Entity, rank: u16, health: f32, hit_points: f32, selected: bool) -> FieldedHero {
    FieldedHero {
        entity,
        template: "Hero".to_owned(),
        image: Some("HeroPortrait".to_owned()),
        rank,
        health,
        hit_points,
        selected,
        rank_progress: 0.25,
    }
}

#[test]
fn the_hero_rows_effects_play_once_on_a_hit_a_level_and_a_change_of_hero() {
    let e = entities(3);
    let mut memory = HeroRowMemory::default();
    let mut hud = HudState::default();
    let mut next = |hud: &HudState| memory.effects(hud);
    assert!(next(&hud).is_empty());
    hud.fielded_heroes = vec![hero(e[0], 1, 1.0, 500.0, false)];
    assert!(next(&hud).is_empty(), "a hero new to an empty slot");
    hud.fielded_heroes[0].hit_points = 450.0;
    assert_eq!(next(&hud), [HeroEffect::Attacked(1)]);
    assert!(next(&hud).is_empty(), "the same hit points again");
    hud.fielded_heroes[0].hit_points = 480.0;
    assert!(next(&hud).is_empty(), "healing");
    hud.fielded_heroes[0].rank = 2;
    assert_eq!(next(&hud), [HeroEffect::LevelUp(1)]);
    hud.fielded_heroes = vec![
        hero(e[1], 1, 1.0, 300.0, false),
        hero(e[0], 2, 1.0, 480.0, false),
    ];
    assert_eq!(
        next(&hud),
        [HeroEffect::Kill(1)],
        "no builder free, so slot 1 is the first hero: slot 2 is new, slot 1 changed hero"
    );
    hud.fielded_heroes.clear();
    assert_eq!(next(&hud), [HeroEffect::Kill(1), HeroEffect::Kill(2)]);
}

fn hero_effects_movie() -> Player {
    let mut s = Synth::new();
    let stop = stop(&mut s);
    let part = s.sprite(vec![vec![Item::Action(stop)]]);
    let button = s.sprite(vec![
        vec![
            Item::Label("_unused"),
            Item::Place(place(1, part).name("SelectedHighlight")),
            Item::Place(place(2, part).name("RankProgress")),
            Item::Place(place(3, part).name("HealthBar")),
        ],
        vec![Item::Action(stop)],
        vec![Item::Label("_up")],
        vec![Item::Action(stop)],
    ]);
    let mut code = Asm::new();
    code.function("SetButtonState", &["index", "state"], |b| {
        b.get_var("state").push_byte(1);
        b.push_str("Hero").get_var("index").op(0x47).op(0x1C);
        b.push_str("gotoAndPlay").op(op::CALL_METHOD).op(op::POP);
    });
    code.function(
        "SetButtonSelectedHighlightState",
        &["index", "state"],
        |b| {
            b.push_str("_root.highlight")
                .get_var("state")
                .op(op::SET_VARIABLE);
        },
    );
    code.function("SetButtonRankProgress", &["index", "value"], |b| {
        b.push_str("_root.progress")
            .get_var("value")
            .op(op::SET_VARIABLE);
    });
    for (name, letter) in [
        ("PlayButtonAttackedEffect", "A"),
        ("PlayButtonLevelUpEffect", "L"),
        ("KillButtonEffects", "K"),
    ] {
        code.function(name, &["index"], |b| {
            b.push_str("_root.log")
                .get_var("_root.log")
                .push_str(letter)
                .op(0x47)
                .op(op::SET_VARIABLE);
        });
    }
    for noop in [
        "Show",
        "SetFaction",
        "SetButtonHealthBar",
        "SetSelectAllHeroesButtonState",
    ] {
        code.function(noop, &[], |_| {});
    }
    code.end();
    let code = s.code(&mut code);
    let row = s.sprite(vec![vec![
        Item::Place(place(1, button).name("Hero1")),
        Item::Place(place(2, button).name("Hero2")),
        Item::Action(code),
        Item::Action(stop),
    ]]);
    player(
        &s,
        &vec![vec![
            Item::Place(place(1, row).name("HeroSelectUI")),
            Item::Action(stop),
        ]],
    )
}

#[test]
fn a_hero_portrait_shows_selection_progress_and_its_effects() {
    let mut p = hero_effects_movie();
    let mut ex = Exposure::new(Readiness::Immediate);
    let mut memory = HeroRowMemory::default();
    let e = entities(1);
    let mut hud = HudState {
        fielded_heroes: vec![hero(e[0], 1, 1.0, 500.0, true)],
        ..HudState::default()
    };
    let mut run = |hud: &HudState, p: &mut Player| {
        for _ in 0..LANDING_FRAMES {
            declare_hero_select(&mut ex, hud, "HeroSelectUI", "_level0.HeroSelectUI");
            let effects = memory.effects(hud);
            super::declare_hero_effects(&mut ex, "HeroSelectUI", &effects);
            ex.frame(p, 1);
        }
    };
    let read = |p: &mut Player, name: &str| {
        let root = p.root();
        p.variable(root, name)
    };
    let played = |p: &mut Player, letter: char| match read(p, "log") {
        Value::String(log) => log.chars().filter(|c| *c == letter).count(),
        _ => 0,
    };
    run(&hud, &mut p);
    assert_eq!(read(&mut p, "highlight"), Value::str("_show"));
    assert_eq!(read(&mut p, "progress"), Value::Number(25.0));
    assert_eq!(played(&mut p, 'A'), 0, "nothing hit it yet");
    hud.fielded_heroes[0].selected = false;
    hud.fielded_heroes[0].hit_points = 420.0;
    run(&hud, &mut p);
    assert_eq!(read(&mut p, "highlight"), Value::str("_hide"));
    assert_eq!(played(&mut p, 'A'), 1, "one hit, one flash");
    hud.fielded_heroes[0].rank = 2;
    run(&hud, &mut p);
    assert_eq!(played(&mut p, 'L'), 1);
    assert_eq!(played(&mut p, 'A'), 1);
    hud.fielded_heroes.clear();
    run(&hud, &mut p);
    assert_eq!(played(&mut p, 'K'), 1);
}

fn hero_select_movie() -> Player {
    let mut s = Synth::new();
    let stop = stop(&mut s);
    let button = s.sprite(vec![
        vec![Item::Label("_unused")],
        vec![Item::Action(stop)],
        vec![Item::Label("_up")],
        vec![Item::Action(stop)],
        vec![Item::Label("_disabled")],
        vec![Item::Action(stop)],
    ]);
    let mut code = Asm::new();
    code.function("SetButtonState", &["index", "state"], |b| {
        b.get_var("state").push_byte(1);
        b.push_str("Hero").get_var("index").op(0x47).op(0x1C);
        b.push_str("gotoAndPlay").op(op::CALL_METHOD).op(op::POP);
    });
    for noop in [
        "Show",
        "SetFaction",
        "SetButtonHealthBar",
        "SetSelectAllHeroesButtonState",
    ] {
        code.function(noop, &[], |_| {});
    }
    code.end();
    let code = s.code(&mut code);
    let row = s.sprite(vec![
        vec![
            Item::Place(place(1, button).name("Hero1")),
            Item::Place(place(2, button).name("Hero2")),
            Item::Action(code),
        ],
        vec![Item::Label("_fadein"), Item::Remove(1)],
        vec![],
        vec![
            Item::Place(place(1, button).name("Hero1")),
            Item::Action(stop),
        ],
    ]);
    player(
        &s,
        &vec![vec![
            Item::Place(place(1, row).name("HeroSelectUI")),
            Item::Action(stop),
        ]],
    )
}

#[test]
fn the_builder_button_survives_the_rows_own_intro() {
    let mut p = hero_select_movie();
    let mut ex = Exposure::new(Readiness::Immediate);
    let e = entities(2);
    let hud = HudState {
        builder_image: Some("BuilderPortrait".to_owned()),
        builders: e,
        ..HudState::default()
    };
    let path = "_level0.HeroSelectUI";
    let mut strings = HashMap::new();
    let mut images = Vec::new();
    for _ in 0..6 {
        declare_hero_select(&mut ex, &hud, "HeroSelectUI", path);
        ex.frame(&mut p, 1);
        for (key, value) in ex.take_string_changes() {
            if let Some(v) = value {
                strings.insert(key, v);
            }
        }
        images.extend(ex.take_image_changes());
    }
    assert!(
        in_segment(&p, "HeroSelectUI.Hero1", "_up"),
        "the re-created builder button is told again"
    );
    assert!(in_segment(&p, "HeroSelectUI.Hero2", "_unused"));
    assert_eq!(
        strings
            .get("_level0.HeroSelectUI_Hero1Rank")
            .map(String::as_str),
        Some("2")
    );
    assert!(images.contains(&ImageChange::Name {
        key: "_level0.HeroSelectUI_Hero1Image".to_owned(),
        content: Some(ImageSource::MappedImage("BuilderPortrait".to_owned())),
    }));
}

fn hero_flash_movie() -> Player {
    let mut s = Synth::new();
    let stop = stop(&mut s);
    let button = s.sprite(vec![vec![Item::Label("_up"), Item::Action(stop)]]);
    let flash = s.sprite(vec![
        vec![Item::Label("_hide"), Item::Action(stop)],
        vec![Item::Label("_show"), Item::Action(stop)],
    ]);
    let mut code = Asm::new();
    code.function("SetButtonFlashEffectState", &["index", "state"], |b| {
        b.get_var("state").push_byte(1);
        b.push_str("FlashEffect").get_var("index").op(0x47).op(0x1C);
        b.push_str("gotoAndPlay").op(op::CALL_METHOD).op(op::POP);
    });
    for noop in [
        "Show",
        "SetFaction",
        "SetButtonState",
        "SetButtonHealthBar",
        "SetSelectAllHeroesButtonState",
    ] {
        code.function(noop, &[], |_| {});
    }
    code.end();
    let code = s.code(&mut code);
    let row = s.sprite(vec![vec![
        Item::Place(place(1, button).name("Hero1")),
        Item::Place(place(2, button).name("Hero2")),
        Item::Place(place(3, flash).name("FlashEffect1")),
        Item::Place(place(4, flash).name("FlashEffect2")),
        Item::Action(code),
    ]]);
    player(
        &s,
        &vec![vec![
            Item::Place(place(1, row).name("HeroSelectUI")),
            Item::Action(stop),
        ]],
    )
}

#[test]
fn the_builder_and_hero_flashes_light_their_own_slots_for_their_own_time() {
    use crate::button_flash::ButtonFlashes;
    use crate::rules::map_scripts::UiFlashTarget;

    let mut p = hero_flash_movie();
    let mut ex = Exposure::new(Readiness::Immediate);
    let e = entities(2);
    let mut hud = HudState {
        builders: vec![e[0]],
        fielded_heroes: vec![hero(e[1], 1, 1.0, 100.0, false)],
        ..HudState::default()
    };
    let mut flashes = ButtonFlashes::default();
    flashes.raise(&UiFlashTarget::SelectBuilder, 30, 0, &hud.fielded_heroes);
    flashes.raise(
        &UiFlashTarget::HeroSelect("HERO".to_owned()),
        90,
        0,
        &hud.fielded_heroes,
    );
    let mut lit_at = |frame: u64| {
        hud.flash = flashes.view(frame);
        for _ in 0..LANDING_FRAMES {
            declare_hero_select(&mut ex, &hud, "HeroSelectUI", "_level0.HeroSelectUI");
            ex.frame(&mut p, 1);
        }
        [1, 2].map(|n| at_label(&p, &format!("HeroSelectUI.FlashEffect{n}"), "_show"))
    };
    assert_eq!(lit_at(0), [true, true]);
    assert_eq!(lit_at(30), [false, true]);
    assert_eq!(lit_at(90), [false, false]);
}

fn help_box_movie() -> Player {
    let mut s = Synth::new();
    let stop = stop(&mut s);
    let mut clip_code = Asm::new();
    for name in ["SampleContentWidth", "Show", "Hide"] {
        clip_code.function(name, &["height"], |f| {
            f.push_str("_root.last").push_str(name).op(op::SET_VARIABLE);
        });
    }
    clip_code.op(op::STOP).end();
    let clip_code = s.code(&mut clip_code);
    let help_box = s.sprite(vec![vec![Item::Action(clip_code)]]);
    let root = vec![vec![
        Item::Place(place(1, help_box).name("helpBox")),
        Item::Action(stop),
    ]];
    player(&s, &root)
}

fn last_call(p: &mut Player) -> Value {
    let root = p.root();
    p.variable(root, "last")
}

fn shown(height: f32) -> super::HelpBoxState {
    super::HelpBoxState::Shown {
        height,
        lines: std::sync::Arc::new(vec![crate::help_box::HelpLineSpec {
            text: "Fortress".to_owned(),
            font: "Albertus MT".to_owned(),
            point_size_bits: 16.0f32.to_bits(),
            bold: false,
            color: [255, 255, 255],
        }]),
    }
}

#[test]
fn an_unmeasured_box_samples_its_content_width() {
    let mut p = help_box_movie();
    let mut ex = Exposure::new(Readiness::Immediate);
    let key = "_level0.helpBox_Content";
    super::declare_help_box(&mut ex, key, &super::HelpBoxState::Sample);
    ex.frame(&mut p, 1);
    assert_eq!(last_call(&mut p), Value::str("SampleContentWidth"));
    assert!(ex.take_image_changes().contains(&ImageChange::Name {
        key: key.to_owned(),
        content: Some(ImageSource::Hidden),
    }));
    super::declare_help_box(&mut ex, key, &super::HelpBoxState::Hidden);
    ex.frame(&mut p, 1);
    assert_eq!(
        last_call(&mut p),
        Value::str("Hide"),
        "measured, the box hides"
    );
}

#[test]
fn show_and_hide_share_one_call_key_so_the_same_height_twice_still_refires() {
    let mut p = help_box_movie();
    let mut ex = Exposure::new(Readiness::Immediate);
    let key = "_level0.helpBox_Content";

    super::declare_help_box(&mut ex, key, &shown(100.0));
    ex.frame(&mut p, 1);
    assert_eq!(ex.stats().calls_issued, 1, "Show lands");

    super::declare_help_box(&mut ex, key, &super::HelpBoxState::Hidden);
    ex.frame(&mut p, 1);
    assert_eq!(ex.stats().calls_issued, 2, "Hide lands");

    super::declare_help_box(&mut ex, key, &shown(100.0));
    ex.frame(&mut p, 1);
    assert_eq!(
        ex.stats().calls_issued,
        3,
        "Show must refire even at the same height, since Hide ran between"
    );
    assert_eq!(last_call(&mut p), Value::str("Show"));
}

#[test]
fn restating_the_same_show_does_not_refire() {
    let mut p = help_box_movie();
    let mut ex = Exposure::new(Readiness::Immediate);
    let key = "_level0.helpBox_Content";

    super::declare_help_box(&mut ex, key, &shown(100.0));
    ex.frame(&mut p, 1);
    assert_eq!(ex.stats().calls_issued, 1);

    super::declare_help_box(&mut ex, key, &shown(100.0));
    ex.frame(&mut p, 1);
    assert_eq!(ex.stats().calls_issued, 1, "unchanged: not reissued");
}

#[test]
fn the_content_image_is_hidden_or_the_lines_shown() {
    let mut p = help_box_movie();
    let mut ex = Exposure::new(Readiness::Immediate);
    let key = "_level0.helpBox_Content";

    super::declare_help_box(&mut ex, key, &super::HelpBoxState::Hidden);
    ex.frame(&mut p, 1);
    let images = ex.take_image_changes();
    assert!(images.contains(&ImageChange::Name {
        key: key.to_owned(),
        content: Some(ImageSource::Hidden),
    }));

    super::declare_help_box(&mut ex, key, &shown(100.0));
    ex.frame(&mut p, 1);
    let images = ex.take_image_changes();
    assert!(images.iter().any(|change| matches!(
        change,
        ImageChange::Name { key: k, content: Some(ImageSource::HelpText(lines)) }
            if k == key && lines.len() == 1
    )));
}

mod spell_store {
    use super::{player, stop, Asm, ImageChange, ImageSource, Item, Player, Synth, Value};
    use crate::hud::{
        declare_player_magic, declare_spell_store, spell_button_state, spell_slot, SpellStoreEvent,
        SpellStoreSession,
    };
    use crate::spell_book::{SpellBookView, SpellSlotView, SpellState};
    use mint_sage_apt_player::expose::resolve_path;
    use mint_sage_apt_player::synth::{op, place};
    use mint_sage_apt_player::{Exposure, Readiness};

    fn recorder(a: &mut Asm, function: &str, record: &str, indexed: bool) {
        let name = format!("_root.{record}");
        if indexed {
            a.function(function, &["index", "state"], |b| {
                b.push_str(&name)
                    .get_var("index")
                    .op(0x47)
                    .get_var("state")
                    .op(op::SET_VARIABLE);
            });
        } else {
            a.function(function, &["value"], |b| {
                b.push_str(&name).get_var("value").op(op::SET_VARIABLE);
            });
        }
    }

    fn store_movie() -> Player {
        let mut s = Synth::new();
        let stop = stop(&mut s);
        let square = s.shape_box([0.0, 0.0, 10.0, 10.0]);
        let picture = s.sprite(vec![vec![Item::Place(place(1, square))]]);
        let button = s.sprite(vec![vec![
            Item::Place(place(1, picture).name("~Enabled")),
            Item::Action(stop),
        ]]);
        let buttons = s.sprite(vec![vec![
            Item::Place(place(1, button).name("Spell1")),
            Item::Place(place(2, button).name("Spell2")),
            Item::Action(stop),
        ]]);
        let store = s.sprite(vec![vec![
            Item::Place(place(1, buttons).name("Buttons")),
            Item::Action(stop),
        ]]);
        let mut code = Asm::new();
        recorder(&mut code, "SetSpellButtonState", "spell", true);
        recorder(&mut code, "SetLayout", "layout", false);
        recorder(&mut code, "ShowSpellHelpText", "help", false);
        recorder(&mut code, "Close", "closed", false);
        code.op(op::STOP).end();
        let code = s.code(&mut code);
        player(
            &s,
            &vec![vec![
                Item::Place(place(1, store).name("SpellStore")),
                Item::Action(code),
            ]],
        )
    }

    fn slot(science: &str, state: SpellState) -> SpellSlotView {
        SpellSlotView {
            science: science.to_owned(),
            image: Some(format!("SB_{science}")),
            name: format!("{science} name"),
            description: format!("{science} text"),
            cost: 5,
            state,
        }
    }

    fn view() -> SpellBookView {
        SpellBookView {
            points: 7,
            slots: vec![
                Some(slot("SCIENCE_Heal", SpellState::Available)),
                None,
                Some(slot("SCIENCE_Old", SpellState::Owned)),
            ],
        }
    }

    fn root(p: &mut Player, name: &str) -> Value {
        let at = p.root();
        p.variable(at, name)
    }

    #[test]
    fn the_store_is_filled_through_its_own_functions() {
        let mut p = store_movie();
        let mut ex = Exposure::new(Readiness::Immediate);
        let session = SpellStoreSession::default();
        for _ in 0..3 {
            declare_spell_store(&mut ex, &view(), &session);
            ex.frame(&mut p, 1);
        }
        assert_eq!(root(&mut p, "spell1"), Value::str("_active"));
        assert_eq!(root(&mut p, "spell2"), Value::str("_unused"));
        assert_eq!(root(&mut p, "layout"), Value::str("_multiplayer"));
        assert_eq!(root(&mut p, "help"), Value::str("_off"));
        let strings = ex.take_string_changes();
        assert!(strings.contains(&("SpellStoreSpellPoints".to_owned(), Some("7".to_owned()))));
        assert!(strings.contains(&("Spell1Cost".to_owned(), Some("5".to_owned()))));
        let picture = resolve_path(&p, "SpellStore.Buttons.Spell1.~Enabled").expect("exists");
        assert!(ex.take_image_changes().contains(&ImageChange::Instance {
            instance: picture,
            content: Some(ImageSource::MappedImage("SB_SCIENCE_Heal".to_owned())),
        }));
    }

    #[test]
    fn the_hovered_spell_is_described_and_a_close_is_asked_for() {
        let mut p = store_movie();
        let mut ex = Exposure::new(Readiness::Immediate);
        let session = SpellStoreSession {
            hovered: Some(1),
            closing: true,
            ..SpellStoreSession::default()
        };
        declare_spell_store(&mut ex, &view(), &session);
        ex.frame(&mut p, 1);
        assert_eq!(root(&mut p, "help"), Value::str("_on"));
        let strings = ex.take_string_changes();
        assert!(strings.contains(&(
            "SpellHelpText".to_owned(),
            Some("SCIENCE_Heal name".to_owned())
        )));
        assert!(strings.contains(&(
            "SpellDescription".to_owned(),
            Some("SCIENCE_Heal text".to_owned())
        )));
        let closing = ex.stats().calls_issued;
        let mut ex = Exposure::new(Readiness::Immediate);
        let mut p = store_movie();
        declare_spell_store(&mut ex, &view(), &SpellStoreSession::default());
        ex.frame(&mut p, 1);
        assert_eq!(
            closing,
            ex.stats().calls_issued + 1,
            "closing issues exactly one more call, Close"
        );
    }

    #[test]
    fn a_spells_button_state_follows_its_state() {
        let owned = slot("A", SpellState::Owned);
        assert_eq!(spell_button_state(Some(&owned), true), "_purchased");
        assert_eq!(
            spell_button_state(Some(&owned), false),
            "_already_purchased"
        );
        assert_eq!(
            spell_button_state(Some(&slot("A", SpellState::Available)), false),
            "_active"
        );
        assert_eq!(
            spell_button_state(Some(&slot("A", SpellState::CantAfford)), false),
            "_disabled"
        );
        assert_eq!(
            spell_button_state(Some(&slot("A", SpellState::Locked)), false),
            "_disabled_level"
        );
        assert_eq!(spell_button_state(None, false), "_unused");
        let session = SpellStoreSession::open(&view());
        assert_eq!(session.owned_at_open, vec!["SCIENCE_Old".to_owned()]);
    }

    #[test]
    fn a_spell_button_is_named_spell_and_its_slot() {
        assert_eq!(spell_slot("Spell1"), Some(1));
        assert_eq!(spell_slot("spell12"), Some(12));
        assert_eq!(spell_slot("Spell0"), None);
        assert_eq!(spell_slot("Spell13"), None);
        assert_eq!(spell_slot("SpellHelpText"), None);
    }

    #[test]
    fn the_stores_events_drive_the_visit() {
        let view = view();
        let mut session = SpellStoreSession::default();
        let on = |s: &mut SpellStoreSession, name: &str, args: &str| s.on_event(&view, name, args);
        assert_eq!(
            on(&mut session, "OnRollOverBttnSpell", "Spell3"),
            SpellStoreEvent::Nothing
        );
        assert_eq!(session.hovered, Some(3));
        on(&mut session, "OnRollOutBttnSpell", "Spell1");
        assert_eq!(session.hovered, Some(3), "a rollout of another button");
        on(&mut session, "OnRollOutBttnSpell", "Spell3");
        assert_eq!(session.hovered, None);
        on(&mut session, "OnBttnSpell", "Spell1");
        assert_eq!(session.provisional, vec!["SCIENCE_Heal".to_owned()]);
        assert_eq!(
            on(&mut session, "OnBttnSpell", "Spell3"),
            SpellStoreEvent::Nothing,
            "owned"
        );
        assert_eq!(
            on(&mut session, "OnBttnSpell", "Spell2"),
            SpellStoreEvent::Nothing,
            "empty"
        );
        on(&mut session, "OnBttnClose", "");
        assert!(session.closing);
        assert_eq!(
            on(&mut session, "OnClosed", ""),
            SpellStoreEvent::Closed(vec!["SCIENCE_Heal".to_owned()])
        );
    }

    #[test]
    fn a_reset_drops_the_visits_buys_and_a_close_confirms_them_in_order() {
        let mut view = view();
        view.slots
            .push(Some(slot("SCIENCE_Rally", SpellState::Available)));
        let mut session = SpellStoreSession::open(&view);
        let press = |session: &mut SpellStoreSession, spell: &str| {
            session.on_event(&view, "OnBttnSpell", spell);
        };
        press(&mut session, "Spell1");
        press(&mut session, "Spell4");
        session.on_event(&view, "OnBttnReset", "");
        assert!(session.provisional.is_empty());
        assert_eq!(session.owned_at_open, vec!["SCIENCE_Old".to_owned()]);
        press(&mut session, "Spell4");
        press(&mut session, "Spell1");
        assert_eq!(
            session.on_event(&view, "OnClosed", ""),
            SpellStoreEvent::Closed(vec!["SCIENCE_Rally".to_owned(), "SCIENCE_Heal".to_owned()])
        );
        assert!(session.provisional.is_empty());
    }

    #[test]
    fn the_spell_book_button_is_enabled_on_its_clip() {
        let mut s = Synth::new();
        let stop = stop(&mut s);
        let clip = s.sprite(vec![vec![Item::Action(stop)]]);
        let magic = s.sprite(vec![
            vec![],
            vec![
                Item::Place(place(1, clip).name("ButtonClip")),
                Item::Action(stop),
            ],
        ]);
        let buttons = s.sprite(vec![vec![
            Item::Place(place(1, magic).name("PlayerMagic")),
            Item::Action(stop),
        ]]);
        let row = s.sprite(vec![vec![
            Item::Place(place(1, buttons).name("Buttons")),
            Item::Action(stop),
        ]]);
        let mut code = Asm::new();
        recorder(&mut code, "EnablePlayerMagicButton", "enabled", false);
        code.op(op::STOP).end();
        let code = s.code(&mut code);
        let mut p = player(
            &s,
            &vec![vec![
                Item::Place(place(1, row).name("PalantirButtons")),
                Item::Action(code),
            ]],
        );
        let mut ex = Exposure::new(Readiness::Immediate);
        declare_player_magic(&mut ex, true);
        ex.frame(&mut p, 1);
        assert_eq!(root(&mut p, "enabled"), Value::Undefined, "no clip yet");
        for _ in 0..3 {
            declare_player_magic(&mut ex, true);
            ex.frame(&mut p, 1);
        }
        assert_eq!(
            root(&mut p, "enabled"),
            Value::str("1"),
            "retail sends a string"
        );
    }
}

#[test]
fn a_site_under_construction_is_still_a_radial_subject() {
    let mut world = mint_sim::World::new();
    let site = crate::render_state::UnitView {
        entity: world.spawn().unwrap(),
        position: mint_movement::Vec2::new(0.0, 0.0),
        ground: 0.0,
        facing: 0.0,
        radius: 5.0,
        height: 5.0,
        kind_of: mint_sage_asset::KindOf::STRUCTURE,
        targetable: false,
        pick_type: mint_sage_asset::PickType::Selectable,
        owner: crate::game::LOCAL_PLAYER,
        selected: true,
        moving: false,
        health: Some(0.2),
        dying: false,
        visibility: mint_sage_shroud::ObjectVisibility::Clear,
        speed: 0.0,
        weapon_cycle: 0.0,
        attack_cycle: 0,
        attack_slot: None,
        kind: None,
        art_kind: None,
        told_kind: None,
        hover_kind: None,
        conditions: mint_sage_scene::Conditions::ACTIVELY_BEING_CONSTRUCTED,
        build_elapsed: Some(30),
        build_total: None,
        under_repair: false,
        held: false,
    };
    assert!(super::is_radial_subject(&site));
}

#[test]
fn a_command_center_ring_is_at_least_two_cells() {
    assert_eq!(super::command_center_ring_cells(0.0, 1000.0), 2.0);
    assert_eq!(super::command_center_ring_cells(5.0, 1000.0), 2.0);
    assert_eq!(super::command_center_ring_cells(130.0, 1000.0), 17.0);
}

#[test]
fn radar_mark_follows_r3_2_s_kindof_order() {
    use crate::apt_overlay::RadarMark;
    let mark = |kinds: &'static [&'static str]| {
        super::radar_mark(
            |token| kinds.contains(&token),
            10.0,
            0.0,
            Some(20.0),
            Some(4.0),
            1000.0,
        )
    };
    assert_eq!(mark(&["COMMANDCENTER"]), RadarMark::Ring(2.0));
    assert_eq!(mark(&["HERO"]), RadarMark::Hero);
    match mark(&["WALL_SEGMENT"]) {
        RadarMark::Wall {
            facing,
            half_major,
            half_minor,
            bounding_radius_cells,
            scale,
        } => {
            assert_eq!((facing, half_major, half_minor), (0.0, 20.0, 4.0));
            assert!((bounding_radius_cells - 1.28).abs() < 0.001);
            assert!((scale - 0.128).abs() < 0.001);
        }
        other => panic!("expected a Wall mark, got {other:?}"),
    }
    assert_eq!(mark(&[]), RadarMark::Blip);
    assert_eq!(mark(&["HERO", "COMMANDCENTER"]), RadarMark::Hero);
}

#[test]
fn radar_color_boosts_saturation_by_1_6_and_keeps_grey_and_alpha() {
    let near = |got: [f32; 4], want: [u8; 3]| {
        for (g, w) in got.iter().zip(want) {
            assert!(
                (g * 255.0 - f32::from(w)).abs() <= 1.0,
                "{got:?} vs {want:?}"
            );
        }
    };
    near(super::radar_color([1.0, 0.0, 0.0, 1.0]), [255, 0, 0]);
    near(
        super::radar_color([200.0 / 255.0, 100.0 / 255.0, 100.0 / 255.0, 1.0]),
        [200, 40, 40],
    );
    assert_eq!(
        super::radar_color([0.5, 0.5, 0.5, 0.25]),
        [0.5, 0.5, 0.5, 0.25]
    );
    assert!((super::radar_color([0.9, 0.5, 0.4, 0.3])[3] - 0.3).abs() < f32::EPSILON);
}

#[test]
fn hero_pulse_is_a_triangle_wave_over_sixty_frames() {
    let at = |t| super::hero_pulse_alpha(t);
    assert_eq!(
        [at(0), at(15), at(30), at(45), at(59)],
        [64, 159, 255, 160, 71]
    );
    assert_eq!(at(60), 64);
}

mod palantir_rank_tests {
    use super::super::{percent, HudState, PalantirMemory};
    use crate::spell_book::{SpellBookHoldings, SpellState};
    use mint_sage_apt_player::Value;

    fn hud_with(rank: u16, evil: bool, states: Vec<(&str, SpellState)>) -> HudState {
        HudState {
            rank,
            evil,
            spell_book: SpellBookHoldings {
                states: states.into_iter().map(|(n, s)| (n.to_owned(), s)).collect(),
                ..SpellBookHoldings::default()
            },
            ..HudState::default()
        }
    }

    #[test]
    fn level_up_fires_once_per_rise_never_on_first_sample_or_a_drop() {
        let mut memory = PalantirMemory::default();
        assert!(
            !memory.update(&hud_with(1, false, vec![]), false, true),
            "first sample"
        );
        assert!(
            memory.update(&hud_with(2, false, vec![]), false, true),
            "rank rose"
        );
        assert!(
            !memory.update(&hud_with(2, false, vec![]), false, true),
            "no change"
        );
        assert!(
            !memory.update(&hud_with(1, false, vec![]), false, true),
            "a drop"
        );
    }

    #[test]
    fn highlight_follows_a_newly_purchasable_slot_and_the_store_clears_it() {
        let mut memory = PalantirMemory::default();
        memory.update(
            &hud_with(1, false, vec![("A", SpellState::Locked)]),
            false,
            true,
        );
        assert!(!memory.highlighted());
        memory.update(
            &hud_with(1, false, vec![("A", SpellState::Available)]),
            false,
            true,
        );
        assert!(memory.highlighted(), "newly purchasable");
        memory.update(
            &hud_with(1, false, vec![("A", SpellState::Available)]),
            true,
            true,
        );
        assert!(!memory.highlighted(), "the store opening clears it");
    }

    #[test]
    fn progress_is_one_at_the_last_rank_not_a_hundred() {
        assert_eq!(percent(1.0), Value::Number(1.0));
        assert_eq!(percent(0.5), Value::Number(50.0));
    }
}

#[test]
fn stealth_hides_foreign_marks_and_pulses_only_an_own_hidden_hero() {
    let gate = super::stealth_alpha;
    assert_eq!(gate(true, false, true, 0), None);
    assert_eq!(gate(false, false, true, 15), Some(1.0));
    assert_eq!(gate(false, true, false, 15), Some(1.0));
    assert_eq!(gate(false, true, true, 0), Some(64.0 / 255.0));
}
