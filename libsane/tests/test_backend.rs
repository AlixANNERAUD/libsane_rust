//! Tests against SANE's virtual `test` backend: `tests/sane.d` enables only
//! that backend, and `tests/sane.d/test.conf` configures it.
//!
//! SANE is initialized once per process at a time, so every test takes a
//! lock before initializing it.

use std::io::Read;
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

use libsane::{
    Capabilities, Constraint, Error, Fixed, FrameFormat, Range, Sane, Unit, Value, ValueType,
    well_known,
};

static LOCK: Mutex<()> = Mutex::new(());
static ENV: Once = Once::new();

/// Takes the process-wide test lock, after pointing SANE to the test
/// configuration (libsane reads `SANE_CONFIG_DIR` once per process, at the
/// first `sane_init`).
fn lock() -> MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    ENV.call_once(|| {
        // SAFETY: called under LOCK, before libsane runs in this process;
        // the other test threads wait for LOCK.
        unsafe {
            std::env::set_var(
                "SANE_CONFIG_DIR",
                concat!(env!("CARGO_MANIFEST_DIR"), "/tests/sane.d"),
            );
        }
    });
    guard
}

/// Runs `test` with an initialized SANE, alone in the process.
fn with_sane<R>(test: impl FnOnce(&Sane) -> R) -> R {
    let _guard = lock();
    let sane = Sane::init().expect("sane_init failed");
    test(&sane)
}

#[test]
fn init_is_exclusive_and_can_be_repeated() {
    with_sane(|sane| {
        assert_eq!(sane.version().major, 1);
        assert_eq!(Sane::init().unwrap_err(), Error::AlreadyInitialized);
    });
    // Dropping the guard released SANE.
    with_sane(|_| {});
}

#[test]
fn lists_the_test_devices() {
    with_sane(|sane| {
        let devices = sane.devices(true).unwrap();
        let names: Vec<_> = devices.iter().map(|device| device.name.as_str()).collect();
        assert_eq!(names, ["test:0", "test:1"]);
        let device = &devices[0];
        assert_eq!(device.vendor, "Noname");
        assert_eq!(device.model, "frontend-tester");
        assert_eq!(device.kind, "virtual device");
        let opened = sane.open_device(device).unwrap();
        assert_eq!(opened.name(), "test:0");
    });
}

#[test]
fn open_errors() {
    with_sane(|sane| {
        assert_eq!(sane.open("nonexistent:0").unwrap_err(), Error::Invalid);
        assert_eq!(sane.open("test\0").unwrap_err(), Error::NulByte);
        // Two devices can be open at once.
        let _first = sane.open("test:0").unwrap();
        let _second = sane.open("test:1").unwrap();
    });
}

#[test]
fn describes_options_and_constraints() {
    with_sane(|sane| {
        let device = sane.open("test:0").unwrap();
        let options = device.options();
        assert!(options.iter().all(|option| option.index > 0));
        assert!(
            options
                .iter()
                .any(|option| option.value_type == ValueType::Group)
        );

        let mode = device.option(well_known::MODE).unwrap();
        assert_eq!(mode.value_type, ValueType::String);
        assert_eq!(
            mode.constraint,
            Constraint::StringList(vec!["Gray".into(), "Color".into()])
        );
        assert!(mode.is_active() && mode.is_settable() && mode.is_readable());

        let depth = device.option(well_known::DEPTH).unwrap();
        assert_eq!(depth.constraint, Constraint::IntList(vec![1, 8, 16]));

        let resolution = device.option(well_known::RESOLUTION).unwrap();
        assert_eq!(resolution.unit, Unit::Dpi);
        assert_eq!(resolution.value_type, ValueType::Fixed);
        assert!(matches!(
            resolution.constraint,
            Constraint::FixedRange(Range { min, max, .. })
                if min == Fixed::from(1) && max == Fixed::from(1200)
        ));

        let fixed = device.option("fixed-constraint-range").unwrap();
        assert_eq!(fixed.value_type, ValueType::Fixed);
        assert!(matches!(fixed.constraint, Constraint::FixedRange(_)));
        let fixed_list = device.option("fixed-constraint-word-list").unwrap();
        assert!(
            matches!(&fixed_list.constraint, Constraint::FixedList(values) if values.contains(&Fixed::from(42)))
        );

        let array = device.option("int-constraint-array").unwrap();
        assert!(array.len() > 1);

        let auto = device.option("bool-soft-select-soft-detect-auto").unwrap();
        assert!(
            auto.capabilities
                .contains(Capabilities::AUTOMATIC | Capabilities::ADVANCED)
        );

        // Lookup by index and by descriptor.
        assert_eq!(device.option(mode.index).unwrap().name, "mode");
        assert_eq!(device.option(mode).unwrap().index, mode.index);
        assert!(device.option("no-such-option").is_none());
    });
}

