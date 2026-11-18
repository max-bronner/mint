use mint_sage_audio::ClientRng;

pub(crate) const FOREGROUND: &str = "LoadScreenForeground";
pub(crate) const SAVE_FILE_LOAD_MUSIC: &str = "SaveFileLoadMusic";

const RANDOM_STILLS: u32 = 21;
const STILL_TOP: f32 = 126.0 / 768.0;
const STILL_PIXEL_ROWS: f32 = 768.0;
const RING: [f32; 4] = [0.468_261_7, 0.875_651_1, 0.530_761_7, 0.958_984_4];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StillMusic {
    Track(Option<String>),
    SaveFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Still {
    pub(crate) image: String,
    pub(crate) music: StillMusic,
}

impl Still {
    pub(crate) fn choose(image: Option<&str>, track: Option<&str>, rng: &mut ClientRng) -> Self {
        match image.filter(|name| !name.is_empty()) {
            Some(image) => Self {
                image: image.to_owned(),
                music: StillMusic::Track(track.map(str::to_owned)),
            },
            None => Self {
                image: random_film(rng),
                music: StillMusic::SaveFile,
            },
        }
    }
}

fn random_film(rng: &mut ClientRng) -> String {
    let pick = rng
        .next_index(RANDOM_STILLS as usize)
        .and_then(|index| u32::try_from(index).ok())
        .unwrap_or(0);
    format!("StillImage_Film{:02}", pick.saturating_add(1))
}

pub(crate) fn still_rect(window: (f32, f32), image_rows: u32) -> [f32; 4] {
    let (w, h) = window;
    let top = STILL_TOP * h;
    let rows = if image_rows == 0 {
        STILL_PIXEL_ROWS * (1.0 - STILL_TOP)
    } else {
        image_rows as f32
    };
    [0.0, top, w, top + rows / STILL_PIXEL_ROWS * h]
}

pub(crate) fn ring_rect(window: (f32, f32)) -> [f32; 4] {
    let (w, h) = window;
    [RING[0] * w, RING[1] * h, RING[2] * w, RING[3] * h]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mission_image_is_kept_and_a_missing_one_gets_a_film_still_in_range() {
        let mut rng = ClientRng::seeded(7);
        let own = Still::choose(Some("MissionStill"), Some("Track"), &mut rng);
        assert_eq!(own.image, "MissionStill");
        assert_eq!(own.music, StillMusic::Track(Some("Track".into())));
        let mut seen = [false; 22];
        for _ in 0..2000 {
            let random = Still::choose(None, Some("Track"), &mut rng);
            assert_eq!(random.music, StillMusic::SaveFile);
            let number: usize = random.image["StillImage_Film".len()..].parse().unwrap();
            seen[number] = true;
        }
        assert!(!seen[0] && seen[1..].iter().all(|hit| *hit));
    }

    #[test]
    fn the_still_starts_at_126_of_768_and_is_as_tall_as_its_rows() {
        let rect = still_rect((1536.0, 1152.0), 384);
        assert_eq!(rect, [0.0, 189.0, 1536.0, 189.0 + 576.0]);
    }
}
