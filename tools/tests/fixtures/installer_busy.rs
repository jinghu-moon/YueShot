//! A test process with YueShot's name. It exits normally when the parent signals.
#![windows_subsystem = "windows"]
use std::{ffi::c_void, ptr::null_mut};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 2);
    let name: Vec<_> = args[0]
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let handle = unsafe { OpenEventW(0x0010_0000, 0, name.as_ptr()) };
    assert_ne!(handle, null_mut());
    std::fs::write(&args[1], b"ready").unwrap();
    let result = unsafe { WaitForSingleObject(handle, 60_000) };
    unsafe { CloseHandle(handle) };
    assert_eq!(result, 0, "parent must signal a normal exit");
}
