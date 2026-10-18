//! `package_open` and `package_save`: the container.

use crate::api::limits::Limits;
use crate::package::Package;
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

/// Opening a package reads its structure and no part.
pub(super) const OPEN: Bench = Bench {
    name: "package_open",
    limit: Limit::Op(Op::Open),
    parses: Parses::Nothing,
    inputs: Inputs::Documents,
    prepare: open,
};

fn open(input: &Input) -> Result<Run, String> {
    let file = input.docx.clone();
    Ok(Box::new(move || {
        let package = Package::open(file, &Limits::default()).map_err(|e| e.to_string())?;
        Ok(Box::new(package) as Kept)
    }))
}

/// Writing a package whose main part was replaced: the rebuild path, which copies the other
/// entries as they are and deflates the new part.
pub(super) const SAVE: Bench = Bench {
    name: "package_save",
    limit: Limit::Op(Op::Save),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: save,
};

fn save(input: &Input) -> Result<Run, String> {
    let mut package =
        Package::open(input.docx.clone(), &Limits::default()).map_err(|e| e.to_string())?;
    let main = package
        .main_part()
        .ok_or("the package has no main part")?
        .to_owned();
    let mut part = input.main_part.clone();
    part.push(b'\n');
    package
        .replace_part(&main, part)
        .map_err(|e| e.to_string())?;
    Ok(Box::new(move || {
        let written = package.write().map_err(|e| e.to_string())?;
        Ok(Box::new((package, written)) as Kept)
    }))
}