#[test]
fn reads_and_sets_typed_values() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();

        assert_eq!(device.value("int").unwrap(), Value::Int(42));
        device.set_value("int", -7).unwrap();
        assert_eq!(device.value("int").unwrap(), Value::Int(-7));

        assert_eq!(
            device.value("bool-soft-select-soft-detect").unwrap(),
            Value::Bool(false)
        );
        device
            .set_value("bool-soft-select-soft-detect", true)
            .unwrap();
        assert_eq!(
            device.value("bool-soft-select-soft-detect").unwrap(),
            Value::Bool(true)
        );

        device.set_value("fixed", 1.5).unwrap();
        assert_eq!(
            device.value("fixed").unwrap(),
            Value::Fixed(Fixed::from_f64(1.5))
        );
        // An integer is accepted for a fixed option.
        device.set_value("fixed", 3).unwrap();
        assert_eq!(device.value("fixed").unwrap().as_f64(), Some(3.0));

        device.set_value("string", "hello").unwrap();
        assert_eq!(
            device.value("string").unwrap(),
            Value::String("hello".into())
        );
        device
            .set_value("string-constraint-string-list", "Second entry")
            .unwrap();
        assert_eq!(
            device
                .value("string-constraint-string-list")
                .unwrap()
                .as_str(),
            Some("Second entry")
        );

        let len = device.option("int-constraint-array").unwrap().len();
        let values: Vec<i32> = (0..len as i32).collect();
        device
            .set_value("int-constraint-array", values.clone())
            .unwrap();
        assert_eq!(
            device.value("int-constraint-array").unwrap(),
            Value::IntArray(values)
        );

        let info = device.set_value("int-inexact", 100).unwrap();
        assert!(info.inexact());

        device.press("button").unwrap();
        device
            .set_auto("bool-soft-select-soft-detect-auto")
            .unwrap();
    });
}

#[test]
fn rejects_invalid_values() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();
        assert!(matches!(
            device.set_value("int", "text"),
            Err(Error::TypeMismatch {
                expected: ValueType::Int,
                ..
            })
        ));
        assert!(matches!(
            device.set_value("int-constraint-array", 1),
            Err(Error::InvalidValue { .. })
        ));
        let size = device.option("string").unwrap().size;
        assert!(matches!(
            device.set_value("string", "x".repeat(size)),
            Err(Error::InvalidValue { .. })
        ));
        assert_eq!(
            device.set_value("mode", "Sepia").unwrap_err(),
            Error::Invalid
        );
        assert!(
            matches!(device.set_value("no-such-option", 1), Err(Error::UnknownOption(name)) if name == "no-such-option")
        );
        assert!(matches!(
            device.value("button"),
            Err(Error::WriteOnlyOption(_))
        ));
        assert!(matches!(
            device.press("int"),
            Err(Error::TypeMismatch { .. })
        ));
        // Inactive in gray mode.
        assert!(matches!(
            device.set_value("three-pass", true),
            Err(Error::InactiveOption(_))
        ));
        assert!(matches!(
            device.value("three-pass"),
            Err(Error::InactiveOption(_))
        ));
    });
}

#[test]
fn reloads_options_when_the_backend_asks() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();
        let before = device.option("three-pass").unwrap().clone();
        assert!(!before.is_active());

        let info = device
            .set_value(well_known::MODE, well_known::MODE_COLOR)
            .unwrap();
        assert!(info.reload_options());
        assert!(info.reload_params());
        // The device reloaded its descriptors: three-pass is now active.
        assert!(device.option("three-pass").unwrap().is_active());
        device.set_value("three-pass", true).unwrap();

        device.reload_options().unwrap();
        assert!(device.option("three-pass").unwrap().is_active());
    });
}

