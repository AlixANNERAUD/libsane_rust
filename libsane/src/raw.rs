//! Every call into libsane, and the only module with `unsafe` code.
//!
//! Each function is small and states why its `unsafe` blocks are sound. The
//! invariants they rely on are carried by types:
//!
//! - a [`Context`] exists only while SANE is initialized, at most one at a
//!   time in the process, and is not `Sync`, so libsane is never called from
//!   two threads at once;
//! - a [`Handle`] borrows its `Context`, so it is closed before `sane_exit`,
//!   and is closed exactly once, on drop;
//! - option buffers are allocated from the size libsane reports for the
//!   option, right before the call that writes into them.
//!
//! Nothing here panics while libsane is on the stack: the authorization
//! callback, the only Rust function called by libsane, catches panics.

#![allow(unsafe_code)]

use std::cell::Cell;
use std::ffi::{CStr, CString, c_char, c_void};
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use sane_sys as sys;

use crate::error::{Error, Result, check};
use crate::option::{
    Buffer, Capabilities, Constraint, OptionDescriptor, Range, SetInfo, Unit, ValueType,
};
use crate::sane::{AuthCallback, Credentials, DeviceInfo};
use crate::{Fixed, Parameters};

/// Whether a [`Context`] exists.
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// The authorization callback of the current [`Context`].
static AUTH_CALLBACK: Mutex<Option<Box<AuthCallback>>> = Mutex::new(None);

/// Proof that SANE is initialized; calls `sane_exit` on drop.
pub(crate) struct Context {
    /// libsane is not thread-safe: `Send` but not `Sync`.
    _not_sync: PhantomData<Cell<()>>,
}

impl Context {
    /// Calls `sane_init`, unless a `Context` already exists.
    pub(crate) fn init(auth: Option<Box<AuthCallback>>) -> Result<(Self, sys::SANE_Int)> {
        if INITIALIZED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(Error::AlreadyInitialized);
        }
        let callback: sys::SANE_Auth_Callback = auth.is_some().then_some(auth_trampoline as _);
        *AUTH_CALLBACK.lock().unwrap_or_else(PoisonError::into_inner) = auth;

        let mut version: sys::SANE_Int = 0;
        // SAFETY: the flag above guarantees SANE is not initialized, and no
        // other thread can call libsane until this Context is created.
        // `version` is a valid out pointer; the callback is a valid function
        // for as long as SANE is initialized.
        let status = unsafe { sys::sane_init(&mut version, callback) };
        if let Err(error) = check(status) {
            AUTH_CALLBACK
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            INITIALIZED.store(false, Ordering::Release);
            return Err(error);
        }
        Ok((
            Self {
                _not_sync: PhantomData,
            },
            version,
        ))
    }

    /// Calls `sane_get_devices` and copies the list, which libsane owns.
    pub(crate) fn devices(&self, local_only: bool) -> Result<Vec<DeviceInfo>> {
        let mut list: *mut *const sys::SANE_Device = ptr::null_mut();
        // SAFETY: SANE is initialized (`self`); `list` is a valid out pointer.
        let status =
            unsafe { sys::sane_get_devices(&mut list as *mut _ as *mut _, local_only.into()) };
        check(status)?;
        let mut devices = Vec::new();
        if list.is_null() {
            return Ok(devices);
        }
        for index in 0.. {
            // SAFETY: the list is NULL-terminated and stays valid until the
            // next sane_get_devices or sane_exit, neither of which can run
            // before this function returns (`Context` is not `Sync`).
            let device = unsafe { *list.add(index) };
            // SAFETY: a non-null entry points to a valid SANE_Device.
            let Some(device) = (unsafe { device.as_ref() }) else {
                break;
            };
            // SAFETY: the strings of a device are NULL or valid C strings,
            // valid as long as the list.
            let info = unsafe {
                DeviceInfo {
                    raw_name: cstring(device.name),
                    name: string(device.name),
                    vendor: string(device.vendor),
                    model: string(device.model),
                    kind: string(device.type_),
                }
            };
            devices.push(info);
        }
        Ok(devices)
    }

    /// Calls `sane_open`.
    pub(crate) fn open(&self, name: &CStr) -> Result<Handle<'_>> {
        let mut handle: sys::SANE_Handle = ptr::null_mut();
        // SAFETY: SANE is initialized; `name` is a valid C string and
        // `handle` a valid out pointer.
        let status = unsafe { sys::sane_open(name.as_ptr(), &mut handle) };
        check(status)?;
        let handle =
            NonNull::new(handle).ok_or(Error::Protocol("sane_open returned a null handle"))?;
        Ok(Handle {
            handle,
            _context: PhantomData,
        })
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: SANE is initialized, and every Handle borrowed this Context,
        // so all of them are already closed.
        unsafe { sys::sane_exit() };
        AUTH_CALLBACK
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        INITIALIZED.store(false, Ordering::Release);
    }
}

