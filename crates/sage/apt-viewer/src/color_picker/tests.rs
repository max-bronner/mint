use super::Palette;

fn palette() -> Palette {
    let px: [[u8; 4]; 4] = [
        [9, 9, 9, 9],
        [255, 0, 0, 255],
        [255, 0, 0, 0],
        [0, 255, 0, 255],
    ];
    Palette {
        texture_width: 4,
        texture_height: 1,
        rgba: px.concat(),
        origin: [1, 0],
        size: [3, 1],
    }
}

#[test]
fn a_cursor_samples_the_texel_under_it_as_argb() {
    let p = palette();
    assert_eq!(p.sample([0.5, 0.0], [30.0, 10.0]), Some(0xFFFF_0000));
    assert_eq!(p.sample([25.0, 0.0], [30.0, 10.0]), Some(0xFF00_FF00));
    assert_eq!(p.sample([15.0, 0.0], [30.0, 10.0]), Some(0x00FF_0000));
}

#[test]
fn a_cursor_outside_the_texture_samples_nothing() {
    assert_eq!(palette().sample([-20.0, 0.0], [30.0, 10.0]), None);
}

#[test]
fn a_colour_is_found_at_its_nearest_texel_alpha_included() {
    let p = palette();
    assert_eq!(p.locate(0xFF00_FF00, [30.0, 10.0]), Some([20.0, 0.0]));
    assert_eq!(p.locate(0x00FF_0000, [30.0, 10.0]), Some([10.0, 0.0]));
}
