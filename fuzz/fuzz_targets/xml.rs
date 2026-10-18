//! The span tree: parse the input as one part, serialize it clean and with every tag dirty, then
//! make one edit.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    stetdoc_core::fuzz::xml(data);
});