#[test]
fn scans_a_gray_frame() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();
        let estimate = device.parameters().unwrap();
        let frame = device.scan().unwrap();
        let parameters = frame.parameters;
        assert_eq!(parameters.format, FrameFormat::Gray);
        assert_eq!(parameters.depth, 8);
        assert!(parameters.last_frame);
        assert_eq!(parameters.pixels_per_line, estimate.pixels_per_line);
        assert_eq!(parameters.lines, estimate.lines);
        assert_eq!(frame.data.len(), parameters.bytes_per_line * frame.height());
        // Solid black.
        assert!(frame.data.iter().all(|&byte| byte == 0));

        // The device can scan again.
        device.set_value("test-picture", "Solid white").unwrap();
        let frame = device.scan().unwrap();
        assert!(frame.data.iter().all(|&byte| byte == 0xff));
    });
}

#[test]
fn reads_in_chunks_and_cancels() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();
        device.set_value("read-limit", true).unwrap();
        device.set_value("read-limit-size", 100).unwrap();

        let mut scan = device.start().unwrap();
        let total = scan.parameters().bytes_per_line * scan.parameters().lines.unwrap();
        let mut chunk = [0; 1000];
        let len = scan.read(&mut chunk).unwrap();
        assert!(len > 0 && len <= 100);
        // `io::Read` reads the rest of the frame.
        let mut rest = Vec::new();
        scan.read_to_end(&mut rest).unwrap();
        assert_eq!(len + rest.len(), total);
        assert!(scan.is_frame_done());
        assert_eq!(scan.read(&mut chunk).unwrap(), 0);
        assert!(!scan.next_frame().unwrap());
        drop(scan);

        // Cancel in the middle of a frame, then scan again.
        let mut scan = device.start().unwrap();
        assert!(scan.read(&mut chunk).unwrap() > 0);
        assert!(matches!(scan.next_frame(), Err(Error::InvalidState(_))));
        scan.cancel();
        assert_eq!(device.scan().unwrap().data.len(), total);
    });
}

#[test]
fn scans_color_at_every_depth() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();
        device
            .set_value(well_known::MODE, well_known::MODE_COLOR)
            .unwrap();
        device.set_value("test-picture", "Color pattern").unwrap();
        for depth in [1, 8, 16] {
            device.set_value(well_known::DEPTH, depth).unwrap();
            let frame = device.scan().unwrap();
            assert_eq!(frame.parameters.format, FrameFormat::Rgb);
            assert_eq!(frame.parameters.depth, depth as u32);
            // 1-bit lines hold one byte per channel for each group of 8 pixels.
            let expected = match depth {
                1 => frame.width().div_ceil(8) * 3,
                _ => frame.width() * 3 * depth as usize / 8,
            };
            assert_eq!(frame.parameters.bytes_per_line, expected);
            assert_eq!(
                frame.data.len(),
                frame.parameters.bytes_per_line * frame.height()
            );
        }
    });
}

/// The samples of every line, without the padding bits at the end of 1-bit
/// lines (backends may leave them set).
fn pixels(frame: &libsane::Frame) -> Vec<Vec<u8>> {
    let width = frame.width();
    (0..frame.height())
        .map(|y| {
            let mut line = frame.line(y).to_vec();
            if frame.parameters.depth == 1 && width % 8 != 0 {
                let channels = frame.parameters.format.channels();
                let last = width / 8 * channels;
                for byte in &mut line[last..] {
                    *byte &= !(0xff >> (width % 8));
                }
            }
            line
        })
        .collect()
}

