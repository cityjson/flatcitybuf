fn main() {
    // Generate C++ bridge code
    // Note: We only generate the bridge here; CMake compiles it.
    // This prevents duplicate symbol errors that would occur if both
    // build.rs and CMake compiled the same bridge code.
    cxx_build::bridge("src/lib.rs").flag_if_supported("-std=c++17");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/reader.rs");
    println!("cargo:rerun-if-changed=src/writer.rs");
}
