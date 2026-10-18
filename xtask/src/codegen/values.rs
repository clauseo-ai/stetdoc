//! Attribute value types: SDK validators -> disjunctive normal form over a few predicate
//! kinds. The predicate kinds are interpreted by `schema/value.rs`; keep the numbers in
//! `Pred::code` in step with `PRED_*` there.

use super::model::{AttrDef, EnumDef, Model, Validator};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pred {
    /// No constraint.
    Any,
    /// Integer lexical form within `min..=max`.
    Int(i64, i64),
    /// `xsd:hexBinary` whose decoded length is within `min..=max` bytes.
    Hex(i64, i64),
    /// `true`, `false`, `on`, `off`, `0`, `1`.
    OnOff,
    /// `true`, `false`, `0`, `1`.
    Bool,
    DateTime,
    Base64,
    /// One of the values of enumeration `id`.
    Enum(i64),
    /// One of the known patterns, by id (see `PATTERNS`).
    Pattern(i64),
    /// String length (characters) within `min..=max`.
    Len(i64, i64),
}

impl Pred {
    pub fn code(&self) -> (u8, i64, i64) {
        match *self {
            Pred::Any => (0, 0, 0),
            Pred::Int(a, b) => (1, a, b),
            Pred::Hex(a, b) => (2, a, b),
            Pred::OnOff => (3, 0, 0),
            Pred::Bool => (4, 0, 0),
            Pred::DateTime => (5, 0, 0),
            Pred::Base64 => (6, 0, 0),
            Pred::Enum(a) => (7, a, 0),
            Pred::Pattern(a) => (8, a, 0),
            Pred::Len(a, b) => (9, a, b),
        }
    }
}

/// Patterns the data uses. `schema/value.rs` implements each by hand; an unknown pattern
/// stops the generator so a new one cannot be silently ignored.
pub const PATTERNS: &[&str] = &[
    "[0-9]+(\\.[0-9]+)?(mm|cm|in|pt|pc|pi)",
    "[0-9a-fA-F]*",
    "-?[0-9]+(\\.[0-9]+)?(mm|cm|in|pt|pc|pi)",
    "-?[0-9]+(\\.[0-9]+)?%",
    "\\{[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}\\}",
    "[01]*",
    "[^,]*",
    // ST_TextScalePercent
    "0*(600|([0-5]?[0-9]?[0-9]))%",
];

pub type Clause = Vec<Pred>;
pub type Spec = Vec<Clause>;

pub struct EnumTable {
    /// Enumerations the data names but that live in a namespace we do not load (DrawingML).
    /// Their attributes accept any string.
    pub missing: std::collections::BTreeSet<String>,
    /// Sorted, deduplicated value sets; the index is the enumeration id.
    pub sets: Vec<Vec<String>>,
    by_set: BTreeMap<Vec<String>, usize>,
}

impl EnumTable {
    pub fn new() -> EnumTable {
        EnumTable {
            missing: Default::default(),
            sets: Vec::new(),
            by_set: BTreeMap::new(),
        }
    }
    fn id(&mut self, e: &EnumDef) -> i64 {
        if let Some(i) = self.by_set.get(&e.values) {
            return *i as i64;
        }
        let i = self.sets.len();
        self.sets.push(e.values.clone());
        self.by_set.insert(e.values.clone(), i);
        i as i64
    }
}

fn version_rank(v: &str) -> u32 {
    match v {
        "Office2007" => 1,
        "Office2010" => 2,
        "Office2013" => 3,
        "Office2016" => 4,
        "Office2019" => 5,
        "Office2021" => 6,
        "Microsoft365" => 7,
        _ => 0,
    }
}

fn arg_i64(v: &Validator, name: &str) -> Option<i64> {
    v.args.get(name).and_then(|s| s.parse().ok())
}

