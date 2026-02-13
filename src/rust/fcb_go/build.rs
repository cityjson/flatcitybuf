fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let output_dir = std::path::Path::new(&crate_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("go")
        .join("include");

    std::fs::create_dir_all(&output_dir).ok();

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_language(cbindgen::Language::C)
        .with_include_guard("FCB_CORE_H")
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(output_dir.join("fcb_core.h"));
}
