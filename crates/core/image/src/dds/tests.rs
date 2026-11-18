use super::{BlockFormat, Dds};
use crate::synth::{alpha_block, colour_block, DdsBuilder, DDPF_FOURCC};
use crate::Error;

const RED_565: u16 = 0xf800;
const BLUE_565: u16 = 0x001f;
const INDICES_0123: u32 = 0b11_10_01_00;

fn bc1(block: [u8; 8], width: u32, height: u32) -> Dds {
    Dds::parse(&DdsBuilder::new(width, height).data(&block)).unwrap()
}

#[test]
fn a_dxt1_block_with_the_first_endpoint_larger_decodes_four_opaque_colours() {
    let dds = bc1(colour_block(RED_565, BLUE_565, INDICES_0123), 4, 4);
    let image = dds.decode().unwrap();

    assert_eq!(image.pixel(0, 0), Some([255, 0, 0, 255]), "index 0");
    assert_eq!(image.pixel(1, 0), Some([0, 0, 255, 255]), "index 1");
    assert_eq!(image.pixel(2, 0), Some([170, 0, 85, 255]), "index 2");
    assert_eq!(image.pixel(3, 0), Some([85, 0, 170, 255]), "index 3");
    assert_eq!(image.pixel(0, 3), Some([255, 0, 0, 255]));
}

#[test]
fn a_dxt1_block_with_the_first_endpoint_smaller_decodes_three_colours_and_a_transparent_one() {
    let dds = bc1(colour_block(BLUE_565, RED_565, INDICES_0123), 4, 4);
    let image = dds.decode().unwrap();

    assert_eq!(image.pixel(0, 0), Some([0, 0, 255, 255]), "index 0");
    assert_eq!(image.pixel(1, 0), Some([255, 0, 0, 255]), "index 1");
    assert_eq!(image.pixel(2, 0), Some([127, 0, 127, 255]), "index 2");
    assert_eq!(
        image.pixel(3, 0),
        Some([0, 0, 0, 0]),
        "index 3 in three-colour mode is transparent black"
    );
}

