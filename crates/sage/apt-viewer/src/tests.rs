use super::*;
use mint_sage_apt::{Export, Movie, Rect};
use mint_sage_str::{StringRecord, StringTable};
use std::path::Path;

fn table() -> StringTable {
    let mut table = StringTable::new();
    table.insert([
        StringRecord {
            label: "APT:SoloPlay".to_owned(),
            text: "SOLO PLAY".to_owned(),
            speech: None,
        },
        StringRecord {
            label: "GUI:Player".to_owned(),
            text: "Player".to_owned(),
            speech: None,
        },
    ]);
    table
}

#[test]
fn a_dollar_key_resolves_through_the_apt_namespace() {
    assert_eq!(localise("$SoloPlay", &table()), "SOLO PLAY");
    assert!(localises("$SoloPlay", &table()));
}

#[test]
fn style_modifiers_are_not_part_of_the_key() {
    assert_eq!(localise("$SoloPlay&dropshadow", &table()), "SOLO PLAY");
    assert!(localises("$SoloPlay&dropshadow", &table()));
}

#[test]
fn text_style_reads_the_dropshadow_modifier() {
    assert!(text_style("$PlayerRank&dropshadow").drop_shadow);
    assert!(!text_style("$PlayerRank").drop_shadow, "no modifier at all");
    assert!(!text_style("plain text").drop_shadow, "no marker");
    assert!(
        !text_style("$Something&someothermodifier").drop_shadow,
        "an unrecognised modifier is ignored, not mistaken for dropshadow"
    );
}

#[test]
fn literal_text_drops_its_dropshadow_marker() {
    assert!(text_style("40%&dropShadow").drop_shadow);
    assert_eq!(localise("40%&dropShadow", &table()), "40%");
    assert_eq!(localise("R&D", &table()), "R&D", "any other & is text");
}

#[test]
fn an_already_namespaced_key_resolves_raw() {
    assert_eq!(localise("$GUI:Player", &table()), "Player");
}

#[test]
fn text_without_a_dollar_is_left_alone() {
    assert_eq!(localise("SOLO PLAY", &table()), "SOLO PLAY");
    assert_eq!(localise("$", &table()), "$");
    assert!(!localises("SOLO PLAY", &table()));
}

#[test]
fn a_dynamic_string_wins_over_the_static_table() {
    let mut dynamic = HashMap::new();
    dynamic.insert("SoloPlay".to_owned(), "1250".to_owned());
    assert_eq!(localise_with("$SoloPlay", &dynamic, &table()), "1250");
    assert_eq!(
        localise_with("$SoloPlay&dropshadow", &dynamic, &table()),
        "1250"
    );
    assert_eq!(localise_with("$GUI:Player", &dynamic, &table()), "Player");
    assert_eq!(localise_with("SoloPlay", &dynamic, &table()), "SoloPlay");
}

#[test]
fn a_missing_key_reports_itself_the_way_the_engine_does() {
    assert_eq!(localise("$Nope", &table()), "MISSING: 'APT:Nope'");
    assert!(!localises("$Nope", &table()));
}

fn nav_label_box() -> TextBox {
    TextBox {
        bounds: Rect {
            left: -2.0,
            top: -2.0,
            right: 158.0,
            bottom: 39.2,
        },
        alignment: 2,
        text_width: 75.2,
        ascent: 10.5,
        descent: 3.5,
        win_ascent: 10.5,
        win_descent: 3.5,
    }
}

#[test]
fn alignment_two_centres_the_run_in_its_box() {
    let text_box = nav_label_box();
    let [x, _] = text_origin(text_box, TextAnchor::Centred);
    let run_centre = x + text_box.text_width / 2.0;
    let box_centre = (text_box.bounds.left + text_box.bounds.right) / 2.0;
    assert!(
        (run_centre - box_centre).abs() < 0.01,
        "centred text puts the run's middle on the box's middle; \
             got {run_centre} against {box_centre}"
    );
}

#[test]
fn centring_is_not_the_same_as_left_aligning() {
    let mut text_box = nav_label_box();
    let [centred, _] = text_origin(text_box, TextAnchor::Centred);
    text_box.alignment = 0;
    let [left, _] = text_origin(text_box, TextAnchor::Centred);
    assert!(
        centred - left > 39.0 && centred - left < 42.0,
        "centring a 75.2-unit run in a 156-unit content box moves it \
             ~40 units right; got {}",
        centred - left
    );
}

#[test]
fn alignment_one_puts_the_run_flush_against_the_right_gutter() {
    let mut text_box = nav_label_box();
    text_box.alignment = 1;
    let [x, _] = text_origin(text_box, TextAnchor::Centred);
    let run_right = x + text_box.text_width;
    assert!((run_right - (text_box.bounds.right - TEXT_GUTTER)).abs() < 0.01);
}

