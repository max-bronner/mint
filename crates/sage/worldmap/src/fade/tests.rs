use super::*;

fn selected_shine() -> IconObjectBase {
    IconObjectBase {
        hide_when_unselected: Some(true),
        fade_type_for_selection: Some(FadeType::InOut),
        fade_method: Some(FadeMethod::Additive),
        fade_hold_percent: Some(0.16),
        ..IconObjectBase::default()
    }
}

fn hilighted_decal() -> IconObjectBase {
    IconObjectBase {
        hide_when_unhilighted: Some(true),
        fade_type_for_hilighting: Some(FadeType::In),
        fade_type_for_unhilighting: Some(FadeType::Out),
        ..IconObjectBase::default()
    }
}

#[test]
fn a_selection_inout_rises_then_settles_on_the_hold_and_a_deselect_hides_at_once() {
    let object = selected_shine();
    let mut fade = ObjectFade::new(&object);
    assert!(!fade.shown());
    fade.set_flags(&object, SELECTED);
    assert!(fade.shown());
    assert_eq!(fade.alpha(), 0.1, "the start runs one update");
    let mut seen = vec![fade.alpha()];
    for _ in 0..25 {
        fade.update();
        seen.push(fade.alpha());
    }
    assert_eq!(seen[9], 1.0);
    assert_eq!(seen[10], 0.9);
    assert_eq!(*seen.last().unwrap(), 0.16);
    assert!(fade.shown(), "a non-zero hold keeps it");
    fade.set_flags(&object, 0);
    assert!(!fade.shown());
}

#[test]
fn a_highlight_fades_in_and_an_unhighlight_fades_out_then_hides() {
    let object = hilighted_decal();
    let mut fade = ObjectFade::new(&object);
    fade.set_flags(&object, HILIGHTED);
    assert!(fade.shown());
    assert_eq!(fade.alpha(), 0.0);
    for _ in 0..10 {
        fade.update();
    }
    assert_eq!(fade.alpha(), 1.0);
    fade.set_flags(&object, 0);
    assert!(fade.shown(), "shown until the fade ends");
    for _ in 0..10 {
        fade.update();
    }
    assert!(!fade.shown());
    assert_eq!(fade.alpha(), 0.0);
}

#[test]
fn additive_scales_the_colour_and_opacity_the_alpha() {
    let white = [1.0; 4];
    assert_eq!(
        tint(Some(FadeMethod::Additive), white, 0.5),
        [0.5, 0.5, 0.5, 1.0]
    );
    assert_eq!(tint(None, white, 0.5), [1.0, 1.0, 1.0, 0.5]);
}
