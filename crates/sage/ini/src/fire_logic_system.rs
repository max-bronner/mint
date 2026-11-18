use crate::tree::Block;
use crate::value::parse_int;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FireDataError {
    #[error("{file}:{line}: TerrainCellType index {index} out of range, expected 0..3")]
    CellTypeIndex { index: i64, file: String, line: u32 },
    #[error("{file}:{line}: {field} value {value} out of range, expected 0..{max}")]
    FieldOutOfRange {
        field: &'static str,
        value: i64,
        max: u32,
        file: String,
        line: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FireMaterial {
    pub fuel: u16,
    pub max_burn_rate: u16,
    pub decay: u16,
    pub resistance: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FireLogicSystem {
    pub max_cells_burning: i64,
    pub fuel_regeneration_rate: i64,
    pub cell_types: [FireMaterial; 4],
}

impl Default for FireLogicSystem {
    fn default() -> Self {
        Self {
            max_cells_burning: i64::from(i32::MAX),
            fuel_regeneration_rate: 1,
            cell_types: [FireMaterial::default(); 4],
        }
    }
}

fn bounded_field(
    block: &Block,
    keyword: &'static str,
    max: u32,
) -> Result<Option<u16>, FireDataError> {
    let Some(field) = block.field(keyword) else {
        return Ok(None);
    };
    let Some(value) = field.value().and_then(parse_int) else {
        return Ok(None);
    };
    if value < 0 || value > i64::from(max) {
        return Err(FireDataError::FieldOutOfRange {
            field: keyword,
            value,
            max,
            file: field.file.to_string(),
            line: field.line,
        });
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(Some(value as u16))
}

fn read_fire_material(block: &Block, base: FireMaterial) -> Result<FireMaterial, FireDataError> {
    Ok(FireMaterial {
        fuel: bounded_field(block, "Fuel", 0xFFFF)?.unwrap_or(base.fuel),
        max_burn_rate: bounded_field(block, "MaxBurnRate", 0xFFF)?.unwrap_or(base.max_burn_rate),
        decay: bounded_field(block, "Decay", 0x3FF)?.unwrap_or(base.decay),
        resistance: bounded_field(block, "Resistance", 0xFF)?.unwrap_or(base.resistance),
    })
}

pub fn read_terrain_cell_type(block: &Block) -> Result<(usize, FireMaterial), FireDataError> {
    let index = block
        .name()
        .and_then(parse_int)
        .ok_or_else(|| FireDataError::CellTypeIndex {
            index: -1,
            file: block.file.to_string(),
            line: block.line,
        })?;
    if !(0..=3).contains(&index) {
        return Err(FireDataError::CellTypeIndex {
            index,
            file: block.file.to_string(),
            line: block.line,
        });
    }
    let slot = usize::try_from(index).unwrap_or(0);
    Ok((slot, read_fire_material(block, FireMaterial::default())?))
}

pub fn read_fire_logic_system(
    block: &Block,
    base: FireLogicSystem,
) -> Result<FireLogicSystem, FireDataError> {
    let mut system = base;
    if let Some(value) = block
        .field("MaxCellsBurning")
        .and_then(|f| f.value())
        .and_then(parse_int)
    {
        system.max_cells_burning = value;
    }
    if let Some(value) = block
        .field("FuelRegenerationRate")
        .and_then(|f| f.value())
        .and_then(parse_int)
    {
        system.fuel_regeneration_rate = value;
    }
    for cell_block in block.blocks_named("TerrainCellType") {
        let (index, material) = read_terrain_cell_type(cell_block)?;
        if let Some(slot) = system.cell_types.get_mut(index) {
            *slot = material;
        }
    }
    Ok(system)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flammability {
    pub material: FireMaterial,
    pub fuel_factor: Option<f32>,
}

pub fn read_flammability(block: &Block) -> Result<Flammability, FireDataError> {
    Ok(Flammability {
        material: read_fire_material(block, FireMaterial::default())?,
        fuel_factor: block
            .field("FuelFactor")
            .and_then(|f| f.value())
            .and_then(crate::value::parse_float),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{BlockTable, Document};

    fn root(text: &str, keyword: &str) -> Block {
        let document = Document::parse("t.ini", text, &BlockTable::corpus_path()).unwrap();
        let found = document.roots_named(keyword).next().cloned();
        found.unwrap()
    }

    #[test]
    fn a_system_reads_its_four_cell_types_by_index() {
        let block = root(
            "FireLogicSystem\n\
             MaxCellsBurning = 5\n\
             FuelRegenerationRate = 2\n\
             TerrainCellType 1\n\
               Fuel = 100\n\
               MaxBurnRate = 10\n\
               Decay = 1\n\
               Resistance = 2\n\
             End\n\
             End\n",
            "FireLogicSystem",
        );
        let system = read_fire_logic_system(&block, FireLogicSystem::default()).unwrap();
        assert_eq!(system.max_cells_burning, 5);
        assert_eq!(system.fuel_regeneration_rate, 2);
        assert_eq!(system.cell_types[1].fuel, 100);
        assert_eq!(system.cell_types[0], FireMaterial::default());
    }

    #[test]
    fn a_cell_type_index_outside_0_3_errors() {
        let block = root(
            "FireLogicSystem\nTerrainCellType 4\n  Fuel = 1\nEnd\nEnd\n",
            "FireLogicSystem",
        );
        assert!(matches!(
            read_fire_logic_system(&block, FireLogicSystem::default()),
            Err(FireDataError::CellTypeIndex { index: 4, .. })
        ));
    }

    #[test]
    fn a_resistance_value_outside_its_bound_errors() {
        let block = root(
            "FireLogicSystem\nTerrainCellType 1\n  Resistance = 256\nEnd\nEnd\n",
            "FireLogicSystem",
        );
        assert!(matches!(
            read_fire_logic_system(&block, FireLogicSystem::default()),
            Err(FireDataError::FieldOutOfRange {
                field: "Resistance",
                value: 256,
                max: 0xFF,
                ..
            })
        ));
    }

    #[test]
    fn an_object_flammability_block_reads_its_quartet() {
        let object = root(
            "Object Test\n  Flammability\n    Fuel = 50\n    MaxBurnRate = 5\n    Decay = 1\n    Resistance = 10\n    FuelFactor = 1.5\n  End\nEnd\n",
            "Object",
        );
        let block = object.blocks_named("Flammability").next().unwrap();
        let flammability = read_flammability(block).unwrap();
        assert_eq!(flammability.material.fuel, 50);
        assert_eq!(flammability.fuel_factor, Some(1.5));
    }
}