#[test]
fn an_unknown_alignment_draws_left_rather_than_refusing_to_draw() {
    let mut text_box = nav_label_box();
    text_box.alignment = 99;
    let [x, _] = text_origin(text_box, TextAnchor::Centred);
    assert!((x - (text_box.bounds.left + TEXT_GUTTER)).abs() < 0.01);
}

#[test]
fn top_anchoring_puts_the_first_baseline_one_ascent_below_the_gutter() {
    let text_box = nav_label_box();
    let [_, y] = text_origin(text_box, TextAnchor::Top);
    assert!((y - (text_box.bounds.top + TEXT_GUTTER + text_box.ascent)).abs() < 0.01);
}

#[test]
fn centred_anchoring_centres_the_line_box_not_the_baseline() {
    let text_box = nav_label_box();
    let [_, y] = text_origin(text_box, TextAnchor::Centred);
    let top = y - text_box.win_ascent;
    let bottom = y + text_box.win_descent;
    let box_centre = (text_box.bounds.top + text_box.bounds.bottom) / 2.0;
    assert!(
        ((top + bottom) / 2.0 - box_centre).abs() < 0.01,
        "line box {top}..{bottom} should straddle {box_centre}"
    );
}

#[test]
fn centred_uses_the_gdi_line_box() {
    let hhea = nav_label_box();
    let gdi = TextBox {
        win_ascent: 13.244,
        ..hhea
    };
    let [_, y_hhea] = text_origin(hhea, TextAnchor::Centred);
    let [_, y_gdi] = text_origin(gdi, TextAnchor::Centred);
    assert!(
        (y_gdi - y_hhea - 1.372).abs() < 0.001,
        "{y_hhea} -> {y_gdi}"
    );
    let [_, top_hhea] = text_origin(hhea, TextAnchor::Top);
    let [_, top_gdi] = text_origin(gdi, TextAnchor::Top);
    assert!(
        (top_hhea - top_gdi).abs() < f32::EPSILON,
        "Top is Flash's rule"
    );
}

#[test]
fn an_edit_text_wraps_iff_multiline() {
    let measure = |s: &str| s.len() as f32;
    assert_eq!(
        edit_text_lines("aa bb cc", true, 5.0, measure),
        vec!["aa bb", "cc"],
        "multiline wraps at the width"
    );
    assert_eq!(
        edit_text_lines("aa bb\ncc", true, 50.0, measure),
        vec!["aa bb", "cc"]
    );
    assert_eq!(
        edit_text_lines("aa bb cc", false, 5.0, measure),
        vec!["aa bb cc"],
        "a plain field does not wrap"
    );
    assert_eq!(
        edit_text_lines("aa\nbb", false, 1.0, measure),
        vec!["aa\nbb"],
        "a plain field is one run"
    );
    assert_eq!(
        edit_text_lines(" two  spaces ", true, 50.0, measure),
        vec![" two  spaces "],
        "one line keeps its spacing"
    );
}

#[test]
fn the_field_anchor_follows_multiline_and_wordwrap() {
    assert_eq!(TextAnchor::Field.for_field(true, true), TextAnchor::Top);
    assert_eq!(
        TextAnchor::Field.for_field(true, false),
        TextAnchor::Centred
    );
    assert_eq!(
        TextAnchor::Field.for_field(false, false),
        TextAnchor::Centred
    );
    assert_eq!(
        TextAnchor::Centred.for_field(true, true),
        TextAnchor::Centred
    );
    assert_eq!(TextAnchor::Top.for_field(false, false), TextAnchor::Top);
}

#[test]
fn lines_stack_under_the_first_and_a_centred_block_is_centred() {
    let text_box = nav_label_box();
    assert_eq!(
        line_origin(text_box, TextAnchor::Top, 0, 3, 10.0),
        text_origin(text_box, TextAnchor::Top)
    );
    let [_, y0] = line_origin(text_box, TextAnchor::Top, 0, 3, 10.0);
    let [_, y2] = line_origin(text_box, TextAnchor::Top, 2, 3, 10.0);
    assert!((y2 - y0 - 20.0).abs() < 0.001);
    let [_, one] = text_origin(text_box, TextAnchor::Centred);
    let [_, first] = line_origin(text_box, TextAnchor::Centred, 0, 3, 10.0);
    let [_, last] = line_origin(text_box, TextAnchor::Centred, 2, 3, 10.0);
    assert!(
        ((first + last) / 2.0 - one).abs() < 0.001,
        "{first} {last} {one}"
    );
    let mut centred = text_box;
    centred.alignment = 2;
    let narrow = TextBox {
        text_width: 10.0,
        ..centred
    };
    let [wide_x, _] = line_origin(centred, TextAnchor::Top, 1, 2, 10.0);
    let [narrow_x, _] = line_origin(narrow, TextAnchor::Top, 1, 2, 10.0);
    assert!(
        narrow_x > wide_x,
        "a shorter centred line starts further in"
    );
}

