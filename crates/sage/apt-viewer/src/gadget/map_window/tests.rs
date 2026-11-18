use super::*;

const TEMPLATE: &str = "WINDOW
 WINDOWTYPE = USER;
 SCREENRECT = UPPERLEFT: 100 100, BOTTOMRIGHT: 262 236, CREATIONRESOLUTION: 800 600;
 CHILD
 WINDOW
  WINDOWTYPE = STATICTEXT;
  SCREENRECT = UPPERLEFT: 0 0, BOTTOMRIGHT: 99 99, CREATIONRESOLUTION: 800 600;
  ENABLEDDRAWDATA = IMAGE: NotAButton, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 END
 WINDOW
  WINDOWTYPE = PUSHBUTTON;
  SCREENRECT = UPPERLEFT: 10 20, BOTTOMRIGHT: 30 44, CREATIONRESOLUTION: 800 600;
  FONT = NAME: \"Face\", SIZE: 16, BOLD: 0;
  TEXTCOLOR = ENABLED: 255 0 0 255, ENABLEDBORDER: 0 0 0 255, DISABLED: 0 255 0 255, DISABLEDBORDER: 0 0 0 255, HILITE: 0 0 255 255, HILITEBORDER: 0 0 0 255;
  ENABLEDDRAWDATA = IMAGE: Ring, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0, IMAGE: RingDown, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
  HILITEDRAWDATA = IMAGE: RingLit, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
  DISABLEDDRAWDATA = IMAGE: RingDim, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 END
 ENDALLCHILDREN
END
";

fn spot(x: f32, y: f32) -> Spot {
    Spot {
        at: [x, y],
        label: None,
    }
}

#[test]
fn the_look_is_the_push_button_childs() {
    let look = MapWindowLook::read(&Layout::parse(TEMPLATE).unwrap_or_default());
    assert_eq!(
        look.spot,
        [
            Some("Ring".to_owned()),
            Some("RingLit".to_owned()),
            Some("RingDim".to_owned())
        ]
    );
    assert_eq!(look.pushed.as_deref(), Some("RingDown"));
    assert_eq!(look.spot_size, [20.0, 24.0]);
    assert_eq!(look.creation, [800.0, 600.0]);
    assert_eq!(look.text[0], [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(look.text[1], [0.0, 0.0, 1.0, 1.0], "hilite is the second");
    assert_eq!(look.font, "Face");
    assert_eq!(look.points, crate::gadget::combo::TEXT_POINTS);
    let units = look.spot_units([1024.0, 768.0]);
    assert!((units[0] - 25.6).abs() < 1e-4 && (units[1] - 30.72).abs() < 1e-4);
}

#[test]
fn an_inset_takes_half_a_texel_off_each_side() {
    let r = inset_half_texel([0.25, 0.5, 0.5, 0.25], [25.0, 10.0]);
    let want = [0.26, 0.5125, 0.48, 0.225];
    for (a, b) in r.iter().zip(want) {
        assert!((a - b).abs() < 1e-6, "{r:?}");
    }
    assert_eq!(
        inset_half_texel([0.0, 0.0, 1.0, 1.0], [1.0, 0.0]),
        [0.0, 0.0, 1.0, 1.0]
    );
}

#[test]
fn the_picture_keeps_the_maps_proportions() {
    let bounds = [10.0, 20.0, 110.0, 120.0];
    let tall = picture_rect(bounds, [4000.0, 5000.0]);
    assert_eq!(tall, [20.0, 20.0, 100.0, 120.0]);
    let wide = picture_rect(bounds, [5000.0, 2500.0]);
    assert_eq!(wide, [10.0, 45.0, 110.0, 95.0]);
    assert_eq!(picture_rect(bounds, [7.0, 7.0]), bounds);
    assert_eq!(picture_rect(bounds, [0.0, 7.0]), bounds, "no extent");
}

#[test]
fn a_button_is_centred_on_its_start() {
    let rects = spot_rects([0.0, 0.0, 100.0, 200.0], &[spot(0.25, 0.75)], [10.0, 20.0]);
    assert_eq!(rects, vec![[20.0, 40.0, 30.0, 60.0]]);
}

#[test]
fn an_overlapping_button_moves_off_the_earlier_one() {
    let size = [10.0, 10.0];
    let picture = [0.0, 0.0, 100.0, 100.0];
    let right = spot_rects(picture, &[spot(0.5, 0.5), spot(0.58, 0.48)], size);
    assert_eq!(right[1][0], right[0][2] + 1.0);
    assert!((right[1][1] - 47.0).abs() < 1e-4, "only x moved");
    let down = spot_rects(picture, &[spot(0.5, 0.5), spot(0.52, 0.42)], size);
    assert_eq!(down[1][1], down[0][3] + 1.0);
    assert!((down[1][0] - 47.0).abs() < 1e-4, "only y moved");
    let tie = spot_rects(picture, &[spot(0.5, 0.5), spot(0.55, 0.45)], size);
    assert_eq!([tie[1][0], tie[1][1]], [tie[0][2] + 1.0, tie[0][3] + 1.0]);
    let apart = spot_rects(picture, &[spot(0.125, 0.125), spot(0.875, 0.875)], size);
    assert_eq!(apart[1], [82.5, 7.5, 92.5, 17.5], "no overlap, no move");
}

#[test]
fn a_click_finds_its_button() {
    let content = MapWindowContent {
        spots: vec![spot(0.2, 0.8), spot(0.8, 0.2)],
        ..MapWindowContent::default()
    };
    let mut view = MapWindowView::new(std::sync::Arc::new(content.clone()));
    view.rects = vec![[0.0, 0.0, 10.0, 10.0], [5.0, 5.0, 15.0, 15.0]];
    assert_eq!(view.spot_at([2.0, 2.0]), Some(0));
    assert_eq!(view.spot_at([7.0, 7.0]), Some(1), "the later is on top");
    assert_eq!(view.spot_at([20.0, 20.0]), None);
    assert_eq!(view.spot_at([10.0, 3.0]), None, "right edge is outside");
    view.hover = Some(1);
    assert_eq!(view.state(1), State::Hilite);
    assert_eq!(view.state(0), State::Enabled);
    let mut off = MapWindowView::new(std::sync::Arc::new(MapWindowContent {
        enabled: false,
        ..content
    }));
    off.rects = view.rects.clone();
    assert_eq!(off.spot_at([2.0, 2.0]), None);
    assert_eq!(off.state(0), State::Disabled);
}
