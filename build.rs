use std::env;
use std::path::PathBuf;

use bindgen;

#[cfg(not(feature = "unicode_support"))]
fn main() {
}

#[cfg(feature = "unicode_support")]
fn main() {
    println!("cargo:rustc-link-search=/usr/lib");
    println!("cargo:rustc-link-lib=fontconfig");
    println!("cargo:rerun-if-changed=src/bindgen_wrapper.h");

    let bindings = bindgen::Builder::default()
        .header("src/bindgen_wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("unicode_support_bindings.rs"))
        .expect("Couldn't write bindings");
}
