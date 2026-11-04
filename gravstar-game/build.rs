//! SDL2 (built into the program on Windows) uses the Windows registry, whose
//! functions are in advapi32.lib, but the SDL2 bindings do not ask for that
//! library, so linking fails without this.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=advapi32");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
