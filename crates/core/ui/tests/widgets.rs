#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used
)]

use mint_render::{Camera, Error, Renderer, Scene, UiQuad, Vec3};
use mint_ui::{
    Button, ButtonStyle, FontId, GlyphAtlas, GlyphAtlasConfig, HitRegion, InputRouter, Label, List,
    PointerButton, Rect, WidgetId, WidgetState,
};

const SIZE: u32 = 64;

fn renderer(size: u32) -> Option<Renderer> {
    match Renderer::offscreen(size, size) {
        Ok(renderer) => Some(renderer),
        Err(Error::NoAdapter) => {
            eprintln!(
                "SKIPPED: no graphics adapter on this machine, so nothing in \
                 tests/widgets.rs was actually verified"
            );
            None
        }
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    }
}

fn pixel(rgba: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * size + x) * 4) as usize;
    [
        rgba[index],
        rgba[index + 1],
        rgba[index + 2],
        rgba[index + 3],
    ]
}

fn ui_scene(quads: &[UiQuad]) -> Scene<'_> {
    let mut scene = Scene::new(Camera {
        eye: Vec3::new(0.0, 0.0, 1.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        fov_y: std::f32::consts::FRAC_PI_2,
        near: 0.05,
        far: 10.0,
    });
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene.with_ui_quads(quads)
}

fn distinct_button_style() -> ButtonStyle {
    ButtonStyle {
        idle: [1.0, 0.0, 0.0, 1.0],
        hovered: [0.0, 1.0, 0.0, 1.0],
        pressed: [0.0, 0.0, 1.0, 1.0],
    }
}

#[test]
fn a_button_changes_colour_on_hover_and_press_under_scripted_input() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let button = Button::new(
        WidgetId(1),
        Rect {
            min: [16.0, 16.0],
            max: [48.0, 48.0],
        },
        "Start",
    )
    .with_style(distinct_button_style());
    let targets = [button.hit_target()];
    let mut router = InputRouter::new();

    renderer
        .render(&ui_scene(&[button.quad(&router)]))
        .expect("render idle");
    assert_eq!(
        pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32),
        [255, 0, 0, 255],
        "idle: red"
    );

    router.pointer_moved(&targets, [32.0, 32.0]);
    renderer
        .render(&ui_scene(&[button.quad(&router)]))
        .expect("render hovered");
    assert_eq!(
        pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32),
        [0, 255, 0, 255],
        "hovered: green"
    );

    router.pointer_button(&targets, [32.0, 32.0], PointerButton::Primary, true);
    renderer
        .render(&ui_scene(&[button.quad(&router)]))
        .expect("render pressed");
    assert_eq!(
        pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32),
        [0, 0, 255, 255],
        "pressed: blue"
    );

    router.pointer_button(&targets, [32.0, 32.0], PointerButton::Primary, false);
    renderer
        .render(&ui_scene(&[button.quad(&router)]))
        .expect("render released");
    assert_eq!(
        pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32),
        [0, 255, 0, 255],
        "released while still hovered: green again"
    );
}