/// Called by libsane when a resource needs a username and password.
extern "C" fn auth_trampoline(
    resource: sys::SANE_String_Const,
    username: *mut sys::SANE_Char,
    password: *mut sys::SANE_Char,
) {
    // Never unwind into libsane: a panic in the callback means no credentials.
    let credentials: Credentials = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: libsane passes a valid C string (or NULL).
        let resource = unsafe { string(resource) };
        let callback = AUTH_CALLBACK.lock().unwrap_or_else(PoisonError::into_inner);
        callback.as_ref().and_then(|callback| callback(&resource))
    }))
    .ok()
    .flatten()
    .unwrap_or_default();
    // SAFETY: libsane passes buffers of SANE_MAX_USERNAME_LEN and
    // SANE_MAX_PASSWORD_LEN bytes.
    unsafe {
        copy_credential(
            &credentials.username,
            username,
            sys::SANE_MAX_USERNAME_LEN as usize,
        );
        copy_credential(
            &credentials.password,
            password,
            sys::SANE_MAX_PASSWORD_LEN as usize,
        );
    }
}

/// Copies `value` into the C buffer `buffer` of `capacity` bytes, truncated
/// at the first NUL or to `capacity - 1` bytes, and NUL-terminated.
///
/// # Safety
///
/// `buffer` is null or valid for writes of `capacity` bytes.
unsafe fn copy_credential(value: &str, buffer: *mut c_char, capacity: usize) {
    if buffer.is_null() || capacity == 0 {
        return;
    }
    let bytes = value.as_bytes();
    let len = bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(bytes.len())
        .min(capacity - 1);
    // SAFETY: `len + 1 <= capacity` bytes are written into `buffer`, which
    // the caller guarantees is valid for `capacity` bytes; a Rust string
    // cannot overlap a C buffer.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast::<u8>(), len);
        *buffer.add(len) = 0;
    }
}

/// An open device; calls `sane_close` on drop.
pub(crate) struct Handle<'c> {
    handle: NonNull<c_void>,
    /// Closed before `sane_exit`; not `Send` nor `Sync` (`&Context` is not
    /// `Send`).
    _context: PhantomData<&'c Context>,
}

