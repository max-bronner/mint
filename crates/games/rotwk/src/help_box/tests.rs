use std::time::Duration;

use mint_sage_ini::{BlockTable, Document, MemorySources, Reader};
use mint_sage_str::{StringRecord, StringTable};

use super::{
    build_shortcut, command_button_help, help_text_lines, native_help, read_help_box_config,
    read_object_help, strip_mnemonic, with_language_fonts, world_help, HelpBoxConfig, HelpBoxTimer,
    HelpText, HoverKey, LineStyle, ObjectHelp, StatKind,
};
use crate::command_bar::SlotAction;

fn parse(text: &str) -> Document {
    let mut sources = MemorySources::new();
    sources.insert("data/ini/ingameui.ini", text.as_bytes().to_vec());
    let table = BlockTable::corpus_path();
    Reader::new(&sources)
        .read("data/ini/ingameui.ini", &table)
        .expect("synthetic ingameui.ini parses")
}

fn table(pairs: &[(&str, &str)]) -> StringTable {
    let mut table = StringTable::new();
    table.insert(pairs.iter().map(|&(label, text)| StringRecord {
        label: label.to_owned(),
        text: text.to_owned(),
        speech: None,
    }));
    table
}

#[test]
fn nothing_shows_before_the_delay() {
    let mut timer = HelpBoxTimer::default();
    let delay = Duration::from_millis(500);
    assert_eq!(
        timer.update(Some(HoverKey::Ring(0)), Duration::from_millis(0), delay),
        None
    );
    assert_eq!(
        timer.update(Some(HoverKey::Ring(0)), Duration::from_millis(499), delay),
        None
    );
}

#[test]
fn the_same_hover_held_past_the_delay_shows() {
    let mut timer = HelpBoxTimer::default();
    let delay = Duration::from_millis(500);
    let _ = timer.update(Some(HoverKey::Ring(0)), Duration::from_millis(0), delay);
    assert_eq!(
        timer.update(Some(HoverKey::Ring(0)), Duration::from_millis(500), delay),
        Some(HoverKey::Ring(0))
    );
    assert_eq!(
        timer.update(Some(HoverKey::Ring(0)), Duration::from_millis(900), delay),
        Some(HoverKey::Ring(0))
    );
}

#[test]
fn a_different_hover_hides_at_once_and_restarts_the_wait() {
    let mut timer = HelpBoxTimer::default();
    let delay = Duration::from_millis(500);
    let _ = timer.update(Some(HoverKey::Ring(0)), Duration::from_millis(0), delay);
    let _ = timer.update(Some(HoverKey::Ring(0)), Duration::from_millis(500), delay);
    assert_eq!(
        timer.update(Some(HoverKey::Ring(1)), Duration::from_millis(500), delay),
        None,
        "hides at once"
    );
    assert_eq!(
        timer.update(Some(HoverKey::Ring(1)), Duration::from_millis(600), delay),
        None,
        "the new wait has not elapsed yet"
    );
    assert_eq!(
        timer.update(Some(HoverKey::Ring(1)), Duration::from_millis(1000), delay),
        Some(HoverKey::Ring(1))
    );
}

#[test]
fn losing_the_hover_hides_at_once() {
    let mut timer = HelpBoxTimer::default();
    let delay = Duration::from_millis(500);
    let _ = timer.update(Some(HoverKey::Ring(0)), Duration::from_millis(0), delay);
    let _ = timer.update(Some(HoverKey::Ring(0)), Duration::from_millis(500), delay);
    assert_eq!(timer.update(None, Duration::from_millis(600), delay), None);
}

#[test]
fn only_a_build_slot_carries_a_shortcut() {
    assert_eq!(build_shortcut(&SlotAction::Build(0)), Some(1));
    assert_eq!(build_shortcut(&SlotAction::Build(8)), Some(9));
    assert_eq!(build_shortcut(&SlotAction::RadialPop), None);
    assert_eq!(
        build_shortcut(&SlotAction::Unimplemented("an unrecognised command")),
        None
    );
}

