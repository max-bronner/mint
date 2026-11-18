#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::cast_possible_truncation
)]

use mint_movie::{convert_reference, FramePlanes, PlaneRef, VideoSurface};
use mint_render::{Camera, Error, Renderer, Scene, Vec3};

const WIDTH: u32 = 32;
const CODED_HEIGHT: u32 = 32;

fn renderer(height: u32) -> Option<Renderer> {
    match Renderer::offscreen(WIDTH, height) {
        Ok(renderer) => Some(renderer),
        Err(Error::NoAdapter) => {
            eprintln!("SKIPPED: no graphics adapter, so the shader was not compared");
            None
        }
        Err(error) => panic!("offscreen renderer: {error}"),
    }
}

fn noise(seed: u32, len: usize) -> Vec<u8> {
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

fn plane(width: u32, height: u32, data: &[u8]) -> PlaneRef<'_> {
    PlaneRef {
        width,
        height,
        data,
    }
}

fn camera() -> Camera {
    Camera {
        eye: Vec3::new(0.0, 0.0, 10.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        fov_y: std::f32::consts::FRAC_PI_2,
        near: 0.05,
        far: 100.0,
    }
}

fn draw(renderer: &mut Renderer, surface: &VideoSurface) -> Vec<u8> {
    let mut scene = Scene::new(camera());
    scene.clear_color = [0.0, 0.0, 0.0, 0.0];
    let instance = surface.instance().expect("uploaded");
    renderer
        .render(&scene.with_instances(&[instance]))
        .expect("render");
    renderer.read_pixels().expect("readback")
}

#[test]
fn the_shader_matches_the_cpu_reference_on_every_pixel() {
    check(20);
}

#[test]
fn an_odd_height_picture_drops_its_last_display_row_and_pairs_chroma_from_the_bottom() {
    check(21);
}

fn check(height: u32) {
    let Some(mut renderer) = renderer(height) else {
        return;
    };
    let mut surface = VideoSurface::new(&mut renderer).expect("surface");
    let (coded_w, coded_h) = (WIDTH as usize, CODED_HEIGHT as usize);
    for (seed, has_alpha) in [(7u32, true), (99, false), (123, false)] {
        let y = noise(seed, coded_w * coded_h);
        let u = noise(seed + 1, coded_w * coded_h / 4);
        let v = noise(seed + 2, coded_w * coded_h / 4);
        let a = noise(seed + 3, coded_w * coded_h);
        let frame = FramePlanes {
            width: WIDTH,
            height,
            bottom_up: true,
            y: plane(WIDTH, CODED_HEIGHT, &y),
            u: plane(WIDTH / 2, CODED_HEIGHT / 2, &u),
            v: plane(WIDTH / 2, CODED_HEIGHT / 2, &v),
            alpha: has_alpha.then(|| plane(WIDTH, CODED_HEIGHT, &a)),
        };
        surface.upload(&mut renderer, &frame).expect("upload");
        let pixels = draw(&mut renderer, &surface);
        let mut wrong = 0;
        let h = height as usize;
        for row in 0..h {
            let source_row = h - 1 - row;
            for x in 0..WIDTH as usize {
                let luma = y[source_row * coded_w + x];
                let alpha = has_alpha.then(|| a[source_row * coded_w + x]);
                let want = if row >= (h / 2) * 2 {
                    [0; 4]
                } else {
                    let chroma = (h / 2 - 1 - row / 2) * (coded_w / 2) + x / 2;
                    convert_reference(luma, u[chroma], v[chroma], alpha)
                };
                let at = (row * WIDTH as usize + x) * 4;
                if pixels.get(at..at + 4) != Some(&want[..]) {
                    wrong += 1;
                }
            }
        }
        assert_eq!(wrong, 0, "{wrong} of {} pixels differ", WIDTH * height);
    }
}
