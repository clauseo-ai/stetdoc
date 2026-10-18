//! The committed patch list for the SDK data.
//!
//! The Open XML SDK's schema data is the Office 2013+ validation model. It is stricter than
//! ECMA-376 in some places, omits elements that older Word still writes and current Word still
//! reads, and omits some Office extension attributes. Every rule below exists because documents
//! that Word wrote (paragraphs carrying `w:rsidR`, Word statistics in `app.xml`) were reported
//! as violations of something ECMA-376 or the Office extension specification permits.
//!
//! Each `Rule` has a reason and a reference. The reference names the `wml.xsd` type or group
//! of ECMA-376 Transitional (Part 4 of the 5th edition ships the schemas; Part 1 describes the
//! elements) and, for the Office extensions, the MS-DOCX element. The XSD itself is not part of
//! the repository: it was used only as a second opinion while finding these. An optional local
//! oracle may fetch it by pinned hash into `target/`; CI does not run it.
//!
//! Applying the rules changes the generated tables and nothing else. The generator prints how
//! often each fired in the header of `generated/mod.rs`, so a rule that stops matching after an
//! input update is noticed.

mod rules;

use super::model::{AttrDef, Model, Namespace, ONCE, Occurs, Particle, TypeDef, Validator};
use rules::{Patch, Rule, SMART, rules};
use std::collections::BTreeMap;

pub use rules::{attr_overrides, canonical_order_types};

fn string_attr(qname: &str, required: bool) -> AttrDef {
    AttrDef {
        qname: qname.to_string(),
        ty: "StringValue".to_string(),
        validators: if required {
            vec![Validator {
                name: "RequiredValidator".into(),
                ty: None,
                union_id: None,
                version: None,
                args: BTreeMap::new(),
            }]
        } else {
            Vec::new()
        },
        file: 0,
    }
}

fn leaf(name: &str, attrs: Vec<AttrDef>) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        class: name.to_string(),
        own_attrs: attrs,
        ..TypeDef::default()
    }
}

fn elem(decl: &str, occurs: Occurs) -> Particle {
    Particle::Element {
        decl: decl.to_string(),
        occurs,
    }
}

/// Smart tags: the element types the rules refer to.
fn new_types(model: &Model) -> Vec<TypeDef> {
    let mut out = vec![
        leaf(
            "w:CT_Attr/w:attr",
            vec![
                string_attr("w:uri", false),
                string_attr("w:name", true),
                string_attr("w:val", true),
            ],
        ),
        leaf(
            "w:CT_SmartTagType/w:smartTagType",
            vec![
                string_attr("w:namespaceuri", false),
                string_attr("w:name", false),
                string_attr("w:url", false),
            ],
        ),
        TypeDef {
            name: "w:CT_SmartTagPr/w:smartTagPr".into(),
            class: "SmartTagProperties".into(),
            particle: Some(Particle::Seq {
                items: vec![elem("w:CT_Attr/w:attr", Occurs { min: 0, max: None })],
                occurs: ONCE,
            }),
            ..TypeDef::default()
        },
        leaf("w:CT_OnOff/w:doNotEmbedSmartTags", Vec::new()),
        leaf("w:CT_OnOff/w:saveSmartTagsAsXml", Vec::new()),
    ];
    // `w:smartTag` holds what `w:customXml` holds, with `smartTagPr` in place of `customXmlPr`.
    if let Some(custom) = model.types.get("w:CT_CustomXmlRun/w:customXml")
        && let Some(Particle::Seq { items, occurs }) = model.particle_of(custom).cloned()
    {
        let items = items
            .into_iter()
            .map(|p| match p {
                Particle::Element { decl, occurs } if decl == "w:CT_CustomXmlPr/w:customXmlPr" => {
                    Particle::Element {
                        decl: "w:CT_SmartTagPr/w:smartTagPr".into(),
                        occurs,
                    }
                }
                other => other,
            })
            .collect();
        out.push(TypeDef {
            name: "w:CT_SmartTagRun/w:smartTag".into(),
            class: "SmartTagRun".into(),
            own_attrs: vec![string_attr("w:uri", false), string_attr("w:element", true)],
            particle: Some(Particle::Seq { items, occurs }),
            ..TypeDef::default()
        });
    }
    out
}