#[test]
fn a_command_button_assembles_its_four_lines() {
    let strings = table(&[
        ("CONTROLBAR:ConstructThing", "Build Thing"),
        ("CONTROLBAR:ToolTipThing", "Makes a Thing"),
        ("TOOLTIP:Cost", "Cost: %d"),
        ("TOOLTIP:Shortcut", "Shortcut: %s"),
    ]);
    let help = command_button_help(
        Some("CONTROLBAR:ConstructThing"),
        Some("CONTROLBAR:ToolTipThing"),
        &SlotAction::Build(2),
        Some(250),
        &strings,
    );
    assert_eq!(help.title, "Build Thing");
    assert_eq!(help.stat.as_deref(), Some("Cost: 250"));
    assert_eq!(help.shortcut.as_deref(), Some("Shortcut: 3"));
    assert_eq!(help.description.as_deref(), Some("Makes a Thing"));
}

#[test]
fn no_cost_and_no_text_label_are_handled() {
    let strings = table(&[]);
    let help = command_button_help(None, None, &SlotAction::RadialPop, None, &strings);
    assert_eq!(help.title, "");
    assert_eq!(help.stat, None);
    assert_eq!(help.shortcut, None);
    assert_eq!(help.description, None);

    let help = command_button_help(None, None, &SlotAction::Build(0), Some(0), &strings);
    assert_eq!(help.stat, None, "a zero cost is not shown");
}

#[test]
fn a_command_buttons_title_strips_its_mnemonic_too() {
    let strings = table(&[("CONTROLBAR:FortressUpgrades", "Fortress &Upgrades")]);
    let help = command_button_help(
        Some("CONTROLBAR:FortressUpgrades"),
        None,
        &SlotAction::Unimplemented("an unrecognised command"),
        None,
        &strings,
    );
    assert_eq!(help.title, "Fortress Upgrades");
}

#[test]
fn a_world_object_shows_its_health_not_a_cost() {
    let strings = table(&[
        ("OBJECT:MenFortress", "Fortress"),
        (
            "OBJECT:MenFortressDescription",
            "Recruits Heroes and Builders",
        ),
        ("TOOLTIP:MaxHealth", "Health: %d"),
    ]);
    let object = ObjectHelp {
        display_name: Some("OBJECT:MenFortress".to_owned()),
        literal_name: None,
        description: Some("OBJECT:MenFortressDescription".to_owned()),
    };
    let help = world_help(&object, Some(7500.0), &strings);
    assert_eq!(help.title, "Fortress");
    assert_eq!(help.stat.as_deref(), Some("Health: 7500"));
    assert_eq!(help.stat_kind, StatKind::Health);
    assert_eq!(help.shortcut, None);
    assert_eq!(
        help.description.as_deref(),
        Some("Recruits Heroes and Builders")
    );
}

#[test]
fn no_health_shows_no_stat_line() {
    let strings = table(&[("TOOLTIP:MaxHealth", "Health: %d")]);
    let object = ObjectHelp::default();
    assert_eq!(world_help(&object, None, &strings).stat, None);
    assert_eq!(world_help(&object, Some(0.0), &strings).stat, None);
}

#[test]
fn the_builder_button_matches_the_owners_screenshot() {
    let strings = table(&[
        ("CONTROLBAR:SelectNearestBuilder", "Select Nearest &Builder"),
        (
            "CONTROLBAR:SelectNearestBuilderDescription",
            "Click here to select the nearest builder who is ready to work",
        ),
        ("TOOLTIP:Shortcut", "Shortcut: %s"),
    ]);
    let help = native_help("SelectNearestBuilder", &strings).expect("a known name");
    assert_eq!(
        help.title, "Select Nearest Builder",
        "the mnemonic is stripped"
    );
    assert_eq!(help.shortcut.as_deref(), Some("Shortcut: B"));
    assert_eq!(
        help.description.as_deref(),
        Some("Click here to select the nearest builder who is ready to work")
    );
    assert_eq!(help.stat, None);
}

