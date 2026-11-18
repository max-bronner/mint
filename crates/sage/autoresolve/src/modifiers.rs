#[derive(Debug, Clone, PartialEq)]
pub struct Strengthen {
    pub count: i32,
    pub rows: Vec<StrengthenRow>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrengthenRow {
    pub count: i32,
    pub weapon: f32,
    pub armor: f32,
    pub experience: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Percents {
    pub attack: i32,
    pub defense: i32,
    pub experience: i32,
}

#[derive(Debug, Clone, Default)]
pub struct BonusInputs {
    pub region: Percents,
    pub player: Percents,
    pub extra_start_resources: i32,
    pub free_builder: i32,
    pub free_inn_units: i32,
    pub strengthen: Vec<Strengthen>,
    pub resource: Option<(f32, f32)>,
    pub handicap: Option<(f32, f32, f32)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Multipliers {
    pub weapon: f32,
    pub armor: f32,
    pub experience: f32,
}

fn attack_factor(percent: i32) -> f32 {
    1.0 + 0.01 * percent as f32
}

fn defense_factor(percent: i32) -> Option<f32> {
    let f = 1.0 + 0.01 * percent as f32;
    (f > 0.0).then(|| 1.0 / f)
}

#[must_use]
pub fn multipliers(input: &BonusInputs) -> Multipliers {
    let mut acc = (1.0_f32, 1.0_f32, 1.0_f32);
    for percents in [input.region, input.player] {
        acc.0 *= attack_factor(percents.attack);
        if let Some(f) = defense_factor(percents.defense) {
            acc.1 *= f;
        }
        acc.2 *= attack_factor(percents.experience);
    }
    let mut m = Multipliers {
        weapon: 1.0,
        armor: 1.0,
        experience: 1.0,
    };
    for table in &input.strengthen {
        let row = table
            .rows
            .iter()
            .filter(|r| r.count <= table.count)
            .max_by_key(|r| r.count);
        if let Some(row) = row {
            m.weapon *= row.weapon;
            m.armor *= row.armor;
            m.experience *= row.experience;
        }
    }
    m.weapon *= acc.0;
    m.armor *= acc.1;
    m.experience *= acc.2;
    if let Some((weapon, armor)) = input.resource {
        m.weapon *= weapon;
        m.armor *= armor;
    }
    if let Some((weapon, armor, experience)) = input.handicap {
        m.weapon *= weapon;
        m.armor *= armor;
        m.experience *= experience;
    }
    m
}

#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn territory_share(bonus: i32, owned: i32, regions: i32) -> i32 {
    let share = owned as f32 / regions as f32;
    (bonus as f32 * share).trunc() as i32
}
