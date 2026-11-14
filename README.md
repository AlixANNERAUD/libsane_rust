# libsane_rust

[![CI](https://github.com/AlixANNERAUD/libsane_rust/actions/workflows/ci.yml/badge.svg)](https://github.com/AlixANNERAUD/libsane_rust/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/AlixANNERAUD/libsane_rust/graph/badge.svg)](https://codecov.io/gh/AlixANNERAUD/libsane_rust)
[![libsane on crates.io](https://img.shields.io/crates/v/libsane.svg?label=libsane)](https://crates.io/crates/libsane)
[![sane-sys on crates.io](https://img.shields.io/crates/v/sane-sys.svg?label=sane-sys)](https://crates.io/crates/sane-sys)
[![docs.rs](https://img.shields.io/docsrs/libsane)](https://docs.rs/libsane)

Rust bindings to [SANE (Scanner Access Now Easy)](http://www.sane-project.org/),
the scanner library of Linux and other Unix systems.

| Crate | |
| --- | --- |
| [`libsane`](libsane) | Safe, idiomatic API: list scanners, read and set options, scan to bytes or to an `image::DynamicImage`. |
| [`sane-sys`](sane-sys) | Raw FFI to libsane, generated with bindgen and linked through pkg-config. |

```rust
use libsane::{Sane, well_known};

let sane = Sane::init()?;
for device in sane.devices(false)? {
    println!("{}: {} {} ({})", device.name, device.vendor, device.model, device.kind);
}

let mut device = sane.open("test:0")?;
device.set_value(well_known::MODE, well_known::MODE_COLOR)?;
device.set_value(well_known::RESOLUTION, 300)?;
let frame = device.scan()?; // raw bytes + parameters
frame.to_image()?.save("scan.png")?; // with the `image` feature
```

## System requirements

Both crates need libsane and its development files, pkg-config to find them,
and libclang for bindgen:

| System | Command |
| --- | --- |
| Debian, Ubuntu | `sudo apt install libsane-dev pkg-config libclang-dev` |
| Fedora | `sudo dnf install sane-backends-devel pkgconf-pkg-config clang-devel` |
| Arch Linux | `sudo pacman -S sane pkgconf clang` |
| macOS (Homebrew) | `brew install sane-backends pkg-config llvm` |

If libsane is installed somewhere else, point `PKG_CONFIG_PATH` to the
directory that holds `sane-backends.pc`.

## Notes

- **One initialization per process.** `Sane::init` fails with
  `Error::AlreadyInitialized` while another `Sane` exists. Dropping it calls
  `sane_exit`; devices borrow it, so they are closed first.
- **Blocking.** Every call blocks, sometimes for seconds. From async code, run
  the SANE session on a blocking thread, for example with
  `tokio::task::spawn_blocking`.
- **Threads.** libsane is not thread-safe: a `Sane` can move to another thread
  but cannot be shared, and its devices stay on its thread.
- **No scanner?** SANE's `test` backend provides virtual devices (`test:0`,
  `test:1`). Enable it by uncommenting `test` in `/etc/sane.d/dll.conf`, or
  point `SANE_CONFIG_DIR` to a directory whose `dll.conf` lists it, like
  [`libsane/tests/sane.d`](libsane/tests/sane.d).

## Example

[`scan_to_png`](libsane/examples/scan_to_png.rs) lists the devices and scans
one page to a PNG file:

```sh
SANE_CONFIG_DIR=libsane/tests/sane.d cargo run -p libsane --example scan_to_png --features image -- test:0 scan.png
```

## Development

Tasks are defined in [`mise.toml`](mise.toml) and mirror CI:

```sh
mise install                          # Rust toolchain and tools
mise bootstrap packages apply         # libsane-dev, pkg-config, libclang (Debian, Ubuntu)
mise run check                        # format, clippy, tests, example
mise run miri                         # unit tests under Miri
mise run doc                          # documentation as docs.rs builds it
```

The tests run against SANE's `test` backend, configured by
[`libsane/tests/sane.d`](libsane/tests/sane.d), so they need no scanner.
`unsafe` code lives only in [`libsane/src/raw.rs`](libsane/src/raw.rs), in
small documented functions; the rest of the crate denies it. Releases are
described in [RELEASING.md](RELEASING.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.

`sane-sys/include/sane/sane.h` is an unmodified copy of SANE's public-domain
header, used only to build the documentation on docs.rs.
