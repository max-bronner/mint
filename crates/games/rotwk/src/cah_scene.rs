use mint_sage_scene::camera_transform::{bfme_camera, BfmeView};
use mint_sim::Entity;

use crate::cah_examine::{examine_choice, examine_frames, ClientRandom};
use crate::cah_screen::{clock_seed, Preview};
use crate::cah_view::placement;
use crate::game::Game;
use crate::rules::{CahPreview, ExamineRaise};

const CHEER: &str = "CREATE_A_HERO_SELECTED_CHEER";
const SWAP: &str = "USER_75";
const MAX_LOCATIONS: usize = 1024;

pub(crate) fn location_index(name: &str) -> Option<usize> {
    let (_, rest) = name.split_once('_')?;
    let rest = rest.trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    Some(digits.parse().unwrap_or(0))
}

pub(crate) fn build_locations(
    poses: &[(String, Entity, [f32; 2], f32)],
) -> Vec<Option<(Entity, f32)>> {
    let mut locations: Vec<Option<(Entity, f32)>> = Vec::new();
    for (name, entity, _, facing) in poses {
        let Some(index) = location_index(name).filter(|index| *index < MAX_LOCATIONS) else {
            continue;
        };
        if locations.len() <= index {
            locations.resize(index + 1, None);
        }
        if let Some(slot) = locations.get_mut(index) {
            *slot = Some((*entity, *facing));
        }
    }
    locations
}

fn is_swap(same_object: bool, changed: &[String]) -> bool {
    !same_object && !changed.is_empty()
}

pub(crate) fn changed_upgrades(held: Option<&[String]>, wanted: &[String]) -> Vec<String> {
    match held {
        Some(held) => wanted
            .iter()
            .filter(|name| !held.contains(name))
            .cloned()
            .collect(),
        None => wanted.to_vec(),
    }
}

const FULL_SCREEN_HFOV: f32 = 0.872_664_6;

fn rect_hfov(width_fraction: f32) -> f32 {
    FULL_SCREEN_HFOV * width_fraction.clamp(0.0, 1.0)
}

#[derive(Default)]
pub(crate) struct CahScene {
    locations: Option<Vec<Option<(Entity, f32)>>>,
    dressed: Option<(Entity, usize, usize, Vec<String>)>,
    sent: Option<CahPreview>,
    swap_ticks: u8,
    cheer: bool,
    reported: bool,
    rect: Option<(f32, f32)>,
    skip_examine: bool,
    random: Option<ClientRandom>,
}

impl CahScene {
    pub(crate) fn set_rect(&mut self, aspect: f32, width_fraction: f32) {
        self.rect = Some((aspect, width_fraction));
    }

    pub(crate) fn frame(&mut self, game: &mut Game, preview: &Preview) {
        let locations = self
            .locations
            .get_or_insert_with(|| build_locations(&game.scene_poses()));
        let Some((entity, map_angle)) = locations.get(preview.set.map_location).copied().flatten()
        else {
            if !self.reported {
                self.reported = true;
                println!(
                    "cah scene: no map object is named for location {} ({} location(s) found)",
                    preview.set.map_location,
                    locations.iter().flatten().count()
                );
            }
            return;
        };
        let Some(at) = game.scene_position(entity) else {
            return;
        };
        let placed = placement(
            &preview.set,
            map_angle,
            at,
            &preview.view,
            preview.on_powers,
        );
        let (max_height, pitch, window_aspect) = game.scene_lens();
        let aspect = self.rect.map_or(window_aspect, |(aspect, _)| aspect);
        let mut view = placed.view(BfmeView::default_offset(max_height, pitch));
        if let Some((_, fraction)) = self.rect {
            view.hfov = rect_hfov(fraction);
        }
        let camera = game.camera_mut();
        let base = camera.camera();
        camera.set_scripted(Some(bfme_camera(&view, aspect, base.near, base.far)));

        let same = self
            .dressed
            .as_ref()
            .filter(|(e, c, s, _)| *e == entity && *c == preview.class && *s == preview.subclass)
            .map(|(_, _, _, held)| held.as_slice());
        let upgrades = changed_upgrades(same, &preview.upgrades);
        let swapped = is_swap(same.is_some(), &upgrades);
        let redressed = !upgrades.is_empty();
        if redressed {
            self.dressed = Some((
                entity,
                preview.class,
                preview.subclass,
                preview.upgrades.clone(),
            ));
            self.swap_ticks = u8::from(swapped);
            println!(
                "cah scene: dress {entity:?} class {} sub {} +{}",
                preview.class,
                preview.subclass,
                upgrades.join(" ")
            );
        }
        self.cheer = preview.cheer;
        let mut command = CahPreview {
            target: entity,
            facing: preview.view.facing(map_angle),
            upgrades,
            conditions: self.conditions(swapped),
            examine: None,
        };
        let examine = self.examine(game, entity, preview);
        if self.sent.as_ref() != Some(&command) || examine.is_some() {
            self.sent = Some(command.clone());
            command.examine = examine;
            game.scene_submit(command);
        }
    }