#[test]
fn merges_three_pass_scans() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();
        device
            .set_value(well_known::MODE, well_known::MODE_COLOR)
            .unwrap();
        device.set_value("test-picture", "Color pattern").unwrap();
        for depth in [1, 8, 16] {
            device.set_value(well_known::DEPTH, depth).unwrap();
            device.set_value("three-pass", false).unwrap();
            let single_pass = device.scan().unwrap();

            device.set_value("three-pass", true).unwrap();
            device.set_value("three-pass-order", "GBR").unwrap();
            let scan = device.start().unwrap();
            assert_eq!(scan.parameters().format, FrameFormat::Green);
            assert!(!scan.parameters().last_frame);
            drop(scan);

            let three_pass = device.scan().unwrap();
            assert_eq!(three_pass.parameters.format, FrameFormat::Rgb);
            assert!(three_pass.parameters.last_frame);
            assert_eq!(
                three_pass.parameters, single_pass.parameters,
                "depth {depth}"
            );
            assert_eq!(pixels(&three_pass), pixels(&single_pass), "depth {depth}");
        }
    });
}

#[test]
fn handles_unknown_line_counts() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();
        device.set_value("hand-scanner", true).unwrap();
        assert_eq!(device.parameters().unwrap().lines, None);
        let frame = device.scan().unwrap();
        assert!(frame.height() > 0);
        assert_eq!(
            frame.data.len(),
            frame.parameters.bytes_per_line * frame.height()
        );
    });
}

#[test]
fn maps_read_statuses_to_errors() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();
        let cases = [
            ("SANE_STATUS_JAMMED", Error::Jammed),
            ("SANE_STATUS_NO_DOCS", Error::NoDocs),
            ("SANE_STATUS_COVER_OPEN", Error::CoverOpen),
            ("SANE_STATUS_IO_ERROR", Error::Io),
            ("SANE_STATUS_NO_MEM", Error::NoMem),
            ("SANE_STATUS_ACCESS_DENIED", Error::AccessDenied),
            ("SANE_STATUS_DEVICE_BUSY", Error::DeviceBusy),
            ("SANE_STATUS_UNSUPPORTED", Error::Unsupported),
            ("SANE_STATUS_CANCELLED", Error::Cancelled),
            ("SANE_STATUS_INVAL", Error::Invalid),
        ];
        for (status, error) in cases {
            device.set_value("read-return-value", status).unwrap();
            assert_eq!(device.scan().unwrap_err(), error, "{status}");
        }
        // EOF ends the frame at once: an empty image.
        device
            .set_value("read-return-value", "SANE_STATUS_EOF")
            .unwrap();
        assert_eq!(device.scan().unwrap().height(), 0);
    });
}

#[test]
fn feeds_pages_until_the_feeder_is_empty() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();
        device
            .set_value(well_known::SOURCE, "Automatic Document Feeder")
            .unwrap();
        let mut scan = device.start().unwrap();
        let mut pages = 0;
        loop {
            scan.read_image().unwrap();
            pages += 1;
            if !scan.next_page().unwrap() {
                break;
            }
        }
        // The test backend's feeder holds 10 pages.
        assert_eq!(pages, 10);
    });
}

#[test]
fn authorization_callback_is_installed() {
    let _guard = lock();
    let sane =
        Sane::init_with_auth(|_resource| Some(libsane::Credentials::new("user", "password")))
            .unwrap();
    assert_eq!(Sane::init().unwrap_err(), Error::AlreadyInitialized);
    drop(sane);
}

#[cfg(feature = "image")]
#[test]
fn converts_frames_to_images() {
    with_sane(|sane| {
        let mut device = sane.open("test:0").unwrap();
        device.set_value("test-picture", "Grid").unwrap();
        let frame = device.scan().unwrap();
        let image = frame.to_image().unwrap();
        assert_eq!(image.width() as usize, frame.width());
        assert_eq!(image.height() as usize, frame.height());
        assert!(matches!(image, image::DynamicImage::ImageLuma8(_)));

        device
            .set_value(well_known::MODE, well_known::MODE_COLOR)
            .unwrap();
        device.set_value(well_known::DEPTH, 16).unwrap();
        let image = device.scan().unwrap().to_image().unwrap();
        assert!(matches!(image, image::DynamicImage::ImageRgb16(_)));

        device
            .set_value(well_known::MODE, well_known::MODE_GRAY)
            .unwrap();
        device.set_value(well_known::DEPTH, 1).unwrap();
        let image = device.scan().unwrap().to_image().unwrap().into_luma8();
        // The grid has black and white squares.
        assert!(image.pixels().any(|pixel| pixel.0 == [0]));
        assert!(image.pixels().any(|pixel| pixel.0 == [255]));
    });
}
