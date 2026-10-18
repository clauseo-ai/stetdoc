//! Sets the linker flags a Node-API addon needs on each platform.

fn main() {
    napi_build::setup();
    // A macOS library records its install name, which by default is the absolute path it was built at. The addon is loaded
    // by path and never linked against, so a neutral name keeps the build machine's paths out of the shipped file.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/stetdoc.node");
    }
}
