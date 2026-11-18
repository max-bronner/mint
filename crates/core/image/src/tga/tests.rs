use super::{
    decode, info, Error, MAX_DIMENSION, TYPE_GREYSCALE, TYPE_GREYSCALE_RLE, TYPE_TRUE_COLOR,
    TYPE_TRUE_COLOR_RLE,
};
use crate::synth::TgaBuilder;

fn quad_bgra() -> Vec<u8> {
    let mut data = Vec::new();
    for pixel in [
        [0x00, 0x00, 0xff, 0xff],
        [0x00, 0xff, 0x00, 0xee],
        [0xff, 0x00, 0x00, 0xdd],
        [0xff, 0xff, 0xff, 0xcc],
    ] {
        data.extend_from_slice(&pixel);
    }
    data
}

#[test]
fn a_32_bit_pixel_decodes_with_red_and_blue_the_right_way_round() {
    let tga = TgaBuilder::new(1, 1)
        .top_origin()
        .data(&[0x11, 0x22, 0x33, 0x44]);
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([0x33, 0x22, 0x11, 0x44]));
}

#[test]
fn a_24_bit_pixel_gets_an_opaque_alpha() {
    let tga = TgaBuilder::new(1, 1)
        .bits_per_pixel(24)
        .top_origin()
        .data(&[0x11, 0x22, 0x33]);
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([0x33, 0x22, 0x11, 0xff]));
}

#[test]
fn a_bottom_up_image_is_flipped_so_the_first_decoded_row_is_the_top_one() {
    let bottom = [0x00, 0x00, 0xff, 0xff];
    let top = [0xff, 0x00, 0x00, 0xff];
    let mut data = Vec::new();
    data.extend_from_slice(&bottom);
    data.extend_from_slice(&top);

    let tga = TgaBuilder::new(1, 2).data(&data);
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([0x00, 0x00, 0xff, 0xff]), "top row");
    assert_eq!(
        image.pixel(0, 1),
        Some([0xff, 0x00, 0x00, 0xff]),
        "bottom row"
    );
}

#[test]
fn a_top_origin_image_is_not_flipped() {
    let tga = TgaBuilder::new(2, 2).top_origin().data(&quad_bgra());
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([0xff, 0x00, 0x00, 0xff]));
    assert_eq!(image.pixel(1, 1), Some([0xff, 0xff, 0xff, 0xcc]));
}

#[test]
fn a_right_origin_image_has_each_row_mirrored() {
    let tga = TgaBuilder::new(2, 2)
        .top_origin()
        .right_origin()
        .data(&quad_bgra());
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(1, 0), Some([0xff, 0x00, 0x00, 0xff]));
    assert_eq!(image.pixel(0, 0), Some([0x00, 0xff, 0x00, 0xee]));
}

#[test]
fn an_id_field_and_colour_map_are_stepped_over_to_find_the_pixels() {
    let tga = TgaBuilder::new(1, 1)
        .top_origin()
        .id_field(b"written by something")
        .colour_map(4, 32)
        .data(&[0x11, 0x22, 0x33, 0x44]);
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([0x33, 0x22, 0x11, 0x44]));
}

#[test]
fn a_run_length_packet_repeats_one_pixel() {
    let packets = [0x83, 0x11, 0x22, 0x33, 0x44];
    let tga = TgaBuilder::new(4, 1)
        .image_type(TYPE_TRUE_COLOR_RLE)
        .top_origin()
        .data(&packets);
    let image = decode(&tga).unwrap();
    for x in 0..4 {
        assert_eq!(image.pixel(x, 0), Some([0x33, 0x22, 0x11, 0x44]));
    }
}

#[test]
fn a_literal_packet_copies_pixels_one_for_one() {
    let mut packets = vec![0x01];
    packets.extend_from_slice(&[0x00, 0x00, 0xff, 0xff]);
    packets.extend_from_slice(&[0xff, 0x00, 0x00, 0x80]);
    let tga = TgaBuilder::new(2, 1)
        .image_type(TYPE_TRUE_COLOR_RLE)
        .top_origin()
        .data(&packets);
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([0xff, 0x00, 0x00, 0xff]));
    assert_eq!(image.pixel(1, 0), Some([0x00, 0x00, 0xff, 0x80]));
}

