# libsane

[![crates.io](https://img.shields.io/crates/v/libsane.svg)](https://crates.io/crates/libsane)
[![docs.rs](https://img.shields.io/docsrs/libsane)](https://docs.rs/libsane)

Safe, idiomatic Rust bindings to
[SANE (Scanner Access Now Easy)](http://www.sane-project.org/), the scanner
library of Linux and other Unix systems.

- `Sane` initializes the library, once per process at a time, and finds and
  opens devices; dropping it calls `sane_exit`.
- `Device` reads and sets options with typed values (bool, int, fixed, string,
  button), constraints (range, word list, string list) and capabilities, and
  reloads its option descriptors when the backend asks for it.
- `Scan` reads the image data in chunks (`std::io::Read`) or whole, as a
  `Frame` of raw bytes and parameters; three-pass scans are merged into one RGB
  frame. With the `image` feature, `Frame::to_image` returns an
  `image::DynamicImage`.
- Every `SANE_Status` maps to a variant of `Error`; nothing panics across the
  FFI boundary.

```rust
use libsane::{Sane, well_known};

let sane = Sane::init()?;
for device in sane.devices(false)? {
    println!("{}: {} {} ({})", device.name, device.vendor, device.model, device.kind);
}

let mut device = sane.open("test:0")?;
device.set_value(well_known::MODE, well_known::MODE_GRAY)?;
device.set_value(well_known::RESOLUTION, 150)?;
let frame = device.scan()?;
println!("{}x{} pixels, {} bytes", frame.width(), frame.height(), frame.data.len());
```

The API is blocking: from async code, run the SANE session on a blocking
thread, for example with `tokio::task::spawn_blocking`.

## System requirements

libsane and its development files, pkg-config and libclang, for example
`sudo apt install libsane-dev pkg-config libclang-dev` on Debian and Ubuntu.
See [`sane-sys`](https://crates.io/crates/sane-sys) for other systems.

Without a scanner, SANE's `test` backend provides virtual devices (`test:0`,
`test:1`): uncomment `test` in `/etc/sane.d/dll.conf`, or point
`SANE_CONFIG_DIR` to a directory whose `dll.conf` lists it.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option.
