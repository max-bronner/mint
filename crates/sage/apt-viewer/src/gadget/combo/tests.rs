use super::*;

fn triple(image: &str) -> String {
    format!("IMAGE: {image}, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0")
}

fn data(key: &str, images: &[&str]) -> String {
    let items: Vec<String> = images.iter().map(|i| triple(i)).collect();
    format!(" {key} = {};\n", items.join(", "))
}

fn combo_template() -> String {
    let mut t = String::from(
        "WINDOW\n WINDOWTYPE = COMBOBOX;\n FONT = NAME: \"Face\", SIZE: 16, BOLD: 0;\n \
         TEXTCOLOR = ENABLED: 255 0 0 255, ENABLEDBORDER: 0 0 0 255, DISABLED: 0 255 0 255, \
         DISABLEDBORDER: 0 0 0 255, HILITE: 0 0 255 255, HILITEBORDER: 0 0 0 255;\n \
         COMBOBOXDATA = ISEDITABLE: 0, MAXCHARS: 16, MAXDISPLAY: 4, ASCIIONLY: 0, LETTERSANDNUMBERS: 0;\n",
    );
    t += &data(
        "COMBOBOXDROPDOWNBUTTONENABLEDDRAWDATA",
        &["Btn", "BtnPushed"],
    );
    t += &data("COMBOBOXDROPDOWNBUTTONHILITEDRAWDATA", &["BtnLit"]);
    t += &data("COMBOBOXDROPDOWNBUTTONDISABLEDDRAWDATA", &["BtnDim"]);
    t += &data(
        "COMBOBOXLISTBOXENABLEDDRAWDATA",
        &["Back", "NoImage", "NoImage", "Sel"],
    );
    t += &data(
        "COMBOBOXLISTBOXHILITEDRAWDATA",
        &["Back", "NoImage", "NoImage", "SelLit"],
    );
    t += &data("LISTBOXENABLEDUPBUTTONDRAWDATA", &["Up"]);
    t += "END\n";
    t
}

const IMAGE_COMBO: &str = "WINDOW
 WINDOWTYPE = USER;
 FONT = NAME: \"Face\", SIZE: 16, BOLD: 0;
 CHILD
 WINDOW
  WINDOWTYPE = PUSHBUTTON;
  ENABLEDDRAWDATA = IMAGE: Drop, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0, IMAGE: DropPushed, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
  HILITEDRAWDATA = IMAGE: DropLit, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
  DISABLEDDRAWDATA = IMAGE: DropDim, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 END
 WINDOW
  WINDOWTYPE = SCROLLLISTBOX;
  ENABLEDDRAWDATA = IMAGE: Back, COLOR: 0 0 0 255, BORDERCOLOR: 0 0 0 0, IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0, IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0, IMAGE: ListSel, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 END
 ENDALLCHILDREN
END
";

fn names<I: Clone>(v: &[Option<I>]) -> Vec<Option<I>> {
    v.to_vec()
}

