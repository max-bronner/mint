use super::*;

const LIST: &str = "FILE_VERSION = 2;\r
STARTLAYOUTBLOCK\r
  LAYOUTINIT = [None];\r
ENDLAYOUTBLOCK\r
WINDOW\r
  WINDOWTYPE = SCROLLLISTBOX;\r
  SCREENRECT = UPPERLEFT: 0 0,\r
               BOTTOMRIGHT: 160 160,\r
               CREATIONRESOLUTION: 800 600;\r
  NAME = \"List.wnd:\";\r
  STATUS = ENABLED;\r
  STYLE = SCROLLLISTBOX+MOUSETRACK;\r
  FONT = NAME: \"Some Font\", SIZE: 10, BOLD: 0;\r
  TEXTCOLOR = ENABLED:  254 254 254 255, ENABLEDBORDER:  0 0 0 255,\r
              DISABLED: 192 192 192 255, DISABLEDBORDER: 64 64 64 255,\r
              HILITE:   128 128 255 255, HILITEBORDER:   0 0 128 255;\r
  ENABLEDDRAWDATA = IMAGE: NoImage, COLOR: 255 255 255 0, BORDERCOLOR: 106 171 84 0,\r
                    IMAGE: Highlight, COLOR: 255 255 0 255, BORDERCOLOR: 254 254 254 300;\r
  LISTBOXDATA = LENGTH: 999,\r
                AUTOSCROLL: 0,\r
                SCROLLIFATEND: 1,\r
                SCROLLBAR: 1,\r
                COLUMNS: 2,\r
                COLUMNSWIDTH: 70,\r
                COLUMNSWIDTH: 30,\r
                FORCESELECT: 1;\r
END\r
";

#[test]
fn a_list_template_reads_as_the_engine_reads_it() {
    let layout = Layout::parse(LIST).unwrap();
    assert_eq!(layout.version, Some(2));
    assert_eq!(layout.field("LAYOUTINIT"), Some("[None]"));
    assert_eq!(layout.windows.len(), 1);
    let w = &layout.windows[0];
    assert_eq!(w.line, 5);
    assert_eq!(w.window_type(), Some("SCROLLLISTBOX"));
    assert_eq!(w.name().as_deref(), Some("List.wnd:"));
    assert_eq!(w.status(), vec!["ENABLED"]);
    assert_eq!(w.style(), vec!["SCROLLLISTBOX", "MOUSETRACK"]);
    assert_eq!(
        w.screen_rect(),
        Some(ScreenRect {
            upper_left: [0, 0],
            bottom_right: [160, 160],
            creation_resolution: [800, 600],
        })
    );
    assert_eq!(
        w.font(),
        Some(Font {
            name: "Some Font".to_owned(),
            size: 10,
            bold: false,
        })
    );
    let colours = w.text_colors().unwrap();
    assert_eq!(colours.enabled, [254, 254, 254, 255]);
    assert_eq!(colours.hilite_border, [0, 0, 128, 255]);
    let draw = w.draw_data("ENABLEDDRAWDATA").unwrap();
    assert_eq!(draw.len(), 2);
    assert_eq!(draw[0].image, None, "NoImage is none");
    assert_eq!(draw[0].border, [106, 171, 84, 0]);
    assert_eq!(draw[1].image.as_deref(), Some("Highlight"));
    assert_eq!(draw[1].border, [254, 254, 254, 255], "clamped to 255");
    assert_eq!(w.draw_keys(), vec!["ENABLEDDRAWDATA"]);
    let list = w.list_box().unwrap();
    assert_eq!(list.length, 999);
    assert!(!list.auto_scroll && list.scroll_if_at_end && list.scroll_bar);
    assert_eq!(list.columns, 2);
    assert_eq!(list.column_widths, vec![70, 30]);
    assert!(list.force_select, "read by label, not by position");
    let last = w.fields.last().unwrap();
    assert_eq!(
        (last.key.as_str(), last.line),
        ("LISTBOXDATA", 19),
        "multi-line statements count their lines"
    );
}

#[test]
fn a_field_stated_twice_reads_its_last() {
    let layout = Layout::parse(
        "WINDOW
 NAME = \"a\";
 NAME = \"b\";
 FONT = NAME: \"x\", SIZE: 8, SIZE: 12;
END
",
    )
    .unwrap();
    let w = &layout.windows[0];
    assert_eq!(w.name().as_deref(), Some("b"));
    assert_eq!(w.font().map(|f| f.size), Some(12));
}