#[test]
fn an_unknown_native_name_is_none() {
    let strings = table(&[]);
    assert_eq!(native_help("SomethingElse", &strings), None);
}

#[test]
fn strip_mnemonic_removes_every_ampersand() {
    assert_eq!(
        strip_mnemonic("Select Nearest &Builder"),
        "Select Nearest Builder"
    );
    assert_eq!(strip_mnemonic("plain"), "plain");
}

#[test]
fn the_delay_and_fonts_read_from_the_last_block() {
    let doc = parse(
        "InGameUI\n\
         UnitHelpTextDelay = 1.0\n\
         HelpBoxNameFont = \"Albertus MT\"\n\
         HelpBoxNamePointSize = 16\n\
         HelpBoxNameBold = No\n\
         HelpBoxNameColor = R:255 G:255 B:255\n\
         HelpBoxCostPointSize = 14\n\
         HelpBoxCostColor = R:255 G:204 B:0\n\
         End\n",
    );
    let config = read_help_box_config(std::slice::from_ref(&doc));
    assert_eq!(config.delay, Duration::from_secs_f32(1.0));
    assert_eq!(config.name.font, "Albertus MT");
    assert_eq!(config.name.point_size, 16.0);
    assert!(!config.name.bold);
    assert_eq!(config.name.color, [255, 255, 255]);
    assert_eq!(config.cost.font, "Albertus MT");
    assert_eq!(config.cost.point_size, 14.0);
    assert_eq!(config.cost.color, [255, 204, 0]);
}

#[test]
fn a_later_block_overrides_an_earlier_ones_field() {
    let doc = parse(
        "InGameUI\n  UnitHelpTextDelay = 1.0\nEnd\n\
         InGameUI\n  UnitHelpTextDelay = 2.5\nEnd\n",
    );
    let config = read_help_box_config(std::slice::from_ref(&doc));
    assert_eq!(config.delay, Duration::from_secs_f32(2.5));
}

#[test]
fn missing_fields_keep_their_default() {
    let doc = parse("InGameUI\n  UnitHelpTextDelay = 2.0\nEnd\n");
    let config = read_help_box_config(std::slice::from_ref(&doc));
    assert_eq!(config.delay, Duration::from_secs_f32(2.0));
    assert_eq!(config.description, LineStyle::default());
}

fn parse_language(text: &str) -> Document {
    let mut sources = MemorySources::new();
    sources.insert("language.ini", text.as_bytes().to_vec());
    Reader::new(&sources)
        .read("language.ini", &super::help_box_table())
        .expect("synthetic language.ini parses")
}

#[test]
fn language_fonts_replace_family_size_and_bold_but_not_colour() {
    let doc = parse_language(
        "Language
         HelpBoxNameFont = \"Omnia LT Std\" 18 Yes; the name
         HelpBoxDescriptionFont = \"Albertus MT\" 12 No; used to display
         End
",
    );
    let config = with_language_fonts(HelpBoxConfig::default(), std::slice::from_ref(&doc));
    assert_eq!(config.name.font, "Omnia LT Std");
    assert_eq!(config.name.point_size, 18.0);
    assert!(config.name.bold);
    assert_eq!(
        config.name.color,
        [255, 255, 255],
        "colour is ingameui.ini's"
    );
    assert_eq!(config.description.point_size, 12.0);
    assert_eq!(
        config.cost,
        HelpBoxConfig::default().cost,
        "not named: kept"
    );
}

#[test]
fn an_incomplete_language_font_is_ignored() {
    let doc = parse_language(
        "Language
  HelpBoxNameFont = \"Albertus MT\" 18
End
",
    );
    let config = with_language_fonts(HelpBoxConfig::default(), std::slice::from_ref(&doc));
    assert_eq!(config.name, HelpBoxConfig::default().name);
}