#[test]
fn a_run_length_image_is_flipped_like_an_uncompressed_one() {
    let mut packets = vec![0x00];
    packets.extend_from_slice(&[0x00, 0x00, 0xff, 0xff]);
    packets.push(0x00);
    packets.extend_from_slice(&[0xff, 0x00, 0x00, 0xff]);
    let tga = TgaBuilder::new(1, 2)
        .image_type(TYPE_TRUE_COLOR_RLE)
        .data(&packets);
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([0x00, 0x00, 0xff, 0xff]));
}

#[test]
fn the_header_reports_what_it_declares_without_reading_pixels() {
    let tga = TgaBuilder::new(8, 4).top_origin().alpha_bits(8).data(&[]);
    let header = info(&tga).unwrap();
    assert_eq!(header.width, 8);
    assert_eq!(header.height, 4);
    assert_eq!(header.bits_per_pixel, 32);
    assert_eq!(header.image_type, TYPE_TRUE_COLOR);
    assert!(header.top_origin);
    assert_eq!(header.attribute_bits, 8);
    assert_eq!(header.data_offset, 18);
}

#[test]
fn a_header_shorter_than_eighteen_bytes_errors() {
    for len in 0..18 {
        let bytes = vec![0u8; len];
        assert!(
            matches!(decode(&bytes), Err(Error::HeaderTruncated { .. })),
            "{len}-byte input"
        );
    }
}

#[test]
fn a_zero_dimension_errors_rather_than_producing_an_empty_image() {
    for (w, h) in [(0, 4), (4, 0), (0, 0)] {
        let tga = TgaBuilder::new(w, h).top_origin().data(&[]);
        assert!(
            matches!(decode(&tga), Err(Error::ZeroDimension { .. })),
            "{w}x{h}"
        );
    }
}

#[test]
fn a_dimension_past_the_cap_errors_before_any_pixel_is_decoded() {
    for (w, h) in [
        (MAX_DIMENSION + 1, 4),
        (4, MAX_DIMENSION + 1),
        (u16::MAX, u16::MAX),
    ] {
        let tga = TgaBuilder::new(w, h).top_origin().data(&[]);
        assert!(
            matches!(decode(&tga), Err(Error::TgaTooLarge { .. })),
            "{w}x{h}"
        );
    }
}

#[test]
fn an_unsupported_image_type_is_named_rather_than_guessed_at() {
    for image_type in [0, 1, 9, 32, 33, 82] {
        let tga = TgaBuilder::new(1, 1).image_type(image_type).data(&[]);
        assert!(
            matches!(
                decode(&tga),
                Err(Error::TgaUnsupportedImageType { image_type: got }) if got == image_type
            ),
            "type {image_type}"
        );
    }
}

#[test]
fn an_unsupported_pixel_depth_is_named_rather_than_guessed_at() {
    for bits in [1, 4, 8, 15, 31, 64] {
        let tga = TgaBuilder::new(1, 1).bits_per_pixel(bits).data(&[]);
        assert!(
            matches!(decode(&tga), Err(Error::TgaUnsupportedDepth { bits: got }) if got == bits),
            "{bits} bpp on a true-colour image"
        );
    }
    for bits in [16, 24, 32] {
        let tga = TgaBuilder::new(1, 1)
            .image_type(TYPE_GREYSCALE)
            .bits_per_pixel(bits)
            .data(&[]);
        assert!(
            matches!(decode(&tga), Err(Error::TgaUnsupportedDepth { bits: got }) if got == bits),
            "{bits} bpp on a greyscale image"
        );
    }
}

#[test]
fn a_16_bit_pixel_expands_each_channel_so_that_all_ones_is_255() {
    let tga = TgaBuilder::new(1, 1)
        .bits_per_pixel(16)
        .top_origin()
        .data(&0x7fffu16.to_le_bytes());
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([255, 255, 255, 255]));
}

