//! Link the executable against `libbinaryninjacore`.
//!
//! `binaryninjacore-sys` finds the core through `BINARYNINJADIR` and exports
//! its directory as `DEP_BINARYNINJACORE_PATH`; Cargo passes a `links` crate's
//! flags on to libraries but not to the final executable, so it is done here.
//! The rpath lets `bnx` run without `LD_LIBRARY_PATH`.
fn main() {
    let Some(link_path) = std::env::var_os("DEP_BINARYNINJACORE_PATH") else {
        panic!("DEP_BINARYNINJACORE_PATH is unset: set BINARYNINJADIR to the Binary Ninja 6.0 install before building (see README.md)");
    };
    let link_path = link_path.to_string_lossy();
    println!("cargo::rustc-link-lib=dylib=binaryninjacore");
    println!("cargo::rustc-link-search={link_path}");
    #[cfg(target_os = "linux")]
    println!("cargo::rustc-link-arg=-Wl,-rpath,{link_path},-L{link_path}");
    #[cfg(target_os = "macos")]
    println!("cargo::rustc-link-arg=-Wl,-rpath,{link_path}");
}
