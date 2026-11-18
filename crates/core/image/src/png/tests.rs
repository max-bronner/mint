use zune_png::zune_core::bit_depth::BitDepth;
use zune_png::zune_core::options::EncoderOptions;
use zune_png::PngEncoder;

use super::*;

fn encoded(width: usize, height: usize, colorspace: ColorSpace, samples: &[u8]) -> Vec<u8> {
    let options = EncoderOptions::new(width, height, colorspace, BitDepth::Eight);
    let mut out = Vec::new();
    PngEncoder::new(samples, options)
        .encode(&mut out)
        .expect("encode");
    out
}

#[test]
fn rgba_keeps_its_alpha_and_grey_alpha_is_widened() {
    let rgba = [1, 2, 3, 0, 10, 20, 30, 128, 0, 0, 0, 255, 255, 255, 255, 7];
    let image = decode(&encoded(2, 2, ColorSpace::RGBA, &rgba)).expect("valid");
    assert_eq!((image.width(), image.height()), (2, 2));
    assert_eq!(image.pixels(), rgba);

    let luma_alpha = [9, 0, 200, 99];
    let image = decode(&encoded(2, 1, ColorSpace::LumaA, &luma_alpha)).expect("valid");
    assert_eq!(image.pixels(), [9, 9, 9, 0, 200, 200, 200, 99]);
}

#[test]
fn opaque_rgb_gains_an_opaque_alpha() {
    let image = decode(&encoded(1, 1, ColorSpace::RGB, &[5, 6, 7])).expect("valid");
    assert_eq!(image.pixels(), [5, 6, 7, 255]);
}

#[test]
fn corruption_and_oversize_are_errors_not_pictures() {
    let mut file = encoded(2, 2, ColorSpace::RGBA, &[0; 16]);
    let truncated = file.get(..file.len() - 20).expect("long enough").to_vec();
    assert!(decode(&truncated).is_err());
    file.splice(16..20, 9000u32.to_be_bytes());
    assert!(decode(&file).is_err());
    assert!(decode(b"\x89PNG\r\n\x1a\n").is_err());
}
