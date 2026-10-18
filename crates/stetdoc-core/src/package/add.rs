//! Adding a part: the part, its content type and its relationship, all or nothing.

use super::errors::{invalid, limit, malformed};
use super::{Added, ContentTypes, NewPart, Package, Relationships, names, opc};
use crate::api::error::StetError;
use crate::xml::Bytes;

/// What `add_part` will do, worked out before anything changes.
#[derive(Default)]
struct AddPlan {
    /// Existing parts that get new bytes.
    replace: Vec<(String, Vec<u8>)>,
    /// New parts, in the order they are added.
    add: Vec<(String, Vec<u8>)>,
    /// The id of the relationship that was added, if one was.
    id: Option<String>,
}

impl Package {
    /// Adds a part, registers its content type in `[Content_Types].xml` and, if asked, a
    /// relationship to it. Returns the new relationship id when one was added. The new entry
    /// goes after the existing ones and its timestamp is copied from the main document part.
    ///
    /// Either everything happens or nothing does. All the new bytes are built first, and the
    /// package changes only if every one of them could be.
    pub fn add_part(&mut self, new: NewPart<'_>) -> Result<Option<String>, StetError> {
        let plan = self.plan_add(&new)?;
        for (name, bytes) in plan.replace {
            self.replace_checked(&name, bytes);
        }
        for (name, bytes) in plan.add {
            self.added.push(Added {
                name,
                content: Bytes::from(bytes),
            });
        }
        Ok(plan.id)
    }

    /// Adds a relationship from `source` (empty for the package itself) to `target`, which is an
    /// external reference such as a URL and names no part. The relationship part of `source` is made
    /// if it does not exist, with its content type registered. Returns the new relationship id. Either
    /// everything happens or nothing does.
    pub fn add_external_relationship(
        &mut self,
        source: &str,
        rel_type: &str,
        target: &str,
    ) -> Result<String, StetError> {
        if !source.is_empty() && !self.contains(source) {
            return Err(invalid(format!("there is no part named {source:?}")));
        }
        let rels_name = opc::rels_part_name(source);
        let mut plan = AddPlan::default();
        match self.part(&rels_name)? {
            Some(existing) => {
                let rels = Relationships::parse(&rels_name, &existing, &self.limits)?;
                let id = rels.next_id();
                let updated = opc::with_relationship(
                    &rels_name,
                    &existing,
                    &id,
                    rel_type,
                    target,
                    true,
                    &self.limits,
                )?;
                self.check_replace(&rels_name, updated.len())?;
                plan.replace.push((rels_name, updated));
                plan.id = Some(id);
            }
            None => {
                let id = "rId1".to_owned();
                let bytes = opc::new_rels_part(&id, rel_type, target, true);
                self.check_new(&rels_name, bytes.len(), &plan)?;
                let types = self
                    .part(opc::CONTENT_TYPES_PART)?
                    .ok_or_else(|| malformed("the package has no [Content_Types].xml"))?;
                if let Some(updated) =
                    self.registration(&types, &rels_name, opc::CT_RELATIONSHIPS)?
                {
                    self.check_replace(opc::CONTENT_TYPES_PART, updated.len())?;
                    plan.replace
                        .push((opc::CONTENT_TYPES_PART.to_owned(), updated));
                }
                plan.add.push((rels_name, bytes));
                plan.id = Some(id);
            }
        }
        for (name, bytes) in plan.replace {
            self.replace_checked(&name, bytes);
        }
        for (name, bytes) in plan.add {
            self.added.push(Added {
                name,
                content: Bytes::from(bytes),
            });
        }
        plan.id
            .ok_or_else(|| invalid("no relationship id was made"))
    }