#[test]
fn a_16_bit_pixel_puts_each_channel_in_the_right_place() {
    let red = TgaBuilder::new(1, 1)
        .bits_per_pixel(16)
        .top_origin()
        .data(&0x7c00u16.to_le_bytes());
    assert_eq!(decode(&red).unwrap().pixel(0, 0), Some([255, 0, 0, 255]));

    let blue = TgaBuilder::new(1, 1)
        .bits_per_pixel(16)
        .top_origin()
        .data(&0x001fu16.to_le_bytes());
    assert_eq!(decode(&blue).unwrap().pixel(0, 0), Some([0, 0, 255, 255]));

    let green = TgaBuilder::new(1, 1)
        .bits_per_pixel(16)
        .top_origin()
        .data(&0x0200u16.to_le_bytes());
    assert_eq!(decode(&green).unwrap().pixel(0, 0), Some([0, 132, 0, 255]));
}

#[test]
fn a_16_bit_image_honours_bit_15_only_when_it_declares_an_attribute_bit() {
    let transparent = TgaBuilder::new(1, 1)
        .bits_per_pixel(16)
        .alpha_bits(1)
        .top_origin()
        .data(&0x001fu16.to_le_bytes());
    assert_eq!(
        decode(&transparent).unwrap().pixel(0, 0),
        Some([0, 0, 255, 0]),
        "bit 15 clear and one attribute bit declared means transparent"
    );

    let opaque = TgaBuilder::new(1, 1)
        .bits_per_pixel(16)
        .alpha_bits(1)
        .top_origin()
        .data(&0x801fu16.to_le_bytes());
    assert_eq!(decode(&opaque).unwrap().pixel(0, 0), Some([0, 0, 255, 255]));

    let no_alpha_declared = TgaBuilder::new(1, 1)
        .bits_per_pixel(16)
        .top_origin()
        .data(&0x001fu16.to_le_bytes());
    assert_eq!(
        decode(&no_alpha_declared).unwrap().pixel(0, 0),
        Some([0, 0, 255, 255])
    );
}

#[test]
fn a_greyscale_pixel_becomes_an_opaque_grey() {
    let tga = TgaBuilder::new(2, 1)
        .image_type(TYPE_GREYSCALE)
        .bits_per_pixel(8)
        .top_origin()
        .data(&[0x00, 0x7f]);
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([0, 0, 0, 255]));
    assert_eq!(image.pixel(1, 0), Some([0x7f, 0x7f, 0x7f, 255]));
}

#[test]
fn a_run_length_encoded_greyscale_image_decodes_like_its_raw_form() {
    let tga = TgaBuilder::new(4, 1)
        .image_type(TYPE_GREYSCALE_RLE)
        .bits_per_pixel(8)
        .top_origin()
        .data(&[0x82, 0x40, 0x00, 0xff]);
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([0x40, 0x40, 0x40, 255]));
    assert_eq!(image.pixel(2, 0), Some([0x40, 0x40, 0x40, 255]));
    assert_eq!(image.pixel(3, 0), Some([0xff, 0xff, 0xff, 255]));
}

#[test]
fn a_16_bit_run_length_image_decodes_two_bytes_per_pixel() {
    let mut packets = vec![0x81];
    packets.extend_from_slice(&0xfc00u16.to_le_bytes());
    packets.push(0x01);
    packets.extend_from_slice(&0x83e0u16.to_le_bytes());
    packets.extend_from_slice(&0x001fu16.to_le_bytes());
    let tga = TgaBuilder::new(4, 1)
        .image_type(TYPE_TRUE_COLOR_RLE)
        .bits_per_pixel(16)
        .alpha_bits(1)
        .top_origin()
        .data(&packets);
    let image = decode(&tga).unwrap();
    assert_eq!(image.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(1, 0), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(2, 0), Some([0, 255, 0, 255]));
    assert_eq!(image.pixel(3, 0), Some([0, 0, 255, 0]));
}

#[test]
fn dimensions_larger_than_the_pixel_data_error_before_allocating() {
    let tga = TgaBuilder::new(4096, 4096).top_origin().data(&[0; 4]);
    match decode(&tga) {
        Err(Error::DataTruncated {
            needed, available, ..
        }) => {
            assert_eq!(needed, 4096 * 4096 * 4);
            assert_eq!(available, 4);
        }
        other => panic!("expected DataTruncated, got {other:?}"),
    }
}

