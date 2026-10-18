//! The files path through `Document`: put the input into `document.md` of a small document, whole, spliced in or appended,
//! and apply the readable files as an edit.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    stetdoc_core::fuzz::files(data);
});