    fn examine(&mut self, game: &Game, entity: Entity, preview: &Preview) -> Option<ExamineRaise> {
        let (applies, params) = preview.examine.as_ref()?;
        let mut raised = None;
        for apply in applies {
            let playing = game
                .scene_clip_names(entity)
                .iter()
                .any(|clip| params.is_special(clip));
            let random = self
                .random
                .get_or_insert_with(|| ClientRandom::new(clock_seed()));
            let Some(condition) = examine_choice(
                apply,
                &mut self.skip_examine,
                playing,
                params.chance,
                || random.percent(),
            ) else {
                continue;
            };
            let field = if condition.ends_with("SELF") {
                &params.own
            } else {
                &params.weapon
            };
            let name = format!("{}{field}", mint_sage_ini::anim_state::MODEL_TOKEN);
            if let Some(frames) = game.scene_clip_frames(entity, &name) {
                let held = examine_frames(frames, params.tweak);
                println!("cah scene: examine {condition} for {held} retail frame(s) ({frames} clip frames)");
                raised = Some(ExamineRaise {
                    condition,
                    frames: held,
                });
            }
        }
        raised
    }

    pub(crate) fn after_step(&mut self, game: &mut Game) {
        if self.swap_ticks == 0 {
            return;
        }
        self.swap_ticks -= 1;
        if self.swap_ticks > 0 {
            return;
        }
        let Some(mut command) = self.sent.clone() else {
            return;
        };
        command.upgrades.clear();
        command.conditions = self.conditions(false);
        self.sent = Some(command.clone());
        game.scene_submit(command);
    }

    fn conditions(&self, swapping: bool) -> String {
        let mut tokens = Vec::new();
        if self.cheer {
            tokens.push(CHEER);
        }
        if swapping {
            tokens.push(SWAP);
        }
        tokens.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_drawn_into_part_of_the_window_keeps_the_full_screen_scale() {
        assert!((rect_hfov(1.0) - FULL_SCREEN_HFOV).abs() < 1e-6);
        assert!((rect_hfov(0.5) - FULL_SCREEN_HFOV / 2.0).abs() < 1e-6);
        assert!((rect_hfov(2.0) - FULL_SCREEN_HFOV).abs() < 1e-6);
    }

    #[test]
    fn the_index_is_the_number_after_the_first_underscore_and_later_names_win() {
        assert_eq!(location_index("Spot_3"), Some(3));
        assert_eq!(location_index("Spot_x"), Some(0));
        assert_eq!(location_index("Spot"), None);
        let a = Entity::from_parts(1, 0);
        let b = Entity::from_parts(2, 0);
        let poses = vec![
            ("P_2".to_owned(), a, [0.0; 2], 0.5),
            ("Q_2".to_owned(), b, [0.0; 2], 1.5),
            ("Plain".to_owned(), a, [0.0; 2], 0.0),
        ];
        let locations = build_locations(&poses);
        assert_eq!(locations.len(), 3);
        assert_eq!(locations[2], Some((b, 1.5)));
        assert_eq!(locations[0], None);
    }

    #[test]
    fn only_a_new_object_class_or_subclass_swaps() {
        let upgrades = vec!["A".to_owned()];
        assert!(is_swap(false, &upgrades));
        assert!(!is_swap(true, &upgrades));
    }

    #[test]
    fn a_re_dress_gives_only_the_changed_upgrades_to_the_same_object() {
        let held = vec!["A".to_owned(), "B".to_owned()];
        let wanted = vec!["A".to_owned(), "C".to_owned()];
        assert_eq!(changed_upgrades(Some(&held), &wanted), vec!["C".to_owned()]);
        assert_eq!(changed_upgrades(None, &wanted), wanted);
    }
}