impl Handle<'_> {
    /// Calls `sane_get_option_descriptor` and copies the descriptor, or
    /// returns `None` past the last option.
    pub(crate) fn descriptor(&self, index: usize) -> Option<OptionDescriptor> {
        let raw_index = sys::SANE_Int::try_from(index).ok()?;
        // SAFETY: the handle is open.
        let descriptor =
            unsafe { sys::sane_get_option_descriptor(self.handle.as_ptr(), raw_index) };
        // SAFETY: a non-null descriptor is valid until the options are
        // reloaded or the handle is closed, neither of which can happen
        // before it is copied here.
        unsafe {
            descriptor
                .as_ref()
                .map(|descriptor| descriptor_from_raw(index, descriptor))
        }
    }

    /// Reads the value of an option into a buffer sized for it; returns the
    /// current descriptor of the option with the buffer.
    pub(crate) fn get_value(&self, index: usize) -> Result<(OptionDescriptor, Buffer)> {
        let current = self
            .descriptor(index)
            .ok_or_else(|| Error::UnknownOption(format!("#{index}")))?;
        let mut buffer = Buffer::for_option(current.value_type, current.size)
            .ok_or_else(|| Error::WriteOnlyOption(current.label()))?;
        // SAFETY: the buffer holds at least `size` bytes, the most libsane
        // writes for this option, whose descriptor was read just above.
        unsafe { self.control(index, sys::SANE_ACTION_GET_VALUE, buffer_ptr(&mut buffer))? };
        Ok((current, buffer))
    }

    /// Sets the value of an option.
    pub(crate) fn set_value(&mut self, index: usize, mut buffer: Buffer) -> Result<SetInfo> {
        let current = self
            .descriptor(index)
            .ok_or_else(|| Error::UnknownOption(format!("#{index}")))?;
        if buffer.byte_len() < current.size {
            return Err(Error::InvalidState(
                "the option changed size: reload the options",
            ));
        }
        // SAFETY: the buffer holds at least `size` bytes, the most libsane
        // reads or writes for this option, whose descriptor was read just
        // above.
        unsafe { self.control(index, sys::SANE_ACTION_SET_VALUE, buffer_ptr(&mut buffer)) }
    }

    /// Presses a button option (sets it, without a value).
    pub(crate) fn press(&mut self, index: usize) -> Result<SetInfo> {
        // SAFETY: libsane ignores the value of button options.
        unsafe { self.control(index, sys::SANE_ACTION_SET_VALUE, ptr::null_mut()) }
    }

    /// Lets the backend choose the value of an option.
    pub(crate) fn set_auto(&mut self, index: usize) -> Result<SetInfo> {
        // SAFETY: SANE_ACTION_SET_AUTO takes no value.
        unsafe { self.control(index, sys::SANE_ACTION_SET_AUTO, ptr::null_mut()) }
    }

    /// Calls `sane_control_option`.
    ///
    /// # Safety
    ///
    /// `value` is valid for the reads and writes `action` performs on the
    /// option: `size` bytes for getting or setting a value, nothing for
    /// buttons and `SANE_ACTION_SET_AUTO`.
    unsafe fn control(
        &self,
        index: usize,
        action: sys::SANE_Action,
        value: *mut c_void,
    ) -> Result<SetInfo> {
        let index = sys::SANE_Int::try_from(index)
            .map_err(|_| Error::UnknownOption(format!("#{index}")))?;
        let mut info: sys::SANE_Int = 0;
        // SAFETY: the handle is open, `value` is valid as the caller
        // guarantees, and `info` is a valid out pointer.
        let status = unsafe {
            sys::sane_control_option(self.handle.as_ptr(), index, action, value, &mut info)
        };
        check(status)?;
        Ok(SetInfo::from_raw(info))
    }

    /// Calls `sane_get_parameters`.
    pub(crate) fn parameters(&self) -> Result<Parameters> {
        let mut raw = sys::SANE_Parameters {
            format: 0,
            last_frame: 0,
            bytes_per_line: 0,
            pixels_per_line: 0,
            lines: 0,
            depth: 0,
        };
        // SAFETY: the handle is open and `raw` is a valid out pointer.
        let status = unsafe { sys::sane_get_parameters(self.handle.as_ptr(), &mut raw) };
        check(status)?;
        Parameters::from_raw(&raw)
    }

    /// Calls `sane_start`.
    pub(crate) fn start(&mut self) -> Result<()> {
        // SAFETY: the handle is open.
        check(unsafe { sys::sane_start(self.handle.as_ptr()) })
    }

    /// Calls `sane_read` once: the number of bytes read (possibly 0), or
    /// `None` at the end of the frame (`SANE_STATUS_EOF`).
    pub(crate) fn read(&mut self, buffer: &mut [u8]) -> Result<Option<usize>> {
        let max = sys::SANE_Int::try_from(buffer.len()).unwrap_or(sys::SANE_Int::MAX);
        let mut len: sys::SANE_Int = 0;
        // SAFETY: the handle is open; libsane writes at most `max` bytes,
        // no more than the length of `buffer`; `len` is a valid out pointer.
        let status =
            unsafe { sys::sane_read(self.handle.as_ptr(), buffer.as_mut_ptr(), max, &mut len) };
        match status {
            sys::SANE_STATUS_EOF => Ok(None),
            status => {
                check(status)?;
                usize::try_from(len)
                    .ok()
                    .filter(|&len| len <= buffer.len())
                    .map(Some)
                    .ok_or(Error::Protocol("sane_read returned an invalid length"))
            }
        }
    }

    /// Calls `sane_cancel`.
    pub(crate) fn cancel(&mut self) {
        // SAFETY: the handle is open.
        unsafe { sys::sane_cancel(self.handle.as_ptr()) }
    }
}