fn base_pred(ty: &str, model: &Model, file: usize, enums: &mut EnumTable) -> Result<Pred, String> {
    Ok(match ty {
        "StringValue" => Pred::Any,
        "OnOffValue" => Pred::OnOff,
        "BooleanValue" => Pred::Bool,
        "DateTimeValue" => Pred::DateTime,
        "Base64BinaryValue" => Pred::Base64,
        "HexBinaryValue" => Pred::Hex(0, i64::MAX),
        "Int32Value" => Pred::Int(i64::from(i32::MIN), i64::from(i32::MAX)),
        "UInt32Value" => Pred::Int(0, i64::from(u32::MAX)),
        "Int16Value" => Pred::Int(i64::from(i16::MIN), i64::from(i16::MAX)),
        "UInt16Value" => Pred::Int(0, i64::from(u16::MAX)),
        "ByteValue" => Pred::Int(0, 255),
        "Int64Value" | "IntegerValue" => Pred::Int(i64::MIN, i64::MAX),
        "UInt64Value" => Pred::Int(0, i64::MAX),
        other => {
            let Some(name) = other
                .strip_prefix("EnumValue<")
                .and_then(|r| r.strip_suffix('>'))
            else {
                return Err(format!("unknown attribute type {other}"));
            };
            let short = name.rsplit('.').next().unwrap_or(name);
            let Some(e) = find_enum(model, |e| e.name == short, file) else {
                enums.missing.insert(short.to_string());
                return Ok(Pred::Any);
            };
            Pred::Enum(enums.id(e))
        }
    })
}

fn find_enum(model: &Model, f: impl Fn(&EnumDef) -> bool, file: usize) -> Option<&EnumDef> {
    model
        .enums
        .iter()
        .find(|e| e.file == file && f(e))
        .or_else(|| model.enums.iter().find(|e| f(e)))
}

/// Named simple types the SDK refers to but does not define in its JSON. Each is the
/// plain restriction from the ECMA-376 transitional schema.
fn named_pred(ty: &str, v: &Validator) -> Result<Option<Pred>, String> {
    Ok(Some(match ty {
        "w:ST_NonNegativeDecimalNumber"
        | "w:ST_UnsignedDecimalNumber"
        | "w:ST_TwipsMeasure_O12"
        | "w:ST_HpsMeasure_O12" => Pred::Int(0, i64::MAX),
        "w:ST_UnsignedDecimalNumberMin1" => Pred::Int(1, i64::MAX),
        "w:ST_SignedDecimalNumberMax-1" => Pred::Int(i64::MIN, -1),
        "w:ST_SignedDecimalNumberMax-2" => Pred::Int(i64::MIN, -2),
        "w:ST_DecimalNumber"
        | "xsd:integer"
        | "w:ST_SignedTwipsMeasure_O12"
        | "w:ST_SignedHpsMeasure_O12" => Pred::Int(i64::MIN, i64::MAX),
        "w:ST_HexColorRGB" => Pred::Hex(
            arg_i64(v, "Length").unwrap_or(3),
            arg_i64(v, "Length").unwrap_or(3),
        ),
        "w:ST_StylePaneSortMethods_O12" => Pred::Len(
            arg_i64(v, "Length").unwrap_or(2),
            arg_i64(v, "Length").unwrap_or(2),
        ),
        _ => return Ok(None),
    }))
}

/// Predicates from one validator (an AND of its constraints).
fn validator_preds(
    v: &Validator,
    base: &Pred,
    model: &Model,
    file: usize,
    enums: &mut EnumTable,
) -> Result<Vec<Pred>, String> {
    let mut out = Vec::new();
    match v.name.as_str() {
        "RequiredValidator" | "OfficeVersionValidator" => {}
        "EnumValidator" => {
            let ty = v.ty.as_deref().ok_or("EnumValidator without Type")?;
            let e = find_enum(model, |e| e.ty == ty, file)
                .ok_or_else(|| format!("enumeration {ty} not found"))?;
            out.push(Pred::Enum(enums.id(e)));
        }
        "NumberValidator" => {
            if let Some(ty) = &v.ty {
                match named_pred(ty, v)? {
                    Some(p) => out.push(p),
                    None => return Err(format!("unknown number type {ty}")),
                }
            } else {
                let lo = arg_i64(v, "MinInclusive")
                    .or_else(|| arg_i64(v, "MinExclusive").map(|x| x + 1));
                let hi = arg_i64(v, "MaxInclusive")
                    .or_else(|| arg_i64(v, "MaxExclusive").map(|x| x - 1));
                out.push(Pred::Int(lo.unwrap_or(i64::MIN), hi.unwrap_or(i64::MAX)));
            }
        }
        "StringValidator" => {
            if let Some(ty) = &v.ty {
                match named_pred(ty, v)? {
                    Some(p) => out.push(p),
                    None => return Err(format!("unknown string type {ty}")),
                }
            } else {
                let hex = matches!(base, Pred::Hex(..));
                let (len_lo, len_hi) = {
                    let exact = arg_i64(v, "Length");
                    (
                        exact.or_else(|| arg_i64(v, "MinLength")).unwrap_or(0),
                        exact
                            .or_else(|| arg_i64(v, "MaxLength"))
                            .unwrap_or(i64::MAX),
                    )
                };
                if let Some(p) = v.args.get("Pattern") {
                    let id = PATTERNS
                        .iter()
                        .position(|x| x == p)
                        .ok_or_else(|| format!("unknown pattern {p}"))?;
                    out.push(Pred::Pattern(id as i64));
                }
                if len_lo > 0 || len_hi != i64::MAX {
                    out.push(if hex {
                        Pred::Hex(len_lo, len_hi)
                    } else {
                        Pred::Len(len_lo, len_hi)
                    });
                }
                // IsToken and IsNcName are not enforced: Word does not reject them.
            }
        }
        other => return Err(format!("unknown validator {other}")),
    }
    Ok(out)
}