#[test]
fn the_largest_accepted_image_errors_rather_than_reserving_its_full_size() {
    let tga = TgaBuilder::new(MAX_DIMENSION, MAX_DIMENSION)
        .top_origin()
        .data(&[0; 16]);
    assert!(matches!(decode(&tga), Err(Error::DataTruncated { .. })));
}

#[test]
fn a_run_length_image_claiming_more_pixels_than_its_packets_could_hold_errors() {
    let tga = TgaBuilder::new(MAX_DIMENSION, MAX_DIMENSION)
        .image_type(TYPE_TRUE_COLOR_RLE)
        .top_origin()
        .data(&[0x83, 0x11, 0x22, 0x33, 0x44]);
    assert!(matches!(decode(&tga), Err(Error::DataTruncated { .. })));
}

#[test]
fn a_run_length_packet_running_past_the_declared_image_errors() {
    let tga = TgaBuilder::new(1, 1)
        .image_type(TYPE_TRUE_COLOR_RLE)
        .top_origin()
        .data(&[0x87, 0x11, 0x22, 0x33, 0x44, 0, 0, 0, 0, 0, 0, 0, 0]);
    match decode(&tga) {
        Err(Error::TgaRleOverrun {
            pixels, declared, ..
        }) => {
            assert_eq!(pixels, 7);
            assert_eq!(declared, 1);
        }
        other => panic!("expected TgaRleOverrun, got {other:?}"),
    }
}

#[test]
fn a_run_length_packet_cut_off_mid_pixel_errors() {
    let tga = TgaBuilder::new(4, 1)
        .image_type(TYPE_TRUE_COLOR_RLE)
        .top_origin()
        .data(&[0x83, 0x11, 0x22, 0x33, 0x44, 0x00, 0x11]);
    let cut = TgaBuilder::new(8, 1)
        .image_type(TYPE_TRUE_COLOR_RLE)
        .top_origin()
        .data(&[0x83, 0x11, 0x22, 0x33, 0x44, 0x03, 0x00]);
    assert!(decode(&tga).is_ok());
    assert!(matches!(decode(&cut), Err(Error::DataTruncated { .. })));
}

#[test]
fn every_prefix_of_a_valid_image_errors_rather_than_panicking() {
    let full = TgaBuilder::new(2, 2).top_origin().data(&quad_bgra());
    for len in 0..full.len() {
        let prefix = &full[..len];
        assert!(
            decode(prefix).is_err(),
            "prefix of {len} bytes decoded when it should not"
        );
    }
    assert!(decode(&full).is_ok());
}

#[test]
fn every_prefix_of_a_valid_run_length_image_errors_rather_than_panicking() {
    let mut packets = vec![0x81];
    packets.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]);
    packets.push(0x01);
    packets.extend_from_slice(&[0x00, 0x00, 0xff, 0xff]);
    packets.extend_from_slice(&[0xff, 0x00, 0x00, 0xff]);
    let full = TgaBuilder::new(2, 2)
        .image_type(TYPE_TRUE_COLOR_RLE)
        .top_origin()
        .data(&packets);
    for len in 0..full.len() {
        assert!(decode(&full[..len]).is_err(), "prefix of {len} bytes");
    }
    assert!(decode(&full).is_ok());
}

#[test]
fn flipping_any_byte_of_a_valid_image_never_panics() {
    let full = TgaBuilder::new(4, 4).top_origin().data(&[0x7f; 4 * 4 * 4]);
    for index in 0..full.len() {
        for bit in 0..8 {
            let mut corrupt = full.clone();
            corrupt[index] ^= 1 << bit;
            let _ = decode(&corrupt);
        }
    }
}

#[test]
fn flipping_any_byte_of_a_valid_run_length_image_never_panics() {
    let mut packets = Vec::new();
    for _ in 0..4 {
        packets.push(0x83);
        packets.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]);
    }
    let full = TgaBuilder::new(4, 4)
        .image_type(TYPE_TRUE_COLOR_RLE)
        .top_origin()
        .data(&packets);
    for index in 0..full.len() {
        for bit in 0..8 {
            let mut corrupt = full.clone();
            corrupt[index] ^= 1 << bit;
            let _ = decode(&corrupt);
        }
    }
}
