use super::*;

const TEMPLATE: &str = "WINDOW
 WINDOWTYPE = HORZSLIDER;
 ENABLEDDRAWDATA = IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 DISABLEDDRAWDATA = IMAGE: Bar, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: Gap, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 HILITEDRAWDATA = IMAGE: Outline, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 SLIDERDATA = MINVALUE: 10, MAXVALUE: 20;
 SLIDERTHUMBENABLEDDRAWDATA = IMAGE: End, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 SLIDERTHUMBHILITEDRAWDATA = IMAGE: EndLit, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
END
";

#[test]
fn the_look_reads_the_segments_from_the_disabled_slots() {
    let look = SliderLook::read(&Layout::parse(TEMPLATE).unwrap_or_default());
    assert_eq!(look.filled.as_deref(), Some("Bar"));
    assert_eq!(look.empty.as_deref(), Some("Gap"));
    assert_eq!(look.outline.as_deref(), Some("Outline"));
    assert_eq!(look.thumb[1].as_deref(), Some("EndLit"));
    assert_eq!(look.range, [10, 20]);
    let resolved = look.resolved(|name: &String| (name == "Bar").then_some((1u8, [6.0, 24.0])));
    assert_eq!((resolved.filled, resolved.segment), (Some(1), [6.0, 24.0]));
    assert_eq!(resolved.outline, None);
}

#[test]
fn the_thumb_moves_by_units_per_step() {
    let bounds = [0.0, 0.0, 113.0, 24.0];
    assert!((units_per_step(113.0, [0, 100]) - 1.0).abs() < 1e-6);
    let t = thumb_rect(bounds, 50, [0, 100]);
    assert!((t[0] - 50.0).abs() < 1e-4 && (t[2] - 63.0).abs() < 1e-4);
    assert!((t[1] - 32.0 / 3.0).abs() < 1e-4);
    assert!(
        (thumb_rect(bounds, 500, [0, 100])[0] - 100.0).abs() < 1e-4,
        "clamped"
    );
}

#[test]
fn a_drag_clamps_at_the_ends() {
    let bounds = [0.0, 0.0, 113.0, 24.0];
    assert_eq!(value_at(bounds, 106.5, [0, 100]), 100);
    assert_eq!(value_at(bounds, 6.0, [0, 100]), 0);
    assert_eq!(value_at(bounds, 56.5, [0, 100]), 50);
}

#[test]
fn a_click_jumps_a_fifth_toward_the_pointer() {
    let bounds = [0.0, 0.0, 113.0, 24.0];
    assert_eq!(clicked_value(bounds, 100.0, 0, [0, 100]), 22);
    assert_eq!(clicked_value(bounds, 10.0, 0, [0, 100]), 3);
    assert_eq!(clicked_value(bounds, 1.0, 50, [0, 100]), 27);
}

#[test]
fn segments_fill_to_the_value_and_centre() {
    let bounds = [0.0, 0.0, 50.0, 24.0];
    let (starts, filled) = segments(bounds, 6.0, 50, [0, 100]);
    assert_eq!(starts, vec![2.0, 10.0, 18.0, 26.0, 34.0, 42.0]);
    assert_eq!(filled, 4);
    assert_eq!(segments(bounds, 6.0, 0, [0, 100]).1, 0);
    assert_eq!(segments(bounds, 6.0, 100, [0, 100]).1, 6);
    assert!(segments(bounds, 0.0, 50, [0, 100]).0.is_empty());
}

#[test]
fn a_slider_view_clicks_drags_and_follows_its_host() {
    let mut view = SliderView::new(SliderContent::default());
    view.bounds = Some([0.0, 0.0, 113.0, 24.0]);
    view.range = [0, 100];
    assert_eq!(view.click([100.0, 10.0]), Some(22));
    assert_eq!(view.drag([56.5, 10.0]), Some(50));
    assert_eq!(view.drag([56.5, 10.0]), None, "unchanged");
    assert_eq!(view.click([200.0, 10.0]), None, "outside");
    view.set_content(SliderContent {
        value: 80,
        enabled: false,
    });
    assert_eq!(view.value, 80);
    assert_eq!(view.drag([10.0, 10.0]), None, "disabled");
}