#[test]
fn the_per_field_rule_is_the_default_anchor() {
    assert_eq!(TextAnchor::default(), TextAnchor::Field);
}

#[test]
fn the_two_anchors_disagree_and_the_hotkey_cycles_them() {
    let text_box = nav_label_box();
    let [_, top] = text_origin(text_box, TextAnchor::Top);
    let [_, centred] = text_origin(text_box, TextAnchor::Centred);
    assert!(
        (centred - top).abs() > 1.0,
        "the two readings must be visibly different or the toggle is \
             pointless; got {top} and {centred}"
    );
    assert_eq!(TextAnchor::Field.toggled(), TextAnchor::Top);
    assert_eq!(TextAnchor::Top.toggled(), TextAnchor::Centred);
    assert_eq!(TextAnchor::Centred.toggled(), TextAnchor::Field);
}

#[test]
fn instance_widget_id_separates_two_places_of_the_same_character() {
    let a = instance_widget_id(&[199, 92], "menuexport", 318);
    let b = instance_widget_id(&[37, 92], "menuexport", 318);
    assert_ne!(
        a, b,
        "MainMenu places one nav button five times; one id for all \
             five is the bug this replaces"
    );
    assert_eq!(a, instance_widget_id(&[199, 92], "menuexport", 318));
}
fn empty_movie_record() -> Movie {
    Movie {
        frame_count: 0,
        frames_offset: 0,
        character_count: 0,
        characters_offset: 0,
        screen_width: 1024,
        screen_height: 768,
        unknown_36: 33,
        import_count: 0,
        imports_offset: 0,
        export_count: 0,
        exports_offset: 0,
    }
}

fn shape(left: f32, top: f32, right: f32, bottom: f32, geometry_id: u32) -> Character {
    Character::Shape {
        bounds: Rect {
            left,
            top,
            right,
            bottom,
        },
        geometry_id,
    }
}

#[test]
fn a_direct_non_placeholder_character_resolves_to_itself() {
    let mut set = MovieSet::new();
    set.insert(AptMovie {
        name: "menu".to_string(),
        record: empty_movie_record(),
        characters: vec![shape(0.0, 0.0, 1.0, 1.0, 4)],
        imports: Vec::new(),
        exports: Vec::new(),
    });
    let (movie, id, character) = resolve_character(&set, "menu", 0, 0).unwrap();
    assert_eq!(movie.name, "menu");
    assert_eq!(id, 0);
    assert!(matches!(character, Character::Shape { .. }));
}

#[test]
fn a_placeholder_resolves_across_movies_via_its_import() {
    let mut set = MovieSet::new();
    set.insert(AptMovie {
        name: "menuexport".to_string(),
        record: empty_movie_record(),
        characters: vec![shape(0.0, 0.0, 10.0, 10.0, 7)],
        imports: Vec::new(),
        exports: vec![Export {
            name: "ButtonMed".to_string(),
            character: 0,
        }],
    });
    set.insert(AptMovie {
        name: "mainmenu".to_string(),
        record: empty_movie_record(),
        characters: vec![Character::Placeholder],
        imports: vec![Import {
            movie: "menuexport".to_string(),
            name: "ButtonMed".to_string(),
            local_character: 0,
        }],
        exports: Vec::new(),
    });
    let (movie, id, character) = resolve_character(&set, "mainmenu", 0, 0).unwrap();
    assert_eq!(movie.name, "menuexport", "assets come from the exporter");
    assert_eq!(id, 0);
    assert!(matches!(character, Character::Shape { .. }));
}

#[test]
fn a_placeholder_with_no_matching_import_resolves_to_nothing_not_a_panic() {
    let mut set = MovieSet::new();
    set.insert(AptMovie {
        name: "mainmenu".to_string(),
        record: empty_movie_record(),
        characters: vec![Character::Placeholder],
        imports: Vec::new(),
        exports: Vec::new(),
    });
    assert!(resolve_character(&set, "mainmenu", 0, 0).is_none());
}

#[test]
fn a_self_importing_movie_is_bounded_by_max_import_chase_not_an_infinite_loop() {
    let mut set = MovieSet::new();
    set.insert(AptMovie {
        name: "a".to_string(),
        record: empty_movie_record(),
        characters: vec![Character::Placeholder],
        imports: vec![Import {
            movie: "a".to_string(),
            name: "self".to_string(),
            local_character: 0,
        }],
        exports: vec![Export {
            name: "self".to_string(),
            character: 0,
        }],
    });
    assert!(resolve_character(&set, "a", 0, 0).is_none());
}

#[test]
fn compose_affine_with_identity_parent_returns_the_child_unchanged() {
    let identity = ([[1.0, 0.0], [0.0, 1.0]], [0.0, 0.0]);
    let child = ([[2.0, 0.0], [0.0, 3.0]], [5.0, 7.0]);
    assert_eq!(compose_affine(identity, child), child);
}

