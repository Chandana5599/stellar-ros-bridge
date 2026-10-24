#[cfg(all(target_os = "windows", target_env = "gnu"))]
fn main() {
    use std::path::PathBuf;
    use std::process::Command;

    println!("cargo:rerun-if-changed=src/memset_explicit.s");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let object = out_dir.join("memset_explicit.o");
    let status = Command::new("as")
        .arg("--64")
        .arg("-o")
        .arg(&object)
        .arg("src/memset_explicit.s")
        .status()
        .expect("failed to run the GNU assembler");
    assert!(status.success(), "assembling memset_explicit.s failed");

    // Pass the object explicitly at the end of the linker command so it also
    // resolves references coming from the bundled libsodium static library.
    println!("cargo:rustc-link-arg={}", object.display());
}

#[cfg(not(all(target_os = "windows", target_env = "gnu")))]
fn main() {}
