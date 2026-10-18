//! `cargo xtask codegen`: generates `crates/stetdoc-core/src/schema/generated/` from the
//! pinned Open XML SDK schema data.
//!
//! The output depends only on the pinned input and on this code: iteration is over sorted
//! maps, there are no timestamps and no environment reads, so two runs give identical bytes.

mod automaton;
mod command;
mod emit;
mod input;
mod model;
mod paths;
mod spec;
mod supplement;
mod values;

use automaton::{Dfa, NONE, compile};
use model::{Model, Particle, TypeDef, element_qname, split_qname};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use values::{EnumTable, Spec, attr_spec};

pub use command::{check, run};
pub use input::Input;

/// Everything the emitter prints.
pub struct Ir {
    pub supplements: Vec<String>,
    pub commit: String,
    pub digest: String,
    pub namespaces: Vec<NsRow>,
    pub strict_aliases: Vec<(String, usize)>,
    /// Namespaces Word reads as another that are not Strict, with the index of that other.
    pub preview_aliases: Vec<(String, usize)>,
    /// Element and attribute names, sorted by (namespace index, local name).
    pub names: Vec<(usize, String)>,
    pub decls: Vec<DeclRow>,
    pub models: Vec<ModelRow>,
    pub attr_sets: Vec<Vec<AttrRow>>,
    pub specs: Vec<Spec>,
    pub enums: Vec<Vec<String>>,
    pub parts: Vec<PartRow>,
    pub content_types: Vec<(String, usize)>,
}

pub struct NsRow {
    pub uri: String,
    pub prefix: String,
    pub ext: bool,
    /// At least one element of the namespace has a content model in the tables.
    pub understood: bool,
}

pub struct DeclRow {
    pub name: u16,
    pub ty: String,
    pub leaf_text: bool,
    pub opaque: bool,
    pub canonical: bool,
    pub reorderable: bool,
    pub model: u16,
    pub attrs: u16,
}

pub struct ModelRow {
    pub alpha_name: Vec<u16>,
    pub alpha_decl: Vec<u16>,
    pub dfa: Dfa,
}

#[derive(Clone, Copy)]
pub struct AttrRow {
    pub name: u16,
    pub spec: u16,
    pub required: bool,
}

pub struct PartRow {
    pub variant: String,
    pub root: u16,
    pub rel_type: String,
    pub default_path: String,
}

/// What identifies a content model: two declarations with the same key share one table row.
type ModelKey = (
    Vec<u16>,
    Vec<u16>,
    Vec<u32>,
    bool,
    Vec<u16>,
    Vec<bool>,
    Vec<u16>,
);

/// Generated files, as (path relative to the `generated/` directory, contents).
pub fn generate(codegen_dir: &Path) -> Result<Vec<(String, String)>, String> {
    let input = Input::load(codegen_dir)?;
    let ir = build(&input)?;
    Ok(emit::files(&ir))
}

pub fn output_dir(repo: &Path) -> PathBuf {
    repo.join("crates/stetdoc-core/src/schema/generated")
}