#[test]
fn compose_affine_applies_the_parent_scale_to_the_childs_translation() {
    let parent = ([[2.0, 0.0], [0.0, 2.0]], [10.0, 20.0]);
    let child = ([[1.0, 0.0], [0.0, 1.0]], [1.0, 1.0]);
    let (_, translate) = compose_affine(parent, child);
    assert_eq!(translate, [12.0, 22.0]);
}

#[test]
fn letterbox_fits_a_wider_window_by_the_narrower_axis_and_centres_the_rest() {
    let fit = Letterbox::fit(1024.0, 768.0, 1024.0, 1024.0);
    assert!((fit.scale[0] - 1.0).abs() < 1e-4);
    assert!((fit.scale[1] - 1.0).abs() < 1e-4);
    assert!(
        (fit.offset[0]).abs() < 1e-4,
        "no horizontal letterbox expected"
    );
    assert!(fit.offset[1] > 0.0, "vertical letterbox expected");
}

#[test]
fn letterbox_round_trips_a_window_position_back_to_stage_space() {
    let fit = Letterbox::fit(1024.0, 768.0, 1280.0, 800.0);
    let stage_point = [500.0, 300.0];
    let transform = fit.as_transform();
    let (affine, translate) = transform;
    let window_point = [
        affine[0][0] * stage_point[0] + translate[0],
        affine[1][1] * stage_point[1] + translate[1],
    ];
    let back = fit.to_stage(window_point);
    assert!((back[0] - stage_point[0]).abs() < 1e-3);
    assert!((back[1] - stage_point[1]).abs() < 1e-3);
}

#[test]
fn letterbox_never_divides_by_zero_on_a_degenerate_stage() {
    let fit = Letterbox::fit(0.0, 0.0, 800.0, 600.0);
    assert_eq!(fit.scale, [1.0, 1.0]);
    let back = fit.to_stage([10.0, 10.0]);
    assert_eq!(back, [10.0, 10.0]);
}

#[test]
fn stretch_fills_the_window_on_both_axes_independently_with_no_offset() {
    let s = Letterbox::stretch(1024.0, 768.0, 1920.0, 1080.0);
    assert!((s.scale[0] - 1920.0 / 1024.0).abs() < 1e-4);
    assert!((s.scale[1] - 1080.0 / 768.0).abs() < 1e-4);
    assert_eq!(s.offset, [0.0, 0.0]);
    let stage_point = [500.0, 300.0];
    let (affine, translate) = s.as_transform();
    let window_point = [
        affine[0][0] * stage_point[0] + translate[0],
        affine[1][1] * stage_point[1] + translate[1],
    ];
    let back = s.to_stage(window_point);
    assert!((back[0] - stage_point[0]).abs() < 1e-3);
    assert!((back[1] - stage_point[1]).abs() < 1e-3);
}

#[test]
fn scale_mode_toggles_and_round_trips_its_flag_spelling() {
    assert_eq!(ScaleMode::default(), ScaleMode::Stretch);
    assert_eq!(ScaleMode::Stretch.toggled(), ScaleMode::Anchored);
    assert_eq!(ScaleMode::Anchored.toggled(), ScaleMode::Stretch);
    assert_eq!(ScaleMode::parse("Stretch"), Some(ScaleMode::Stretch));
    assert_eq!(ScaleMode::parse("ANCHORED"), Some(ScaleMode::Anchored));
    assert_eq!(
        ScaleMode::parse("letterbox"),
        None,
        "letterbox is not an offered mode"
    );
    assert_eq!(ScaleMode::Stretch.as_str(), "stretch");
    assert_eq!(ScaleMode::Anchored.as_str(), "anchored");
}

#[test]
fn a_corner_element_anchors_flush_to_its_own_corner() {
    let elements = [("palantir", [0.0, 513.0, 402.0, 767.0])];
    let table = anchor_table(&elements, 1024.0, 768.0, 1920.0, 1080.0, 8.0);
    let lb = table["palantir"];
    let scale = 1080.0 / 768.0;
    assert_eq!(lb.scale, [scale, scale]);
    assert!((lb.offset[0]).abs() < 1e-3);
    assert_eq!(lb.offset[1], 0.0);
    let bottom = lb.scale[1] * 767.0 + lb.offset[1];
    assert!((bottom - 767.0 * scale).abs() < 1e-3);
}