/// How often each rule matched the pinned input.
///
/// A rule that matches nothing is an error: after an input update it would mean the SDK data
/// changed under it (the thing it corrects moved or was fixed), and a correction that silently
/// stops applying is how a false violation comes back. `require_all_hit` stops generation.
pub struct Report {
    /// Per rule, in order: what it does, how often it matched, why.
    rules: Vec<RuleHits>,
    /// The types the smart tag rules refer to. Added unconditionally; not rules of their own.
    types: Vec<String>,
}

struct RuleHits {
    what: String,
    hits: usize,
    note: String,
}

impl Report {
    fn new(rules: &[Rule]) -> Report {
        Report {
            rules: rules
                .iter()
                .map(|r| RuleHits {
                    what: r.patch.describe(),
                    hits: 0,
                    note: format!("{} [{}]", r.reason, r.reference),
                })
                .collect(),
            types: Vec::new(),
        }
    }

    /// Records one match of rule number `rule`.
    pub fn hit(&mut self, rule: usize) {
        if let Some(r) = self.rules.get_mut(rule) {
            r.hits += 1;
        }
    }

    /// The lines for the header of the generated module.
    pub fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .rules
            .iter()
            .map(|r| format!("{} ({}x): {}", r.what, r.hits, r.note))
            .collect();
        out.extend(self.types.iter().cloned());
        out
    }

    /// Fails when a rule matched nothing.
    pub fn require_all_hit(&self) -> Result<(), String> {
        let idle: Vec<String> = self
            .rules
            .iter()
            .filter(|r| r.hits == 0)
            .map(|r| format!("  {}", r.what))
            .collect();
        if idle.is_empty() {
            return Ok(());
        }
        Err(format!(
            "these corrections matched nothing in the pinned input (the SDK data changed, or the rule \
             is wrong). Fix or remove them in xtask/src/codegen/supplement/rules.rs:\n{}",
            idle.join("\n")
        ))
    }
}

fn add_alternative(p: &mut Particle, a: &str, b: &str, rule: usize, report: &mut Report) {
    match p {
        Particle::Choice { items, .. } => {
            let mut extra = Vec::new();
            for i in items.iter() {
                if let Particle::Element { decl, occurs } = i
                    && decl == a
                {
                    extra.push(Particle::Element {
                        decl: b.to_string(),
                        occurs: *occurs,
                    });
                }
            }
            if !extra.is_empty() {
                report.hit(rule);
                items.extend(extra);
            }
            for i in items.iter_mut() {
                add_alternative(i, a, b, rule, report);
            }
        }
        Particle::Seq { items, .. } | Particle::All { items, .. } => {
            for i in items.iter_mut() {
                add_alternative(i, a, b, rule, report);
            }
        }
        Particle::Element { .. } | Particle::Any { .. } => {}
    }
}

fn insert_after(p: &mut Particle, after: &str, new: &str, occurs: Occurs) -> bool {
    match p {
        Particle::Seq { items, .. } => {
            if let Some(pos) = items
                .iter()
                .position(|i| matches!(i, Particle::Element { decl, .. } if decl == after))
            {
                items.insert(
                    pos + 1,
                    Particle::Element {
                        decl: new.to_string(),
                        occurs,
                    },
                );
                return true;
            }
            items
                .iter_mut()
                .any(|i| insert_after(i, after, new, occurs))
        }
        Particle::Choice { items, .. } | Particle::All { items, .. } => items
            .iter_mut()
            .any(|i| insert_after(i, after, new, occurs)),
        Particle::Element { .. } | Particle::Any { .. } => false,
    }
}

