//! An open device: options and scanning.

use std::fmt;

use crate::error::{Error, Result};
use crate::option::{Buffer, OptionDescriptor, OptionId, SetInfo, Value, ValueType};
use crate::raw::Handle;
use crate::{Frame, Parameters, Scan};

/// An open device, closed on drop (`sane_close`).
///
/// The device keeps a copy of its option descriptors, returned by
/// [`options`](Self::options). When setting an option makes the backend
/// report `SANE_INFO_RELOAD_OPTIONS`, the copy is reloaded before the setter
/// returns, so it is always current.
///
/// ```no_run
/// use libsane::{Sane, well_known};
///
/// let sane = Sane::init()?;
/// let mut device = sane.open("test:0")?;
/// device.set_value(well_known::MODE, well_known::MODE_COLOR)?;
/// device.set_value(well_known::RESOLUTION, 300)?;
/// let frame = device.scan()?;
/// println!("{}x{} pixels", frame.width(), frame.height());
/// # Ok::<(), libsane::Error>(())
/// ```
pub struct Device<'s> {
    pub(crate) handle: Handle<'s>,
    name: String,
    options: Vec<OptionDescriptor>,
}

impl<'s> Device<'s> {
    pub(crate) fn new(name: String, handle: Handle<'s>) -> Result<Self> {
        let mut device = Self {
            handle,
            name,
            options: Vec::new(),
        };
        device.reload_options()?;
        Ok(device)
    }

    /// The name the device was opened with.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The descriptors of the options of the device, in order, without
    /// option 0 (the number of options).
    pub fn options(&self) -> &[OptionDescriptor] {
        &self.options
    }

    /// The descriptor of an option, by index or name.
    pub fn option(&self, id: impl OptionId) -> Option<&OptionDescriptor> {
        id.find_in(&self.options)
    }

    /// Reads the option descriptors again from the backend.
    ///
    /// The setters do it when the backend asks for it; this is only needed
    /// if the options changed some other way.
    pub fn reload_options(&mut self) -> Result<()> {
        // Option 0 holds the number of options, itself included.
        let (_, count) = self.handle.get_value(0)?;
        let count = match count {
            Buffer::Words(words) => words.first().copied().unwrap_or(0),
            Buffer::Bytes(_) => return Err(Error::Protocol("option 0 is not an integer")),
        };
        let count =
            usize::try_from(count).map_err(|_| Error::Protocol("negative number of options"))?;
        self.options = (1..count)
            .map_while(|index| self.handle.descriptor(index))
            .collect();
        Ok(())
    }

    fn resolve(&self, id: &impl OptionId) -> Result<&OptionDescriptor> {
        id.find_in(&self.options)
            .ok_or_else(|| Error::UnknownOption(id.label()))
    }

    /// Reads the value of an option, by index or name.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownOption`], [`Error::InactiveOption`],
    /// [`Error::WriteOnlyOption`] (buttons, groups, options without
    /// `SOFT_DETECT`), or the error returned by the backend.
    pub fn value(&self, id: impl OptionId) -> Result<Value> {
        let option = self.resolve(&id)?;
        if !option.is_active() {
            return Err(Error::InactiveOption(option.label()));
        }
        if !option.is_readable()
            || matches!(option.value_type, ValueType::Button | ValueType::Group)
        {
            return Err(Error::WriteOnlyOption(option.label()));
        }
        let (current, buffer) = self.handle.get_value(option.index)?;
        buffer.decode(&current)
    }

    /// Sets the value of an option, by index or name.
    ///
    /// The value converts from `bool`, `i32`, `f64`, [`Fixed`](crate::Fixed),
    /// strings and vectors (see [`Value`]). The backend may round it: when
    /// [`SetInfo::inexact`] is set, read it back with
    /// [`value`](Self::value).
    ///
    /// # Errors
    ///
    /// [`Error::UnknownOption`], [`Error::InactiveOption`],
    /// [`Error::ReadOnlyOption`], [`Error::TypeMismatch`],
    /// [`Error::InvalidValue`], [`Error::NulByte`], or the error returned by
    /// the backend (usually [`Error::Invalid`] for a value outside the
    /// constraint).
    pub fn set_value(&mut self, id: impl OptionId, value: impl Into<Value>) -> Result<SetInfo> {
        let option = self.settable(&id)?;
        let buffer = Buffer::encode(option, &value.into())?;
        let index = option.index;
        let info = self.handle.set_value(index, buffer)?;
        self.after_set(info)
    }

    /// Lets the backend choose the value of an option
    /// (`SANE_ACTION_SET_AUTO`), for options with
    /// [`Capabilities::AUTOMATIC`](crate::Capabilities::AUTOMATIC).
    pub fn set_auto(&mut self, id: impl OptionId) -> Result<SetInfo> {
        let index = self.settable(&id)?.index;
        let info = self.handle.set_auto(index)?;
        self.after_set(info)
    }

    /// Presses a button option.
    pub fn press(&mut self, id: impl OptionId) -> Result<SetInfo> {
        let option = self.settable(&id)?;
        if option.value_type != ValueType::Button {
            return Err(Error::TypeMismatch {
                option: option.label(),
                expected: option.value_type,
            });
        }
        let index = option.index;
        let info = self.handle.press(index)?;
        self.after_set(info)
    }

    fn settable(&self, id: &impl OptionId) -> Result<&OptionDescriptor> {
        let option = self.resolve(id)?;
        if !option.is_active() {
            return Err(Error::InactiveOption(option.label()));
        }
        if !option.is_settable() {
            return Err(Error::ReadOnlyOption(option.label()));
        }
        Ok(option)
    }

    /// Handles `SANE_INFO_RELOAD_OPTIONS`.
    fn after_set(&mut self, info: SetInfo) -> Result<SetInfo> {
        if info.reload_options() {
            self.reload_options()?;
        }
        Ok(info)
    }

    /// The parameters of the next scan (`sane_get_parameters`): an estimate
    /// that changes with the options. [`Scan::parameters`] gives the exact
    /// ones.
    pub fn parameters(&self) -> Result<Parameters> {
        self.handle.parameters()
    }

    /// Starts scanning (`sane_start`).
    ///
    /// The returned [`Scan`] reads the image data; dropping it cancels the
    /// scan. With a document feeder, this fails with [`Error::NoDocs`] when
    /// the feeder is empty.
    pub fn start(&mut self) -> Result<Scan<'_, 's>> {
        self.handle.start()?;
        Scan::new(self)
    }

    /// Scans one image and returns its data: starts a scan, reads every frame
    /// (merging three-pass scans into one RGB frame), then ends the scan.
    pub fn scan(&mut self) -> Result<Frame> {
        self.start()?.read_image()
    }
}

impl fmt::Debug for Device<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("name", &self.name)
            .field("options", &self.options.len())
            .finish_non_exhaustive()
    }
}