fn build(input: &Input) -> Result<Ir, String> {
    let mut model = Model::load(input, spec::SCHEMA_FILES)?;
    let mut applied = supplement::apply(&mut model);
    let overrides = supplement::attr_overrides();
    let canonical_types = supplement::canonical_order_types();

    // Namespaces: "" first, then by URI. One row per URI.
    let mut ns_rows: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for n in &model.namespaces {
        let e = ns_rows
            .entry(n.uri.clone())
            .or_insert((n.prefix.clone(), n.version.is_some()));
        if n.prefix < e.0 {
            *e = (n.prefix.clone(), n.version.is_some());
        }
    }
    ns_rows
        .entry(spec::XML_NS.to_string())
        .or_insert(("xml".into(), false));
    let mut namespaces = vec![NsRow {
        uri: String::new(),
        prefix: String::new(),
        ext: false,
        understood: false,
    }];
    for (uri, (prefix, ext)) in &ns_rows {
        namespaces.push(NsRow {
            uri: uri.clone(),
            prefix: prefix.clone(),
            ext: *ext,
            understood: false,
        });
    }
    let ns_index = |uri: &str| namespaces.iter().position(|n| n.uri == uri);
    let prefix_ns = |prefix: &str| -> Result<usize, String> {
        if prefix.is_empty() {
            // ":id" in the data is an unqualified attribute: namespace row 0.
            return Ok(0);
        }
        let uri = if prefix == "xml" {
            spec::XML_NS
        } else {
            model
                .prefix_uri(prefix)
                .ok_or_else(|| format!("unknown prefix {prefix:?} (uri lookup)"))?
        };
        ns_index(uri).ok_or_else(|| format!("namespace {uri} missing"))
    };
    let mut strict_aliases = Vec::new();
    for (strict, main) in spec::STRICT_ALIASES {
        if let Some(i) = ns_index(main) {
            strict_aliases.push((strict.to_string(), i));
        }
    }
    strict_aliases.sort();
    let mut preview_aliases = Vec::new();
    for (preview, main) in spec::PREVIEW_ALIASES {
        if let Some(i) = ns_index(main) {
            preview_aliases.push((preview.to_string(), i));
        }
    }
    preview_aliases.sort();

    // Element decls: every type with an element name, plus names referenced but not loaded.
    let mut decl_names: BTreeSet<String> = BTreeSet::new();
    for t in model.types.values() {
        if element_qname(&t.name).is_some() {
            decl_names.insert(t.name.clone());
        }
        if let Some(p) = &t.particle {
            collect_decls(p, &mut decl_names);
        }
    }
    let decl_names: Vec<String> = decl_names.into_iter().collect();

    // Names (elements and attributes).
    let mut name_set: BTreeSet<(usize, String)> = BTreeSet::new();
    let add_qname = |q: &str, set: &mut BTreeSet<(usize, String)>| -> Result<(), String> {
        let (p, l) = split_qname(q);
        set.insert((
            prefix_ns(p).map_err(|e| format!("{e}; name {q:?}"))?,
            l.to_string(),
        ));
        Ok(())
    };
    for d in &decl_names {
        if let Some(q) = element_qname(d) {
            add_qname(q, &mut name_set)?;
        }
    }
    for t in model.types.values() {
        for a in model.attrs_of(t) {
            add_qname(&a.qname, &mut name_set)?;
        }
    }
    let names: Vec<(usize, String)> = name_set.into_iter().collect();
    let name_id = |q: &str| -> Result<u16, String> {
        let (p, l) = split_qname(q);
        let key = (
            prefix_ns(p).map_err(|e| format!("{e} for name {q:?}"))?,
            l.to_string(),
        );
        names
            .binary_search(&key)
            .map(|i| i as u16)
            .map_err(|_| format!("name {q} not in table"))
    };
    let decl_id = |d: &str| -> Result<u16, String> {
        decl_names
            .binary_search(&d.to_string())
            .map(|i| i as u16)
            .map_err(|_| format!("decl {d} unknown"))
    };

    // Models, attribute sets, enums, value specs.
    let mut models: Vec<ModelRow> = Vec::new();
    let mut model_ids: BTreeMap<ModelKey, u16> = BTreeMap::new();
    let mut attr_sets: Vec<Vec<AttrRow>> = Vec::new();
    let mut attr_ids: BTreeMap<Vec<(u16, u16, bool)>, u16> = BTreeMap::new();
    let mut specs: Vec<Spec> = Vec::new();
    let mut spec_ids: BTreeMap<Spec, u16> = BTreeMap::new();
    let mut enums = EnumTable::new();

    let mut decls: Vec<DeclRow> = Vec::new();
    for d in &decl_names {
        let q = element_qname(d).ok_or_else(|| format!("decl {d} has no element name"))?;
        let ty = d.split_once('/').map(|x| x.0).unwrap_or("").to_string();
        let ty_name = ty.clone();
        let name = name_id(q)?;
        let t: Option<&TypeDef> = model.types.get(d);
        let Some(t) = t else {
            decls.push(DeclRow {
                name,
                ty,
                leaf_text: false,
                opaque: true,
                canonical: false,
                reorderable: false,
                model: NONE,
                attrs: NONE,
            });
            continue;
        };
        // Content model.
        let mut model_idx = NONE;
        if let Some(p) = model.particle_of(t) {
            let mut sym_decl: BTreeMap<u16, u16> = BTreeMap::new();
            let resolve = |dn: &str| -> Result<u32, String> {
                let q = element_qname(dn)
                    .ok_or_else(|| format!("{dn}: particle names an abstract type"))?;
                Ok(u32::from(name_id(q)?))
            };
            let dfa = compile(p, &resolve).map_err(|e| format!("{d}: {e}"))?;
            collect_symbol_decls(p, &name_id, &decl_id, &mut sym_decl)?;
            let alpha_name: Vec<u16> = dfa.alphabet.iter().map(|s| *s as u16).collect();
            let mut alpha_decl = Vec::new();
            for n in &alpha_name {
                alpha_decl.push(
                    *sym_decl
                        .get(n)
                        .ok_or_else(|| format!("{d}: no decl for symbol"))?,
                );
            }
            let key = (
                alpha_name.clone(),
                alpha_decl.clone(),
                dfa.alphabet.clone(),
                dfa.any,
                dfa.trans.clone(),
                dfa.accept.clone(),
                dfa.rank.clone(),
            );
            let next = models.len() as u16;
            model_idx = *model_ids.entry(key).or_insert_with(|| {
                models.push(ModelRow {
                    alpha_name,
                    alpha_decl,
                    dfa,
                });
                next
            });
        }
        // Attributes.
        let mut rows: Vec<(u16, u16, bool)> = Vec::new();
        for a in model.attrs_of(t) {
            let (mut spec, required) =
                attr_spec(&a, &model, &mut enums).map_err(|e| format!("{d} {}: {e}", a.qname))?;
            if let Some(over) = overrides
                .iter()
                .find(|o| o.ty == ty_name && o.attr == a.qname)
            {
                spec = over.alternatives.clone();
                spec.iter_mut().for_each(|c| c.sort());
                spec.sort();
                applied.hit(over.rule);
            }
            let next = specs.len() as u16;
            let sid = *spec_ids.entry(spec.clone()).or_insert_with(|| {
                specs.push(spec);
                next
            });
            rows.push((name_id(&a.qname)?, sid, required));
        }
        rows.sort();
        let attrs_idx = if rows.is_empty() {
            NONE
        } else {
            let next = attr_sets.len() as u16;
            *attr_ids.entry(rows.clone()).or_insert_with(|| {
                attr_sets.push(
                    rows.iter()
                        .map(|r| AttrRow {
                            name: r.0,
                            spec: r.1,
                            required: r.2,
                        })
                        .collect(),
                );
                next
            })
        };
        let canonical = canonical_types.iter().find(|(_, c)| *c == ty_name.as_str());
        if let Some((rule, _)) = canonical {
            applied.hit(*rule);
        }
        decls.push(DeclRow {
            name,
            ty,
            leaf_text: model.leaf_text_of(t),
            opaque: false,
            canonical: canonical.is_some(),
            reorderable: models
                .get(usize::from(model_idx))
                .is_some_and(|m| m.dfa.reorderable),
            model: model_idx,
            attrs: attrs_idx,
        });
    }

    // Parts.
    let mut parts = Vec::new();
    let mut content_types: Vec<(String, usize)> = Vec::new();
    for (i, p) in spec::PARTS.iter().enumerate() {
        let pj = input.json(&format!("parts/{}.json", p.json))?;
        let root_el = pj
            .get("RootElement")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{}: no RootElement", p.json))?;
        let root_ty = pj.get("Root").and_then(|v| v.as_str());
        let root = decl_names
            .iter()
            .position(|d| {
                let Some(q) = element_qname(d) else {
                    return false;
                };
                if split_qname(q).1 != root_el {
                    return false;
                }
                match root_ty {
                    Some(r) => d.starts_with(&format!("{r}/")),
                    None => {
                        model.types.get(d).and_then(|t| t.base.as_deref())
                            == Some("OpenXmlPartRootElement")
                    }
                }
            })
            .ok_or_else(|| format!("{}: root decl not found", p.json))?;
        let mut cts: Vec<String> = p
            .extra_content_types
            .iter()
            .map(|s| s.to_string())
            .collect();
        if let Some(ct) = pj.get("ContentType").and_then(|v| v.as_str()) {
            cts.push(ct.to_string());
        }
        for ct in cts {
            content_types.push((ct, i));
        }
        parts.push(PartRow {
            variant: p.variant.to_string(),
            root: root as u16,
            rel_type: pj
                .get("RelationshipType")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            default_path: p.default_path.to_string(),
        });
    }
    content_types.sort();

    applied.require_all_hit()?;
    if !enums.missing.is_empty() {
        println!(
            "note: enumerations from unloaded namespaces accept any value: {:?}",
            enums.missing
        );
    }
    for d in decls.iter().filter(|d| !d.opaque) {
        if let Some((ns, _)) = names.get(usize::from(d.name))
            && let Some(row) = namespaces.get_mut(*ns)
        {
            row.understood = true;
        }
    }
    Ok(Ir {
        supplements: applied.lines(),
        commit: input.commit().to_string(),
        digest: input.digest.clone(),
        namespaces,
        strict_aliases,
        preview_aliases,
        names,
        decls,
        models,
        attr_sets,
        specs,
        enums: enums.sets,
        parts,
        content_types,
    })
}