    /// Builds what `add_part` will do, without changing the package.
    fn plan_add(&self, new: &NewPart<'_>) -> Result<AddPlan, StetError> {
        let mut plan = AddPlan::default();
        self.check_new(new.name, new.bytes.len(), &plan)?;
        let mut types = self
            .part(opc::CONTENT_TYPES_PART)?
            .ok_or_else(|| malformed("the package has no [Content_Types].xml"))?;
        let mut types_changed = false;
        if let Some(updated) = self.registration(&types, new.name, new.content_type)? {
            types = Bytes::from(updated);
            types_changed = true;
        }
        plan.add.push((new.name.to_owned(), new.bytes.clone()));
        if let Some((source, rel_type)) = new.related_from {
            let rels_name = opc::rels_part_name(source);
            let source_dir = source.rsplit_once('/').map_or("", |(d, _)| d);
            let target = relative_path(source_dir, new.name);
            match self.part(&rels_name)? {
                Some(existing) => {
                    let rels = Relationships::parse(&rels_name, &existing, &self.limits)?;
                    let id = rels.next_id();
                    let updated = opc::with_relationship(
                        &rels_name,
                        &existing,
                        &id,
                        rel_type,
                        &target,
                        false,
                        &self.limits,
                    )?;
                    self.check_replace(&rels_name, updated.len())?;
                    plan.replace.push((rels_name, updated));
                    plan.id = Some(id);
                }
                None => {
                    let id = "rId1".to_owned();
                    let bytes = opc::new_rels_part(&id, rel_type, &target, false);
                    self.check_new(&rels_name, bytes.len(), &plan)?;
                    if let Some(updated) =
                        self.registration(&types, &rels_name, opc::CT_RELATIONSHIPS)?
                    {
                        types = Bytes::from(updated);
                        types_changed = true;
                    }
                    plan.add.push((rels_name, bytes));
                    plan.id = Some(id);
                }
            }
        }
        if types_changed {
            self.check_replace(opc::CONTENT_TYPES_PART, types.len())?;
            plan.replace
                .push((opc::CONTENT_TYPES_PART.to_owned(), types.to_vec()));
        }
        Ok(plan)
    }

    /// Checks a name and size for a part about to be added: the name is safe, no entry has it
    /// ignoring ASCII case (a directory entry counts, since a file and a directory of one name
    /// cannot both be extracted), and nothing already in `plan` has it.
    fn check_new(&self, name: &str, len: usize, plan: &AddPlan) -> Result<(), StetError> {
        names::validate_part_name(name).map_err(invalid)?;
        let taken = |n: &str| n.trim_end_matches('/').eq_ignore_ascii_case(name);
        if self.layout.entries.iter().any(|e| taken(&e.name))
            || self.added.iter().any(|a| taken(&a.name))
            || plan.add.iter().any(|(n, _)| taken(n))
        {
            return Err(invalid(format!("a part named {name:?} already exists")));
        }
        if len as u64 > self.limits.max_entry_bytes {
            return Err(limit(format!(
                "part {name:?} would be {len} bytes, over the limit"
            )));
        }
        Ok(())
    }

    /// The new `[Content_Types].xml` that registers `content_type` for `name`, or `None` if it is
    /// registered already.
    fn registration(
        &self,
        types: &Bytes,
        name: &str,
        content_type: &str,
    ) -> Result<Option<Vec<u8>>, StetError> {
        let current = ContentTypes::parse(types, &self.limits)?;
        match current.type_of(name) {
            Some(t) if t == content_type => Ok(None),
            Some(t) if current.has_override(name) => Err(invalid(format!(
                "{name:?} is registered as {t:?}, not {content_type:?}"
            ))),
            _ => opc::with_override(types, name, content_type, &self.limits).map(Some),
        }
    }
}

/// The relative path from `dir` to the part `name`, written as a URI reference as a relationship
/// target is: `comments.xml` from `word`, or `word/comments.xml` from the package root, with the
/// characters that a URI path cannot hold written as `%XX`.
fn relative_path(dir: &str, name: &str) -> String {
    let mut d: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    let mut n: Vec<&str> = name.split('/').collect();
    while !d.is_empty() && d.first() == n.first() && n.len() > 1 {
        d.remove(0);
        n.remove(0);
    }
    let ups = std::iter::repeat_n("..", d.len()).map(str::to_owned);
    ups.chain(n.into_iter().map(opc::encode_segment))
        .collect::<Vec<_>>()
        .join("/")
}
