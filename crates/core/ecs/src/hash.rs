const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

const SPLITMIX_A: u64 = 0xbf58_476d_1ce4_e5b9;
const SPLITMIX_B: u64 = 0x94d0_49bb_1331_11eb;

const CANONICAL_NAN_F32: u32 = 0x7fc0_0000;
const CANONICAL_NAN_F64: u64 = 0x7ff8_0000_0000_0000;

const WORD: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateHasher {
    state: u64,
    pending: u64,
    filled: u8,
    legacy: bool,
}

impl StateHasher {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: OFFSET_BASIS,
            pending: 0,
            filled: 0,
            legacy: false,
        }
    }

    #[must_use]
    pub const fn legacy() -> Self {
        Self {
            legacy: true,
            ..Self::new()
        }
    }

    #[must_use]
    pub const fn is_legacy(&self) -> bool {
        self.legacy
    }

    const fn absorb(&mut self, word: u64) {
        self.state = (self.state ^ word).wrapping_mul(PRIME);
    }

    fn push(&mut self, value: u64, bytes: u8) {
        if self.legacy {
            self.absorb(if bytes >= WORD {
                value
            } else {
                value | u64::from(bytes) << 56
            });
            return;
        }
        let filled = self.filled.wrapping_add(bytes);
        if filled > WORD {
            self.absorb(self.pending);
            self.pending = value;
            self.filled = bytes;
        } else {
            self.pending |= value.wrapping_shl(u32::from(self.filled).wrapping_mul(8));
            self.filled = filled;
        }
    }

    pub fn write_u8(&mut self, value: u8) {
        self.push(u64::from(value), 1);
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        if !self.legacy {
            self.write_usize(bytes.len());
        }
        let mut chunks = bytes.chunks_exact(8);
        for chunk in chunks.by_ref() {
            let word = <[u8; 8]>::try_from(chunk).map_or(0, u64::from_le_bytes);
            self.push(word, WORD);
        }
        let tail = chunks.remainder();
        if let Ok(len @ 1..) = u8::try_from(tail.len()) {
            let mut buf = [0u8; 8];
            for (slot, byte) in buf.iter_mut().zip(tail) {
                *slot = *byte;
            }
            self.push(u64::from_le_bytes(buf), len);
        }
    }

    pub fn write_u16(&mut self, value: u16) {
        self.push(u64::from(value), 2);
    }

    pub fn write_u32(&mut self, value: u32) {
        self.push(u64::from(value), 4);
    }

    pub fn write_u64(&mut self, value: u64) {
        self.push(value, WORD);
    }

    pub fn write_i32(&mut self, value: i32) {
        self.write_u32(value.cast_unsigned());
    }

    pub fn write_i64(&mut self, value: i64) {
        self.write_u64(value.cast_unsigned());
    }

    pub fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    pub fn write_usize(&mut self, value: usize) {
        self.write_u64(u64::try_from(value).unwrap_or(u64::MAX));
    }

    pub fn write_f32(&mut self, value: f32) {
        let bits = if value.is_nan() {
            CANONICAL_NAN_F32
        } else {
            value.to_bits()
        };
        self.write_u32(bits);
    }

    pub fn write_f64(&mut self, value: f64) {
        let bits = if value.is_nan() {
            CANONICAL_NAN_F64
        } else {
            value.to_bits()
        };
        self.write_u64(bits);
    }

    #[must_use]
    pub const fn finish(&self) -> u64 {
        let mut z = self.state;
        if self.filled >= WORD {
            z = (z ^ self.pending).wrapping_mul(PRIME);
        } else if self.filled > 0 {
            z = (z ^ (self.pending | (self.filled as u64) << 56)).wrapping_mul(PRIME);
        }
        z = (z ^ (z >> 30)).wrapping_mul(SPLITMIX_A);
        z = (z ^ (z >> 27)).wrapping_mul(SPLITMIX_B);
        z ^ (z >> 31)
    }
}

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn hash_of(bytes: &[u8]) -> u64 {
        let mut hasher = StateHasher::new();
        hasher.write_bytes(bytes);
        hasher.finish()
    }

    fn legacy_hash_of(bytes: &[u8]) -> u64 {
        let mut hasher = StateHasher::legacy();
        hasher.write_bytes(bytes);
        hasher.finish()
    }

    fn lcg(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *state >> 11
    }

    fn pseudo_random_bytes(len: usize, seed: u64) -> Vec<u8> {
        let mut state = seed | 1;
        (0..len).map(|_| (lcg(&mut state) >> 22) as u8).collect()
    }

    fn every_writer(hasher: &mut StateHasher) {
        hasher.write_u8(1);
        hasher.write_u32(2);
        hasher.write_u64(3);
        hasher.write_bytes(b"abcdefghij");
        hasher.write_u16(0xbeef);
        hasher.write_bool(true);
    }

    #[test]
    fn both_definitions_are_recorded_constants() {
        let mut legacy = StateHasher::legacy();
        every_writer(&mut legacy);
        assert_eq!(legacy.finish(), 0xc132_032a_5805_668a);
        let mut packed = StateHasher::new();
        every_writer(&mut packed);
        assert_eq!(packed.finish(), 0xa4eb_37fe_c698_707e);
    }

    #[test]
    fn a_single_flipped_bit_changes_the_hash_at_every_offset() {
        for hash in [hash_of, legacy_hash_of] {
            for len in [1usize, 7, 8, 9, 15, 16, 17, 64, 129] {
                let original = pseudo_random_bytes(len, 0x9e37_79b9_7f4a_7c15);
                let baseline = hash(&original);
                for offset in 0..len {
                    for bit in 0..8u32 {
                        let mut mutated = original.clone();
                        mutated[offset] ^= 1u8 << bit;
                        assert_ne!(
                            hash(&mutated),
                            baseline,
                            "len {len}, byte {offset}, bit {bit} did not change the hash"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_single_changed_value_in_a_packed_stream_always_changes_the_hash() {
        let mut rng = 0x5eed_u64;
        let writes: Vec<(u8, u64)> = (0..200)
            .map(|_| {
                let width = [1u8, 2, 4, 8][(lcg(&mut rng) % 4) as usize];
                let mask = u64::MAX >> (64 - u32::from(width) * 8);
                (width, lcg(&mut rng) & mask)
            })
            .collect();
        let hash = |writes: &[(u8, u64)]| {
            let mut hasher = StateHasher::new();
            for &(width, value) in writes {
                match width {
                    1 => hasher.write_u8(value as u8),
                    2 => hasher.write_u16(value as u16),
                    4 => hasher.write_u32(value as u32),
                    _ => hasher.write_u64(value),
                }
            }
            hasher.finish()
        };
        let baseline = hash(&writes);
        for index in 0..writes.len() {
            let width = writes[index].0;
            for bit in 0..u32::from(width) * 8 {
                let mut mutated = writes.clone();
                mutated[index].1 ^= 1 << bit;
                assert_ne!(hash(&mutated), baseline, "write {index}, bit {bit}");
            }
        }
    }

    #[test]
    fn truncation_changes_the_hash() {
        for hash in [hash_of, legacy_hash_of] {
            let full = pseudo_random_bytes(200, 0xdead_beef_cafe_f00d);
            let baseline = hash(&full);
            let mut seen = BTreeSet::new();
            for len in 0..full.len() {
                let prefix = hash(&full[..len]);
                assert_ne!(
                    prefix, baseline,
                    "prefix of {len} bytes hashed as the whole"
                );
                assert!(seen.insert(prefix), "two prefixes collided at {len} bytes");
            }
            let zeroes = [0u8; 24];
            let mut zero_hashes = BTreeSet::new();
            for len in 0..zeroes.len() {
                assert!(
                    zero_hashes.insert(hash(&zeroes[..len])),
                    "{len} zero bytes collided with a shorter run"
                );
            }
        }
    }

    #[test]
    fn legacy_wide_writers_agree_with_write_bytes_of_the_same_le_bytes() {
        let one = |write: &dyn Fn(&mut StateHasher)| {
            let mut hasher = StateHasher::legacy();
            write(&mut hasher);
            hasher.finish()
        };
        for seed in 0..64u64 {
            let value = seed
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                .wrapping_add(0x0123_4567_89ab_cdef);
            assert_eq!(
                one(&|h| h.write_u64(value)),
                legacy_hash_of(&value.to_le_bytes())
            );
            let narrow = value as u32;
            assert_eq!(
                one(&|h| h.write_u32(narrow)),
                legacy_hash_of(&narrow.to_le_bytes())
            );
            let signed = value as i64;
            assert_eq!(
                one(&|h| h.write_i64(signed)),
                legacy_hash_of(&signed.to_le_bytes())
            );
            let half = value as u16;
            assert_eq!(
                one(&|h| h.write_u16(half)),
                legacy_hash_of(&half.to_le_bytes())
            );
            let byte = value as u8;
            assert_eq!(one(&|h| h.write_u8(byte)), legacy_hash_of(&[byte]));
            let float = f32::from_bits(narrow);
            if !float.is_nan() {
                assert_eq!(
                    one(&|h| h.write_f32(float)),
                    legacy_hash_of(&narrow.to_le_bytes())
                );
            }
        }
    }

    #[test]
    fn legacy_whole_word_splits_do_not_change_the_hash() {
        let data = pseudo_random_bytes(128, 7);
        let baseline = legacy_hash_of(&data);
        for split in (0..=data.len()).step_by(8) {
            let mut hasher = StateHasher::legacy();
            hasher.write_bytes(&data[..split]);
            hasher.write_bytes(&data[split..]);
            assert_eq!(hasher.finish(), baseline, "split at {split} disagreed");
        }
    }

    #[test]
    fn packed_values_fill_words_little_endian_without_straddling() {
        let mut pair = StateHasher::new();
        pair.write_u32(0x0403_0201);
        pair.write_u32(0x0807_0605);
        let mut wide = StateHasher::new();
        wide.write_u64(0x0807_0605_0403_0201);
        assert_eq!(pair.finish(), wide.finish());

        let mut closed = StateHasher::new();
        closed.write_u8(1);
        closed.write_u64(9);
        let mut whole = StateHasher::new();
        whole.write_u64(1);
        whole.write_u64(9);
        assert_eq!(closed.finish(), whole.finish());
    }

    #[test]
    fn packed_slices_are_delimited() {
        let mut first = StateHasher::new();
        first.write_bytes(b"ab");
        first.write_bytes(b"c");
        let mut second = StateHasher::new();
        second.write_bytes(b"a");
        second.write_bytes(b"bc");
        assert_ne!(first.finish(), second.finish());
    }

    #[test]
    fn every_nan_payload_hashes_the_same() {
        for fresh in [StateHasher::new, StateHasher::legacy] {
            let mut a = fresh();
            a.write_f32(f32::NAN);
            let mut b = fresh();
            b.write_f32(f32::from_bits(0xffc0_dead));
            assert_eq!(a.finish(), b.finish());

            let mut c = fresh();
            c.write_f64(f64::NAN);
            let mut d = fresh();
            d.write_f64(f64::from_bits(0xfff8_0000_dead_beef));
            assert_eq!(c.finish(), d.finish());
        }
    }

    #[test]
    fn signed_zeroes_are_distinguished() {
        for fresh in [StateHasher::new, StateHasher::legacy] {
            let mut positive = fresh();
            positive.write_f32(0.0);
            let mut negative = fresh();
            negative.write_f32(-0.0);
            assert_ne!(positive.finish(), negative.finish());
        }
    }

    #[test]
    fn order_of_absorption_matters() {
        for fresh in [StateHasher::new, StateHasher::legacy] {
            let mut forward = fresh();
            forward.write_u8(1);
            forward.write_u8(2);
            let mut backward = fresh();
            backward.write_u8(2);
            backward.write_u8(1);
            assert_ne!(forward.finish(), backward.finish());
        }
    }

    #[test]
    fn the_finaliser_mixes_the_raw_state() {
        let mut hasher = StateHasher::new();
        hasher.write_u64(1);
        assert_ne!(hasher.finish(), hasher.state);

        let mut other = StateHasher::new();
        other.write_u64(3);
        assert_ne!(
            (hasher.finish() ^ other.finish()) & 0xffff,
            0,
            "finaliser left the low 16 bits unmixed"
        );
    }
}
