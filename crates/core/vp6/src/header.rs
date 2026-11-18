use crate::bits::BitReader;
use crate::bool_decoder::BoolDecoder;
use crate::{Error, Result};

const VERSION_VP60: u8 = 6;
const VERSION_VP62: u8 = 8;
pub(crate) const PROFILE_SIMPLE: u8 = 0;
pub(crate) const PROFILE_ADVANCED: u8 = 3;

pub(crate) struct Header {
    pub(crate) is_intra: bool,
    pub(crate) is_golden: bool,
    pub(crate) quant: usize,
    pub(crate) multistream: bool,
    pub(crate) use_huffman: bool,
    pub(crate) interlaced: bool,
    pub(crate) offset: usize,
    pub(crate) mb_w: usize,
    pub(crate) mb_h: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StreamState {
    pub(crate) version: u8,
    pub(crate) profile: u8,
    pub(crate) interlaced: bool,
    do_pm: bool,
    pub(crate) autosel_pm: bool,
    pub(crate) var_thresh: u16,
    pub(crate) mv_thresh: u8,
    pub(crate) bicubic: bool,
    pub(crate) filter_alpha: usize,
    pub(crate) loop_filter: bool,
}

pub(crate) fn parse<'a>(src: &'a [u8], st: &mut StreamState) -> Result<(Header, BoolDecoder<'a>)> {
    let mut br = BitReader::new(src);
    let is_intra = !br.read_bool()?;
    let quant = usize::try_from(br.read(6)?).unwrap_or(0);
    let mut multistream = br.read_bool()?;
    let (version, profile, interlaced) = if is_intra {
        let version = u8::try_from(br.read(5)?).unwrap_or(0);
        if !(VERSION_VP60..=VERSION_VP62).contains(&version) {
            return Err(Error::UnsupportedVersion(version));
        }
        let profile = u8::try_from(br.read(2)?).unwrap_or(0);
        if profile != PROFILE_SIMPLE && profile != PROFILE_ADVANCED {
            return Err(Error::UnsupportedProfile(profile));
        }
        (version, profile, br.read_bool()?)
    } else if st.version == 0 {
        return Err(Error::MissingReference);
    } else {
        (st.version, st.profile, st.interlaced)
    };
    let mut offset = 0usize;
    if multistream || profile == PROFILE_SIMPLE {
        offset = usize::try_from(br.read(16)?).unwrap_or(0);
        if offset <= if is_intra { 6 } else { 2 } {
            return Err(Error::InvalidHeader("coefficient partition offset"));
        }
        multistream = true;
    }
    let mut bc = BoolDecoder::new(src.get(br.bytes_read()..).unwrap_or_default());

    let mut hdr = Header {
        is_intra,
        is_golden: is_intra,
        quant,
        multistream,
        use_huffman: false,
        interlaced,
        offset,
        mb_w: 0,
        mb_h: 0,
    };
    st.loop_filter = false;
    if is_intra {
        hdr.mb_h = usize::try_from(bc.read_bits(8)).unwrap_or(0);
        hdr.mb_w = usize::try_from(bc.read_bits(8)).unwrap_or(0);
        let _display_rows = bc.read_bits(8);
        let _display_cols = bc.read_bits(8);
        if hdr.mb_w == 0 || hdr.mb_h == 0 {
            return Err(Error::InvalidHeader("zero macroblock dimension"));
        }
        let _scale = bc.read_bits(2);
    } else {
        hdr.is_golden = bc.read_bool();
        if profile == PROFILE_ADVANCED {
            st.loop_filter = bc.read_bool();
            if st.loop_filter && bc.read_bool() {
                return Err(Error::InvalidHeader("loop filter type"));
            }
            if version == VERSION_VP62 {
                st.do_pm = bc.read_bool();
            }
        }
    }

    if profile == PROFILE_ADVANCED && (is_intra || st.do_pm) {
        st.autosel_pm = bc.read_bool();
        if st.autosel_pm {
            st.var_thresh = u16::try_from(bc.read_bits(5)).unwrap_or(0);
            if version != VERSION_VP62 {
                st.var_thresh = st.var_thresh.wrapping_shl(5);
            }
            st.mv_thresh = u8::try_from(bc.read_bits(3)).unwrap_or(0);
        } else {
            st.bicubic = bc.read_bool();
        }
        st.filter_alpha = if version == VERSION_VP62 {
            usize::try_from(bc.read_bits(4)).unwrap_or(0)
        } else {
            16
        };
    }

    hdr.use_huffman = bc.read_bool();

    st.version = version;
    st.profile = profile;
    st.interlaced = interlaced;
    Ok((hdr, bc))
}
