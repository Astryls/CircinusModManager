fn main() {
    // intel_tex_2 ships a prebuilt C++ object (the ASTC glue) that needs the C++ runtime on
    // unix targets; MSVC objects carry their own default-lib directives.
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match os.as_str() {
        "linux" => println!("cargo:rustc-link-lib=dylib=stdc++"),
        "macos" | "ios" => println!("cargo:rustc-link-lib=dylib=c++"),
        _ => {}
    }
}
