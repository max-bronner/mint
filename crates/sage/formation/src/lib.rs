#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod assign;
pub mod assistant;
pub mod contain;
pub mod drag;
pub mod group;
pub mod table;

pub use assign::{assign_slots, Assignment};
pub use assistant::{FormationAssistant, FormationTemplate, Selection, UnitDefinition};
pub use contain::{
    find_horde_contain, horde_contains, is_horde_contain, read_horde_contain, AodSway,
    ContainReport, HordeContainDef, LeaderRef, RankLine, SlotPosition, HORDE_CONTAIN_MODULES,
    MAX_MODULE_DEPTH, MAX_SLOTS,
};
pub use drag::{
    build as build_drag, optimize as optimize_drag, preview_positions as drag_preview_positions,
    rotate as rotate_drag, DragSlot, DragUnit,
};
pub use group::{lay_out, rotate_offset, GroupMember, GroupSpacing, NO_FORMATION};
pub use table::{
    FormationBasis, FormationSlot, FormationTable, HordeSettings,
    DEFAULT_MELEE_ATTACK_LEASH_DISTANCE,
};