#[test]
fn an_edge_element_anchors_to_the_edge_whatever_frame_it_was_seen_on() {
    let at =
        |bbox: [f32; 4]| anchor_table(&[("bar", bbox)], 1024.0, 768.0, 1920.0, 1080.0, 24.0)["bar"];
    let rest = at([978.0, 68.3, 1024.0, 699.3]);
    let slid_out = at([1031.9, 110.3, 1073.9, 658.3]);
    assert_eq!(rest, slid_out);
    assert!((rest.scale[0] * 1024.0 + rest.offset[0] - 1920.0).abs() < 1e-3);
    assert_eq!(at([10.0, 0.0, 60.0, 50.0]).offset[0], 0.0);
    assert_eq!(at([-50.0, 0.0, -4.0, 50.0]).offset[0], 0.0);
}

#[test]
fn a_full_width_layer_fills_and_leaves_its_neighbours_their_own_docks() {
    let elements = [
        ("frame", [0.0, 0.0, 1024.0, 768.0]),
        ("panel", [300.0, 100.0, 700.0, 500.0]),
        ("left", [10.0, 600.0, 200.0, 700.0]),
    ];
    let table = anchor_table(&elements, 1024.0, 768.0, 1920.0, 1080.0, 24.0);
    assert_eq!(
        table["frame"],
        Letterbox::stretch(1024.0, 768.0, 1920.0, 1080.0)
    );
    let (scale, stretch_x) = (1080.0 / 768.0, 1920.0 / 1024.0);
    let centre = 500.0;
    assert!((table["panel"].offset[0] - (centre * stretch_x - centre * scale)).abs() < 1e-3);
    assert_eq!(table["panel"].scale, [scale, scale]);
    assert_eq!(table["left"].offset[0], 0.0);
}

#[test]
fn a_centred_element_anchors_by_its_own_centre() {
    let elements = [("spellbook", [300.0, 0.0, 700.0, 200.0])];
    let table = anchor_table(&elements, 1024.0, 768.0, 1920.0, 1080.0, 8.0);
    let lb = table["spellbook"];
    let scale = 1080.0 / 768.0;
    let stretch_x = 1920.0 / 1024.0;
    let centre = (300.0 + 700.0) / 2.0;
    let expected_offset_x = centre * stretch_x - centre * scale;
    assert!((lb.offset[0] - expected_offset_x).abs() < 1e-3);
}

#[test]
fn top_level_pieces_sharing_a_corner_are_grouped_not_anchored_independently() {
    let frame = ("frame", [0.0, 500.0, 300.0, 767.0]);
    let orb = ("orb", [10.0, 520.0, 280.0, 765.0]);
    let ring = ("ring", [5.0, 510.0, 290.0, 768.0]);
    let elements = [frame, orb, ring];
    let table = anchor_table(&elements, 1024.0, 768.0, 1920.0, 1080.0, 16.0);
    assert_eq!(table["frame"], table["orb"]);
    assert_eq!(table["orb"], table["ring"]);
    let orb_alone = ("orb", [400.0, 520.0, 600.0, 600.0]);
    let alone = anchor_table(&[orb_alone], 1024.0, 768.0, 1920.0, 1080.0, 16.0)["orb"];
    assert_ne!(
        table["orb"], alone,
        "grouped with frame and ring, orb anchors to their shared left edge; \
             alone and centred, it would anchor to its own centre instead"
    );
}

#[test]
fn overlapping_elements_with_different_edge_readings_still_share_one_group() {
    let ring = ("ring", [0.0, 400.0, 300.0, 700.0]);
    let portrait = ("portrait", [100.0, 450.0, 250.0, 600.0]);
    let elements = [ring, portrait];
    let table = anchor_table(&elements, 1024.0, 768.0, 1920.0, 1080.0, 24.0);
    assert_eq!(table["ring"], table["portrait"]);
    let portrait_alone = anchor_table(&[portrait], 1024.0, 768.0, 1920.0, 1080.0, 24.0)["portrait"];
    assert_ne!(
        table["portrait"], portrait_alone,
        "portrait alone reads as centred; grouped with the left-touching ring it must not"
    );
}

#[test]
fn a_lone_element_in_its_bucket_is_its_own_group() {
    let elements = [
        ("left_one", [0.0, 300.0, 100.0, 400.0]),
        ("right_one", [900.0, 300.0, 1024.0, 400.0]),
    ];
    let table = anchor_table(&elements, 1024.0, 768.0, 1920.0, 1080.0, 8.0);
    assert_ne!(table["left_one"], table["right_one"]);
}

#[test]
fn unpack_color_reads_memory_order_r_g_b_a() {
    let packed = u32::from_le_bytes([0x40, 0x80, 0xC0, 0xFF]);
    let [r, g, b, a] = unpack_color(packed);
    assert!((r - 0x40 as f32 / 255.0).abs() < 1e-6);
    assert!((g - 0x80 as f32 / 255.0).abs() < 1e-6);
    assert!((b - 0xC0 as f32 / 255.0).abs() < 1e-6);
    assert!((a - 1.0).abs() < 1e-6);
}

