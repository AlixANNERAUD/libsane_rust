//! Calls into libsane through the raw bindings, against SANE's virtual
//! `test` backend (`tests/sane.d/dll.conf` enables only that backend).

use std::ffi::CStr;
use std::ptr;

use sane_sys::*;

#[test]
fn init_list_open_and_exit() {
    // SAFETY: this is the only test of this binary, so nothing else reads
    // the environment concurrently.
    unsafe {
        std::env::set_var(
            "SANE_CONFIG_DIR",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/sane.d"),
        )
    };

    let mut version: SANE_Int = 0;
    // SAFETY: SANE is not initialized yet; no authorization callback.
    let status = unsafe { sane_init(&mut version, None) };
    assert_eq!(status, SANE_STATUS_GOOD);
    assert_eq!(SANE_VERSION_MAJOR(version), 1);

    let mut list: *mut *const SANE_Device = ptr::null_mut();
    // SAFETY: SANE is initialized; `list` receives a pointer owned by SANE.
    let status = unsafe { sane_get_devices(&mut list as *mut _ as *mut _, SANE_TRUE as SANE_Bool) };
    assert_eq!(status, SANE_STATUS_GOOD);
    assert!(!list.is_null());
    // SAFETY: the list is NULL-terminated and valid until the next call to
    // sane_get_devices or sane_exit.
    let first = unsafe { &**list };
    let name = unsafe { CStr::from_ptr(first.name) };
    assert_eq!(name.to_str().unwrap(), "test:0");

    let mut handle: SANE_Handle = ptr::null_mut();
    // SAFETY: `name` is a valid C string; `handle` receives the handle.
    let status = unsafe { sane_open(name.as_ptr(), &mut handle) };
    assert_eq!(status, SANE_STATUS_GOOD);
    // SAFETY: `handle` was opened above and is closed once.
    unsafe { sane_close(handle) };

    // SAFETY: a status string is static.
    let message = unsafe { CStr::from_ptr(sane_strstatus(SANE_STATUS_JAMMED)) };
    assert!(!message.to_bytes().is_empty());

    // SAFETY: every handle is closed.
    unsafe { sane_exit() };
}
