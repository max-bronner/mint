use super::*;

fn synth_apt() -> Vec<u8> {
    let mut apt = vec![0u8; 0x54 + 28];
    write_u32(&mut apt, 0x10, 9);
    write_u32(&mut apt, 0x14, CHARACTER_SIGNATURE);
    write_u32(&mut apt, 0x18, 1);
    write_u32(&mut apt, 0x1c, 0);
    write_u32(&mut apt, 0x24, 2);
    write_u32(&mut apt, 0x28, 0x4c);
    write_u32(&mut apt, 0x2c, 1024);
    write_u32(&mut apt, 0x30, 768);
    write_u32(&mut apt, 0x34, 33);
    write_u32(&mut apt, 0x38, 0);
    write_u32(&mut apt, 0x3c, 0);
    write_u32(&mut apt, 0x40, 0);
    write_u32(&mut apt, 0x44, 0);
    write_u32(&mut apt, 0x4c, 0);
    write_u32(&mut apt, 0x50, 0x54);
    write_u32(&mut apt, 0x54, 1);
    write_u32(&mut apt, 0x58, CHARACTER_SIGNATURE);
    write_f32(&mut apt, 0x5c, -1.0);
    write_f32(&mut apt, 0x60, -2.0);
    write_f32(&mut apt, 0x64, 3.0);
    write_f32(&mut apt, 0x68, 4.0);
    write_u32(&mut apt, 0x6c, 1);
    apt
}

const MOVIE_OFFSET: usize = 0x10;

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn write_f32(bytes: &mut [u8], offset: usize, value: f32) {
    write_u32(bytes, offset, value.to_bits());
}

#[test]
fn parses_the_movie_record() {
    let apt = synth_apt();
    let record = parse_movie(&apt, MOVIE_OFFSET).unwrap();
    assert_eq!(record.frame_count, 1);
    assert_eq!(record.character_count, 2);
    assert_eq!(record.characters_offset, 0x4c);
    assert_eq!(record.screen_width, 1024);
    assert_eq!(record.screen_height, 768);
}

#[test]
fn rejects_the_wrong_type() {
    let mut apt = synth_apt();
    write_u32(&mut apt, MOVIE_OFFSET, 5);
    assert!(matches!(
        parse_movie(&apt, MOVIE_OFFSET),
        Err(Error::NotAMovieRecord { ty: 5, .. })
    ));
}

#[test]
fn rejects_the_wrong_signature() {
    let mut apt = synth_apt();
    write_u32(&mut apt, MOVIE_OFFSET + 4, 0);
    assert!(matches!(
        parse_movie(&apt, MOVIE_OFFSET),
        Err(Error::BadSignature { .. })
    ));
}

#[test]
fn character_dictionary_decodes_placeholder_and_shape() {
    let apt = synth_apt();
    let record = parse_movie(&apt, MOVIE_OFFSET).unwrap();
    let chars = parse_characters(&apt, &record).unwrap();
    assert_eq!(chars.len(), 2);
    assert_eq!(chars[0], Character::Placeholder);
    match &chars[1] {
        Character::Shape {
            bounds,
            geometry_id,
        } => {
            assert_eq!(*geometry_id, 1);
            assert_eq!(bounds.left, -1.0);
            assert_eq!(bounds.bottom, 4.0);
        }
        other => panic!("expected Shape, got {other:?}"),
    }
}

#[test]
fn a_slot_whose_pointer_lands_on_garbage_is_a_placeholder_not_an_error() {
    let mut apt = synth_apt();
    write_u32(&mut apt, 0x4c, 0x60);
    let record = parse_movie(&apt, MOVIE_OFFSET).unwrap();
    let chars = parse_characters(&apt, &record).unwrap();
    assert_eq!(chars[0], Character::Placeholder);
}

#[test]
fn a_character_count_past_the_buffer_errors_instead_of_over_allocating() {
    let mut apt = synth_apt();
    write_u32(&mut apt, MOVIE_OFFSET + 0x14, u32::MAX);
    let record = parse_movie(&apt, MOVIE_OFFSET).unwrap();
    assert!(parse_characters(&apt, &record).is_err());
}