#[test]
fn a_script_text_color_replaces_rgb_and_keeps_alpha() {
    use mint_sage_apt_player::Value;
    let authored = [0.5, 0.75, 0.25, 0.6];
    let from_str = apply_text_color(authored, Some(&Value::str("0x7FAABB")));
    assert_eq!(from_str, [127.0 / 255.0, 170.0 / 255.0, 187.0 / 255.0, 0.6]);
    let from_num = apply_text_color(authored, Some(&Value::Number(f64::from(0x00ff_0000u32))));
    assert_eq!(from_num, [1.0, 0.0, 0.0, 0.6]);
    assert_eq!(apply_text_color(authored, None), authored);
    assert_eq!(
        apply_text_color(authored, Some(&Value::str("green"))),
        authored
    );
    assert_eq!(
        apply_text_color(authored, Some(&Value::Undefined)),
        authored
    );
}

#[test]
fn unpack_edittext_color_is_argb_by_value() {
    let [r, g, b, a] = unpack_edittext_color(0xffff_cc00);
    assert!((r - 1.0).abs() < 1e-6);
    assert!((g - 0.8).abs() < 1e-6);
    assert!(b.abs() < 1e-6);
    assert!((a - 1.0).abs() < 1e-6);
    assert_eq!(unpack_edittext_color(0xff00_0000), [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn bundled_font_file_matches_the_three_shipped_names() {
    assert_eq!(bundled_font_file("Albertus MT"), Some("albertusmt.otf"));
    assert_eq!(bundled_font_file("Omnia LT Std"), Some("omnialtstd.ttf"));
    assert_eq!(bundled_font_file("SachaWynter"), Some("sachwt__.ttf"));
}

#[test]
fn bundled_font_file_is_none_for_a_name_with_no_shipped_file() {
    assert_eq!(bundled_font_file("Times New Roman"), None);
    assert_eq!(bundled_font_file(""), None);
}

#[test]
fn each_image_mode_reading_says_which_pictures_are_opaque() {
    let cases = [
        (Some("RenderImage"), None),
        (Some("RenderImage"), Some("ALPHA")),
        (Some("RenderImage"), Some("GRAYSCALE")),
        (Some("RenderImage"), Some("SOLID")),
        (Some("View3D"), None),
    ];
    let row = |reading: ImageModes| cases.map(|(t, m)| reading.opaque(t, m));
    assert_eq!(row(ImageModes::Matte), [false; 5]);
    assert_eq!(
        row(ImageModes::UnsetSolid),
        [true, false, false, true, false]
    );
    assert_eq!(
        row(ImageModes::AlphaOpaque),
        [false, true, false, true, false]
    );
    assert!(ImageModes::AlphaOpaque.opaque(None, Some(" alpha ")));
    assert_eq!(row(ImageModes::RadialDisc), [false; 5]);
    let disc = |reading: ImageModes| cases.map(|(_, m)| reading.disc(m));
    assert_eq!(
        disc(ImageModes::RadialDisc),
        [false, true, false, true, false],
        "the retired, _mode-keyed guess: SOLID/ALPHA only"
    );
    assert_eq!(
        disc(ImageModes::Decoded),
        [true; 5],
        "round 8: unconditional"
    );
    assert_eq!(disc(ImageModes::AlphaOpaque), [false; 5]);
    assert_eq!(ImageModes::default(), ImageModes::Decoded);
    assert_eq!(
        ImageModes::Decoded.next().next().next().next().next(),
        ImageModes::Decoded
    );
}

#[test]
fn decoded_mode_table() {
    let cases = [
        (Some("RenderImage"), None, false, false),
        (Some("RenderImage"), Some("ALPHA"), false, false),
        (Some("RenderImage"), Some("GRAYSCALE"), false, false),
        (Some("RenderImage"), Some("SOLID"), true, false),
        (Some("RenderImage"), Some("SOLID_GRAYSCALE"), true, false),
        (Some("RenderImage"), Some("SOLID_TEXTURE"), true, false),
        (Some("RenderImage"), Some("ADDITIVE"), false, true),
        (Some("RenderImageDisabled"), Some("SOLID"), false, false),
        (Some("RenderImageDisabled"), None, false, false),
    ];
    for (surface_type, mode, opaque, additive) in cases {
        assert_eq!(
            ImageModes::Decoded.opaque(surface_type, mode),
            opaque,
            "{surface_type:?} {mode:?} opaque"
        );
        assert_eq!(
            ImageModes::Decoded.additive(surface_type, mode),
            additive,
            "{surface_type:?} {mode:?} additive"
        );
    }
}

#[test]
fn multiply_color_is_per_channel() {
    let result = multiply_color([0.5, 1.0, 0.0, 1.0], [0.5, 0.5, 1.0, 0.5]);
    assert_eq!(result, [0.25, 0.5, 0.0, 0.5]);
}

#[test]
fn unpack_color_add_reads_0x00rrggbb() {
    let [r, g, b, a] = unpack_color_add(0x0000_8CE0);
    assert!((r - 0.0).abs() < 1e-6, "r = {r}");
    assert!((g - 140.0 / 255.0).abs() < 1e-6, "g = {g}");
    assert!((b - 224.0 / 255.0).abs() < 1e-6, "b = {b}");
    assert_eq!(a, 0.0, "the top byte is never an alpha add");
}

#[test]
fn add_color_is_per_channel() {
    assert_eq!(
        add_color([0.1, 0.2, 0.3, 0.0], [0.05, 0.0, 0.1, 1.0]),
        [0.15, 0.2, 0.4, 1.0]
    );
}

#[test]
fn concat_color_add_scales_the_childs_add_by_the_parents_multiply() {
    let parent_mult = [0.5, 0.5, 0.5, 1.0];
    let parent_add = [0.1, 0.0, 0.0, 0.0];
    let child_add = [0.2, 0.4, 0.0, 0.0];
    let combined = concat_color_add(parent_mult, parent_add, child_add);
    assert_eq!(combined, [0.1 + 0.1, 0.0 + 0.2, 0.0, 0.0]);
}

#[test]
fn concat_color_add_with_an_identity_parent_is_just_the_childs_own_add() {
    let combined = concat_color_add([1.0, 1.0, 1.0, 1.0], [0.0; 4], [0.3, 0.1, 0.0, 0.0]);
    assert_eq!(combined, [0.3, 0.1, 0.0, 0.0]);
}

#[test]
fn widget_id_is_stable_for_the_same_movie_and_character() {
    assert_eq!(widget_id("mainmenu", 7), widget_id("mainmenu", 7));
}

#[test]
fn widget_id_differs_across_movies_for_the_same_character_id() {
    assert_ne!(widget_id("mainmenu", 7), widget_id("skirmish", 7));
}

#[test]
fn widget_id_differs_across_character_ids_in_the_same_movie() {
    assert_ne!(widget_id("mainmenu", 7), widget_id("mainmenu", 8));
}

#[test]
fn normalize_strips_case_and_apt_suffix() {
    assert_eq!(normalize("MainMenu.apt"), "mainmenu");
    assert_eq!(normalize("MAINMENU"), "mainmenu");
}

#[test]
fn list_available_on_a_missing_install_root_is_empty_not_an_error() {
    let library = MovieLibrary::new(Path::new("Z:/does/not/exist"));
    assert!(library.list_available().is_empty());
}

#[test]
fn ensure_loaded_on_a_missing_movie_is_a_reported_error_not_a_panic() {
    let mut library = MovieLibrary::new(Path::new("Z:/does/not/exist"));
    assert!(library.ensure_loaded("nope").is_err());
}

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "mint-apt-viewer-{tag}-{}-{nanos}",
        std::process::id()
    ))
}

