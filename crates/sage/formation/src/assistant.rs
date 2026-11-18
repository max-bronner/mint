use mint_sage_ini::value::{parse_float, parse_int};
use mint_sage_ini::Block;

pub const MAX_ROW_CELLS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitDefinition {
    pub name: String,
    filter: UnitFilter,
    pub preview_object: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct UnitFilter {
    passes_by_default: bool,
    add: Vec<String>,
    remove: Vec<String>,
}

impl UnitFilter {
    fn parse(tokens: &[String]) -> Self {
        let mut filter = Self {
            passes_by_default: true,
            ..Self::default()
        };
        let mut base_seen = false;
        for token in tokens {
            if let Some(name) = token.strip_prefix('+') {
                filter.add.push(name.to_ascii_uppercase());
            } else if let Some(name) = token.strip_prefix('-') {
                filter.remove.push(name.to_ascii_uppercase());
            } else if !base_seen {
                base_seen = true;
                filter.passes_by_default = !token.eq_ignore_ascii_case("NONE");
            }
        }
        filter
    }

    fn passes(&self, kind_of: &[String], name: &str) -> bool {
        let named = |token: &String| {
            name.eq_ignore_ascii_case(token)
                || kind_of.iter().any(|k| k.eq_ignore_ascii_case(token))
        };
        if self.remove.iter().any(named) {
            return false;
        }
        self.passes_by_default || self.add.iter().any(named)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormationTemplate {
    pub name: String,
    pub rows: Vec<Vec<Vec<usize>>>,
    pub last_row_of_class: Vec<Option<usize>>,
}

impl FormationTemplate {
    #[must_use]
    pub fn width(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Selection {
    pub template: usize,
    pub max_drag_length: f32,
    pub max_units_selected: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormationAssistant {
    pub row_padding: f32,
    pub column_padding: f32,
    pub activation_drag_distance: f32,
    pub activation_time: f32,
    pub definitions: Vec<UnitDefinition>,
    pub templates: Vec<FormationTemplate>,
    pub selections: Vec<Selection>,
    pub facing_arrow_templates: [Option<String>; 3],
}

impl FormationAssistant {
    pub fn read(block: &Block) -> Result<Self, String> {
        let number = |keyword: &str, default: f32| {
            block
                .field(keyword)
                .and_then(|field| field.value())
                .and_then(parse_float)
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(default)
        };
        let text = |keyword: &str| {
            block
                .field(keyword)
                .and_then(|field| field.value())
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned)
        };
        let mut assistant = Self {
            row_padding: number("RowPadding", 40.0),
            column_padding: number("ColumnPadding", 50.0),
            activation_drag_distance: number("ActivationDragDistance", 40.0),
            activation_time: number("ActivationTime", 0.3),
            facing_arrow_templates: [
                text("FacingArrowHeadTemplate"),
                text("FacingArrowBodyTemplate"),
                text("FacingArrowBaseTemplate"),
            ],
            ..Self::default()
        };
        for child in block.blocks_named("UnitDefinition") {
            let Some(name) = child.name() else { continue };
            let tokens = child
                .field("ObjectFilter")
                .map(|field| field.args.clone())
                .unwrap_or_default();
            assistant.definitions.push(UnitDefinition {
                name: name.to_owned(),
                filter: UnitFilter::parse(&tokens),
                preview_object: child
                    .field("PreviewObject")
                    .and_then(|field| field.value())
                    .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                    .map(str::to_owned),
            });
        }
        for child in block.blocks_named("FormationTemplate") {
            let Some(name) = child.name() else { continue };
            if assistant
                .templates
                .iter()
                .any(|t| t.name.eq_ignore_ascii_case(name))
            {
                return Err(format!("duplicate formation template name '{name}'"));
            }
            let template = assistant.read_template(child)?;
            assistant.templates.push(template);
        }
        for child in block.blocks_named("FormationSelection") {
            let Some(name) = child.name() else { continue };
            let Some(template) = assistant
                .templates
                .iter()
                .position(|t| t.name.eq_ignore_ascii_case(name))
            else {
                return Err(format!(
                    "formation selection '{name}' names an undefined formation template"
                ));
            };
            let max_drag_length = child
                .field("MaxDragLength")
                .and_then(|field| field.value())
                .and_then(parse_float)
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(f32::MAX);
            let max_units_selected = child
                .field("MaxUnitsSelected")
                .and_then(|field| field.value())
                .and_then(parse_int)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(u32::MAX);
            assistant.selections.push(Selection {
                template,
                max_drag_length,
                max_units_selected,
            });
        }
        assistant
            .selections
            .sort_by(|a, b| a.max_drag_length.total_cmp(&b.max_drag_length));
        Ok(assistant)
    }

    fn read_template(&self, block: &Block) -> Result<FormationTemplate, String> {
        let name = block.name().unwrap_or_default().to_owned();
        let mut rows: Vec<Vec<Vec<usize>>> = Vec::new();
        let mut width: Option<usize> = None;
        for rows_block in block.blocks_named("Rows") {
            for field in rows_block.fields_named("Row") {
                let cells: Vec<Vec<usize>> = field
                    .args
                    .iter()
                    .take(MAX_ROW_CELLS)
                    .map(|cell| {
                        cell.split('|')
                            .map(|class| {
                                let class = class.trim();
                                self.definitions
                                    .iter()
                                    .position(|d| d.name.eq_ignore_ascii_case(class))
                                    .ok_or_else(|| {
                                        format!(
                                            "formation template '{name}': unknown unit type '{class}'"
                                        )
                                    })
                            })
                            .collect::<Result<Vec<usize>, String>>()
                    })
                    .collect::<Result<Vec<Vec<usize>>, String>>()?;
                match width {
                    None => width = Some(cells.len()),
                    Some(expected) if expected != cells.len() => {
                        return Err(format!(
                            "formation template '{name}': row has {} cells, previous rows have {expected}",
                            cells.len()
                        ));
                    }
                    Some(_) => {}
                }
                rows.push(cells);
            }
        }
        let mut last_row_of_class: Vec<Option<usize>> = vec![None; self.definitions.len()];
        for (row_index, row) in rows.iter().enumerate().rev() {
            for cell in row {
                for &class in cell {
                    if let Some(slot) = last_row_of_class.get_mut(class) {
                        if slot.is_none() {
                            *slot = Some(row_index);
                        }
                    }
                }
            }
        }
        Ok(FormationTemplate {
            name,
            rows,
            last_row_of_class,
        })
    }

    pub fn find<'a>(roots: impl IntoIterator<Item = &'a Block>) -> Option<Result<Self, String>> {
        roots
            .into_iter()
            .find(|b| b.keyword.eq_ignore_ascii_case("FormationAssistant"))
            .map(Self::read)
    }

    #[must_use]
    pub fn classify(&self, kind_of: &[String], name: &str) -> Option<usize> {
        self.definitions
            .iter()
            .position(|d| d.filter.passes(kind_of, name))
    }

    #[must_use]
    pub fn choose_template(&self, drag_length: f32, selection_count: usize) -> Option<usize> {
        if self.selections.is_empty() {
            return None;
        }
        if drag_length >= self.activation_drag_distance {
            return self
                .selections
                .iter()
                .find(|s| s.max_drag_length >= drag_length)
                .or_else(|| self.selections.last())
                .map(|s| s.template);
        }
        #[allow(clippy::cast_possible_truncation)]
        let count = selection_count.min(u32::MAX as usize) as u32;
        self.selections
            .iter()
            .filter(|s| s.max_units_selected > count)
            .min_by_key(|s| s.max_units_selected)
            .or_else(|| self.selections.iter().max_by_key(|s| s.max_units_selected))
            .map(|s| s.template)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, MemorySources, Reader};

    const TEXT: &str = "FormationAssistant
    RowPadding = 10.0
    ColumnPadding = 10.0
    ActivationDragDistance = 0.0
    UnitDefinition Inf
        ObjectFilter = NONE +INFANTRY -HERO -ARCHER
    End
    UnitDefinition Cav
        ObjectFilter = NONE +CAVALRY -HERO
    End
    UnitDefinition Hero
        ObjectFilter = NONE +HERO
    End
    UnitDefinition Archer
        ObjectFilter = NONE +ARCHER -CAVALRY
    End
    FormationTemplate 1Wide
        Rows
            Row = Cav|Inf|Hero
            Row = Archer
        End
    End
    FormationTemplate 4Wide
        Rows
            Row = Cav|Inf|Hero Inf|Cav|Hero Inf|Cav|Hero Cav|Inf|Hero
            Row = Archer Archer Archer Archer
        End
    End
    FormationSelection 1Wide
        MaxDragLength = 40.0
    End
    FormationSelection 4Wide
        MaxDragLength = 200.0
    End
End
";

    fn assistant_text(text: &str) -> Result<FormationAssistant, String> {
        let mut sources = MemorySources::new();
        sources.insert("data/ini/formationassistant.ini", text.as_bytes().to_vec());
        let document = Reader::new(&sources)
            .read(
                "data/ini/formationassistant.ini",
                &BlockTable::corpus_path(),
            )
            .unwrap();
        FormationAssistant::find(document.roots()).unwrap()
    }

    fn assistant() -> FormationAssistant {
        assistant_text(TEXT).unwrap()
    }

    #[test]
    fn reads_definitions_templates_and_selections_sorted_by_drag_length() {
        let a = assistant();
        assert_eq!(a.definitions.len(), 4);
        assert_eq!(a.templates.len(), 2);
        assert_eq!(
            a.selections.iter().map(|s| s.template).collect::<Vec<_>>(),
            vec![0, 1],
            "1Wide (40) before 4Wide (200)"
        );
        assert_eq!(
            a.classify(&["INFANTRY".into(), "ARCHER".into()], "X"),
            Some(3)
        );
    }

    #[test]
    fn last_row_of_class_is_the_bottom_most_row_it_appears_in() {
        let a = assistant();
        let archer = a.definitions.iter().position(|d| d.name == "Archer");
        assert_eq!(a.templates[1].last_row_of_class[archer.unwrap()], Some(1));
    }

    #[test]
    fn template_choice_follows_drag_length_lower_bound() {
        let a = assistant();
        assert_eq!(a.choose_template(10.0, 5), Some(0), "short drag: 1Wide");
        assert_eq!(a.choose_template(100.0, 5), Some(1), "past 1Wide: 4Wide");
        assert_eq!(
            a.choose_template(300.0, 5),
            Some(1),
            "past every entry: the last"
        );
    }

    #[test]
    fn a_row_with_a_different_cell_count_is_a_load_error() {
        let text = TEXT.replace("Row = Archer Archer Archer Archer", "Row = Archer Archer");
        let err = assistant_text(&text).unwrap_err();
        assert!(err.contains("cells"), "{err}");
    }

    #[test]
    fn an_undefined_unit_type_in_a_row_is_a_load_error() {
        let text = TEXT.replace("Row = Archer\n", "Row = Ranger\n");
        let err = assistant_text(&text).unwrap_err();
        assert!(err.contains("Ranger"), "{err}");
    }

    #[test]
    fn a_selection_naming_an_undefined_template_is_a_load_error() {
        let text = TEXT.replace("FormationSelection 4Wide", "FormationSelection 8Wide");
        let err = assistant_text(&text).unwrap_err();
        assert!(err.contains("8Wide"), "{err}");
    }

    #[test]
    fn a_duplicate_template_name_is_a_load_error() {
        let text = TEXT.replacen("FormationTemplate 4Wide", "FormationTemplate 1Wide", 1);
        let err = assistant_text(&text).unwrap_err();
        assert!(err.contains("duplicate"), "{err}");
    }
}
