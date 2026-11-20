use mint_sage_ini::living_world::campaign::ActNugget;
use mint_sage_ini::living_world::presentation::ArmySize;
use mint_sage_ini::living_world::region::RegionCampaignDef;
use mint_sage_ini::living_world::LivingWorldData;
use mint_sage_livingworld::{ArmyId, LivingWorldState, PlayerId, RegionId};

const SPOT_COPY_SHARE: f32 = 0.707;

const DEFAULT_MOVE_SPEED: f32 = 5.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegionView {
    pub name: String,
    pub sub_object: String,
    pub center: [f32; 2],
    pub hero_spots: Vec<[f32; 2]>,
    pub garrison_spots: Vec<[f32; 2]>,
    pub connections: Vec<(usize, Vec<[f32; 2]>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArmyMark {
    pub army: ArmyId,
    pub player: PlayerId,
    pub region: RegionId,
    pub spot: [f32; 2],
    pub size: ArmySize,
    pub icon: String,
    pub order: Option<RegionId>,
    pub move_speed: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineEnd {
    pub at: [f32; 2],
    pub region: Option<RegionId>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TerritoryView {
    pub effect: String,
    pub eva: Option<String>,
    pub look_at: Option<[f32; 2]>,
    pub heading: f32,
    pub zoom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seat {
    pub army: ArmyId,
    pub player: PlayerId,
    pub region: RegionId,
    pub hero: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Held {
    army: ArmyId,
    region: RegionId,
    hero: bool,
    slot: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct Slots {
    held: Vec<Held>,
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub regions: Vec<RegionView>,
    pub size_caps: [i32; 3],
    pub territory_effects: Vec<String>,
    pub territories: Vec<TerritoryView>,
    pub default_speed: f32,
    pub line_points: u32,
    pub line_bias: f32,
    hero_spawns: Vec<(String, Option<String>, Option<f32>)>,
    default_icons: Vec<Option<String>>,
}

fn coord(c: &mint_sage_ini::living_world::Coord2) -> [f32; 2] {
    [c.x, c.y]
}

#[must_use]
pub fn extend_spots(spots: &[[f32; 2]], d: f32) -> Vec<[f32; 2]> {
    let shifts = [[d, -d], [-d, -d], [d, d], [-d, d], [d, -d]];
    let mut out = spots.to_vec();
    for shift in shifts {
        out.extend(spots.iter().map(|s| [s[0] + shift[0], s[1] + shift[1]]));
    }
    out
}

#[must_use]
pub fn sample_line(from: [f32; 2], to: [f32; 2], count: u32) -> Vec<[f32; 2]> {
    let n = count.max(2);
    (0..n)
        .map(|i| {
            let t = f64::from(i) / f64::from(n - 1);
            #[allow(clippy::cast_possible_truncation)]
            let t = t as f32;
            [
                from[0] + (to[0] - from[0]) * t,
                from[1] + (to[1] - from[1]) * t,
            ]
        })
        .collect()
}

fn views_of(
    region_campaign: Option<&RegionCampaignDef>,
    info: Option<&mint_sage_ini::living_world::presentation::MapInfoDef>,
) -> Vec<RegionView> {
    let shift = info.and_then(|i| i.hero_army_icon_diameter).unwrap_or(0.0) * SPOT_COPY_SHARE;
    region_campaign
        .map(|rc| {
            let index_of = |name: &str| {
                rc.regions
                    .iter()
                    .position(|r| r.name.eq_ignore_ascii_case(name))
            };
            rc.regions
                .iter()
                .map(|r| {
                    let authored: Vec<[f32; 2]> = r.hero_army_spot.iter().map(coord).collect();
                    let garrison: Vec<[f32; 2]> = r.garrison_army_spot.iter().map(coord).collect();
                    let center = r
                        .center_point
                        .as_ref()
                        .map(coord)
                        .unwrap_or_else(|| mean(&authored).unwrap_or_default());
                    RegionView {
                        name: r.name.clone(),
                        sub_object: r.sub_object.clone().unwrap_or_else(|| r.name.clone()),
                        center,
                        hero_spots: extend_spots(&authored, shift),
                        garrison_spots: extend_spots(&garrison, shift),
                        connections: r
                            .connections
                            .iter()
                            .filter_map(|c| {
                                let to = index_of(c.region.as_deref()?)?;
                                Some((to, c.detour_points.iter().map(coord).collect()))
                            })
                            .collect(),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[must_use]
pub fn region_views(data: &LivingWorldData, campaign: &str, scenario: usize) -> Vec<RegionView> {
    let region_campaign = data
        .campaigns
        .get(campaign)
        .and_then(|c| c.scenarios.get(scenario))
        .and_then(|s| s.region_campaign.as_deref())
        .and_then(|name| data.region_campaigns.get(name));
    views_of(region_campaign, data.map_info.as_ref())
}

impl Layout {
    #[must_use]
    pub fn new(data: &LivingWorldData, state: &LivingWorldState) -> Self {
        Self::of(data, &state.rules.campaign, state.rules.scenario)
    }

    #[must_use]
    pub fn of(data: &LivingWorldData, campaign: &str, scenario: usize) -> Self {
        let campaign = data.campaigns.get(campaign);
        let region_campaign: Option<&RegionCampaignDef> = campaign
            .and_then(|c| c.scenarios.get(scenario))
            .and_then(|s| s.region_campaign.as_deref())
            .and_then(|name| data.region_campaigns.get(name));
        let info = data.map_info.as_ref();
        let regions = views_of(region_campaign, info);
        let size_caps = region_campaign.map_or([0, 120, 240], |rc| {
            [
                rc.hero_only_army_command_points.unwrap_or(0),
                rc.small_army_command_points.unwrap_or(0),
                rc.medium_army_command_points.unwrap_or(0),
            ]
        });
        let territory_effects = region_campaign
            .map(|rc| {
                rc.concurrent_bonuses
                    .iter()
                    .map(|t| t.effect_name.clone().unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default();
        let territories = region_campaign
            .map(|rc| {
                rc.concurrent_bonuses
                    .iter()
                    .map(|t| TerritoryView {
                        effect: t.effect_name.clone().unwrap_or_default(),
                        eva: t.unified_eva_event.clone(),
                        look_at: t.look_at_center.as_ref().map(coord),
                        heading: t.look_at_heading.unwrap_or(0.0),
                        zoom: t.look_at_zoom.unwrap_or(0.0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut hero_spawns = Vec::new();
        for act in campaign.iter().flat_map(|c| c.acts.iter()) {
            for nugget in &act.nuggets {
                if let ActNugget::SpawnArmy(spawn) = nugget {
                    if let Some(hero) = &spawn.hero_template_name {
                        hero_spawns.push((
                            hero.to_ascii_lowercase(),
                            spawn.icon.clone(),
                            spawn.move_speed,
                        ));
                    }
                }
            }
        }
        let default_icons = data
            .player_templates
            .iter()
            .map(|t| t.default_army_icon_name.clone())
            .collect();
        Self {
            regions,
            size_caps,
            territory_effects,
            territories,
            default_speed: info
                .and_then(|i| i.default_army_move_speed)
                .unwrap_or(DEFAULT_MOVE_SPEED),
            line_points: info.and_then(|i| i.num_points_per_army_line).unwrap_or(2),
            line_bias: info.and_then(|i| i.army_line_height_bias).unwrap_or(0.0),
            hero_spawns,
            default_icons,
        }
    }

    #[must_use]
    pub fn size_class(&self, cp: i32) -> ArmySize {
        let [hero, small, medium] = self.size_caps;
        if cp <= hero {
            ArmySize::HeroOnly
        } else if cp <= small {
            ArmySize::Small
        } else if cp <= medium {
            ArmySize::Medium
        } else {
            ArmySize::Large
        }
    }

    fn spots(&self, region: RegionId, hero: bool) -> &[[f32; 2]] {
        self.regions.get(region.index()).map_or(&[], |view| {
            if hero {
                &view.hero_spots
            } else {
                &view.garrison_spots
            }
        })
    }

    fn first_free(&self, held: &[Held], region: RegionId, hero: bool) -> Option<usize> {
        (0..self.spots(region, hero).len()).find(|slot| {
            !held
                .iter()
                .any(|h| h.region == region && h.hero == hero && h.slot == Some(*slot))
        })
    }

    pub fn assign(&self, slots: &mut Slots, seats: &[Seat]) {
        slots
            .held
            .retain(|h| seats.iter().any(|s| s.army == h.army));
        let moved = |h: &Held| {
            seats
                .iter()
                .any(|s| s.army == h.army && (s.region != h.region || s.hero != h.hero))
        };
        let mut movers: Vec<Seat> = seats
            .iter()
            .filter(|s| slots.held.iter().any(|h| h.army == s.army && moved(h)))
            .copied()
            .collect();
        movers.sort_by_key(|s| (s.player, s.army));
        let fresh: Vec<Seat> = seats
            .iter()
            .filter(|s| !slots.held.iter().any(|h| h.army == s.army))
            .copied()
            .collect();
        for seat in &fresh {
            self.take(slots, seat);
        }
        slots
            .held
            .retain(|h| !movers.iter().any(|s| s.army == h.army));
        for seat in &movers {
            self.take(slots, seat);
        }
    }

    fn take(&self, slots: &mut Slots, seat: &Seat) {
        let slot = self.first_free(&slots.held, seat.region, seat.hero);
        slots.held.push(Held {
            army: seat.army,
            region: seat.region,
            hero: seat.hero,
            slot,
        });
    }

    #[must_use]
    pub fn spot_of(&self, slots: &Slots, seat: &Seat) -> [f32; 2] {
        slots
            .held
            .iter()
            .find(|h| h.army == seat.army)
            .and_then(|h| h.slot)
            .and_then(|slot| self.spots(seat.region, seat.hero).get(slot))
            .copied()
            .or_else(|| self.regions.get(seat.region.index()).map(|v| v.center))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn place(&self, state: &LivingWorldState, slots: &mut Slots) -> Vec<ArmyMark> {
        let live: Vec<&mint_sage_livingworld::Army> = state
            .armies
            .values()
            .filter(|a| !a.destroy_queued && self.regions.get(a.region.index()).is_some())
            .collect();
        let seats: Vec<Seat> = live
            .iter()
            .map(|a| Seat {
                army: a.id,
                player: a.player,
                region: a.region,
                hero: a.is_hero_army(),
            })
            .collect();
        self.assign(slots, &seats);
        let mut out = Vec::with_capacity(live.len());
        for (army, seat) in live.into_iter().zip(&seats) {
            let spot = self.spot_of(slots, seat);
            let cp = army.roster.iter().fold(0i32, |sum, m| {
                let each = state.rules.unit(m.unit).command_points;
                sum.saturating_add(each.saturating_mul(i32::try_from(m.count).unwrap_or(i32::MAX)))
            });
            out.push(ArmyMark {
                army: army.id,
                player: army.player,
                region: army.region,
                spot,
                size: self.size_class(cp),
                icon: self.icon_for(state, army.player, army.hero),
                order: army.order,
                move_speed: self.speed_for(state, army.hero),
            });
        }
        out
    }

    fn hero_spawn(
        &self,
        state: &LivingWorldState,
        hero: Option<mint_sage_livingworld::UnitId>,
    ) -> Option<&(String, Option<String>, Option<f32>)> {
        let name = state.rules.unit_names.get(usize::from(hero?.0))?;
        self.hero_spawns
            .iter()
            .find(|(h, _, _)| h.eq_ignore_ascii_case(name))
    }

    fn speed_for(
        &self,
        state: &LivingWorldState,
        hero: Option<mint_sage_livingworld::UnitId>,
    ) -> f32 {
        self.hero_spawn(state, hero)
            .and_then(|(_, _, speed)| *speed)
            .unwrap_or(self.default_speed)
    }

    fn icon_for(
        &self,
        state: &LivingWorldState,
        player: PlayerId,
        hero: Option<mint_sage_livingworld::UnitId>,
    ) -> String {
        if let Some((_, Some(icon), _)) = self.hero_spawn(state, hero) {
            return icon.clone();
        }
        state
            .player(player)
            .and_then(|p| self.default_icons.get(usize::from(p.template.0)))
            .and_then(Clone::clone)
            .unwrap_or_default()
    }

    fn detours(&self, from: RegionId, to: RegionId) -> &[[f32; 2]] {
        self.regions
            .get(from.index())
            .and_then(|v| v.connections.iter().find(|(n, _)| *n == to.index()))
            .map_or(&[], |(_, points)| points)
    }

    #[must_use]
    pub fn route(&self, from: RegionId, to: RegionId, target: [f32; 2]) -> Vec<[f32; 2]> {
        let mut points = self.detours(from, to).to_vec();
        points.push(target);
        points
    }

    #[must_use]
    pub fn line_ends(&self, mark: &ArmyMark) -> Option<(LineEnd, LineEnd)> {
        let dest = mark.order.filter(|d| *d != mark.region)?;
        let target = self.regions.get(dest.index())?;
        Some((
            LineEnd {
                at: mark.spot,
                region: Some(mark.region),
            },
            LineEnd {
                at: target.center,
                region: Some(dest),
            },
        ))
    }
}

fn mean(points: &[[f32; 2]]) -> Option<[f32; 2]> {
    if points.is_empty() {
        return None;
    }
    let n = f32::from(u16::try_from(points.len()).unwrap_or(u16::MAX));
    let sum = points
        .iter()
        .fold([0.0f32, 0.0], |s, p| [s[0] + p[0], s[1] + p[1]]);
    Some([sum[0] / n, sum[1] / n])
}

#[must_use]
pub fn walk_speed_scale(shift_held: bool, turbo_enabled: bool) -> f32 {
    if shift_held && turbo_enabled {
        10.0
    } else {
        1.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Walk {
    pub army: ArmyId,
    pos: [f32; 2],
    waypoints: Vec<[f32; 2]>,
    next: usize,
    heading: Option<[f32; 2]>,
    speed: f32,
}

impl Walk {
    #[must_use]
    pub fn new(army: ArmyId, from: [f32; 2], waypoints: Vec<[f32; 2]>, speed: f32) -> Self {
        if speed > 0.0 && speed.is_finite() {
            Self {
                army,
                pos: from,
                waypoints,
                next: 0,
                heading: None,
                speed,
            }
        } else {
            Self {
                army,
                pos: waypoints.last().copied().unwrap_or(from),
                waypoints: Vec::new(),
                next: 0,
                heading: None,
                speed: 0.0,
            }
        }
    }

    #[must_use]
    pub fn position(&self) -> [f32; 2] {
        self.pos
    }

    #[must_use]
    pub fn done(&self) -> bool {
        self.heading.is_none() && self.next >= self.waypoints.len()
    }

    pub fn step(&mut self, speed_scale: f32) {
        if self.heading.is_none() {
            self.heading = self.waypoints.get(self.next).copied();
            if self.heading.is_some() {
                self.next = self.next.saturating_add(1);
            }
        }
        let Some(target) = self.heading else {
            return;
        };
        let (dx, dy) = (target[0] - self.pos[0], target[1] - self.pos[1]);
        let distance = (dx * dx + dy * dy).sqrt();
        let step = distance.min(self.speed * speed_scale);
        if distance > 0.0 {
            self.pos = [
                self.pos[0] + dx / distance * step,
                self.pos[1] + dy / distance * step,
            ];
        }
        if distance - step < 1.0 {
            self.heading = None;
        }
    }
}

#[must_use]
pub fn walks(layout: &Layout, before: &[ArmyMark], after: &[ArmyMark]) -> Vec<Walk> {
    after
        .iter()
        .filter_map(|now| {
            let then = before.iter().find(|b| b.army == now.army)?;
            (then.region != now.region).then(|| {
                Walk::new(
                    now.army,
                    then.spot,
                    layout.route(then.region, now.region, now.spot),
                    now.move_speed,
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(army: u32, region: u16, spot: [f32; 2]) -> ArmyMark {
        ArmyMark {
            army: ArmyId(army),
            player: PlayerId(0),
            region: RegionId(region),
            spot,
            size: ArmySize::Small,
            icon: String::new(),
            order: None,
            move_speed: 10.0,
        }
    }

    fn two_regions() -> Layout {
        let view = |spots: Vec<[f32; 2]>, connections| RegionView {
            center: [1000.0, 1000.0],
            hero_spots: spots.clone(),
            garrison_spots: spots,
            connections,
            ..RegionView::default()
        };
        Layout {
            regions: vec![
                view(vec![[0.0, 0.0]], vec![(1, vec![[10.0, 0.0], [10.0, 40.0]])]),
                view(vec![[100.0, 0.0]], Vec::new()),
            ],
            default_speed: 5.0,
            ..Layout::default()
        }
    }

    #[test]
    fn a_walk_goes_through_the_detour_points_at_its_speed_per_client_frame() {
        let layout = two_regions();
        let before = [mark(1, 0, [0.0, 0.0])];
        let after = [mark(1, 1, [100.0, 40.0])];
        let mut walk = walks(&layout, &before, &after).remove(0);
        walk.step(1.0);
        assert_eq!(walk.position(), [10.0, 0.0]);
        for _ in 0..4 {
            walk.step(1.0);
        }
        assert_eq!(walk.position(), [10.0, 40.0]);
        let mut frames = 5;
        while !walk.done() {
            walk.step(1.0);
            frames += 1;
        }
        assert_eq!(walk.position(), [100.0, 40.0]);
        assert_eq!(frames, 5 + 9);
    }

    #[test]
    fn a_walk_runs_ten_times_faster_only_with_shift_and_the_turbo_setting() {
        let layout = two_regions();
        let before = [mark(1, 0, [0.0, 0.0])];
        let mut after = [mark(1, 1, [100.0, 0.0])];
        after[0].move_speed = 1.0;
        let travelled = |shift, turbo| {
            let mut walk = walks(&layout, &before, &after).remove(0);
            walk.step(walk_speed_scale(shift, turbo));
            walk.position()[0]
        };
        assert_eq!(travelled(true, true), 10.0);
        assert_eq!(travelled(true, false), 1.0);
        assert_eq!(travelled(false, true), 1.0);
    }

    fn seat(army: u32, player: u8, region: u16, hero: bool) -> Seat {
        Seat {
            army: ArmyId(army),
            player: PlayerId(player),
            region: RegionId(region),
            hero,
        }
    }

    fn roomy_layout(spots: usize) -> Layout {
        let mut layout = two_regions();
        for view in &mut layout.regions {
            view.hero_spots = (0..spots).map(|i| [i as f32, 0.0]).collect();
            view.garrison_spots = (0..spots).map(|i| [0.0, i as f32]).collect();
        }
        layout
    }

    #[test]
    fn an_army_takes_the_first_free_slot_of_its_kind_and_keeps_it() {
        let layout = roomy_layout(3);
        let mut slots = Slots::default();
        let first = [
            seat(1, 0, 0, true),
            seat(2, 0, 0, false),
            seat(3, 1, 0, true),
        ];
        layout.assign(&mut slots, &first);
        let spots: Vec<[f32; 2]> = first.iter().map(|s| layout.spot_of(&slots, s)).collect();
        assert_eq!(spots, [[0.0, 0.0], [0.0, 0.0], [1.0, 0.0]]);
        layout.assign(&mut slots, &[seat(3, 1, 0, true), seat(1, 0, 0, true)]);
        assert_eq!(layout.spot_of(&slots, &seat(3, 1, 0, true)), [1.0, 0.0]);
    }

    #[test]
    fn a_mover_takes_a_slot_in_its_destination_and_frees_the_one_it_left() {
        let layout = roomy_layout(2);
        let mut slots = Slots::default();
        layout.assign(&mut slots, &[seat(1, 0, 0, true), seat(2, 0, 1, true)]);
        layout.assign(&mut slots, &[seat(1, 0, 1, true), seat(2, 0, 1, true)]);
        assert_eq!(layout.spot_of(&slots, &seat(1, 0, 1, true)), [1.0, 0.0]);
        layout.assign(&mut slots, &[seat(1, 0, 1, true), seat(3, 0, 0, true)]);
        assert_eq!(layout.spot_of(&slots, &seat(3, 0, 0, true)), [0.0, 0.0]);
    }

    #[test]
    fn with_no_free_slot_an_army_ends_at_the_region_centre() {
        let layout = roomy_layout(1);
        let mut slots = Slots::default();
        layout.assign(&mut slots, &[seat(1, 0, 0, true)]);
        layout.assign(&mut slots, &[seat(1, 0, 0, true), seat(2, 0, 0, true)]);
        assert_eq!(
            layout.spot_of(&slots, &seat(2, 0, 0, true)),
            [1000.0, 1000.0]
        );
    }

    #[test]
    fn spot_lists_gain_five_shifted_copies_in_the_retail_order() {
        let spots = [[1.0, 2.0], [3.0, 4.0]];
        let out = extend_spots(&spots, 10.0);
        assert_eq!(out.len(), 12);
        assert_eq!(out[2], [11.0, -8.0]);
        assert_eq!(out[4], [-9.0, -8.0]);
        assert_eq!(out[6], [11.0, 12.0]);
        assert_eq!(out[8], [-9.0, 12.0]);
        assert_eq!(out[10], [11.0, -8.0]);
        assert_eq!(out[11], [13.0, -6.0]);
    }

    #[test]
    fn an_army_line_has_at_least_two_samples_and_none_within_one_region() {
        assert_eq!(sample_line([0.0, 0.0], [10.0, 0.0], 0).len(), 2);
        let line = sample_line([0.0, 0.0], [10.0, 0.0], 5);
        assert_eq!(line.len(), 5);
        assert_eq!(line[2], [5.0, 0.0]);
        let layout = two_regions();
        let mut army = mark(1, 0, [0.0, 0.0]);
        army.order = Some(RegionId(0));
        assert!(layout.line_ends(&army).is_none());
        army.order = Some(RegionId(1));
        let (from, to) = layout.line_ends(&army).expect("a line");
        assert_eq!((from.at, to.at), ([0.0, 0.0], [1000.0, 1000.0]));
    }

    #[test]
    fn size_classes_follow_the_three_caps_inclusively() {
        let layout = Layout {
            size_caps: [0, 120, 240],
            ..Layout::default()
        };
        let classes: Vec<ArmySize> = [0, 1, 120, 121, 240, 241]
            .iter()
            .map(|cp| layout.size_class(*cp))
            .collect();
        assert_eq!(
            classes,
            [
                ArmySize::HeroOnly,
                ArmySize::Small,
                ArmySize::Small,
                ArmySize::Medium,
                ArmySize::Medium,
                ArmySize::Large
            ]
        );
    }
}
