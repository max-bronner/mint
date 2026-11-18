use std::collections::{BTreeMap, VecDeque};

use mint_render::{Renderer, Vec3};
use mint_sage_scene::{UnitPose, UnitScene};
use mint_vfs::Vfs;

use super::NO_HOUSE_COLOUR;
use crate::render_state::ProjectileView;
use crate::rules::RotwkRules;
use crate::scene::MatchScene;
use crate::visuals::MAX_STREAK_POINTS;

pub(crate) fn load_projectile_art(
    renderer: &mut Renderer,
    vfs: &Vfs,
    scene: &MatchScene,
    units: &mut UnitScene,
    names: &[String],
) -> ProjectileArt {
    let mut art = ProjectileArt::default();
    let Some(index) = scene.object_index.as_ref() else {
        println!("projectiles: no object index - every shot draws the placeholder cube");
        art.no_index = true;
        return art;
    };
    let compressed = renderer.supports_block_compression();
    let weather = if scene.snowy {
        mint_sage_ini::NuggetWeather::Snowy
    } else {
        mint_sage_ini::NuggetWeather::Normal
    };
    let mut nothing = 0usize;
    let mut textures: BTreeMap<String, Option<mint_render::TextureHandle>> = BTreeMap::new();
    for name in names {
        if let Some(slot) = units.template_for(renderer, vfs, &scene.resolver, index, name) {
            art.models.insert(name.clone(), slot);
        }
        let streaks: Vec<StreakArt> = index
            .streak_draws(name)
            .iter()
            .map(|streak| {
                let file = streak
                    .weather_textures
                    .iter()
                    .find(|(when, _)| *when == weather)
                    .map(|(_, file)| file.as_str())
                    .or(streak.texture.as_deref());
                let texture = match file {
                    Some(file) => *textures.entry(file.to_owned()).or_insert_with(|| {
                        match mint_sage_model::load_texture(vfs, &scene.resolver, file, compressed)
                        {
                            Ok(loaded) => match renderer.upload_texture(&loaded.data) {
                                Ok(handle) => Some(handle),
                                Err(error) => {
                                    println!("projectiles: {file} would not upload: {error}");
                                    None
                                }
                            },
                            Err(error) => {
                                println!("projectiles: {file} would not load: {error}");
                                None
                            }
                        }
                    }),
                    None => None,
                };
                let rgb = streak
                    .color
                    .map_or([1.0, 1.0, 1.0], mint_sage_ini::Rgb8::to_unit);
                StreakArt {
                    texture,
                    length: streak
                        .length
                        .filter(|l| l.is_finite() && *l > 0.0)
                        .unwrap_or(50.0),
                    width: streak
                        .width
                        .filter(|w| w.is_finite() && *w > 0.0)
                        .unwrap_or(0.5),
                    segments: streak.segments.map_or(5, |segments| {
                        usize::try_from(segments)
                            .unwrap_or(usize::MAX)
                            .clamp(1, MAX_STREAK_POINTS - 1)
                    }),
                    tint: [rgb[0], rgb[1], rgb[2], 1.0],
                    additive: streak.additive.unwrap_or(true),
                }
            })
            .collect();
        let drawn = art.models.contains_key(name) || !streaks.is_empty();
        if !streaks.is_empty() {
            art.streaks.insert(name.clone(), streaks);
        }
        if drawn {
            continue;
        }
        if let mint_sage_ini::Template::NoModel(reason) = index.lookup(name) {
            nothing = nothing.saturating_add(1);
            crate::diag::diag!("projectile: {name} draws nothing by its own template ({reason:?})");
            continue;
        }
        art.cubes.insert(name.clone());
    }
    println!(
        "projectiles: {} of {} template(s) draw a model, {} draw W3DStreakDraw ribbon(s), \
         {nothing} draw nothing by their own template, {} draw the placeholder cube{}",
        art.models.len(),
        names.len(),
        art.streaks.len(),
        art.cubes.len(),
        if art.cubes.is_empty() {
            String::new()
        } else {
            let cubes: Vec<&str> = art.cubes.iter().map(String::as_str).collect();
            format!(" - {}", cubes.join(", "))
        }
    );
    art
}

