//! The container: open the input as a package, read every part, then replace one part and write.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    stetdoc_core::fuzz::package(data);
});
