//! Attribute checks: unknown, missing and invalid attributes of one element.

use super::{MC_NS, Sink, ViolationKind, XML_NS, key};
use crate::schema::scope::{Ignorable, is_skipped};
use crate::schema::tables::{self, display_name, display_qname};
use crate::schema::tree::Attr;
use crate::schema::value;

pub(super) struct AttrCtx<'a, N> {
    pub(super) node: N,
    pub(super) decl: u16,
    pub(super) parent_label: &'a str,
    pub(super) chain: &'a [Ignorable],
    pub(super) ign: Option<usize>,
    pub(super) strict: bool,
}

pub(super) fn check_attributes<N: Copy>(
    ctx: &AttrCtx<'_, N>,
    attrs: &[Attr<'_>],
    sink: &mut Sink<N>,
) {
    let mut present: Vec<u16> = Vec::new();
    for a in attrs {
        if a.namespace == MC_NS || a.namespace == XML_NS {
            continue;
        }
        let name = tables::ns_by_uri(a.namespace).and_then(|n| tables::name_id(n, a.local));
        match name.and_then(|n| tables::find_attr(ctx.decl, n)) {
            Some(declared) => {
                present.push(declared.name);
                if !ctx.strict && !value::accepts(declared.spec, &a.value) {
                    sink.push(
                        key(
                            ViolationKind::InvalidAttributeValue,
                            ctx.parent_label,
                            display_qname(a.namespace, a.local),
                            clip(&a.value),
                        ),
                        ctx.node,
                        format!("{:?} is not a valid value", clip(&a.value)),
                    );
                }
            }
            None => {
                if is_skipped(ctx.chain, ctx.ign, a.namespace, a.local) {
                    continue;
                }
                sink.push(
                    key(
                        ViolationKind::UnknownAttribute,
                        ctx.parent_label,
                        display_qname(a.namespace, a.local),
                        String::new(),
                    ),
                    ctx.node,
                    "the schema does not define this attribute here".into(),
                );
            }
        }
    }
    for want in tables::attrs_of(ctx.decl).filter(|a| a.required) {
        if !present.contains(&want.name) {
            sink.push(
                key(
                    ViolationKind::MissingAttribute,
                    ctx.parent_label,
                    display_name(want.name),
                    String::new(),
                ),
                ctx.node,
                "required attribute is missing".into(),
            );
        }
    }
}

pub(super) fn clip(s: &str) -> String {
    s.chars().take(40).collect()
}