#[derive(Debug, Default)]
pub(crate) struct ProjectileArt {
    models: BTreeMap<String, usize>,
    streaks: BTreeMap<String, Vec<StreakArt>>,
    cubes: std::collections::BTreeSet<String>,
    no_index: bool,
    trails: BTreeMap<mint_sim::Entity, VecDeque<Vec3>>,
}

impl ProjectileArt {
    fn draws_cube(&self, name: Option<&str>) -> bool {
        name.is_some_and(|name| self.no_index || self.cubes.contains(name))
    }

    pub(super) fn sort(
        &mut self,
        rules: &RotwkRules,
        projectiles: &[ProjectileView],
    ) -> (
        Vec<UnitPose>,
        Vec<crate::visuals::StreakView>,
        Vec<ProjectileView>,
    ) {
        let mut shot_poses = Vec::new();
        let mut streaks: Vec<crate::visuals::StreakView> = Vec::new();
        let mut cubes: Vec<ProjectileView> = Vec::new();
        let mut streaking: Vec<mint_sim::Entity> = Vec::new();
        for shot in projectiles {
            let name = rules.projectile_template(shot.source, shot.nugget);
            let model = name.and_then(|name| self.models.get(name)).copied();
            if let Some(template) = model {
                shot_poses.push(projectile_pose(shot, template));
            }
            let ribbons = name.and_then(|name| self.streaks.get(name));
            if let Some(ribbons) = ribbons {
                let reach = ribbons
                    .iter()
                    .fold(0.0f32, |reach, streak| reach.max(streak.length));
                let trail = self.trails.entry(shot.entity).or_default();
                record(trail, Vec3::new(shot.x, shot.y, shot.z), reach);
                streaking.push(shot.entity);
                for streak in ribbons {
                    let (points, count) = along_trail(trail, streak.length, streak.segments);
                    streaks.push(crate::visuals::StreakView {
                        points,
                        count,
                        width: streak.width,
                        tint: streak.tint,
                        texture: streak.texture,
                        additive: streak.additive,
                    });
                }
            }
            if model.is_none() && ribbons.is_none() && self.draws_cube(name) {
                cubes.push(*shot);
            }
        }
        streaking.sort_unstable();
        self.trails
            .retain(|entity, _| streaking.binary_search(entity).is_ok());
        (shot_poses, streaks, cubes)
    }
}

#[derive(Debug, Clone, Copy)]
struct StreakArt {
    texture: Option<mint_render::TextureHandle>,
    length: f32,
    width: f32,
    segments: usize,
    tint: [f32; 4],
    additive: bool,
}

const MAX_TRAIL_POINTS: usize = 256;

fn record(trail: &mut VecDeque<Vec3>, head: Vec3, reach: f32) {
    if trail
        .back()
        .is_none_or(|last| (head - *last).length() > MIN_TRAIL_STEP)
    {
        trail.push_back(head);
    }
    while trail.len() > MAX_TRAIL_POINTS {
        trail.pop_front();
    }
    let mut total = path_length(trail);
    while trail.len() > 2 {
        let (Some(oldest), Some(next)) = (trail.front(), trail.get(1)) else {
            break;
        };
        let first = (*next - *oldest).length();
        if total - first < reach {
            break;
        }
        total -= first;
        trail.pop_front();
    }
}

const MIN_TRAIL_STEP: f32 = 1.0e-3;

fn path_length(trail: &VecDeque<Vec3>) -> f32 {
    trail
        .iter()
        .zip(trail.iter().skip(1))
        .map(|(a, b)| (*b - *a).length())
        .sum()
}

