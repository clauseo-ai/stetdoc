//! `schema_units` and `schema_part`: validation, over the span tree.
//!
//! `schema` reads the span tree through its own `SchemaTree` impl for `SpanTree`
//! (`schema/span.rs`), the one the engine uses.

use super::support::tree_of;
use crate::schema::{Ctx, PartKind, validate_part, validate_unit};
use crate::xml::NodeId;
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Validating the fifty paragraphs that fifty operations would touch, spread over the body: the
/// check that an edit makes of each dirty unit.
pub(super) const UNITS: Bench = Bench {
    name: "schema_units",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: units,
};

fn units(input: &Input) -> Result<Run, String> {
    let tree = tree_of(input)?;
    let body: Vec<NodeId> = {
        let root = tree.root();
        root.child_elements()
            .find(|c| c.is(W, "body"))
            .map(|b| {
                b.child_elements()
                    .filter(|c| c.is(W, "p"))
                    .map(|p| p.id())
                    .collect()
            })
            .unwrap_or_default()
    };
    if body.is_empty() {
        return Err("the body has no paragraphs".to_owned());
    }
    let step = (body.len() / 50).max(1);
    let picked: Vec<NodeId> = body.iter().step_by(step).take(50).copied().collect();
    let ctx = Ctx::from_path(PartKind::Document, &[(W, "body"), (W, "p")])
        .ok_or("the schema has no w:body/w:p")?;
    Ok(Box::new(move || {
        let mut violations = 0;
        for unit in &picked {
            violations += validate_unit(ctx, &tree, *unit).set.total();
        }
        Ok(Box::new((tree, std::hint::black_box(violations))) as Kept)
    }))
}

/// Validating the whole part, which is what `check` does and no edit needs.
pub(super) const PART: Bench = Bench {
    name: "schema_part",
    limit: Limit::None,
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: part,
};

fn part(input: &Input) -> Result<Run, String> {
    let tree = tree_of(input)?;
    Ok(Box::new(move || {
        let report = validate_part(PartKind::Document, &tree);
        let total = std::hint::black_box(report.set.total());
        Ok(Box::new((tree, total)) as Kept)
    }))
}
