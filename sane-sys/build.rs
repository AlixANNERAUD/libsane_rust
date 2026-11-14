//! Generates the bindings to `sane/sane.h` with bindgen and links libsane.
//!
//! libsane is found through pkg-config (`sane-backends`), so the usual
//! pkg-config variables apply (`PKG_CONFIG_PATH`, `PKG_CONFIG_SYSROOT_DIR`,
//! `SANE_BACKENDS_NO_PKG_CONFIG`...). Linking can also be overridden without
//! pkg-config through Cargo's `links` override for `sane`.
//!
//! On docs.rs (`DOCS_RS` set) there is no libsane: the bindings are generated
//! from the copy of `sane.h` in `include/` and nothing is linked.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=include/sane/sane.h");
    println!("cargo:rerun-if-env-changed=DOCS_RS");

    let include_paths = if env::var_os("DOCS_RS").is_some() {
        vec![PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("include")]
    } else {
        probe_libsane()
    };

    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .allowlist_function("sane_.*")
        .allowlist_type("SANE_.*")
        .allowlist_var("SANE_.*")
        // Backends may return values the header does not list (newer status
        // codes, frame formats): C enums become integer constants, never Rust
        // enums, so an unknown value is not undefined behavior.
        .default_enum_style(bindgen::EnumVariation::Consts)
        .prepend_enum_name(false)
        .use_core()
        .generate_comments(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    for path in &include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    let bindings = builder
        .generate()
        .expect("bindgen could not generate the bindings to sane/sane.h");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("bindings.rs");
    bindings
        .write_to_file(&out)
        .unwrap_or_else(|error| panic!("could not write {}: {error}", out.display()));
}

/// Finds libsane with pkg-config, which also prints the link directives.
fn probe_libsane() -> Vec<PathBuf> {
    match pkg_config::Config::new()
        .atleast_version("1.0.0")
        .probe("sane-backends")
    {
        Ok(library) => library.include_paths,
        Err(error) => panic!(
            "\n\nsane-sys could not find libsane with pkg-config (package `sane-backends`):\n\
             {error}\n\n\
             Install the SANE development files and pkg-config, for example:\n\
             - Debian, Ubuntu: apt install libsane-dev pkg-config libclang-dev\n\
             - Fedora: dnf install sane-backends-devel pkgconf-pkg-config clang-devel\n\
             - Arch Linux: pacman -S sane pkgconf clang\n\
             - macOS (Homebrew): brew install sane-backends pkg-config llvm\n\
             If libsane is installed in a non-standard location, point PKG_CONFIG_PATH \
             to the directory that holds sane-backends.pc.\n"
        ),
    }
}