impl Drop for Handle<'_> {
    fn drop(&mut self) {
        // SAFETY: the handle is open, and is not used after this.
        unsafe { sys::sane_close(self.handle.as_ptr()) }
    }
}

/// The pointer libsane reads and writes an option value through.
fn buffer_ptr(buffer: &mut Buffer) -> *mut c_void {
    match buffer {
        Buffer::Words(words) => words.as_mut_ptr().cast(),
        Buffer::Bytes(bytes) => bytes.as_mut_ptr().cast(),
    }
}

/// Copies a C string, replacing invalid UTF-8; NULL becomes empty.
///
/// # Safety
///
/// `ptr` is null or a valid NUL-terminated string.
unsafe fn string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: guaranteed by the caller.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Copies a C string as is; NULL becomes empty.
///
/// # Safety
///
/// `ptr` is null or a valid NUL-terminated string.
unsafe fn cstring(ptr: *const c_char) -> CString {
    if ptr.is_null() {
        return CString::default();
    }
    // SAFETY: guaranteed by the caller.
    unsafe { CStr::from_ptr(ptr) }.to_owned()
}

/// Copies an option descriptor.
///
/// # Safety
///
/// The strings of `raw` are null or valid C strings, and its constraint
/// pointer is null or matches its constraint type: a NULL-terminated string
/// list, a word list whose first element is its length, or a range.
unsafe fn descriptor_from_raw(index: usize, raw: &sys::SANE_Option_Descriptor) -> OptionDescriptor {
    let value_type = ValueType::from_raw(raw.type_);
    // SAFETY: guaranteed by the caller.
    let (name, title, description, constraint) = unsafe {
        (
            string(raw.name),
            string(raw.title),
            string(raw.desc),
            constraint_from_raw(value_type, raw),
        )
    };
    OptionDescriptor {
        index,
        name,
        title,
        description,
        value_type,
        unit: Unit::from_raw(raw.unit),
        size: usize::try_from(raw.size).unwrap_or(0),
        capabilities: Capabilities::from_bits(raw.cap as u32),
        constraint,
    }
}

