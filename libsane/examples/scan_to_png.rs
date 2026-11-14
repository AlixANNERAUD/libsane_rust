//! Lists the scanners, then scans one page to a PNG file.
//!
//! ```sh
//! cargo run --example scan_to_png --features image -- [DEVICE] [OUTPUT.png]
//! ```
//!
//! Without a device name, the first device found is used. Without a scanner,
//! enable SANE's virtual `test` backend (uncomment `test` in
//! `/etc/sane.d/dll.conf`, or point `SANE_CONFIG_DIR` to a directory whose
//! `dll.conf` lists it, like `libsane/tests/sane.d`) and use `test:0`.

use std::error::Error;
use std::process::ExitCode;

use libsane::{Constraint, Sane, ValueType, well_known};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let device_name = args.next();
    let output = args.next().unwrap_or_else(|| "scan.png".to_owned());

    let sane = Sane::init()?;
    println!("SANE {}", sane.version());

    let devices = sane.devices(false)?;
    if devices.is_empty() {
        println!("No scanner found.");
    }
    for device in &devices {
        println!(
            "- {}: {} {} ({})",
            device.name, device.vendor, device.model, device.kind
        );
    }

    let mut device = match &device_name {
        Some(name) => sane.open(name)?,
        None => match devices.first() {
            Some(first) => sane.open_device(first)?,
            None => return Err("no device to scan with".into()),
        },
    };
    println!("Scanning with {}", device.name());

    // Ask for a color scan at 150 dpi when the device offers it; keep the
    // defaults otherwise.
    let color = device.option(well_known::MODE).is_some_and(|mode| {
        mode.is_settable()
            && matches!(&mode.constraint, Constraint::StringList(modes) if modes.iter().any(|m| m == well_known::MODE_COLOR))
    });
    if color {
        device.set_value(well_known::MODE, well_known::MODE_COLOR)?;
    }
    let resolution = device
        .option(well_known::RESOLUTION)
        .is_some_and(|resolution| {
            resolution.is_settable()
                && matches!(resolution.value_type, ValueType::Int | ValueType::Fixed)
        });
    if resolution {
        if let Err(error) = device.set_value(well_known::RESOLUTION, 150) {
            eprintln!("Keeping the default resolution: {error}");
        }
    }

    let frame = device.scan()?;
    let parameters = frame.parameters;
    println!(
        "Scanned {}x{} pixels, {:?}, {} bits per sample",
        frame.width(),
        frame.height(),
        parameters.format,
        parameters.depth
    );

    frame.to_image()?.save(&output)?;
    println!("Saved {output}");
    Ok(())
}
