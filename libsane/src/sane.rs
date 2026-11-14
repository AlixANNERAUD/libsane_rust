//! The SANE session: initialization, device discovery.

use std::ffi::CString;
use std::fmt;

use sane_sys as sys;

use crate::Device;
use crate::error::{Error, Result};
use crate::raw::Context;

/// The function called when a backend needs credentials for a resource (for
/// example a password-protected network scanner): receives the resource name,
/// returns the credentials, or `None` to send none.
pub type AuthCallback = dyn Fn(&str) -> Option<Credentials> + Send + Sync;

/// A username and password, returned by an [`AuthCallback`].
///
/// SANE truncates both to 127 bytes.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Credentials {
    /// The username.
    pub username: String,
    /// The password.
    pub password: String,
}

impl Credentials {
    /// Credentials with this username and password.
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// The version of libsane (`SANE_VERSION_CODE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// The major version: the version of the SANE standard (1).
    pub major: u8,
    /// The minor version.
    pub minor: u8,
    /// The build number.
    pub build: u16,
}

impl Version {
    fn from_code(code: sys::SANE_Int) -> Self {
        Self {
            major: sys::SANE_VERSION_MAJOR(code) as u8,
            minor: sys::SANE_VERSION_MINOR(code) as u8,
            build: sys::SANE_VERSION_BUILD(code) as u16,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.build)
    }
}

/// A device found by [`Sane::devices`] (`SANE_Device`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct DeviceInfo {
    /// The unique name of the device, to pass to [`Sane::open`] (for example
    /// `"test:0"` or `"pixma:04A91234_ABCDEF"`).
    pub name: String,
    /// The vendor (manufacturer).
    pub vendor: String,
    /// The model.
    pub model: String,
    /// The kind of device (for example `"flatbed scanner"`).
    pub kind: String,
    /// The name exactly as SANE reported it, which may not be UTF-8.
    pub(crate) raw_name: CString,
}

/// An initialized SANE library: the entry point of this crate.
///
/// SANE can be initialized once per process at a time: [`Sane::init`] fails
/// with [`Error::AlreadyInitialized`] while another `Sane` exists. Dropping
/// it calls `sane_exit`, after which SANE can be initialized again. Devices
/// borrow the `Sane` they were opened from, so they are closed first.
///
/// libsane is not thread-safe: `Sane` can be moved to another thread but not
/// shared, and the [`Device`]s it opens stay on its thread. All calls block;
/// from async code, run them on a blocking thread, for example with
/// `tokio::task::spawn_blocking`:
///
/// ```no_run
/// # async fn scan() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let frame = tokio::task::spawn_blocking(|| -> libsane::Result<_> {
///     let sane = libsane::Sane::init()?;
///     let mut device = sane.open("test:0")?;
///     device.scan()
/// })
/// .await??;
/// # Ok(()) }
/// # mod tokio { pub mod task { pub async fn spawn_blocking<T>(f: impl FnOnce() -> T) -> Result<T, std::io::Error> { Ok(f()) } } }
/// ```
pub struct Sane {
    context: Context,
    version: Version,
}

impl Sane {
    /// Initializes SANE (`sane_init`), without an authorization callback.
    ///
    /// # Errors
    ///
    /// [`Error::AlreadyInitialized`] if another `Sane` exists, or the error
    /// returned by `sane_init`.
    pub fn init() -> Result<Self> {
        Self::init_inner(None)
    }

    /// Initializes SANE (`sane_init`) with a callback that provides
    /// credentials to backends that ask for them.
    ///
    /// The callback runs on the thread that called into SANE. If it panics,
    /// the panic is caught and no credentials are sent.
    ///
    /// # Errors
    ///
    /// As for [`init`](Self::init).
    pub fn init_with_auth(
        callback: impl Fn(&str) -> Option<Credentials> + Send + Sync + 'static,
    ) -> Result<Self> {
        Self::init_inner(Some(Box::new(callback)))
    }

    fn init_inner(callback: Option<Box<AuthCallback>>) -> Result<Self> {
        let (context, code) = Context::init(callback)?;
        Ok(Self {
            context,
            version: Version::from_code(code),
        })
    }

    /// The version of libsane.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Lists the available devices (`sane_get_devices`).
    ///
    /// With `local_only`, devices reached through the network are left out.
    /// Finding devices can take several seconds.
    pub fn devices(&self, local_only: bool) -> Result<Vec<DeviceInfo>> {
        self.context.devices(local_only)
    }

    /// Opens a device by name (`sane_open`). An empty name opens the first
    /// available device.
    ///
    /// # Errors
    ///
    /// [`Error::NulByte`] if the name contains a NUL byte; usually
    /// [`Error::Invalid`] if there is no such device.
    pub fn open(&self, name: &str) -> Result<Device<'_>> {
        let raw_name = CString::new(name).map_err(|_| Error::NulByte)?;
        self.open_raw(name.to_owned(), &raw_name)
    }

    /// Opens a device found by [`devices`](Self::devices).
    pub fn open_device(&self, device: &DeviceInfo) -> Result<Device<'_>> {
        self.open_raw(device.name.clone(), &device.raw_name)
    }

    fn open_raw(&self, name: String, raw_name: &std::ffi::CStr) -> Result<Device<'_>> {
        Device::new(name, self.context.open(raw_name)?)
    }
}

impl fmt::Debug for Sane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sane")
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_from_code() {
        let version = Version::from_code(sys::SANE_VERSION_CODE(1, 3, 1));
        assert_eq!(
            version,
            Version {
                major: 1,
                minor: 3,
                build: 1
            }
        );
        assert_eq!(version.to_string(), "1.3.1");
    }

    #[test]
    fn credentials_hide_the_password() {
        let credentials = Credentials::new("alice", "secret");
        assert!(!format!("{credentials:?}").contains("secret"));
    }
}