/// Copies the constraint of an option descriptor.
///
/// # Safety
///
/// As for [`descriptor_from_raw`].
unsafe fn constraint_from_raw(
    value_type: ValueType,
    raw: &sys::SANE_Option_Descriptor,
) -> Constraint {
    let fixed = value_type == ValueType::Fixed;
    match raw.constraint_type {
        sys::SANE_CONSTRAINT_RANGE => {
            // SAFETY: a range constraint points to a SANE_Range (or is NULL).
            let Some(range) = (unsafe { raw.constraint.range.as_ref() }) else {
                return Constraint::None;
            };
            if fixed {
                Constraint::FixedRange(Range {
                    min: Fixed::from_raw(range.min),
                    max: Fixed::from_raw(range.max),
                    quant: Fixed::from_raw(range.quant),
                })
            } else {
                Constraint::IntRange(Range {
                    min: range.min,
                    max: range.max,
                    quant: range.quant,
                })
            }
        }
        sys::SANE_CONSTRAINT_WORD_LIST => {
            // SAFETY: a word list constraint points to words (or is NULL).
            let list = unsafe { raw.constraint.word_list };
            if list.is_null() {
                return Constraint::None;
            }
            // SAFETY: the first word is the number of words that follow.
            let words = unsafe {
                let len = usize::try_from(*list).unwrap_or(0);
                std::slice::from_raw_parts(list.add(1), len)
            };
            if fixed {
                Constraint::FixedList(words.iter().copied().map(Fixed::from_raw).collect())
            } else {
                Constraint::IntList(words.to_vec())
            }
        }
        sys::SANE_CONSTRAINT_STRING_LIST => {
            // SAFETY: a string list constraint points to strings (or is NULL).
            let list = unsafe { raw.constraint.string_list };
            let mut strings = Vec::new();
            if list.is_null() {
                return Constraint::StringList(strings);
            }
            for index in 0.. {
                // SAFETY: the list is NULL-terminated, and this index is not
                // past the terminator.
                let entry = unsafe { *list.add(index) };
                if entry.is_null() {
                    break;
                }
                // SAFETY: the entries are valid C strings.
                strings.push(unsafe { string(entry) });
            }
            Constraint::StringList(strings)
        }
        _ => Constraint::None,
    }
}

/// Runs under Miri: builds C structures in Rust and reads them back.
#[cfg(test)]
mod tests {
    use super::*;

    fn raw_descriptor(type_: sys::SANE_Value_Type) -> sys::SANE_Option_Descriptor {
        sys::SANE_Option_Descriptor {
            name: c"resolution".as_ptr(),
            title: c"Scan resolution".as_ptr(),
            desc: ptr::null(),
            type_,
            unit: sys::SANE_UNIT_DPI,
            size: 4,
            cap: (sys::SANE_CAP_SOFT_SELECT | sys::SANE_CAP_SOFT_DETECT) as sys::SANE_Int,
            constraint_type: sys::SANE_CONSTRAINT_NONE,
            constraint: sys::SANE_Option_Descriptor__bindgen_ty_1 { range: ptr::null() },
        }
    }

    #[test]
    fn descriptor_strings_and_flags() {
        let raw = raw_descriptor(sys::SANE_TYPE_INT);
        // SAFETY: the strings are static C strings, there is no constraint.
        let descriptor = unsafe { descriptor_from_raw(3, &raw) };
        assert_eq!(descriptor.index, 3);
        assert_eq!(descriptor.name, "resolution");
        assert_eq!(descriptor.title, "Scan resolution");
        assert_eq!(descriptor.description, "");
        assert_eq!(descriptor.value_type, ValueType::Int);
        assert_eq!(descriptor.unit, Unit::Dpi);
        assert!(descriptor.is_active() && descriptor.is_settable() && descriptor.is_readable());
        assert_eq!(descriptor.constraint, Constraint::None);
    }

    #[test]
    fn range_constraints() {
        let range = sys::SANE_Range {
            min: 1 << 16,
            max: 3 << 16,
            quant: 0,
        };
        let mut raw = raw_descriptor(sys::SANE_TYPE_FIXED);
        raw.constraint_type = sys::SANE_CONSTRAINT_RANGE;
        raw.constraint.range = &range;
        // SAFETY: the constraint points to a live range.
        let descriptor = unsafe { descriptor_from_raw(1, &raw) };
        assert_eq!(
            descriptor.constraint,
            Constraint::FixedRange(Range {
                min: Fixed::from(1),
                max: Fixed::from(3),
                quant: Fixed::from(0)
            })
        );
        raw.type_ = sys::SANE_TYPE_INT;
        // SAFETY: as above.
        let descriptor = unsafe { descriptor_from_raw(1, &raw) };
        assert_eq!(
            descriptor.constraint,
            Constraint::IntRange(Range {
                min: 1 << 16,
                max: 3 << 16,
                quant: 0
            })
        );
    }