#[test]
fn a_thousand_item_list_only_draws_and_only_paints_its_visible_rows() {
    let viewport = Rect {
        min: [0.0, 0.0],
        max: [64.0, 64.0],
    };
    let item_height = 8.0;
    let list = List::new(WidgetId(1), viewport, item_height, 1_000);
    let mut state = WidgetState::new();

    let visible = list.visible_rows(&state);
    assert_eq!(visible, 0..8, "64px viewport / 8px rows, no partial row");

    let quads: Vec<UiQuad> = visible
        .clone()
        .map(|index| {
            let rect = list.row_rect(index, &state);
            let color = if index % 2 == 0 {
                [0.0, 1.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0, 1.0]
            };
            let [x, y] = rect.min;
            let [max_x, max_y] = rect.max;
            UiQuad::new(x, y, max_x - x, max_y - y).with_color_transform(color, [0.0; 4])
        })
        .collect();

    assert_eq!(
        quads.len(),
        8,
        "one quad per visible row, not per backing item"
    );

    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    renderer.render(&ui_scene(&quads)).expect("render");
    assert_eq!(
        renderer.draw_calls(),
        1,
        "every row shares one (textureless) material, so they batch into a \
         single GPU draw call regardless of row count - this does not by \
         itself prove culling, the quad count above does"
    );
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, 4, 4)[1], 255, "row 0 (even): green");
    assert_eq!(pixel(&rgba, SIZE, 4, 12)[0], 255, "row 1 (odd): red");
    assert_eq!(pixel(&rgba, SIZE, 4, 60)[0], 255, "row 7 (odd): red");

    list.scroll_by(&mut state, 400.0);
    let visible = list.visible_rows(&state);
    assert_eq!(visible, 50..58);

    let quads: Vec<UiQuad> = visible
        .map(|index| {
            let rect = list.row_rect(index, &state);
            let color = if index % 2 == 0 {
                [0.0, 1.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0, 1.0]
            };
            let [x, y] = rect.min;
            let [max_x, max_y] = rect.max;
            UiQuad::new(x, y, max_x - x, max_y - y).with_color_transform(color, [0.0; 4])
        })
        .collect();
    assert_eq!(quads.len(), 8);

    renderer.render(&ui_scene(&quads)).expect("render scrolled");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, 4, 4)[1], 255, "row 50 (even): green");
    assert_eq!(pixel(&rgba, SIZE, 4, 12)[0], 255, "row 51 (odd): red");
}

#[test]
fn a_font_name_that_resolves_to_nothing_still_renders_a_visible_tofu_box() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let label = Label::new(
        Rect {
            min: [8.0, 8.0],
            max: [56.0, 32.0],
        },
        "X",
    )
    .with_color([1.0, 0.5, 0.0, 1.0]);
    let mut atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config()).expect("atlas");
    atlas.begin_frame();
    let quads = label.glyph_quads(
        FontId(0),
        None,
        &mut atlas,
        16,
        dummy_texture(&mut renderer),
    );
    assert_eq!(quads.len(), 1, "one tofu box for the one visible character");
    assert_eq!(
        quads[0].texture, None,
        "a tofu quad is untextured, unlike a real rasterised glyph"
    );

    renderer.render(&ui_scene(&quads)).expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let sampled = pixel(&rgba, SIZE, 10, 5);
    assert_eq!(
        sampled,
        [255, 127, 0, 255],
        "the tofu box must actually be on screen, in the label's own colour \
         (found {sampled:?} instead - a panic-free but invisible fallback \
         would be exactly as wrong as a panic, per R8)"
    );
}

fn dummy_texture(renderer: &mut Renderer) -> mint_render::TextureHandle {
    let data = mint_render::TextureData::rgba8(1, 1, vec![255, 255, 255, 255]).expect("1x1 rgba");
    renderer.upload_texture(&data).expect("upload")
}

#[test]
fn a_hit_region_produces_a_hit_test_result_and_costs_the_frame_nothing_to_draw() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let region = HitRegion::new(
        WidgetId(1),
        Rect {
            min: [0.0, 0.0],
            max: [64.0, 64.0],
        },
    );
    let targets = [region.hit_target()];
    let mut router = InputRouter::new();

    assert_eq!(
        router.pointer_button(&targets, [32.0, 32.0], PointerButton::Primary, true),
        mint_ui::PointerRouting::Widget(WidgetId(1)),
        "a hit region with nothing drawn for it must still be hit-testable"
    );

    let empty_quads: [UiQuad; 0] = [];
    renderer.render(&ui_scene(&empty_quads)).expect("render");
    let baseline_draw_calls = renderer.draw_calls();

    renderer.render(&ui_scene(&empty_quads)).expect("render");
    assert_eq!(
        renderer.draw_calls(),
        baseline_draw_calls,
        "registering a hit region must not add a single draw call"
    );
    assert_eq!(
        baseline_draw_calls, 0,
        "a frame with no widgets draws nothing"
    );

    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, 32, 32),
        [0, 0, 0, 255],
        "the region covers the whole frame and is hit-active there, but \
         nothing was ever drawn for it - the clear colour must survive"
    );
}
