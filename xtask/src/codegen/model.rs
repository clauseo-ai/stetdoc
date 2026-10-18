//! Reads the SDK's per-namespace schema JSON into plain Rust structures and resolves
//! inheritance (a derived element takes the attributes and particle of its base type).

use super::input::Input;
use serde_json::Value;
use std::collections::BTreeMap;

/// Particle occurrence bounds. `max == None` is unbounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Occurs {
    pub min: u32,
    pub max: Option<u32>,
}

pub const ONCE: Occurs = Occurs {
    min: 1,
    max: Some(1),
};

#[derive(Clone, Debug)]
pub enum Particle {
    Element {
        decl: String,
        occurs: Occurs,
    },
    Any {
        occurs: Occurs,
    },
    Seq {
        items: Vec<Particle>,
        occurs: Occurs,
    },
    Choice {
        items: Vec<Particle>,
        occurs: Occurs,
    },
    All {
        items: Vec<Particle>,
        occurs: Occurs,
    },
}

#[derive(Clone, Debug)]
pub struct Validator {
    pub name: String,
    pub ty: Option<String>,
    pub union_id: Option<i64>,
    pub version: Option<String>,
    pub args: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct AttrDef {
    pub qname: String,
    pub ty: String,
    pub validators: Vec<Validator>,
    /// Index into `Model::files`, to resolve enum names within the same file first.
    pub file: usize,
}

#[derive(Clone, Debug, Default)]
pub struct TypeDef {
    /// `w:CT_PPr/w:pPr`; an abstract base is `w:CT_TrackChange/`.
    pub name: String,
    pub class: String,
    pub file: usize,
    pub base: Option<String>,
    pub leaf_text: bool,
    pub own_attrs: Vec<AttrDef>,
    pub particle: Option<Particle>,
}

#[derive(Clone, Debug)]
pub struct EnumDef {
    pub ty: String,
    pub name: String,
    pub file: usize,
    pub values: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Namespace {
    pub prefix: String,
    pub uri: String,
    pub version: Option<String>,
}

pub struct Model {
    pub namespaces: Vec<Namespace>,
    pub types: BTreeMap<String, TypeDef>,
    pub enums: Vec<EnumDef>,
    class_index: BTreeMap<String, Vec<String>>,
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn parse_occurs(v: &Value) -> Result<Occurs, String> {
    let Some(list) = v.get("Occurs").and_then(Value::as_array) else {
        return Ok(ONCE);
    };
    if list.is_empty() {
        return Ok(ONCE);
    }
    // Several entries are version variants of one particle. The loosest bounds win, so a
    // document valid under any Office version is never reported.
    let mut min = u32::MAX;
    let mut max: Option<u32> = Some(0);
    for o in list {
        let lo = o.get("Min").and_then(Value::as_u64).unwrap_or(0) as u32;
        let hi = o.get("Max").and_then(Value::as_u64).map(|m| m as u32);
        min = min.min(lo);
        max = match (max, hi) {
            (None, _) | (_, None) => None,
            (Some(a), Some(c)) => Some(a.max(c)),
        };
    }
    Ok(Occurs { min, max })
}

fn parse_particle(v: &Value) -> Result<Particle, String> {
    let occurs = parse_occurs(v)?;
    if let Some(name) = v.get("Name").and_then(Value::as_str) {
        return Ok(Particle::Element {
            decl: name.to_string(),
            occurs,
        });
    }
    let kind = v
        .get("Kind")
        .and_then(Value::as_str)
        .ok_or("particle without Name or Kind")?;
    if kind == "Any" {
        return Ok(Particle::Any { occurs });
    }
    let mut items = Vec::new();
    for i in v
        .get("Items")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        items.push(parse_particle(i)?);
    }
    Ok(match kind {
        "Sequence" | "Group" => Particle::Seq { items, occurs },
        "Choice" => Particle::Choice { items, occurs },
        "All" => Particle::All { items, occurs },
        other => return Err(format!("unknown particle kind {other}")),
    })
}

fn parse_validator(v: &Value) -> Validator {
    let mut args = BTreeMap::new();
    for a in v
        .get("Arguments")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let name = s(a, "Name").unwrap_or_default();
        if let Some(val) = s(a, "Value") {
            args.insert(name, val);
        }
    }
    Validator {
        name: s(v, "Name").unwrap_or_default(),
        ty: s(v, "Type"),
        union_id: v.get("UnionId").and_then(Value::as_i64),
        version: s(v, "Version"),
        args,
    }
}

impl Model {
    pub fn load(input: &Input, schema_files: &[&str]) -> Result<Model, String> {
        let namespaces = parse_namespaces(&input.json("namespaces.json")?)?;
        let mut m = Model {
            namespaces,
            types: BTreeMap::new(),
            enums: Vec::new(),
            class_index: BTreeMap::new(),
        };
        for (idx, file) in schema_files.iter().enumerate() {
            let doc = input.json(&format!("schemas/{file}.json"))?;
            for t in doc
                .get("Types")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[])
            {
                let name = s(t, "Name").ok_or("type without Name")?;
                let mut attrs = Vec::new();
                for a in t
                    .get("Attributes")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                {
                    attrs.push(AttrDef {
                        qname: s(a, "QName").ok_or("attribute without QName")?,
                        ty: s(a, "Type").unwrap_or_default(),
                        validators: a
                            .get("Validators")
                            .and_then(Value::as_array)
                            .map(Vec::as_slice)
                            .unwrap_or(&[])
                            .iter()
                            .map(parse_validator)
                            .collect(),
                        file: idx,
                    });
                }
                let def = TypeDef {
                    name: name.clone(),
                    class: s(t, "ClassName").unwrap_or_default(),
                    file: idx,
                    base: s(t, "BaseClass"),
                    leaf_text: b(t, "IsLeafText"),
                    own_attrs: attrs,
                    particle: match t.get("Particle") {
                        Some(p) => Some(parse_particle(p)?),
                        None => None,
                    },
                };
                // An abstract base is declared again in later files with a subset of its
                // content. The first declaration (the main namespace) is the complete one.
                match m.types.get(&name) {
                    Some(prev) if prev.own_attrs.len() >= def.own_attrs.len() => {}
                    _ => {
                        m.types.insert(name, def);
                    }
                }
            }
            for e in doc
                .get("Enums")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[])
            {
                let mut values: Vec<String> = e
                    .get("Facets")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|f| s(f, "Value"))
                    .collect();
                values.sort();
                values.dedup();
                m.enums.push(EnumDef {
                    ty: s(e, "Type").unwrap_or_default(),
                    name: s(e, "Name").unwrap_or_default(),
                    file: idx,
                    values,
                });
            }
        }
        for t in m.types.values() {
            m.class_index
                .entry(t.class.clone())
                .or_default()
                .push(t.name.clone());
        }
        Ok(m)
    }

    fn base_of(&self, t: &TypeDef) -> Option<&TypeDef> {
        let base = t.base.as_ref()?;
        let cands = self.class_index.get(base)?;

        cands
            .iter()
            .filter_map(|n| self.types.get(n))
            .find(|c| c.file == t.file && c.name != t.name)
            .or_else(|| {
                cands
                    .iter()
                    .filter_map(|n| self.types.get(n))
                    .find(|c| c.name != t.name)
            })
    }

    /// Attributes of `t` including everything inherited, base first, own last.
    pub fn attrs_of(&self, t: &TypeDef) -> Vec<AttrDef> {
        let mut chain = vec![t];
        let mut cur = t;
        while let Some(b) = self.base_of(cur) {
            if chain.iter().any(|c| c.name == b.name) {
                break;
            }
            chain.push(b);
            cur = b;
        }
        let mut out: Vec<AttrDef> = Vec::new();
        for c in chain.iter().rev() {
            for a in &c.own_attrs {
                if let Some(slot) = out.iter_mut().find(|x| x.qname == a.qname) {
                    *slot = a.clone();
                } else {
                    out.push(a.clone());
                }
            }
        }
        out
    }

    pub fn particle_of<'a>(&'a self, t: &'a TypeDef) -> Option<&'a Particle> {
        let mut cur = t;
        let mut hops = 0;
        loop {
            if let Some(p) = &cur.particle {
                return Some(p);
            }
            cur = self.base_of(cur)?;
            hops += 1;
            if hops > 8 {
                return None;
            }
        }
    }

    pub fn leaf_text_of(&self, t: &TypeDef) -> bool {
        let mut cur = t;
        for _ in 0..8 {
            if cur.leaf_text {
                return true;
            }
            match self.base_of(cur) {
                Some(b) => cur = b,
                None => return false,
            }
        }
        false
    }

    pub fn prefix_uri(&self, prefix: &str) -> Option<&str> {
        self.namespaces
            .iter()
            .find(|n| n.prefix == prefix)
            .map(|n| n.uri.as_str())
    }
}

fn parse_namespaces(v: &Value) -> Result<Vec<Namespace>, String> {
    let mut out = Vec::new();
    for n in v.as_array().ok_or("namespaces.json is not an array")? {
        let prefix = s(n, "Prefix").unwrap_or_default();
        let uri = s(n, "Uri").unwrap_or_default();
        if prefix.is_empty() || uri.is_empty() {
            continue;
        }
        out.push(Namespace {
            prefix,
            uri,
            version: s(n, "Version"),
        });
    }
    Ok(out)
}

/// `w:CT_P/w:p` -> `w:p`; `None` for an abstract base (`w:CT_Markup/`).
pub fn element_qname(decl: &str) -> Option<&str> {
    let (_, q) = decl.split_once('/')?;
    if q.is_empty() { None } else { Some(q) }
}

pub fn split_qname(q: &str) -> (&str, &str) {
    q.split_once(':').unwrap_or(("", q))
}