#[test]
fn equal_endpoints_select_three_colour_mode_in_dxt1() {
    let dds = bc1(colour_block(RED_565, RED_565, INDICES_0123), 4, 4);
    let image = dds.decode().unwrap();
    assert_eq!(image.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(
        image.pixel(2, 0),
        Some([255, 0, 0, 255]),
        "midpoint of itself"
    );
    assert_eq!(image.pixel(3, 0), Some([0, 0, 0, 0]), "still transparent");
}

#[test]
fn a_dxt5_colour_block_stays_in_four_colour_mode_however_the_endpoints_compare() {
    let mut block = Vec::new();
    block.extend_from_slice(&alpha_block(255, 255, 0));
    block.extend_from_slice(&colour_block(BLUE_565, RED_565, INDICES_0123));
    let dds = Dds::parse(&DdsBuilder::new(4, 4).four_cc(*b"DXT5").data(&block)).unwrap();
    let image = dds.decode().unwrap();

    assert_eq!(image.pixel(0, 0), Some([0, 0, 255, 255]));
    assert_eq!(image.pixel(1, 0), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(2, 0), Some([85, 0, 170, 255]));
    assert_eq!(
        image.pixel(3, 0),
        Some([170, 0, 85, 255]),
        "index 3 is a colour here, not transparency"
    );
}

const ALPHA_INDICES_01234567: u64 = 0x00fa_c688;

#[test]
fn a_dxt5_alpha_block_with_the_first_endpoint_larger_uses_the_eight_value_ramp() {
    let mut block = Vec::new();
    block.extend_from_slice(&alpha_block(255, 0, ALPHA_INDICES_01234567));
    block.extend_from_slice(&colour_block(RED_565, BLUE_565, 0));
    let dds = Dds::parse(&DdsBuilder::new(4, 4).four_cc(*b"DXT5").data(&block)).unwrap();
    let image = dds.decode().unwrap();

    let expected = [255u8, 0, 218, 182, 145, 109, 72, 36];
    for (index, alpha) in expected.into_iter().enumerate() {
        let x = u32::try_from(index % 4).unwrap();
        let y = u32::try_from(index / 4).unwrap();
        assert_eq!(
            image.pixel(x, y).map(|p| p[3]),
            Some(alpha),
            "alpha index {index}"
        );
    }
    assert_eq!(image.pixel(0, 2).map(|p| p[3]), Some(255));
}

#[test]
fn a_dxt5_alpha_block_with_the_first_endpoint_smaller_uses_the_six_value_ramp() {
    let mut block = Vec::new();
    block.extend_from_slice(&alpha_block(0, 255, ALPHA_INDICES_01234567));
    block.extend_from_slice(&colour_block(RED_565, BLUE_565, 0));
    let dds = Dds::parse(&DdsBuilder::new(4, 4).four_cc(*b"DXT5").data(&block)).unwrap();
    let image = dds.decode().unwrap();

    let expected = [0u8, 255, 51, 102, 153, 204, 0, 255];
    for (index, alpha) in expected.into_iter().enumerate() {
        let x = u32::try_from(index % 4).unwrap();
        let y = u32::try_from(index / 4).unwrap();
        assert_eq!(
            image.pixel(x, y).map(|p| p[3]),
            Some(alpha),
            "alpha index {index}"
        );
    }
}

#[test]
fn a_five_six_five_endpoint_expands_so_that_all_ones_is_255() {
    let dds = bc1(colour_block(0xffff, 0x0000, 0), 4, 4);
    let image = dds.decode().unwrap();
    assert_eq!(image.pixel(0, 0), Some([255, 255, 255, 255]));
}

#[test]
fn an_image_narrower_than_one_block_only_writes_the_pixels_it_has() {
    let dds = bc1(colour_block(RED_565, BLUE_565, INDICES_0123), 3, 2);
    let image = dds.decode().unwrap();
    assert_eq!(image.pixels().len(), 3 * 2 * 4);
    assert_eq!(image.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(2, 0), Some([170, 0, 85, 255]));
    assert_eq!(image.pixel(2, 1), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(3, 0), None);
}

#[test]
fn a_mip_chain_is_sliced_into_levels_of_halving_dimensions() {
    let file = DdsBuilder::new(8, 8).mip_count(4).zero_filled();
    let dds = Dds::parse(&file).unwrap();
    let dimensions: Vec<(u32, u32)> = dds
        .levels()
        .iter()
        .map(|l| (l.width(), l.height()))
        .collect();
    assert_eq!(dimensions, vec![(8, 8), (4, 4), (2, 2), (1, 1)]);
    let sizes: Vec<usize> = dds.levels().iter().map(|l| l.blocks().len()).collect();
    assert_eq!(sizes, vec![32, 8, 8, 8]);
}

#[test]
fn a_non_square_mip_chain_stops_shrinking_an_edge_at_one() {
    let file = DdsBuilder::new(8, 2).mip_count(4).zero_filled();
    let dds = Dds::parse(&file).unwrap();
    let dimensions: Vec<(u32, u32)> = dds
        .levels()
        .iter()
        .map(|l| (l.width(), l.height()))
        .collect();
    assert_eq!(dimensions, vec![(8, 2), (4, 1), (2, 1), (1, 1)]);
}

#[test]
fn the_computed_top_level_size_agrees_with_a_correctly_declared_one() {
    let file = DdsBuilder::new(64, 32)
        .linear_size(16 * 8 * 8)
        .zero_filled();
    let dds = Dds::parse(&file).unwrap();
    assert_eq!(dds.computed_linear_size(), 16 * 8 * 8);
    assert_eq!(
        u32::try_from(dds.computed_linear_size()).unwrap(),
        dds.declared_linear_size()
    );
}

#[test]
fn the_block_format_and_its_stride_are_reported() {
    let bc1_file = DdsBuilder::new(4, 4).zero_filled();
    let bc3_file = DdsBuilder::new(4, 4).four_cc(*b"DXT5").zero_filled();
    assert_eq!(Dds::parse(&bc1_file).unwrap().format(), BlockFormat::Bc1);
    assert_eq!(Dds::parse(&bc3_file).unwrap().format(), BlockFormat::Bc3);
    assert_eq!(BlockFormat::Bc1.bytes_per_block(), 8);
    assert_eq!(BlockFormat::Bc3.bytes_per_block(), 16);
    assert_eq!(BlockFormat::Bc1.four_cc(), *b"DXT1");
    assert_eq!(BlockFormat::Bc3.four_cc(), *b"DXT5");
}

#[test]
fn a_header_shorter_than_128_bytes_errors() {
    let full = DdsBuilder::new(4, 4).zero_filled();
    for len in 0..128 {
        assert!(
            matches!(Dds::parse(&full[..len]), Err(Error::HeaderTruncated { .. })),
            "{len}-byte input"
        );
    }
}

#[test]
fn a_file_without_the_dds_magic_errors() {
    let file = DdsBuilder::new(4, 4).magic(*b"DDS\0").zero_filled();
    assert!(matches!(Dds::parse(&file), Err(Error::DdsNotDds { .. })));
}

#[test]
fn a_header_size_other_than_124_errors_rather_than_reading_the_wrong_layout() {
    for size in [0, 32, 123, 125, u32::MAX] {
        let file = DdsBuilder::new(4, 4).header_size(size).zero_filled();
        assert!(
            matches!(
                Dds::parse(&file),
                Err(Error::DdsBadStructureSize {
                    structure: "header",
                    ..
                })
            ),
            "header size {size}"
        );
    }
}

#[test]
fn a_pixel_format_size_other_than_32_errors() {
    let file = DdsBuilder::new(4, 4).pixel_format_size(24).zero_filled();
    assert!(matches!(
        Dds::parse(&file),
        Err(Error::DdsBadStructureSize {
            structure: "pixel format",
            ..
        })
    ));
}

#[test]
fn an_uncompressed_surface_is_refused_by_name() {
    let file = DdsBuilder::new(4, 4)
        .pixel_format_flags(0x40)
        .bit_count(32)
        .zero_filled();
    match Dds::parse(&file) {
        Err(Error::DdsNotBlockCompressed { flags, bit_count }) => {
            assert_eq!(flags, 0x40);
            assert_eq!(bit_count, 32);
        }
        other => panic!("expected DdsNotBlockCompressed, got {other:?}"),
    }
}

#[test]
fn an_unsupported_four_cc_is_named_rather_than_decoded_as_something_else() {
    for four_cc in [*b"DXT2", *b"DXT3", *b"DXT4", *b"ATI1", *b"ATI2", *b"DX10"] {
        let file = DdsBuilder::new(4, 4)
            .four_cc(four_cc)
            .pixel_format_flags(DDPF_FOURCC)
            .data(&[0u8; 64]);
        match Dds::parse(&file) {
            Err(Error::DdsUnsupportedFourCc { fourcc, name }) => {
                assert_eq!(fourcc, four_cc);
                assert_eq!(name.as_bytes(), &four_cc);
            }
            other => panic!("expected DdsUnsupportedFourCc for {four_cc:?}, got {other:?}"),
        }
    }
}

#[test]
fn a_zero_dimension_errors() {
    for (w, h) in [(0, 4), (4, 0), (0, 0)] {
        let file = DdsBuilder::new(w, h).data(&[0u8; 8]);
        assert!(
            matches!(Dds::parse(&file), Err(Error::ZeroDimension { .. })),
            "{w}x{h}"
        );
    }
}

#[test]
fn a_cube_map_or_volume_is_refused_rather_than_read_as_one_face() {
    let cube = DdsBuilder::new(4, 4).caps2(0x200).zero_filled();
    assert!(matches!(
        Dds::parse(&cube),
        Err(Error::DdsUnsupportedLayout {
            layout: "cube map",
            ..
        })
    ));
    let volume = DdsBuilder::new(4, 4).caps2(0x0020_0000).zero_filled();
    assert!(matches!(
        Dds::parse(&volume),
        Err(Error::DdsUnsupportedLayout {
            layout: "volume",
            ..
        })
    ));
    let deep = DdsBuilder::new(4, 4).depth(6).zero_filled();
    assert!(matches!(
        Dds::parse(&deep),
        Err(Error::DdsUnsupportedLayout {
            layout: "volume",
            ..
        })
    ));
}

#[test]
fn more_mip_levels_than_the_dimensions_allow_errors() {
    let file = DdsBuilder::new(4, 4).mip_count(9).data(&[0u8; 4096]);
    match Dds::parse(&file) {
        Err(Error::DdsMipCountImplausible {
            declared, possible, ..
        }) => {
            assert_eq!(declared, 9);
            assert_eq!(possible, 3);
        }
        other => panic!("expected DdsMipCountImplausible, got {other:?}"),
    }
}

#[test]
fn a_mip_count_implying_more_data_than_is_present_errors() {
    let file = DdsBuilder::new(8, 8).mip_count(4).data(&[0u8; 32]);
    match Dds::parse(&file) {
        Err(Error::DataTruncated {
            offset,
            needed,
            available,
            ..
        }) => {
            assert_eq!(offset, 128 + 32, "the second level starts here");
            assert_eq!(needed, 8);
            assert_eq!(available, 0);
        }
        other => panic!("expected DataTruncated, got {other:?}"),
    }
}

#[test]
fn dimensions_far_larger_than_the_payload_error_before_any_allocation() {
    let file = DdsBuilder::new(65535, 65535).data(&[0u8; 72]);
    match Dds::parse(&file) {
        Err(Error::DataTruncated {
            needed, available, ..
        }) => {
            assert_eq!(needed, 16384 * 16384 * 8);
            assert_eq!(available, 72);
        }
        other => panic!("expected DataTruncated, got {other:?}"),
    }
}

#[test]
fn dimensions_chosen_to_overflow_a_naive_size_computation_error() {
    let file = DdsBuilder::new(u32::MAX, u32::MAX).data(&[0u8; 64]);
    assert!(matches!(
        Dds::parse(&file),
        Err(Error::DataTruncated { .. } | Error::DimensionsOverflow { .. })
    ));

    let file = DdsBuilder::new(1 << 31, 1 << 31).data(&[0u8; 64]);
    assert!(matches!(
        Dds::parse(&file),
        Err(Error::DataTruncated { .. } | Error::DimensionsOverflow { .. })
    ));
}

#[test]
fn asking_for_a_mip_level_that_does_not_exist_errors() {
    let dds = bc1(colour_block(RED_565, BLUE_565, 0), 4, 4);
    match dds.decode_level(1) {
        Err(Error::DdsNoSuchLevel { level, levels }) => {
            assert_eq!(level, 1);
            assert_eq!(levels, 1);
        }
        other => panic!("expected DdsNoSuchLevel, got {other:?}"),
    }
    assert!(dds.level(1).is_none());
}

#[test]
fn every_prefix_of_a_valid_file_errors_rather_than_panicking() {
    let full = DdsBuilder::new(8, 8).mip_count(4).zero_filled();
    for len in 0..full.len() {
        assert!(Dds::parse(&full[..len]).is_err(), "prefix of {len} bytes");
    }
    assert!(Dds::parse(&full).is_ok());
}

#[test]
fn flipping_any_byte_of_a_valid_file_never_panics() {
    let mut blocks = Vec::new();
    blocks.extend_from_slice(&alpha_block(200, 40, 0x0123_4567_89ab));
    blocks.extend_from_slice(&colour_block(RED_565, BLUE_565, 0x1b1b_1b1b));
    let full = DdsBuilder::new(4, 4).four_cc(*b"DXT5").data(&blocks);
    for index in 0..full.len() {
        for bit in 0..8 {
            let mut corrupt = full.clone();
            corrupt[index] ^= 1 << bit;
            if let Ok(dds) = Dds::parse(&corrupt) {
                let _ = dds.decode();
            }
        }
    }
}

#[test]
fn a_block_shorter_than_its_stride_is_ignored_rather_than_read_past() {
    let mut blocks = colour_block(RED_565, BLUE_565, 0).to_vec();
    blocks.extend_from_slice(&[0xff; 3]);
    let file = DdsBuilder::new(4, 4).data(&blocks);
    let dds = Dds::parse(&file).unwrap();
    assert_eq!(dds.decode().unwrap().pixel(0, 0), Some([255, 0, 0, 255]));
}
