//! Errors: one variant per `SANE_Status`, plus the errors of the wrapper.

use std::fmt;
use std::io;

use sane_sys as sys;

use crate::option::ValueType;

/// A specialized [`Result`](std::result::Result) for SANE operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// An error returned by SANE or by this crate.
///
/// The first variants map the `SANE_Status` codes one to one (see
/// [`Error::from_status`] and [`Error::status`]); the others are reported by
/// this crate before or after calling SANE.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// `SANE_STATUS_UNSUPPORTED`: the operation is not supported.
    Unsupported,
    /// `SANE_STATUS_CANCELLED`: the operation was cancelled.
    Cancelled,
    /// `SANE_STATUS_DEVICE_BUSY`: the device is busy, try again later.
    DeviceBusy,
    /// `SANE_STATUS_INVAL`: the data or argument is invalid (this includes
    /// opening a device that does not exist).
    Invalid,
    /// `SANE_STATUS_EOF`: no more data is available.
    Eof,
    /// `SANE_STATUS_JAMMED`: the document feeder is jammed.
    Jammed,
    /// `SANE_STATUS_NO_DOCS`: the document feeder is out of documents.
    NoDocs,
    /// `SANE_STATUS_COVER_OPEN`: the scanner cover is open.
    CoverOpen,
    /// `SANE_STATUS_IO_ERROR`: an error occurred during device I/O.
    Io,
    /// `SANE_STATUS_NO_MEM`: out of memory.
    NoMem,
    /// `SANE_STATUS_ACCESS_DENIED`: access to the resource was denied.
    AccessDenied,
    /// `SANE_STATUS_WARMING_UP` (12): the lamp is not ready, retry later.
    WarmingUp,
    /// `SANE_STATUS_HW_LOCKED` (13): the scanner mechanism is locked for
    /// transport.
    HwLocked,
    /// A status code this crate does not know.
    UnknownStatus(u32),

    /// SANE is already initialized in this process: only one [`Sane`]
    /// guard may exist at a time.
    ///
    /// [`Sane`]: crate::Sane
    AlreadyInitialized,
    /// A string passed to SANE contains a NUL byte.
    NulByte,
    /// The device has no option with this name or index.
    UnknownOption(String),
    /// The option is inactive: it cannot be read or set until another option
    /// makes it active.
    InactiveOption(String),
    /// The option cannot be set by software.
    ReadOnlyOption(String),
    /// The option cannot be read by software (it has no value, or is only
    /// set through the hardware).
    WriteOnlyOption(String),
    /// The value does not match the type of the option.
    TypeMismatch {
        /// The name of the option.
        option: String,
        /// The type of the option.
        expected: ValueType,
    },
    /// The value has the right type but does not fit the option (a string
    /// too long, an array of the wrong length).
    InvalidValue {
        /// The name of the option.
        option: String,
        /// What is wrong with the value.
        reason: String,
    },
    /// The frame format or depth cannot be handled by this operation.
    UnsupportedFrame {
        /// The format of the frame.
        format: crate::FrameFormat,
        /// The number of bits per sample.
        depth: u32,
    },
    /// The backend broke the SANE protocol (negative sizes, inconsistent
    /// frames...).
    Protocol(&'static str),
    /// The operation is not valid in the current state.
    InvalidState(&'static str),
}

impl Error {
    /// The error for a `SANE_Status`, or `None` for `SANE_STATUS_GOOD`.
    pub fn from_status(status: sys::SANE_Status) -> Option<Self> {
        Some(match status {
            sys::SANE_STATUS_GOOD => return None,
            sys::SANE_STATUS_UNSUPPORTED => Self::Unsupported,
            sys::SANE_STATUS_CANCELLED => Self::Cancelled,
            sys::SANE_STATUS_DEVICE_BUSY => Self::DeviceBusy,
            sys::SANE_STATUS_INVAL => Self::Invalid,
            sys::SANE_STATUS_EOF => Self::Eof,
            sys::SANE_STATUS_JAMMED => Self::Jammed,
            sys::SANE_STATUS_NO_DOCS => Self::NoDocs,
            sys::SANE_STATUS_COVER_OPEN => Self::CoverOpen,
            sys::SANE_STATUS_IO_ERROR => Self::Io,
            sys::SANE_STATUS_NO_MEM => Self::NoMem,
            sys::SANE_STATUS_ACCESS_DENIED => Self::AccessDenied,
            sys::SANE_STATUS_WARMING_UP => Self::WarmingUp,
            sys::SANE_STATUS_HW_LOCKED => Self::HwLocked,
            other => Self::UnknownStatus(other),
        })
    }

    /// The `SANE_Status` of this error, or `None` for the errors of this
    /// crate.
    pub fn status(&self) -> Option<sys::SANE_Status> {
        Some(match self {
            Self::Unsupported => sys::SANE_STATUS_UNSUPPORTED,
            Self::Cancelled => sys::SANE_STATUS_CANCELLED,
            Self::DeviceBusy => sys::SANE_STATUS_DEVICE_BUSY,
            Self::Invalid => sys::SANE_STATUS_INVAL,
            Self::Eof => sys::SANE_STATUS_EOF,
            Self::Jammed => sys::SANE_STATUS_JAMMED,
            Self::NoDocs => sys::SANE_STATUS_NO_DOCS,
            Self::CoverOpen => sys::SANE_STATUS_COVER_OPEN,
            Self::Io => sys::SANE_STATUS_IO_ERROR,
            Self::NoMem => sys::SANE_STATUS_NO_MEM,
            Self::AccessDenied => sys::SANE_STATUS_ACCESS_DENIED,
            Self::WarmingUp => sys::SANE_STATUS_WARMING_UP,
            Self::HwLocked => sys::SANE_STATUS_HW_LOCKED,
            Self::UnknownStatus(status) => *status,
            _ => return None,
        })
    }
}

/// Turns a `SANE_Status` into a `Result`.
pub(crate) fn check(status: sys::SANE_Status) -> Result<()> {
    match Error::from_status(status) {
        None => Ok(()),
        Some(error) => Err(error),
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("operation is not supported"),
            Self::Cancelled => f.write_str("operation was cancelled"),
            Self::DeviceBusy => f.write_str("device is busy, try again later"),
            Self::Invalid => f.write_str("invalid argument or data"),
            Self::Eof => f.write_str("no more data available"),
            Self::Jammed => f.write_str("document feeder jammed"),
            Self::NoDocs => f.write_str("document feeder out of documents"),
            Self::CoverOpen => f.write_str("scanner cover is open"),
            Self::Io => f.write_str("error during device I/O"),
            Self::NoMem => f.write_str("out of memory"),
            Self::AccessDenied => f.write_str("access to resource has been denied"),
            Self::WarmingUp => f.write_str("lamp not ready, please retry"),
            Self::HwLocked => f.write_str("scanner mechanism locked for transport"),
            Self::UnknownStatus(status) => write!(f, "unknown SANE status {status}"),
            Self::AlreadyInitialized => f.write_str("SANE is already initialized in this process"),
            Self::NulByte => f.write_str("string contains a NUL byte"),
            Self::UnknownOption(option) => write!(f, "unknown option `{option}`"),
            Self::InactiveOption(option) => write!(f, "option `{option}` is inactive"),
            Self::ReadOnlyOption(option) => write!(f, "option `{option}` cannot be set"),
            Self::WriteOnlyOption(option) => write!(f, "option `{option}` cannot be read"),
            Self::TypeMismatch { option, expected } => {
                write!(f, "option `{option}` expects a value of type {expected:?}")
            }
            Self::InvalidValue { option, reason } => {
                write!(f, "invalid value for option `{option}`: {reason}")
            }
            Self::UnsupportedFrame { format, depth } => {
                write!(f, "unsupported frame: {format:?} with depth {depth}")
            }
            Self::Protocol(message) => write!(f, "backend broke the SANE protocol: {message}"),
            Self::InvalidState(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        let kind = match error {
            Error::Unsupported => io::ErrorKind::Unsupported,
            Error::Invalid | Error::NulByte => io::ErrorKind::InvalidInput,
            Error::Eof => io::ErrorKind::UnexpectedEof,
            Error::NoMem => io::ErrorKind::OutOfMemory,
            Error::AccessDenied => io::ErrorKind::PermissionDenied,
            Error::DeviceBusy => io::ErrorKind::ResourceBusy,
            Error::Protocol(_) => io::ErrorKind::InvalidData,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_round_trips() {
        assert_eq!(Error::from_status(sys::SANE_STATUS_GOOD), None);
        for status in 1..=20 {
            let error = Error::from_status(status).unwrap();
            assert_eq!(error.status(), Some(status), "{error:?}");
        }
        assert_eq!(
            Error::from_status(sys::SANE_STATUS_JAMMED),
            Some(Error::Jammed)
        );
        assert_eq!(Error::from_status(42), Some(Error::UnknownStatus(42)));
        assert_eq!(Error::AlreadyInitialized.status(), None);
    }

    #[test]
    fn check_maps_good_to_ok() {
        assert_eq!(check(sys::SANE_STATUS_GOOD), Ok(()));
        assert_eq!(check(sys::SANE_STATUS_NO_DOCS), Err(Error::NoDocs));
    }

    #[test]
    fn io_error_keeps_the_sane_error() {
        let error = io::Error::from(Error::AccessDenied);
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        let inner = error.into_inner().unwrap().downcast::<Error>().unwrap();
        assert_eq!(*inner, Error::AccessDenied);
    }
}