#[test]
fn a_health_line_takes_the_description_font() {
    let mut config = HelpBoxConfig::default();
    config.cost.point_size = 14.0;
    config.description.point_size = 12.0;
    let text = |stat_kind| HelpText {
        title: "Fortress".to_owned(),
        stat: Some("7500".to_owned()),
        stat_kind,
        ..HelpText::default()
    };
    let health = help_text_lines(&text(StatKind::Health), &config);
    assert_eq!(health[1].point_size(), 12.0);
    let cost = help_text_lines(&text(StatKind::Cost), &config);
    assert_eq!(cost[1].point_size(), 14.0);
}

#[test]
fn an_objects_display_name_and_description_are_read() {
    let mut sources = MemorySources::new();
    sources.insert(
        "data/ini/object/fortress.ini",
        "Object MenFortressCitadel\n\
         DisplayName = OBJECT:MenFortress\n\
         Description = OBJECT:MenFortressDescription\n\
         End\n"
            .as_bytes()
            .to_vec(),
    );
    let doc = Reader::new(&sources)
        .read("data/ini/object/fortress.ini", &BlockTable::corpus_path())
        .expect("synthetic object.ini parses");
    let read = read_object_help(std::slice::from_ref(&doc));
    let help = read
        .get("menfortresscitadel")
        .expect("the object is keyed lower-cased");
    assert_eq!(help.display_name.as_deref(), Some("OBJECT:MenFortress"));
    assert_eq!(
        help.description.as_deref(),
        Some("OBJECT:MenFortressDescription")
    );
}

#[test]
fn none_clears_a_field_and_a_bare_object_is_not_kept() {
    let mut sources = MemorySources::new();
    sources.insert(
        "data/ini/object/x.ini",
        "Object HasNone\n  DisplayName = NONE\n  Description = OBJECT:XDesc\nEnd\n\
         Object HasNeither\n  SelectPortrait = Something\nEnd\n"
            .as_bytes()
            .to_vec(),
    );
    let doc = Reader::new(&sources)
        .read("data/ini/object/x.ini", &BlockTable::corpus_path())
        .expect("synthetic object.ini parses");
    let read = read_object_help(std::slice::from_ref(&doc));
    let has_none = read.get("hasnone").expect("kept: it names a description");
    assert_eq!(has_none.display_name, None);
    assert_eq!(has_none.description.as_deref(), Some("OBJECT:XDesc"));
    assert!(!read.contains_key("hasneither"));
}

#[test]
fn every_present_line_becomes_its_own_styled_spec_in_order() {
    let config = HelpBoxConfig::default();
    let text = HelpText {
        title: "Fortress".to_owned(),
        stat: Some("Cost: 300".to_owned()),
        stat_kind: StatKind::Cost,
        shortcut: Some("Shortcut: B".to_owned()),
        description: Some("Recruits Heroes and Builders".to_owned()),
    };
    let lines = help_text_lines(&text, &config);
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].text, "Fortress");
    assert_eq!(lines[0].point_size(), config.name.point_size);
    assert_eq!(lines[0].color, config.name.color);
    assert_eq!(lines[1].text, "Cost: 300");
    assert_eq!(lines[1].point_size(), config.cost.point_size);
    assert_eq!(lines[2].text, "Shortcut: B");
    assert_eq!(lines[3].text, "Recruits Heroes and Builders");
    assert_eq!(lines[3].color, config.description.color);
}

#[test]
fn an_absent_line_is_skipped_not_drawn_empty() {
    let config = HelpBoxConfig::default();
    let text = HelpText {
        title: "Players".to_owned(),
        stat: None,
        stat_kind: StatKind::Cost,
        shortcut: None,
        description: Some("View Player Status and Tribute".to_owned()),
    };
    let lines = help_text_lines(&text, &config);
    assert_eq!(lines.len(), 2, "only the title and the description");
    assert_eq!(lines[0].text, "Players");
    assert_eq!(lines[1].text, "View Player Status and Tribute");
}

#[test]
fn a_line_specs_point_size_round_trips_through_its_bits() {
    let config = HelpBoxConfig::default();
    let text = HelpText {
        title: "T".to_owned(),
        ..HelpText::default()
    };
    let lines = help_text_lines(&text, &config);
    assert_eq!(lines[0].point_size(), config.name.point_size);
}