    #[test]
    fn word_list_constraints() {
        let words: [sys::SANE_Word; 4] = [3, 1, 8, 16];
        let mut raw = raw_descriptor(sys::SANE_TYPE_INT);
        raw.constraint_type = sys::SANE_CONSTRAINT_WORD_LIST;
        raw.constraint.word_list = words.as_ptr();
        // SAFETY: the list starts with its length.
        let descriptor = unsafe { descriptor_from_raw(1, &raw) };
        assert_eq!(descriptor.constraint, Constraint::IntList(vec![1, 8, 16]));
        let empty: [sys::SANE_Word; 1] = [0];
        raw.constraint.word_list = empty.as_ptr();
        raw.type_ = sys::SANE_TYPE_FIXED;
        // SAFETY: as above.
        let descriptor = unsafe { descriptor_from_raw(1, &raw) };
        assert_eq!(descriptor.constraint, Constraint::FixedList(vec![]));
    }

    #[test]
    fn string_list_constraints() {
        let strings = [c"Gray".as_ptr(), c"Color".as_ptr(), ptr::null()];
        let mut raw = raw_descriptor(sys::SANE_TYPE_STRING);
        raw.constraint_type = sys::SANE_CONSTRAINT_STRING_LIST;
        raw.constraint.string_list = strings.as_ptr();
        // SAFETY: the list is NULL-terminated.
        let descriptor = unsafe { descriptor_from_raw(1, &raw) };
        assert_eq!(
            descriptor.constraint,
            Constraint::StringList(vec!["Gray".into(), "Color".into()])
        );
        raw.constraint.string_list = ptr::null();
        // SAFETY: a NULL list is accepted.
        let descriptor = unsafe { descriptor_from_raw(1, &raw) };
        assert_eq!(descriptor.constraint, Constraint::StringList(vec![]));
    }

    #[test]
    // `c_char` is `u8` on some targets and `i8` on others.
    #[allow(clippy::unnecessary_cast)]
    fn credentials_are_truncated_and_terminated() {
        let mut buffer = [0x55 as c_char; 6];
        // SAFETY: the buffer holds 6 bytes.
        unsafe { copy_credential("username", buffer.as_mut_ptr(), buffer.len()) };
        assert_eq!(buffer.map(|c| c as u8), *b"usern\0");
        // SAFETY: as above.
        unsafe { copy_credential("a\0b", buffer.as_mut_ptr(), buffer.len()) };
        assert_eq!(
            buffer[..2].iter().map(|&c| c as u8).collect::<Vec<_>>(),
            b"a\0"
        );
        // SAFETY: a null buffer is ignored.
        unsafe { copy_credential("x", ptr::null_mut(), 4) };
    }

    #[test]
    fn strings_handle_null_and_invalid_utf8() {
        // SAFETY: null is accepted.
        assert_eq!(unsafe { string(ptr::null()) }, "");
        // SAFETY: a static C string.
        assert_eq!(unsafe { string(c"caf\xe9".as_ptr()) }, "caf\u{fffd}");
        // SAFETY: as above.
        let raw = unsafe { cstring(c"caf\xe9".as_ptr()) };
        assert_eq!(raw.as_bytes(), b"caf\xe9");
    }

    #[test]
    fn default_credentials_are_empty() {
        let credentials = Credentials::default();
        assert!(credentials.username.is_empty() && credentials.password.is_empty());
    }
}