fn along_trail(
    trail: &VecDeque<Vec3>,
    length: f32,
    segments: usize,
) -> ([Vec3; MAX_STREAK_POINTS], usize) {
    let mut points = [Vec3::ZERO; MAX_STREAK_POINTS];
    let mut count = 0usize;
    let Some(&head) = trail.back() else {
        return (points, 0);
    };
    let segments = segments.clamp(1, MAX_STREAK_POINTS - 1);
    #[allow(clippy::cast_precision_loss)]
    let step = length / segments as f32;
    put(&mut points, &mut count, head);
    let mut walked = 0.0f32;
    let mut next = step;
    let mut newer = head;
    for &older in trail.iter().rev().skip(1) {
        let leg = (older - newer).length();
        while count <= segments && leg > 0.0 && walked + leg >= next {
            put(
                &mut points,
                &mut count,
                newer + (older - newer) * ((next - walked) / leg),
            );
            next += step;
        }
        if count > segments {
            break;
        }
        walked += leg;
        newer = older;
    }
    if count <= segments && walked > next - step {
        put(&mut points, &mut count, newer);
    }
    if let Some(drawn) = points.get_mut(..count) {
        drawn.reverse();
    }
    (points, count)
}

fn put(points: &mut [Vec3; MAX_STREAK_POINTS], count: &mut usize, point: Vec3) {
    if let Some(slot) = points.get_mut(*count) {
        *slot = point;
        *count = count.saturating_add(1);
    }
}

fn projectile_placement(shot: &ProjectileView) -> mint_render::Mat4 {
    let forward = shot.direction;
    let mut left = mint_render::Vec3::new(-forward.y, forward.x, 0.0);
    let length = (left.x * left.x + left.y * left.y).sqrt();
    if !length.is_finite() || length <= 1.0e-4 {
        left = mint_render::Vec3::new(0.0, 1.0, 0.0);
    } else {
        left = mint_render::Vec3::new(left.x / length, left.y / length, 0.0);
    }
    let up = mint_render::Vec3::new(
        forward.y * left.z - forward.z * left.y,
        forward.z * left.x - forward.x * left.z,
        forward.x * left.y - forward.y * left.x,
    );
    mint_render::Mat4::from_cols([
        [forward.x, forward.y, forward.z, 0.0],
        [left.x, left.y, left.z, 0.0],
        [up.x, up.y, up.z, 0.0],
        [shot.x, shot.y, shot.z, 1.0],
    ])
}

fn projectile_pose(shot: &ProjectileView, template: usize) -> UnitPose {
    UnitPose {
        opacity: 1.0,
        id: shot.entity.index(),
        generation: shot.entity.generation(),
        template,
        placement: projectile_placement(shot),
        conditions: mint_sage_scene::Conditions::default(),
        attack_cycle: 0,
        attack_slot: None,
        speed: 0.0,
        weapon_cycle: 0.0,
        house_color: NO_HOUSE_COLOUR,
        hero_colours: None,
        shroud_masked: false,
        progress: None,
        ghost: None,
        casts_shadow: true,
        look: mint_sage_scene::LookKey::PLAIN,
        ride: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_streak_bends_along_a_curved_flight() {
        let mut trail = VecDeque::new();
        for step in 0..=40u8 {
            let x = f32::from(step) * 2.0;
            record(&mut trail, Vec3::new(x, 0.0, x * (80.0 - x) / 40.0), 30.0);
        }
        let (points, count) = along_trail(&trail, 30.0, 3);
        assert_eq!(count, 4);
        let (tail, head) = (points[0], points[3]);
        assert_eq!(head, Vec3::new(80.0, 0.0, 0.0));
        let path: f32 = points
            .windows(2)
            .take(3)
            .map(|pair| (pair[1] - pair[0]).length())
            .sum();
        assert!(
            (path - 30.0).abs() < 0.5,
            "{path} is not the authored length"
        );
        let chord = head - tail;
        let middle = points[1] - tail;
        let off = middle.cross(chord).length() / chord.length();
        assert!(off > 0.5, "the ribbon is straight: {off} off its chord");
    }
}
