use mint_sage_ini::value::split_tag;
use mint_sage_ini::{Block, Field};

use super::Reads;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContactTags(u8);

impl ContactTags {
    pub const REPAIR: Self = Self(1);
    pub const SWOOP: Self = Self(1 << 1);
    pub const GRAB: Self = Self(1 << 2);
    pub const RAM: Self = Self(1 << 3);
    pub const BOMB: Self = Self(1 << 4);
    pub const MENU: Self = Self(1 << 5);

    const NAMES: [(&'static str, Self); 6] = [
        ("Repair", Self::REPAIR),
        ("Swoop", Self::SWOOP),
        ("Grab", Self::GRAB),
        ("Ram", Self::RAM),
        ("Bomb", Self::BOMB),
        ("Menu", Self::MENU),
    ];

    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(written, _)| written.eq_ignore_ascii_case(name))
            .map(|(_, tag)| *tag)
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactPoint {
    pub position: [f32; 3],
    pub tags: ContactTags,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContactLabels(Vec<ContactTags>);

impl ContactLabels {
    #[must_use]
    pub fn tags(&self) -> ContactTags {
        self.0
            .iter()
            .fold(ContactTags::default(), |held, tag| held.union(*tag))
    }

    pub fn iter(&self) -> impl Iterator<Item = ContactTags> + '_ {
        self.0.iter().copied()
    }
}

#[must_use]
pub fn nearest_labelled_point<'a>(
    points: &'a [ContactPoint],
    labels: &ContactLabels,
    from: [f32; 3],
) -> Option<&'a ContactPoint> {
    let distance = |point: &ContactPoint| {
        let [x, y, z] = point.position;
        let (dx, dy, dz) = (x - from[0], y - from[1], z - from[2]);
        dx * dx + dy * dy + dz * dz
    };
    labels.iter().find_map(|label| {
        points
            .iter()
            .filter(|point| point.tags.contains(label))
            .fold(None, |best: Option<(f32, &ContactPoint)>, point| {
                let d = distance(point);
                match best {
                    Some((best_d, _)) if best_d <= d => best,
                    _ => Some((d, point)),
                }
            })
            .map(|(_, point)| point)
    })
}

pub(super) fn contact_points(lineage: &[&Block], reads: &Reads<'_>) -> Vec<ContactPoint> {
    const KEYWORDS: [&str; 2] = ["GeometryContactPoint", "AttackContactPoint"];
    let Some(block) = lineage.iter().rev().find(|block| {
        KEYWORDS
            .iter()
            .any(|k| block.fields_named(k).next().is_some())
    }) else {
        return Vec::new();
    };
    KEYWORDS
        .iter()
        .flat_map(|keyword| block.fields_named(keyword))
        .filter_map(|field| contact_point(field, reads))
        .collect()
}

fn contact_point(field: &Field, reads: &Reads<'_>) -> Option<ContactPoint> {
    let axis = |tag: &str| reads.tagged(field, tag);
    let (x, y, z) = (axis("X"), axis("Y"), axis("Z"));
    let (Some(x), Some(y), Some(z)) = (x, y, z) else {
        reads
            .errors
            .field(reads.context, field, "X:<number> Y:<number> Z:<number>");
        return None;
    };
    Some(ContactPoint {
        position: [x, y, z],
        tags: tags(field, contact_tag_tokens(&field.args), reads)?,
    })
}

pub(super) fn special_contact_points(field: Option<&Field>, reads: &Reads<'_>) -> ContactLabels {
    let Some(field) = field else {
        return ContactLabels::default();
    };
    let mut labels = Vec::new();
    for token in &field.args {
        let Some(tag) = ContactTags::named(token) else {
            reads
                .errors
                .value(reads.context, field, token, "a contact point tag");
            return ContactLabels::default();
        };
        if !labels.contains(&tag) {
            labels.push(tag);
        }
    }
    ContactLabels(labels)
}

fn tags<'a>(
    field: &Field,
    tokens: impl Iterator<Item = &'a str>,
    reads: &Reads<'_>,
) -> Option<ContactTags> {
    let mut held = ContactTags::default();
    for token in tokens {
        let Some(tag) = ContactTags::named(token) else {
            reads
                .errors
                .value(reads.context, field, token, "a contact point tag");
            return None;
        };
        held = held.union(tag);
    }
    Some(held)
}

fn contact_tag_tokens(args: &[String]) -> impl Iterator<Item = &str> {
    let mut skip_next = false;
    args.iter().filter_map(move |token| {
        if std::mem::take(&mut skip_next) {
            return None;
        }
        match split_tag(token) {
            Some((_, value)) => {
                skip_next = value.is_empty();
                None
            }
            None => Some(token.as_str()),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(position: [f32; 3], tags: ContactTags) -> ContactPoint {
        ContactPoint { position, tags }
    }

    #[test]
    fn the_first_label_held_wins_then_the_nearest_point_in_3d() {
        let repair_high = point([5.0, 0.0, 10.0], ContactTags::REPAIR);
        let repair_low = point([0.0, 6.0, 0.0], ContactTags::REPAIR);
        let swoop = point([1.0, 0.0, 0.0], ContactTags::SWOOP);
        let tie = point([0.0, -6.0, 0.0], ContactTags::REPAIR);
        let points = [repair_high, repair_low, swoop, tie];
        let labels = |tags: &[ContactTags]| ContactLabels(tags.to_vec());
        let cases = [
            (
                labels(&[ContactTags::REPAIR, ContactTags::SWOOP]),
                Some(repair_low),
            ),
            (
                labels(&[ContactTags::SWOOP, ContactTags::REPAIR]),
                Some(swoop),
            ),
            (
                labels(&[ContactTags::BOMB, ContactTags::REPAIR]),
                Some(repair_low),
            ),
            (labels(&[ContactTags::BOMB]), None),
        ];
        for (labels, expected) in cases {
            assert_eq!(
                nearest_labelled_point(&points, &labels, [0.0; 3]).copied(),
                expected,
                "{labels:?}"
            );
        }
    }
}