#[test]
fn a_combo_template_reads_its_parts_from_its_own_keys() {
    let look = ComboLook::read(&Layout::parse(&combo_template()).unwrap());
    assert_eq!(
        names(&look.button),
        vec![
            Some("Btn".to_owned()),
            Some("BtnLit".to_owned()),
            Some("BtnDim".to_owned())
        ]
    );
    assert_eq!(look.pushed.as_deref(), Some("BtnPushed"));
    assert_eq!(look.list.body.image.as_deref(), Some("Back"));
    assert_eq!(look.list.selected[0].image.as_deref(), Some("Sel"));
    assert_eq!(look.list.selected[1].image.as_deref(), Some("SelLit"));
    assert_eq!(look.list.up[0].as_deref(), Some("Up"));
    assert_eq!(look.max_display, 4);
    assert_eq!(look.points, TEXT_POINTS);
    assert_eq!(look.list.points, TEXT_POINTS);
    assert_eq!(look.font, "Face");
    assert_eq!(look.text[State::Disabled as usize], [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(look.text[State::Hilite as usize], [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn an_image_combo_reads_its_button_and_list_from_its_children() {
    let look = ComboLook::read(&Layout::parse(IMAGE_COMBO).unwrap());
    assert_eq!(
        names(&look.button),
        vec![
            Some("Drop".to_owned()),
            Some("DropLit".to_owned()),
            Some("DropDim".to_owned())
        ]
    );
    assert_eq!(look.pushed.as_deref(), Some("DropPushed"));
    assert_eq!(look.list.body.image.as_deref(), Some("Back"));
    assert_eq!(look.list.selected[0].image.as_deref(), Some("ListSel"));
    let resolved = look.resolved(|name: &String| (name == "DropLit").then_some((9u8, [1.0; 2])));
    assert_eq!(resolved.button, [None, Some(9), None]);
}

#[test]
fn the_box_the_button_and_the_list_have_their_sizes() {
    let bounds = [10.0, 20.0, 110.0, 47.0];
    assert_eq!(button_rect(bounds, 1.0), [83.0, 20.0, 110.0, 47.0]);
    assert_eq!(text_rect(bounds, 1.0), [10.0, 20.0, 83.0, 47.0]);
    assert_eq!(
        button_rect([0.0, 0.0, 10.0, 30.0], 1.0),
        [0.0, 0.0, 10.0, 30.0]
    );
    assert_eq!(button_rect(bounds, 0.5), [96.5, 20.0, 110.0, 47.0]);
    assert_eq!(list_height(16.0, 3, 10), 16.0 * 3.0 + 6.0 + 4.0);
    assert_eq!(list_height(16.0, 21, 10), 16.0 * 10.0 + 20.0 + 4.0);
    assert!(!needs_scroll_bar(10, 10));
    assert!(needs_scroll_bar(11, 10));
    assert_eq!(list_rect(bounds, 50.0, false), [10.0, 47.0, 110.0, 97.0]);
    assert_eq!(list_rect(bounds, 50.0, true), [10.0, -30.0, 110.0, 20.0]);
}

#[test]
fn a_list_with_no_room_below_opens_above() {
    let low = [0.0, 600.0, 100.0, 627.0];
    assert!(opens_upward(low, 200.0, 0.0, 768.0));
    assert!(!opens_upward(low, 100.0, 0.0, 768.0), "fits below");
    let high = [0.0, 50.0, 100.0, 77.0];
    assert!(
        !opens_upward(high, 800.0, 0.0, 768.0),
        "more room below than above"
    );
}

fn content(n: usize, selected: Option<usize>, enabled: bool) -> Arc<ComboContent> {
    Arc::new(ComboContent {
        items: (0..n)
            .map(|i| ComboItem::text(format!("item {i}")))
            .collect(),
        selected,
        enabled,
    })
}

#[test]
fn a_box_opens_and_a_choice_closes_it() {
    let mut view = ComboView::new(content(3, Some(1), false));
    view.toggle();
    assert!(!view.open, "disabled");
    view.set_content(content(3, Some(1), true));
    view.toggle();
    assert!(view.open && view.list.is_some());
    assert_eq!(view.state(), State::Hilite, "open shows lit");
    assert_eq!(view.choose(1), None, "already selected");
    assert!(!view.open);
    view.toggle();
    assert_eq!(view.choose(2), Some(2));
    assert_eq!(view.selected, Some(2));
    view.toggle();
    assert_eq!(view.choose(9), None, "out of range");
    view.toggle();
    view.toggle();
    assert!(!view.open, "a second toggle closes");
}

#[test]
fn opening_a_combo_tracks_the_hover_row_from_the_choice() {
    let mut view = ComboView::new(content(4, Some(2), true));
    view.toggle();
    let list = view.list.as_ref().unwrap();
    assert!(list.tracking);
    assert_eq!(list.hover_row, Some(2));
}

#[test]
fn the_hosts_content_stands_and_disabling_closes() {
    let mut view = ComboView::new(content(3, Some(0), true));
    view.toggle();
    view.set_content(content(3, Some(2), true));
    assert_eq!(view.selected, Some(2));
    assert!(view.open);
    assert_eq!(view.list.as_ref().and_then(|l| l.selected), Some(2));
    view.set_content(content(3, Some(2), false));
    assert!(!view.open);
}

#[test]
fn arrows_move_the_selection_and_confirm_closes() {
    let mut view = ComboView::new(content(3, Some(0), true));
    assert_eq!(view.key(Nav::Down), None, "closed");
    view.toggle();
    assert_eq!(view.key(Nav::Up), None, "at the top");
    assert_eq!(view.key(Nav::Down), Some(1));
    assert_eq!(view.key(Nav::Down), Some(2));
    assert_eq!(view.key(Nav::Down), None, "at the bottom");
    assert!(view.open);
    assert_eq!(view.list.as_ref().and_then(|l| l.selected), Some(2));
    assert_eq!(view.key(Nav::Confirm), None);
    assert!(!view.open);
    let mut none = ComboView::new(content(3, None, true));
    none.toggle();
    assert_eq!(
        none.key(Nav::Down),
        Some(0),
        "no selection starts at the top"
    );
    none.key(Nav::Cancel);
    assert!(!none.open);
}

#[test]
fn the_drop_down_rows_are_the_entries() {
    let items = vec![
        ComboItem::text("a"),
        ComboItem {
            swatch: Some([1.0, 0.0, 0.0, 1.0]),
            ..ComboItem::text("red")
        },
    ];
    let mut view = ComboView::new(Arc::new(ComboContent {
        items,
        selected: Some(1),
        enabled: true,
    }));
    let list = view.list_content();
    assert!(matches!(list.rows[0][0], ListCell::Text(ref t) if t == "a"));
    assert!(matches!(list.rows[1][0], ListCell::Swatch(c) if c == [1.0, 0.0, 0.0, 1.0]));
    assert_eq!(list.selected, Some(1));
    assert_eq!(list.scroll_bar, Some(false));
    view.max_display = 1;
    assert_eq!(view.list_content().scroll_bar, Some(true));
}
