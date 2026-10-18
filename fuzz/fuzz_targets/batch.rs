//! A batch through `Document`: parse the input as JSON or block text, write it as JSON and parse it again, then apply it to a
//! small document with the document's base token.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    stetdoc_core::fuzz::batch(data);
});
