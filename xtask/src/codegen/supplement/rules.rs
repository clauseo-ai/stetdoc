//! The rules themselves: what is corrected in the SDK data, why, and where it is specified.

use crate::codegen::model::Occurs;
use crate::codegen::values::Pred;

/// One correction.
pub enum Patch {
    /// Wherever `beside` is one alternative of a choice, `new` is another.
    AddAlternative {
        beside: &'static str,
        new: &'static str,
    },
    /// In the sequence that directly contains `after`, add `new` right after it.
    InsertAfter {
        container: &'static str,
        after: &'static str,
        new: &'static str,
        occurs: Occurs,
    },
    /// Elements of `ty` take the content model of `source`.
    AliasModel {
        ty: &'static str,
        source: &'static str,
    },
    /// The schema leaves the children of `ty` unordered; the generated sequence is Word's order.
    CanonicalOrder { ty: &'static str },
    /// The SDK repeats the top-level groups of `ty` without bound, which accepts its children in any
    /// order and any number; each occurs at most once, so the canonical order is checked.
    GroupsOnce { ty: &'static str },
    /// `attr` of `ty` is not required.
    RelaxRequired {
        ty: &'static str,
        attr: &'static str,
    },
    /// The enumeration `ty` has more values.
    EnumExtra {
        ty: &'static str,
        values: &'static [&'static str],
    },
    /// The value type of `attr` of `ty` is replaced: an OR of ANDs of predicates.
    AttrType {
        ty: &'static str,
        attr: &'static str,
        alternatives: Vec<Vec<Pred>>,
    },
    /// `ty` gains an optional attribute of an Office extension namespace.
    AddAttr {
        ty: &'static str,
        attr: &'static str,
        value_type: &'static str,
    },
    /// The tables know the Office extension namespace `uri`, with `prefix` as the one to declare.
    /// It has no content model; knowing it is what makes a `mc:Choice` that requires it a branch
    /// Word reads.
    AddNamespace {
        prefix: &'static str,
        uri: &'static str,
    },
}

/// A patch with its justification.
pub struct Rule {
    pub patch: Patch,
    pub reason: &'static str,
    pub reference: &'static str,
}

pub(super) const SMART: &str = "ECMA-376 Part 1, 17.5.1 (smart tag elements); wml.xsd CT_SmartTagRun, CT_SmartTagPr, CT_Attr, \
                     CT_SmartTagType, group EG_ContentRunContent";

/// The rules, in the order they are applied.
pub fn rules() -> Vec<Rule> {
    let once = Occurs {
        min: 0,
        max: Some(1),
    };
    vec![
        Rule {
            patch: Patch::AddAlternative {
                beside: "w:CT_CustomXmlRun/w:customXml",
                new: "w:CT_SmartTagRun/w:smartTag",
            },
            reason: "The SDK dropped smart tags with Office 2013. Word 2003 and 2007 documents contain them \
                     and current Word reads them: 43 Word-authored corpus documents, 393 elements.",
            reference: SMART,
        },
        Rule {
            patch: Patch::InsertAfter {
                container: "w:CT_Settings",
                after: "w:CT_ReadingModeInkLockDown/w:readModeInkLockDown",
                new: "w:CT_SmartTagType/w:smartTagType",
                occurs: Occurs { min: 0, max: None },
            },
            reason: "Settings that go with smart tags; 46 Word-authored documents.",
            reference: "ECMA-376 Part 1, 17.15.1 (settings); wml.xsd CT_Settings, CT_SmartTagType",
        },
        Rule {
            patch: Patch::InsertAfter {
                container: "w:CT_Settings",
                after: "w:CT_ShapeDefaults/w:shapeDefaults",
                new: "w:CT_OnOff/w:doNotEmbedSmartTags",
                occurs: once,
            },
            reason: "Same; 64 Word-authored documents.",
            reference: "ECMA-376 Part 1, 17.15.1 (settings); wml.xsd CT_Settings",
        },
        Rule {
            patch: Patch::InsertAfter {
                container: "w:CT_WebSettings",
                after: "w:CT_TargetScreenSz/w:targetScreenSz",
                new: "w:CT_OnOff/w:saveSmartTagsAsXml",
                occurs: once,
            },
            reason: "Same, in web settings.",
            reference: "ECMA-376 Part 1, 17.11 (web settings); wml.xsd CT_WebSettings",
        },
        Rule {
            patch: Patch::AddAlternative {
                beside: "w:CT_Drawing/w:drawing",
                new: "w16se:CT_SymEx/w16se:symEx",
            },
            reason: "Word 2016+ writes w16se:symEx in runs; the SDK defines the element and does not list it \
                     in the run content model (4 Word-authored documents).",
            reference: "MS-DOCX, w16se:symEx (Office 2016 symbol extension)",
        },
        Rule {
            patch: Patch::AddAttr {
                ty: "w:CT_Num",
                attr: "w16cid:durableId",
                value_type: "Int32Value",
            },
            reason: "Word 2019+ writes w16cid:durableId on every w:num. The SDK has w:durableId, which no \
                     document uses (565 Word-authored documents, 8,323 elements).",
            reference: "MS-DOCX, w16cid:durableId (Office 2019 durable ids)",
        },
        Rule {
            patch: Patch::AttrType {
                ty: "w16se:CT_SymEx",
                attr: "w16se:char",
                alternatives: vec![vec![Pred::Pattern(1), Pred::Len(1, 8)]],
            },
            reason: "The SDK allows two hex bytes; Word writes five hex digits for symbols outside the Basic \
                     Multilingual Plane (emoji, 4 documents).",
            reference: "MS-DOCX, w16se:symEx, attribute char",
        },
        Rule {
            patch: Patch::AliasModel {
                ty: "w:CT_RPrStyle",
                source: "w:CT_RPr/w:rPr",
            },
            reason: "The SDK keeps Word 2007 restricted copies of the run property type for styles and \
                     numbering levels. Word writes w:rtl in styles (3 documents) and w:highlight in \
                     numbering levels (8).",
            reference: "wml.xsd CT_Style/rPr and CT_Lvl/rPr are CT_RPr",
        },
        Rule {
            patch: Patch::AliasModel {
                ty: "w:CT_RPrList",
                source: "w:CT_RPr/w:rPr",
            },
            reason: "As above, numbering levels.",
            reference: "wml.xsd CT_Lvl/rPr is CT_RPr",
        },
        Rule {
            patch: Patch::AliasModel {
                ty: "w:CT_RPrBaseStyleable",
                source: "w:CT_RPr/w:rPr",
            },
            reason: "As above: the same restricted copy, used for run properties in w:rPrDefault and \
                     w:tblStylePr.",
            reference: "wml.xsd CT_RPrDefault and CT_TblStylePr use CT_RPr",
        },
        Rule {
            patch: Patch::AliasModel {
                ty: "w:CT_TrPrBaseStyleable",
                source: "w:CT_TrPrBase/w:trPr",
            },
            reason: "As above, for row properties in w:style and w:tblStylePr.",
            reference: "wml.xsd CT_TrPrBase",
        },
        Rule {
            patch: Patch::CanonicalOrder { ty: "w:CT_RPr" },
            reason: "The schema makes the children of rPr an unbounded choice. Word writes them in the \
                     order the SDK gives. A departure is reported as non-canonical order.",
            reference: "wml.xsd group EG_RPrBase (xsd:choice, maxOccurs unbounded), EG_RPrContent",
        },
        Rule {
            patch: Patch::CanonicalOrder { ty: "w:CT_ParaRPr" },
            reason: "As above, paragraph mark run properties.",
            reference: "wml.xsd EG_RPrBase",
        },
        Rule {
            patch: Patch::CanonicalOrder {
                ty: "w:CT_RPrOriginal",
            },
            reason: "As above, the old properties in rPrChange.",
            reference: "wml.xsd EG_RPrBase",
        },
        Rule {
            patch: Patch::CanonicalOrder {
                ty: "w:CT_ParaRPrOriginal",
            },
            reason: "As above, the old paragraph mark properties in a paragraph mark rPrChange.",
            reference: "wml.xsd EG_RPrBase",
        },
        Rule {
            patch: Patch::GroupsOnce {
                ty: "w:CT_RPrOriginal",
            },
            reason: "The SDK repeats the run property group of the old properties in rPrChange without bound, \
                     where w:CT_RPr has it once, so the canonical order above checked nothing: a property out of \
                     order or twice in a snapshot went unseen, and rejecting the change, which puts it back in the \
                     run, looked like adding it (tdf168070).",
            reference: "wml.xsd CT_RPrOriginal, group EG_RPrBase",
        },
        Rule {
            patch: Patch::GroupsOnce {
                ty: "w:CT_ParaRPrOriginal",
            },
            reason: "As above, the old paragraph mark properties in a paragraph mark rPrChange.",
            reference: "wml.xsd CT_ParaRPrOriginal, group EG_RPrBase",
        },
        Rule {
            patch: Patch::CanonicalOrder {
                ty: "w:CT_RPrStyle",
            },
            reason: "As above, for the aliased style types.",
            reference: "wml.xsd EG_RPrBase",
        },
        Rule {
            patch: Patch::CanonicalOrder { ty: "w:CT_RPrList" },
            reason: "As above, run properties in numbering levels.",
            reference: "wml.xsd EG_RPrBase",
        },
        Rule {
            patch: Patch::CanonicalOrder {
                ty: "w:CT_RPrBaseStyleable",
            },
            reason: "As above, run properties in w:rPrDefault and w:tblStylePr.",
            reference: "wml.xsd EG_RPrBase",
        },
        Rule {
            patch: Patch::CanonicalOrder {
                ty: "w:CT_TrPrBase",
            },
            reason: "trPr is an unbounded choice in the schema. The SDK already models it as repeating groups, \
                     so no order is enforced; the flag keeps the rule uniform.",
            reference: "wml.xsd CT_TrPrBase",
        },
        Rule {
            patch: Patch::CanonicalOrder { ty: "w:CT_TrPr" },
            reason: "As above, row properties of a table row (w:trPr in w:tr).",
            reference: "wml.xsd CT_TrPr",
        },
        Rule {
            patch: Patch::RelaxRequired {
                ty: "w:CT_Cnf",
                attr: "w:val",
            },
            reason: "The schema marks it optional.",
            reference: "wml.xsd CT_Cnf",
        },
        Rule {
            patch: Patch::RelaxRequired {
                ty: "w:CT_CompatSetting",
                attr: "w:name",
            },
            reason: "The schema marks it optional.",
            reference: "wml.xsd CT_CompatSetting",
        },
        Rule {
            patch: Patch::RelaxRequired {
                ty: "w:CT_CompatSetting",
                attr: "w:uri",
            },
            reason: "The schema marks it optional.",
            reference: "wml.xsd CT_CompatSetting",
        },
        Rule {
            patch: Patch::RelaxRequired {
                ty: "w:CT_CompatSetting",
                attr: "w:val",
            },
            reason: "The schema marks it optional.",
            reference: "wml.xsd CT_CompatSetting",
        },
        Rule {
            patch: Patch::RelaxRequired {
                ty: "w:CT_SectType",
                attr: "w:val",
            },
            reason: "The schema marks it optional.",
            reference: "wml.xsd CT_SectType",
        },
        Rule {
            patch: Patch::EnumExtra {
                ty: "w:ST_LevelJustification",
                values: &["start", "end"],
            },
            reason: "The SDK's level justification stops at left and right; Word 2013+ writes start and end.",
            reference: "wml.xsd ST_Jc (CT_Lvl/lvlJc is CT_Jc)",
        },
        Rule {
            patch: Patch::EnumExtra {
                ty: "w:ST_TblJc",
                values: &["start", "end"],
            },
            reason: "As above, table justification.",
            reference: "wml.xsd ST_JcTable",
        },
        Rule {
            patch: Patch::AttrType {
                ty: "w:CT_OnOffOnly",
                attr: "w:val",
                alternatives: vec![vec![Pred::OnOff]],
            },
            reason: "The SDK types 19 elements as on/off only (CT_OnOffOnly: autoRedefine, bidiVisual, \
                     cantSplit, flatBorders, hidden, hideMark, linkedToFile, locked, noBorder, \
                     noResizeAllowed, noWrap, personal, personalCompose, personalReply, qFormat, \
                     semiHidden, tblHeader, tcFitText, unhideWhenUsed). The XSD types every one of them \
                     CT_OnOff. Word wrote true, 1 and 0 on w:cantSplit, w:tblHeader and w:bidiVisual \
                     (4 documents).",
            reference: "wml.xsd CT_OnOff, ST_OnOff",
        },
        Rule {
            patch: Patch::AttrType {
                ty: "w:CT_TextScale",
                attr: "w:val",
                alternatives: vec![vec![Pred::Int(0, 600)], vec![Pred::Pattern(7)]],
            },
            reason: "ST_TextScale starts at 0; the SDK at 1. Word wrote w:w w:val=\"0\" in 13 documents.",
            reference: "wml.xsd ST_TextScale, ST_TextScaleDecimal, ST_TextScalePercent",
        },
        Rule {
            patch: Patch::AddNamespace {
                prefix: "cx1",
                uri: "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex",
            },
            reason: "Word 2016 and later write mc:Choice Requires=\"cx1\" around a chart that Word 2013 \
                     cannot draw, with a picture of it as the fallback. The SDK lists only the 2014 \
                     chartex namespace. A choice that requires a namespace the tables lack is not \
                     taken, so Word's own chart would read as its picture: 24 choices in 10 \
                     Word-authored corpus documents.",
            reference: "MS-ODRAWXML, chart extension namespaces; ECMA-376 Part 3, mc:Choice and Requires",
        },
        Rule {
            patch: Patch::AddNamespace {
                prefix: "cx2",
                uri: "http://schemas.microsoft.com/office/drawing/2015/10/21/chartex",
            },
            reason: "As above, the chart extension of the next Word 2016 build: 2 choices in 1 \
                     Word-authored document.",
            reference: "MS-ODRAWXML, chart extension namespaces; ECMA-376 Part 3, mc:Choice and Requires",
        },
        Rule {
            patch: Patch::AddNamespace {
                prefix: "cx3",
                uri: "http://schemas.microsoft.com/office/drawing/2016/5/10/chartex",
            },
            reason: "As above, a later chart extension: 1 choice in 1 Word-authored document.",
            reference: "MS-ODRAWXML, chart extension namespaces; ECMA-376 Part 3, mc:Choice and Requires",
        },
        Rule {
            patch: Patch::AddNamespace {
                prefix: "wpi",
                uri: "http://schemas.microsoft.com/office/word/2010/wordprocessingInk",
            },
            reason: "Word 2010 and later write mc:Choice Requires=\"wpi\" around ink, with a picture as \
                     the fallback. The SDK has no namespace for it: 2 choices in 2 Word-authored \
                     corpus documents.",
            reference: "MS-DOCX, wpi (Word 2010 ink); ECMA-376 Part 3, mc:Choice and Requires",
        },
    ]
}

impl Patch {
    /// What the patch does, in a line, for the header of the generated module.
    pub fn describe(&self) -> String {
        match self {
            Patch::AddAlternative { beside, new } => {
                format!("add alternative {new} beside {beside}")
            }
            Patch::InsertAfter {
                container,
                after,
                new,
                ..
            } => format!("insert {new} after {after} in {container}"),
            Patch::AliasModel { ty, source } => format!("{ty} uses the content model of {source}"),
            Patch::CanonicalOrder { ty } => format!("{ty} has a canonical child order"),
            Patch::GroupsOnce { ty } => format!("{ty} repeats each of its groups at most once"),
            Patch::RelaxRequired { ty, attr } => format!("{ty} {attr} is not required"),
            Patch::EnumExtra { ty, values } => format!("{ty} gains {values:?}"),
            Patch::AttrType { ty, attr, .. } => format!("{ty} {attr} takes another value type"),
            Patch::AddAttr { ty, attr, .. } => format!("{ty} gains attribute {attr}"),
            Patch::AddNamespace { prefix, uri } => format!("namespace {prefix} is {uri}"),
        }
    }
}

/// The types whose children the schema leaves unordered, with the index of the rule.
pub fn canonical_order_types() -> Vec<(usize, &'static str)> {
    rules()
        .into_iter()
        .enumerate()
        .filter_map(|(i, r)| match r.patch {
            Patch::CanonicalOrder { ty } => Some((i, ty)),
            _ => None,
        })
        .collect()
}

/// A value type replacement and the index of its rule.
pub struct AttrOverride {
    pub rule: usize,
    pub ty: &'static str,
    pub attr: &'static str,
    pub alternatives: Vec<Vec<Pred>>,
}

/// Value type replacements.
pub fn attr_overrides() -> Vec<AttrOverride> {
    rules()
        .into_iter()
        .enumerate()
        .filter_map(|(rule, r)| match r.patch {
            Patch::AttrType {
                ty,
                attr,
                alternatives,
            } => Some(AttrOverride {
                rule,
                ty,
                attr,
                alternatives,
            }),
            _ => None,
        })
        .collect()
}