#[test]
fn ensure_loaded_with_multiple_roots_reports_an_error_when_none_have_the_movie() {
    let a = temp_dir("root-a");
    let b = temp_dir("root-b");
    std::fs::create_dir_all(a.join("apt")).expect("create root a");
    std::fs::create_dir_all(b.join("apt")).expect("create root b");
    let mut library = MovieLibrary::new_with_roots([a.clone(), b.clone()]);
    assert!(library.ensure_loaded("ingamesidecommandbar").is_err());
}

#[test]
fn list_available_unions_and_dedups_across_every_root() {
    let a = temp_dir("list-a");
    let b = temp_dir("list-b");
    std::fs::create_dir_all(a.join("apt")).expect("create root a");
    std::fs::create_dir_all(b.join("apt")).expect("create root b");
    std::fs::write(a.join("apt").join("mainmenu.big"), b"stub").expect("write stub");
    std::fs::write(b.join("apt").join("mainmenu.big"), b"stub").expect("write stub");
    std::fs::write(b.join("apt").join("liblibraryonly.big"), b"stub").expect("write stub");
    let library = MovieLibrary::new_with_roots([a, b]);
    let names = library.list_available();
    assert_eq!(
        names,
        vec!["liblibraryonly".to_owned(), "mainmenu".to_owned()],
        "one entry per distinct name, sorted, even though \"mainmenu\" exists in both roots"
    );
}

use mint_sage_apt_player::synth::{self, Synth};
use mint_sage_apt_player::{Library as PlayerLibrary, Limits, Player};

fn synth_player(s: &Synth, root: &synth::Frames) -> Player {
    let (c, apt) = s.build(root);
    let mut lib = PlayerLibrary::new();
    let id = lib.add("test", &c, apt).expect("synthetic movie parses");
    lib.link();
    Player::new(Arc::new(lib), id, Limits::default()).expect("player starts")
}