#[test]
fn ru_path_matches_the_measured_convention() {
    assert_eq!(ru_path("background", 4), "background_geometry/4.ru");
}

#[test]
fn shared_atlas_path_matches_the_measured_convention() {
    assert_eq!(
        shared_atlas_path("strategicdetailsregion", 1),
        "art/textures/apt_strategicdetailsregion_1.tga"
    );
}

#[test]
fn apt_movie_load_ties_const_and_apt_together() {
    let apt = synth_apt();
    let mut cst = b"Apt constant file\x1a\0\0".to_vec();
    cst.extend_from_slice(&(MOVIE_OFFSET as u32).to_le_bytes());
    cst.extend_from_slice(&0u32.to_le_bytes());
    cst.extend_from_slice(&(cst.len() as u32 + 4).to_le_bytes());
    let movie = AptMovie::load("Background", &cst, &apt).unwrap();
    assert_eq!(movie.name, "background");
    assert_eq!(movie.characters.len(), 2);
}

#[test]
fn every_prefix_of_a_valid_apt_buffer_parses_without_panicking() {
    let apt = synth_apt();
    for len in 0..=apt.len() {
        let prefix = &apt[..len];
        if let Ok(record) = parse_movie(prefix, MOVIE_OFFSET) {
            let _ = parse_characters(prefix, &record);
            let _ = parse_imports(prefix, &record);
            let _ = parse_exports(prefix, &record);
        }
    }
}

#[test]
fn flipping_any_single_bit_never_panics() {
    let apt = synth_apt();
    for byte_index in 0..apt.len() {
        for bit in 0..8u8 {
            let mut corrupted = apt.clone();
            if let Some(byte) = corrupted.get_mut(byte_index) {
                *byte ^= 1 << bit;
            }
            if let Ok(record) = parse_movie(&corrupted, MOVIE_OFFSET) {
                let _ = parse_characters(&corrupted, &record);
            }
        }
    }
}

fn write_cstr(bytes: &mut [u8], offset: usize, text: &str) {
    bytes[offset..offset + text.len()].copy_from_slice(text.as_bytes());
}

fn synth_text_apt() -> Vec<u8> {
    let mut apt = vec![0u8; 0x300];

    write_u32(&mut apt, 0x00, 3);
    write_u32(&mut apt, 0x04, CHARACTER_SIGNATURE);
    write_u32(&mut apt, 0x08, 0x100);
    write_u32(&mut apt, 0x0c, 3);
    write_u32(&mut apt, 0x10, 0x220);
    write_cstr(&mut apt, 0x100, "Albertus MT");
    write_u32(&mut apt, 0x220, 7);
    write_u32(&mut apt, 0x224, 42);
    write_u32(&mut apt, 0x228, 99);

    write_u32(&mut apt, 0x40, 2);
    write_u32(&mut apt, 0x44, CHARACTER_SIGNATURE);
    write_f32(&mut apt, 0x48, -1.0);
    write_f32(&mut apt, 0x4c, -2.0);
    write_f32(&mut apt, 0x50, 3.0);
    write_f32(&mut apt, 0x54, 4.0);
    write_u32(&mut apt, 0x58, 5);
    write_u32(&mut apt, 0x5c, 1);
    write_u32(&mut apt, 0x60, 0x1122_3344);
    write_f32(&mut apt, 0x64, 18.5);
    write_u32(&mut apt, 0x68, 0);
    write_u32(&mut apt, 0x6c, 1);
    write_u32(&mut apt, 0x70, 0);
    write_u32(&mut apt, 0x74, 0x260);
    write_u32(&mut apt, 0x78, 0x270);
    write_cstr(&mut apt, 0x260, "Hello");
    write_cstr(&mut apt, 0x270, "_parent.buttonName");

    write_u32(&mut apt, 0x90, 10);
    write_u32(&mut apt, 0x94, CHARACTER_SIGNATURE);
    write_f32(&mut apt, 0x98, -5.0);
    write_f32(&mut apt, 0x9c, -6.0);
    write_f32(&mut apt, 0xa0, 7.0);
    write_f32(&mut apt, 0xa4, 8.0);
    write_f32(&mut apt, 0xa8, 1.0);
    write_f32(&mut apt, 0xac, 0.0);
    write_f32(&mut apt, 0xb0, 0.0);
    write_f32(&mut apt, 0xb4, 1.0);
    write_f32(&mut apt, 0xb8, 10.0);
    write_f32(&mut apt, 0xbc, 20.0);
    write_u32(&mut apt, 0xc0, 1);
    write_u32(&mut apt, 0xc4, 0x2a0);

    write_u32(&mut apt, 0x2a0, 9);
    write_f32(&mut apt, 0x2a4, 0.5);
    write_f32(&mut apt, 0x2a8, 1.0);
    write_f32(&mut apt, 0x2ac, 0.25);
    write_f32(&mut apt, 0x2b0, 0.75);
    write_f32(&mut apt, 0x2c4, 2.0);
    write_f32(&mut apt, 0x2c8, 3.0);
    write_f32(&mut apt, 0x2cc, 14.0);
    write_u32(&mut apt, 0x2d0, 1);
    write_u32(&mut apt, 0x2d4, 0x2e0);
    write_u32(&mut apt, 0x2e0, 65);
    write_u32(&mut apt, 0x2e4, 7);

    apt
}

