//! Scan documents and images from Rust: safe, idiomatic bindings to
//! [SANE (Scanner Access Now Easy)](http://www.sane-project.org/),
//! the scanner API of Linux, the BSDs and macOS, for flatbed scanners,
//! document feeders and multifunction printers.
//!
//! ```no_run
//! use libsane::{Sane, well_known};
//!
//! let sane = Sane::init()?;
//! for device in sane.devices(false)? {
//!     println!("{} ({} {}, {})", device.name, device.vendor, device.model, device.kind);
//! }
//!
//! let mut device = sane.open("test:0")?;
//! device.set_value(well_known::MODE, well_known::MODE_GRAY)?;
//! device.set_value(well_known::RESOLUTION, 150)?;
//! let frame = device.scan()?;
//! println!("{} bytes, {:?}", frame.data.len(), frame.parameters);
//! # Ok::<(), libsane::Error>(())
//! ```
//!
//! # Overview
//!
//! - [`Sane`] initializes the library (once per process at a time) and finds
//!   and opens devices. Dropping it calls `sane_exit`.
//! - [`Device`] reads and sets [options](OptionDescriptor) with typed
//!   [values](Value), and starts scans.
//! - [`Scan`] reads the image data in chunks (it implements [`std::io::Read`])
//!   or whole, as a [`Frame`]: raw bytes and their [`Parameters`]. With the
//!   `image` feature, [`Frame::to_image`] converts it to an
//!   `image::DynamicImage`.
//! - Every `SANE_Status` maps to a variant of [`Error`].
//!
//! # Blocking and threads
//!
//! Every call blocks, sometimes for seconds (finding devices, warming up the
//! lamp, scanning). From async code, run the whole SANE session on a blocking
//! thread, for example with `tokio::task::spawn_blocking` (see [`Sane`]).
//!
//! libsane is not thread-safe: a [`Sane`] can move to another thread, but
//! cannot be shared between threads, and its devices stay on its thread.
//!
//! # System requirements
//!
//! libsane and its development files, found with pkg-config, and libclang
//! (see the [`sane-sys`](sane_sys) crate). Without a scanner, SANE's `test`
//! backend provides virtual devices: enable it in `dll.conf`.
//!
//! # Features
//!
//! - `image`: [`Frame::to_image`].

#![deny(unsafe_code)]
#![warn(
    missing_docs,
    missing_debug_implementations,
    clippy::undocumented_unsafe_blocks
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod device;
mod error;
mod fixed;
mod frame;
mod option;
mod raw;
mod sane;
mod scan;

pub use device::Device;
pub use error::{Error, Result};
pub use fixed::Fixed;
pub use frame::{Frame, FrameFormat, Parameters};
pub use option::{
    Capabilities, Constraint, OptionDescriptor, OptionId, Range, SetInfo, Unit, Value, ValueType,
    well_known,
};
pub use sane::{AuthCallback, Credentials, DeviceInfo, Sane, Version};
pub use scan::Scan;

/// The raw FFI bindings this crate is built on.
pub use sane_sys as sys;
