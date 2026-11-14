# libsane — scan documents and images from Rust

[![CI](https://github.com/AlixANNERAUD/libsane_rust/actions/workflows/ci.yml/badge.svg)](https://github.com/AlixANNERAUD/libsane_rust/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/AlixANNERAUD/libsane_rust/graph/badge.svg)](https://codecov.io/gh/AlixANNERAUD/libsane_rust)
[![libsane on crates.io](https://img.shields.io/crates/v/libsane.svg?label=libsane)](https://crates.io/crates/libsane)
[![sane-sys on crates.io](https://img.shields.io/crates/v/sane-sys.svg?label=sane-sys)](https://crates.io/crates/sane-sys)
[![docs.rs](https://img.shields.io/docsrs/libsane?label=docs.rs)](https://docs.rs/libsane)
[![MSRV 1.85](https://img.shields.io/badge/MSRV-1.85-blue)](Cargo.toml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

**Safe, idiomatic Rust bindings to [SANE](http://www.sane-project.org/)
(Scanner Access Now Easy), the scanner API of Linux, the BSDs and macOS.**
Find flatbed scanners, document feeders and multifunction printers, set the
resolution, mode and scan area, and get the page as raw bytes or as an
[`image`](https://crates.io/crates/image) ready to save as PNG or JPEG, or to
feed to OCR.

```rust
use libsane::{Sane, well_known};

let sane = Sane::init()?;
let mut scanner = sane.open("test:0")?;           // or a name from sane.devices()
scanner.set_value(well_known::MODE, well_known::MODE_COLOR)?;
scanner.set_value(well_known::RESOLUTION, 300)?;  // dpi
scanner.scan()?.to_image()?.save("page.png")?;    // `image` feature
```

<p align="center">
  <img src="assets/test-pattern-scan.png" width="236" alt="A color test pattern scanned with the scan_to_png example from SANE's virtual test backend">
  <br>
  <em>Scanned by <a href="libsane/examples/scan_to_png.rs"><code>scan_to_png</code></a> from SANE's virtual <code>test</code> scanner: no hardware needed to try it.</em>
</p>

## Why libsane?

- **Every SANE scanner.** USB and network scanners from HP, Canon, Epson,
  Brother, Fujitsu, Kodak and many more, through the
  [backends of sane-backends](http://www.sane-project.org/sane-supported-devices.html),
  including driverless eSCL/AirScan devices with `sane-airscan`.
- **Safe by construction.** The SANE rules are enforced by types: one
  initialization per process, devices closed before `sane_exit`, buffers sized
  from the option descriptors. `unsafe` code lives in a single module of small,
  documented functions, and the code that decodes SANE's C structures runs
  under Miri.
- **Typed options.** Bool, int, fixed-point, string and button options with
  their constraints (range, word list, string list) and capabilities; the
  option list reloads itself when the backend reports
  `SANE_INFO_RELOAD_OPTIONS`.
- **Every way to read a scan.** Chunks through `std::io::Read`, a whole
  `Frame` of raw bytes and parameters, or an `image::DynamicImage`. Gray, RGB,
  1/8/16-bit, three-pass scanners and hand scanners with an unknown height are
  handled, as are document feeders, page after page.
- **Typed errors.** Every `SANE_Status` maps to an `Error` variant
  (`Jammed`, `NoDocs`, `CoverOpen`...); no panic crosses the FFI boundary.
- **Tested without a scanner.** The test suite drives SANE's virtual `test`
  backend in CI, option types, frame formats and error codes included.

## Crates

| Crate | Description |
| --- | --- |
| [`libsane`](libsane) | The safe API: devices, options, scans, frames, images. Start here. |
| [`sane-sys`](sane-sys) | Raw FFI to libsane, generated with bindgen and linked through pkg-config. |

## Getting started

1. Install libsane, pkg-config and libclang (for bindgen):

   | System | Command |
   | --- | --- |
   | Debian, Ubuntu | `sudo apt install libsane-dev pkg-config libclang-dev` |
   | Fedora | `sudo dnf install sane-backends-devel pkgconf-pkg-config clang-devel` |
   | Arch Linux | `sudo pacman -S sane pkgconf clang` |
   | macOS (Homebrew) | `brew install sane-backends pkg-config llvm` |

   If libsane is installed somewhere else, point `PKG_CONFIG_PATH` to the
   directory that holds `sane-backends.pc`.

2. Add the crate, with the `image` feature to get images:

   ```sh
   cargo add libsane --features image
   ```

3. List the scanners and scan a page:

   ```rust
   use libsane::Sane;

   fn main() -> Result<(), Box<dyn std::error::Error>> {
       let sane = Sane::init()?;
       let devices = sane.devices(false)?;
       for device in &devices {
           println!("{}: {} {} ({})", device.name, device.vendor, device.model, device.kind);
       }
       let mut scanner = sane.open_device(devices.first().ok_or("no scanner found")?)?;
       let frame = scanner.scan()?;
       println!("{}x{} pixels", frame.width(), frame.height());
       frame.to_image()?.save("scan.png")?;
       Ok(())
   }
   ```

**No scanner at hand?** SANE ships a virtual one. Uncomment `test` in
`/etc/sane.d/dll.conf`, or point `SANE_CONFIG_DIR` to a directory whose
`dll.conf` lists it, like [`libsane/tests/sane.d`](libsane/tests/sane.d), and
open `test:0`:

```sh
SANE_CONFIG_DIR=libsane/tests/sane.d cargo run -p libsane --example scan_to_png --features image -- test:0 scan.png
```

## Good to know

- **Blocking API.** SANE calls block, sometimes for seconds (discovery, lamp
  warm-up, scanning). From async code, run the session on a blocking thread,
  for example with `tokio::task::spawn_blocking`.
- **One session per process.** `Sane::init` returns
  `Error::AlreadyInitialized` while another `Sane` exists; dropping it calls
  `sane_exit`, after which SANE can be initialized again.
- **Threads.** libsane is not thread-safe: a `Sane` can move to another thread
  but cannot be shared, and its devices stay on its thread.
- **Document feeders.** `Scan::next_page` scans the next sheet without ending
  the batch, and returns `false` when the feeder is empty.
- **Windows.** SANE is not available on Windows, which uses WIA and TWAIN
  instead.

## Documentation

- [`libsane` API documentation](https://docs.rs/libsane) on docs.rs
- [`sane-sys` API documentation](https://docs.rs/sane-sys) on docs.rs
- [The SANE standard](https://sane-project.gitlab.io/standard/) for the
  meaning of every option and status

## Development

Tasks are defined in [`mise.toml`](mise.toml) and mirror CI:

```sh
mise install                          # Rust toolchain and tools
mise bootstrap packages apply         # libsane-dev, pkg-config, libclang (Debian, Ubuntu)
mise run check                        # format, clippy, tests, example
mise run miri                         # unit tests under Miri
mise run doc                          # documentation as docs.rs builds it
```

Pull request titles follow [Conventional Commits](https://www.conventionalcommits.org)
and become the changelog; releases are described in [RELEASING.md](RELEASING.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.

`sane-sys/include/sane/sane.h` is an unmodified copy of SANE's public-domain
header, used only to build the documentation on docs.rs.