#[test]
fn font_character_reads_its_name_and_glyph_table() {
    let apt = synth_text_apt();
    match decode_character(&apt, 0x00, 3).unwrap() {
        Character::Font { name, glyphs } => {
            assert_eq!(name.as_deref(), Some("Albertus MT"));
            assert_eq!(glyphs, vec![7, 42, 99]);
        }
        other => panic!("expected Font, got {other:?}"),
    }
}

#[test]
fn edittext_character_reads_text_and_variable() {
    let apt = synth_text_apt();
    match decode_character(&apt, 0x40, 2).unwrap() {
        Character::EditText {
            text,
            variable,
            font,
            color,
            ..
        } => {
            assert_eq!(text.as_deref(), Some("Hello"));
            assert_eq!(variable.as_deref(), Some("_parent.buttonName"));
            assert_eq!(font, 5);
            assert_eq!(color, 0x1122_3344);
        }
        other => panic!("expected EditText, got {other:?}"),
    }
}

#[test]
fn text_character_reads_its_record_and_glyph_run() {
    let apt = synth_text_apt();
    match decode_character(&apt, 0x90, 10).unwrap() {
        Character::Text {
            translate, records, ..
        } => {
            assert_eq!(translate, Vector2 { x: 10.0, y: 20.0 });
            assert_eq!(records.len(), 1);
            let record = &records[0];
            assert_eq!(record.font, 9);
            assert_eq!(record.color, [1.0, 0.5, 0.75, 0.25]);
            assert_eq!(record.offset, Vector2 { x: 2.0, y: 3.0 });
            assert_eq!(record.text_height, 14.0);
            assert_eq!(
                record.glyphs,
                vec![Glyph {
                    index: 65,
                    advance: 7
                }]
            );
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn every_prefix_of_the_text_fixture_decodes_without_panicking() {
    let apt = synth_text_apt();
    for len in 0..=apt.len() {
        let prefix = &apt[..len];
        let _ = decode_character(prefix, 0x00, 3);
        let _ = decode_character(prefix, 0x40, 2);
        let _ = decode_character(prefix, 0x90, 10);
    }
}

#[test]
fn flipping_any_bit_in_the_text_fixture_never_panics() {
    let apt = synth_text_apt();
    for byte_index in 0..apt.len() {
        for bit in 0..8u8 {
            let mut corrupted = apt.clone();
            if let Some(byte) = corrupted.get_mut(byte_index) {
                *byte ^= 1 << bit;
            }
            let _ = decode_character(&corrupted, 0x00, 3);
            let _ = decode_character(&corrupted, 0x40, 2);
            let _ = decode_character(&corrupted, 0x90, 10);
        }
    }
}

#[test]
fn a_hostile_count_errors_instead_of_over_allocating() {
    let mut apt = synth_text_apt();
    write_u32(&mut apt, 0x0c, u32::MAX);
    assert!(decode_character(&apt, 0x00, 3).is_err());
    let mut apt2 = synth_text_apt();
    write_u32(&mut apt2, 0xc0, u32::MAX);
    assert!(decode_character(&apt2, 0x90, 10).is_err());
}