#[test]
fn anchor_cache_keeps_a_stable_transform_while_its_content_animates() {
    let mut s = Synth::new();
    let shape = s.shape_box([0.0, 0.0, 20.0, 20.0]);
    let sprite = s.sprite(vec![
        vec![synth::Item::Place(synth::place(1, shape).at(0.0, 0.0))],
        vec![synth::Item::Place(synth::modify(1).at(200.0, 0.0))],
    ]);
    let root = vec![vec![synth::Item::Place(
        synth::place(1, sprite).at(0.0, 0.0),
    )]];
    let mut p = synth_player(&s, &root);

    p.tick().expect("first tick settles frame 0");
    let top = p
        .instance(p.root())
        .expect("root instance")
        .child_at(1)
        .expect("sprite placed at depth 1");

    let (stage_w, stage_h) = (1024.0, 768.0);
    let (window_w, window_h) = (1920.0, 1080.0);
    let raw_frame0 =
        player_anchors(&p, stage_w, stage_h, window_w, window_h, ANCHOR_EDGE_SLACK)[&top];

    p.tick().expect("second tick reaches frame 1");
    let raw_frame1 =
        player_anchors(&p, stage_w, stage_h, window_w, window_h, ANCHOR_EDGE_SLACK)[&top];
    assert_ne!(
        raw_frame0.offset, raw_frame1.offset,
        "player_anchors recomputed fresh from the live tree must actually be unstable here - \
             otherwise this synthetic movie does not reproduce the owner's bug"
    );

    let mut cache = AnchorCache::new();
    let cached_first =
        cache.letterboxes(&p, stage_w, stage_h, window_w, window_h, ANCHOR_EDGE_SLACK)[&top];
    for i in 0..600 {
        p.tick().expect("tick");
        let cached =
            cache.letterboxes(&p, stage_w, stage_h, window_w, window_h, ANCHOR_EDGE_SLACK)[&top];
        assert_eq!(
            cached, cached_first,
            "frame {i}: the cached anchor moved even though no new top-level element appeared"
        );
    }
}

#[test]
fn a_click_through_the_anchored_transform_hits_the_button_drawn_there() {
    let mut s = Synth::new();
    let art = s.shape_box([0.0, 0.0, 40.0, 40.0]);
    let button = s.button_with_mesh(
        vec![(0x1, art, 1, 0.0, 0.0), (0x8, art, 2, 0.0, 0.0)],
        Vec::new(),
        vec![[0.0, 0.0], [40.0, 0.0], [0.0, 40.0]],
    );
    let root = vec![vec![synth::Item::Place(
        synth::place(1, button).at(0.0, 700.0),
    )]];
    let mut p = synth_player(&s, &root);
    p.tick().expect("first tick settles frame 0");

    let button_id = p
        .instance(p.root())
        .expect("root instance")
        .child_at(1)
        .expect("button placed at depth 1");

    let (stage_w, stage_h) = (1024.0, 768.0);
    let (window_w, window_h) = (1920.0, 1080.0);
    let mut cache = AnchorCache::new();
    let letterboxes =
        cache.letterboxes(&p, stage_w, stage_h, window_w, window_h, ANCHOR_EDGE_SLACK);
    let lb = *letterboxes
        .get(&button_id)
        .expect("the button is a top-level element");

    let bounds = p.local_bounds(button_id).expect("button has bounds");
    let world = p
        .world_matrix(button_id)
        .expect("button has a world matrix");
    let (wx0, wy0) = world.apply(bounds[0], bounds[1]);
    let (wx1, wy1) = world.apply(bounds[2], bounds[3]);
    let on_screen = [
        wx0 * lb.scale[0] + lb.offset[0],
        wy0 * lb.scale[1] + lb.offset[1],
        wx1 * lb.scale[0] + lb.offset[0],
        wy1 * lb.scale[1] + lb.offset[1],
    ];
    let window_pos = [
        (on_screen[0] + on_screen[2]) / 2.0,
        (on_screen[1] + on_screen[3]) / 2.0,
    ];

    let stage_point = cache.resolve_point(
        &p,
        ScaleMode::Anchored,
        stage_w,
        stage_h,
        window_w,
        window_h,
        ANCHOR_EDGE_SLACK,
        window_pos,
    );
    p.pointer_move(stage_point[0], stage_point[1])
        .expect("pointer move");
    assert_eq!(
        p.hovered(),
        Some(button_id),
        "a click at the button's own on-screen (Anchored) position must resolve to a stage \
             point inside it"
    );

    let expected = lb.to_stage(window_pos);
    assert_eq!(
        stage_point, expected,
        "resolve_point must invert through this element's own cached anchor, not a \
             freshly (or wrongly) computed transform"
    );
    let naive_stretch = Letterbox::stretch(stage_w, stage_h, window_w, window_h);
    assert_ne!(
        naive_stretch.scale[0], lb.scale[0],
        "the two transforms must actually differ here, or the check above proves nothing"
    );
}