#[test]
fn children_nest_under_their_window() {
    let text = "WINDOW\n WINDOWTYPE = USER;\n NAME = \"Map\";\n CHILD\n WINDOW\n  WINDOWTYPE = PUSHBUTTON;\n  NAME = \"Start0\";\n END\n CHILD\n WINDOW\n  WINDOWTYPE = PUSHBUTTON;\n  NAME = \"Start1\";\n END\n ENDALLCHILDREN\n STATUS = ENABLED;\nEND\n";
    let layout = Layout::parse(text).unwrap();
    let map = &layout.windows[0];
    assert_eq!(map.children.len(), 2);
    assert_eq!(map.children[1].name().as_deref(), Some("Start1"));
    assert_eq!(map.status(), vec!["ENABLED"], "fields after the children");
    let names: Vec<_> = layout
        .all_windows()
        .iter()
        .filter_map(|w| w.name())
        .collect();
    assert_eq!(names, vec!["Map", "Start0", "Start1"]);
}

#[test]
fn a_broken_file_is_an_error_naming_where() {
    assert_eq!(
        Layout::parse("WINDOW\n  WINDOWTYPE = USER\n"),
        Err(Error::Unterminated {
            line: 2,
            key: "WINDOWTYPE".to_owned()
        })
    );
    assert!(matches!(
        Layout::parse("WINDOW\n  nonsense;\nEND\n"),
        Err(Error::NotAStatement { line: 2, .. })
    ));
    assert_eq!(
        Layout::parse("WINDOW\n  WINDOWTYPE = USER;\n"),
        Err(Error::UnclosedWindow { line: 1 })
    );
    assert!(matches!(
        Layout::parse("END\n"),
        Err(Error::Misplaced { keyword: "END", .. })
    ));
    assert!(matches!(
        Layout::parse("WINDOW\nWINDOW\nEND\nEND\n"),
        Err(Error::Misplaced {
            keyword: "WINDOW",
            ..
        })
    ));
    assert!(matches!(
        Layout::parse(
            "WINDOW
CHILD
WINDOW
END
ENDALLCHILDREN
WINDOW
END
END
"
        ),
        Err(Error::Misplaced {
            keyword: "WINDOW",
            line: 6
        })
    ));
}

#[test]
fn a_depth_bomb_stops_at_the_cap() {
    let depth = MAX_DEPTH + 1;
    let mut text = String::new();
    for _ in 0..depth {
        text.push_str("WINDOW\n CHILD\n");
    }
    for _ in 0..depth {
        text.push_str("END\n");
    }
    assert!(matches!(Layout::parse(&text), Err(Error::TooDeep { .. })));
    let mut ok = String::new();
    for _ in 0..MAX_DEPTH {
        ok.push_str("WINDOW\n CHILD\n");
    }
    for _ in 0..MAX_DEPTH {
        ok.push_str("END\n");
    }
    assert!(Layout::parse(&ok).is_ok());
}

#[test]
fn truncation_and_corruption_never_panic() {
    for end in 0..=LIST.len() {
        if let Some(prefix) = LIST.get(..end) {
            let _ = Layout::parse(prefix).map(|l| l.all_windows().len());
        }
    }
    let bytes = LIST.as_bytes();
    for i in 0..bytes.len() {
        for b in [b';', b'=', b'"', b',', b':', b'\n', b'W', 0xFF] {
            let mut copy = bytes.to_vec();
            copy[i] = b;
            let text = String::from_utf8_lossy(&copy);
            if let Ok(layout) = Layout::parse(&text) {
                for w in layout.all_windows() {
                    let _ = (w.screen_rect(), w.font(), w.text_colors(), w.list_box());
                    for key in w.draw_keys() {
                        let _ = w.draw_data(key);
                    }
                }
            }
        }
    }
}

#[test]
fn labelled_items_read_as_the_engine_tokenises_them() {
    assert_eq!(
        labelled("loose, NAME: \"A, B: C\", SIZE : 10, BOLD: 0"),
        vec![
            (String::new(), "loose".to_owned()),
            ("NAME".to_owned(), "\"A, B: C\"".to_owned()),
            ("SIZE".to_owned(), "10".to_owned()),
            ("BOLD".to_owned(), "0".to_owned()),
        ]
    );
    let items =
        draw_items("IMAGE: NoImage COLOR: 1 2 3 4, BORDERCOLOR: 5 6 7 8, IMAGE: A, COLOR: 9 9 9 9");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].image, None);
    assert_eq!(items[0].color, [1, 2, 3, 4]);
    assert_eq!(items[0].border, [5, 6, 7, 8]);
    assert_eq!(items[1].image.as_deref(), Some("A"));
}