fn collect_decls(p: &Particle, out: &mut BTreeSet<String>) {
    match p {
        Particle::Element { decl, .. } => {
            if element_qname(decl).is_some() {
                out.insert(decl.clone());
            }
        }
        Particle::Any { .. } => {}
        Particle::Seq { items, .. }
        | Particle::Choice { items, .. }
        | Particle::All { items, .. } => {
            for i in items {
                collect_decls(i, out);
            }
        }
    }
}

/// Maps each element name id in a particle to its declaration. A name that two different
/// declarations share inside one content model would be ambiguous; that is an error.
fn collect_symbol_decls(
    p: &Particle,
    name_id: &dyn Fn(&str) -> Result<u16, String>,
    decl_id: &dyn Fn(&str) -> Result<u16, String>,
    out: &mut BTreeMap<u16, u16>,
) -> Result<(), String> {
    match p {
        Particle::Element { decl, .. } => {
            let q = element_qname(decl).ok_or("abstract particle")?;
            let (n, d) = (name_id(q)?, decl_id(decl)?);
            if let Some(prev) = out.insert(n, d)
                && prev != d
            {
                return Err(format!(
                    "name {q} is declared twice with different types in one content model"
                ));
            }
        }
        Particle::Any { .. } => {}
        Particle::Seq { items, .. }
        | Particle::Choice { items, .. }
        | Particle::All { items, .. } => {
            for i in items {
                collect_symbol_decls(i, name_id, decl_id, out)?;
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("codegen")
    }

    #[test]
    fn generation_is_deterministic() {
        let a = generate(&dir()).expect("generates");
        let b = generate(&dir()).expect("generates");
        assert_eq!(a, b);
        assert!(
            a.iter()
                .all(|(_, c)| c.starts_with("// @generated by `cargo xtask codegen`"))
        );
    }

    #[test]
    fn checked_in_tables_are_current() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repo root")
            .to_path_buf();
        for (name, content) in generate(&dir()).expect("generates") {
            let on_disk =
                std::fs::read_to_string(output_dir(&root).join(&name)).unwrap_or_default();
            assert_eq!(on_disk, content, "{name} is stale: run cargo xtask codegen");
        }
    }

    #[test]
    fn a_changed_input_file_is_refused() {
        let tmp = std::env::temp_dir().join(format!("stetdoc-pin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("input")).expect("mkdir");
        let pin = std::fs::read_to_string(dir().join("pin.json")).expect("pin");
        std::fs::write(tmp.join("pin.json"), pin).expect("write");
        for entry in walk(&dir().join("input")) {
            let rel = entry
                .strip_prefix(dir().join("input"))
                .expect("rel")
                .to_path_buf();
            let to = tmp.join("input").join(&rel);
            std::fs::create_dir_all(to.parent().expect("parent")).expect("mkdir");
            std::fs::copy(&entry, &to).expect("copy");
        }
        std::fs::write(tmp.join("input/namespaces.json"), "[]").expect("tamper");
        let err = generate(&tmp).expect_err("tampered input must be refused");
        assert!(err.contains("does not match the pin"), "{err}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn walk(d: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(d).expect("dir").flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
        out
    }
}
