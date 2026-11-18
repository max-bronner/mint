use std::sync::Arc;

use super::*;

const TEMPLATE: &str = "WINDOW
 WINDOWTYPE = CHECKBOX;
 FONT = NAME: \"Face\", SIZE: 14, BOLD: 0;
 TEXTCOLOR = ENABLED: 255 0 0 255, ENABLEDBORDER: 0 0 0 255,
   DISABLED: 0 255 0 255, DISABLEDBORDER: 0 0 0 255,
   HILITE: 0 0 255 255, HILITEBORDER: 0 0 0 255;
 ENABLEDDRAWDATA = IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: Off, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: On, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 HILITEDRAWDATA = IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: OffLit, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: OnLit, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 DISABLEDDRAWDATA = IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: OffDim, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: OnDim, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
END
";

#[test]
fn the_look_reads_unchecked_and_checked_per_state() {
    let look = CheckLook::read(&Layout::parse(TEMPLATE).unwrap_or_default());
    assert_eq!((look.font.as_str(), look.points), ("Face", 14.0));
    let names = look
        .boxes
        .clone()
        .map(|pair| pair.map(|n| n.unwrap_or_default()));
    assert_eq!(
        names,
        [
            ["Off".to_owned(), "On".to_owned()],
            ["OffLit".to_owned(), "OnLit".to_owned()],
            ["OffDim".to_owned(), "OnDim".to_owned()],
        ]
    );
    assert_eq!(look.text[State::Hilite as usize], [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(look.text[State::Disabled as usize], [0.0, 1.0, 0.0, 1.0]);
    let resolved = look.resolved(|name: &String| (name == "On").then_some(7u8));
    assert_eq!(resolved.boxes[0], [None, Some(7)]);
}

#[test]
fn the_box_is_the_height_less_six_from_the_left() {
    let bounds = [10.0, 20.0, 110.0, 54.0];
    assert_eq!(check_box(bounds, 1.0, 1.0), [10.0, 23.0, 38.0, 51.0]);
    assert_eq!(check_label(bounds), [44.0, 37.0]);
    assert_eq!(
        check_box([0.0, 0.0, 10.0, 4.0], 1.0, 1.0),
        [0.0, 3.0, 0.0, 3.0]
    );
    assert_eq!(check_box(bounds, 0.75, 1.0), [10.0, 23.0, 31.0, 51.0]);
    assert_eq!(check_box(bounds, 1.0, 0.5), [10.0, 21.5, 41.0, 52.5]);
}

#[test]
fn a_click_on_an_enabled_box_toggles_it() {
    let mut view = CheckView::new(Arc::new(CheckContent::default()));
    view.bounds = Some([0.0, 0.0, 100.0, 34.0]);
    assert_eq!(view.click([5.0, 5.0]), Some(true));
    assert_eq!(
        view.click([90.0, 30.0]),
        Some(false),
        "the label toggles too"
    );
    assert_eq!(view.click([101.0, 5.0]), None);
    view.set_content(Arc::new(CheckContent {
        checked: true,
        enabled: false,
        label: None,
    }));
    assert!(view.checked, "the host's state");
    assert_eq!(view.click([5.0, 5.0]), None, "disabled");
    assert_eq!(view.state(), State::Disabled);
    view.set_content(Arc::new(CheckContent::default()));
    view.hover = true;
    assert_eq!(view.state(), State::Hilite);
}
