//! Builders for the styles tests: a cascade from style XML, and direct properties from a run's XML.

use super::cascade::Cascade;
use crate::wordml::testing::{document, parse, part};
use crate::wordml::{PropsRef, SpanTree, StyleDefs, TableSite, TblLook, VMerge};
use crate::xml::NodeRef;

/// A cascade from the inside of a `w:styles` element.
pub(super) fn cascade(styles: &str) -> Cascade {
    cascade_with(styles, None, None)
}

/// A cascade with a theme part and a settings part, given as XML text.
pub(super) fn cascade_with(styles: &str, theme: Option<&str>, settings: Option<&str>) -> Cascade {
    let tree = part("word/styles.xml", "styles", styles);
    let theme = theme.map(|t| parse("word/theme/theme1.xml", t));
    let settings = settings.map(|s| part("word/settings.xml", "settings", s));
    Cascade::build(&StyleDefs::read(&tree), theme.as_ref(), settings.as_ref())
}

/// A document tree whose body holds `body`.
pub(super) fn tree(body: &str) -> SpanTree {
    document(body)
}

/// The first element named `name` below `node`, in document order.
pub(super) fn first<'a>(node: NodeRef<'a>, name: &str) -> Option<NodeRef<'a>> {
    for c in node.child_elements() {
        if c.local_name() == name {
            return Some(c);
        }
        if let Some(found) = first(c, name) {
            return Some(found);
        }
    }
    None
}

/// The direct properties of the first `w:rPr` or `w:pPr` in `tree`.
pub(super) fn props<'a>(tree: &'a SpanTree, container: &str) -> Option<PropsRef<'a>> {
    first(tree.root(), container).map(PropsRef::new)
}

/// A `w:style` element.
pub(super) fn style(kind: &str, id: &str, based_on: Option<&str>, inner: &str) -> String {
    let based = based_on.map_or(String::new(), |b| format!("<w:basedOn w:val=\"{b}\"/>"));
    format!("<w:style w:type=\"{kind}\" w:styleId=\"{id}\">{based}{inner}</w:style>")
}

/// The default paragraph style and the default character style, with no formatting.
pub(super) fn defaults() -> String {
    format!(
        "{}{}",
        "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"/>",
        "<w:style w:type=\"character\" w:default=\"1\" w:styleId=\"DefaultParagraphFont\"/>"
    )
}

/// A table site with the given look.
pub(super) fn site(
    style_id: Option<&str>,
    look: TblLook,
    row: usize,
    row_count: usize,
    column: usize,
    column_count: usize,
) -> TableSite {
    TableSite {
        style_id: style_id.map(Into::into),
        look,
        row,
        row_count,
        column,
        column_count,
        span: 1,
        v_merge: VMerge::None,
        row_cnf: None,
        cell_cnf: None,
    }
}

/// A look with the first row and the first column on, and banding on.
pub(super) fn look(first_row: bool, last_row: bool, first_col: bool, last_col: bool) -> TblLook {
    TblLook {
        first_row,
        last_row,
        first_column: first_col,
        last_column: last_col,
        no_h_band: false,
        no_v_band: false,
    }
}
