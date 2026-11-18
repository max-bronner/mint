use mint_sage_ini::tables::structure::{self, Verified};
use mint_sage_ini::tables::{Primitive, BLOCKS};

fn primitive_label(p: Primitive) -> String {
    match p {
        Primitive::Int => "Int".to_owned(),
        Primitive::Real => "Real".to_owned(),
        Primitive::Percent => "Percent".to_owned(),
        Primitive::Bool => "Bool".to_owned(),
        Primitive::DurationMs => "DurationMs".to_owned(),
        Primitive::AsciiString => "AsciiString".to_owned(),
        Primitive::IdentifierList => "IdentifierList".to_owned(),
        Primitive::ObjectFilter => "ObjectFilter".to_owned(),
        Primitive::BitFlags(names) => format!("BitFlags({})", names.len()),
        Primitive::Block(name) => format!("Block({name})"),
    }
}

fn main() {
    for block in BLOCKS {
        println!("BLOCK\t{}", block.name);
        for field in block.field_tokens() {
            println!(
                "FIELD\t{}\t{}\t{}",
                block.name,
                field.token,
                primitive_label(field.primitive)
            );
        }
    }
    for row in structure::openers() {
        let (kind, detail) = match row.verified {
            Verified::FileScopeRegistry => ("registry", "-"),
            Verified::Document(group) => ("document", group),
        };
        println!(
            "OPENER\t{}\t{}\t{}\t{kind}\t{detail}",
            row.context,
            row.keyword,
            i32::from(row.opens)
        );
    }
    for c in structure::closed_contexts() {
        println!("CLOSED\t{}\t{}", c.context, c.group);
    }
    for a in structure::aliases() {
        println!("ALIAS\t{}\t{}", a.from, a.to);
    }
}
