//! Embeds `macos/Info.plist` into the binary on macOS.
//!
//! An executable that is not inside an `.app` has no `Info.plist`, and macOS
//! treats a process without one as not Retina-capable: it renders at 1x and
//! upscales, so `cargo run` looks blurry next to the released bundle. Linking
//! the plist into a `__TEXT,__info_plist` section gives the bare binary the
//! same bundle dictionary the app has, and AppKit reads it at launch.
//!
//! Every other platform gets nothing from this file.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=macos/Info.plist");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    let manifest = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    let source = manifest.join("macos").join("Info.plist");
    let plist = std::fs::read_to_string(&source)
        .unwrap_or_else(|e| panic!("reading {}: {e}", source.display()));

    let out = PathBuf::from(env("OUT_DIR")).join("Info.plist");
    std::fs::write(
        &out,
        plist.replace("__VERSION__", &env("CARGO_PKG_VERSION")),
    )
    .unwrap_or_else(|e| panic!("writing {}: {e}", out.display()));

    // `-sectcreate` takes its arguments comma-separated through the compiler
    // driver, which leaves no way to quote a path containing a comma. Cargo's
    // target directory does not normally have one.
    println!(
        "cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        out.display()
    );
}

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("cargo did not set {key}"))
}