/// Value type of one attribute as an OR of AND-clauses, and whether it is required.
pub fn attr_spec(
    a: &AttrDef,
    model: &Model,
    enums: &mut EnumTable,
) -> Result<(Spec, bool), String> {
    let base = base_pred(&a.ty, model, a.file, enums)?;
    // Required: some variant requires it and none relaxes it.
    let mut required = false;
    let mut relaxed = false;
    for v in a
        .validators
        .iter()
        .filter(|v| v.name == "RequiredValidator")
    {
        if v.args.get("IsRequired").map(String::as_str) == Some("False") {
            relaxed = true;
        } else {
            required = true;
        }
    }
    let required = required && !relaxed;

    // Group validators by SDK version. A document valid under any version's rules is
    // accepted, so the clauses of all versions are alternatives.
    let mut groups: BTreeMap<u32, Vec<&Validator>> = BTreeMap::new();
    let common: Vec<&Validator> = a
        .validators
        .iter()
        .filter(|v| v.version.is_none())
        .collect();
    for v in a.validators.iter().filter(|v| v.version.is_some()) {
        groups
            .entry(version_rank(v.version.as_deref().unwrap_or("")))
            .or_default()
            .push(v);
    }
    if groups.is_empty() {
        groups.insert(0, Vec::new());
    }
    let mut spec: Spec = Vec::new();
    for (_, versioned) in groups {
        let members: Vec<&Validator> = common.iter().copied().chain(versioned).collect();
        // AND across validators, OR within one UnionId.
        let has_union = members.iter().any(|v| v.union_id.is_some());
        // With a union the members state the allowed forms, so the base kind is dropped.
        let mut clauses: Spec = vec![if has_union { Vec::new() } else { vec![base] }];
        let mut unions: BTreeMap<i64, Vec<&Validator>> = BTreeMap::new();
        for v in &members {
            match v.union_id {
                Some(u) => unions.entry(u).or_default().push(v),
                None => {
                    let preds = validator_preds(v, &base, model, a.file, enums)?;
                    for c in &mut clauses {
                        c.extend(preds.iter().copied());
                    }
                }
            }
        }
        for (_, alts) in unions {
            let mut alt_preds: Vec<Vec<Pred>> = Vec::new();
            for v in alts {
                alt_preds.push(validator_preds(v, &base, model, a.file, enums)?);
            }
            // A union member replaces the base constraint (base is the loosest kind).
            let mut next: Spec = Vec::new();
            for c in &clauses {
                for ap in &alt_preds {
                    let mut n = c.clone();
                    n.extend(ap.iter().copied());
                    next.push(n);
                }
            }
            clauses = next;
        }
        spec.extend(clauses);
    }
    // Simplify: drop `Any`, sort within clauses, dedup, and let an empty clause win.
    let mut norm: Spec = Vec::new();
    for mut c in spec {
        c.retain(|p| *p != Pred::Any);
        c.sort();
        c.dedup();
        if !norm.contains(&c) {
            norm.push(c);
        }
    }
    if norm.iter().any(Vec::is_empty) {
        norm = vec![Vec::new()];
    }
    norm.sort();
    Ok((norm, required))
}