/// Applies the model-level rules (everything except the canonical-order flag and the value-type
/// replacements, which the generator applies where it builds those tables and reports through
/// [`Report::hit`]).
pub fn apply(model: &mut Model) -> Report {
    let list = rules();
    let mut report = Report::new(&list);
    for t in new_types(model) {
        report.types.push(format!(
            "add type {} (1x): types the smart tag rules refer to [{SMART}]",
            t.name
        ));
        model.types.insert(t.name.clone(), t);
    }
    for (i, rule) in list.iter().enumerate() {
        match &rule.patch {
            Patch::AliasModel { ty, source } => {
                let src = model
                    .types
                    .get(*source)
                    .and_then(|t| model.particle_of(t))
                    .cloned();
                let Some(src) = src else { continue };
                for (name, t) in model.types.iter_mut() {
                    if name.starts_with(&format!("{ty}/")) {
                        t.particle = Some(src.clone());
                        report.hit(i);
                    }
                }
            }
            Patch::AddAlternative { beside, new } => {
                for t in model.types.values_mut() {
                    if let Some(p) = t.particle.as_mut() {
                        add_alternative(p, beside, new, i, &mut report);
                    }
                }
            }
            Patch::InsertAfter {
                container,
                after,
                new,
                occurs,
            } => {
                for (name, t) in model.types.iter_mut() {
                    if name.starts_with(&format!("{container}/"))
                        && let Some(p) = t.particle.as_mut()
                        && insert_after(p, after, new, *occurs)
                    {
                        report.hit(i);
                    }
                }
            }
            Patch::RelaxRequired { ty, attr } => {
                for (name, t) in model.types.iter_mut() {
                    if name.starts_with(&format!("{ty}/")) {
                        for a in t.own_attrs.iter_mut().filter(|a| a.qname == *attr) {
                            let before = a.validators.len();
                            a.validators.retain(|v| v.name != "RequiredValidator");
                            if a.validators.len() != before {
                                report.hit(i);
                            }
                        }
                    }
                }
            }
            Patch::EnumExtra { ty, values } => {
                for e in model.enums.iter_mut().filter(|e| e.ty == *ty) {
                    e.values.extend(values.iter().map(|s| s.to_string()));
                    e.values.sort();
                    e.values.dedup();
                    report.hit(i);
                }
            }
            Patch::AddAttr {
                ty,
                attr,
                value_type,
            } => {
                for (name, t) in model.types.iter_mut() {
                    if name.starts_with(&format!("{ty}/"))
                        && !t.own_attrs.iter().any(|a| a.qname == *attr)
                    {
                        t.own_attrs.push(AttrDef {
                            qname: attr.to_string(),
                            ty: value_type.to_string(),
                            validators: Vec::new(),
                            file: 0,
                        });
                        report.hit(i);
                    }
                }
            }
            Patch::AddNamespace { prefix, uri } => {
                if !model.namespaces.iter().any(|n| n.uri == *uri) {
                    model.namespaces.push(Namespace {
                        prefix: prefix.to_string(),
                        uri: uri.to_string(),
                        version: Some("Office2016".to_string()),
                    });
                    report.hit(i);
                }
            }
            Patch::GroupsOnce { ty } => {
                for (name, t) in model.types.iter_mut() {
                    if name.starts_with(&format!("{ty}/"))
                        && let Some(Particle::Seq { items, .. }) = t.particle.as_mut()
                    {
                        for item in items.iter_mut() {
                            if let Particle::Seq { occurs, .. } | Particle::Choice { occurs, .. } =
                                item
                                && occurs.max.is_none()
                            {
                                occurs.max = Some(1);
                                report.hit(i);
                            }
                        }
                    }
                }
            }
            Patch::CanonicalOrder { .. } | Patch::AttrType { .. } => {}
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(patch: Patch) -> Rule {
        Rule {
            patch,
            reason: "because",
            reference: "wml.xsd",
        }
    }

    #[test]
    fn a_rule_that_matches_nothing_stops_generation() {
        let list = vec![
            rule(Patch::RelaxRequired {
                ty: "w:CT_Gone",
                attr: "w:val",
            }),
            rule(Patch::CanonicalOrder { ty: "w:CT_RPr" }),
        ];
        let mut report = Report::new(&list);
        report.hit(1);
        let err = report.require_all_hit().expect_err("rule 0 never matched");
        assert!(err.contains("w:CT_Gone w:val is not required"), "{err}");
        assert!(!err.contains("canonical"), "{err}");
        report.hit(0);
        assert!(report.require_all_hit().is_ok());
        // every rule is in the header, with its count and its reason
        let lines = report.lines();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with("(1x): because [wml.xsd]"), "{lines:?}");
    }

    #[test]
    fn every_committed_rule_has_a_reason_and_a_reference() {
        for r in rules() {
            assert!(r.reason.len() > 10, "{}", r.patch.describe());
            assert!(
                ["wml.xsd", "MS-DOCX", "MS-ODRAWXML"]
                    .iter()
                    .any(|s| r.reference.contains(s)),
                "{}",
                r.patch.describe()
            );
        }
    }
}
