//! The process's own working set, for measurement only.
//!
//! An external sampler sees one number for the whole process and cannot say which part grew. The
//! scroll capture shares that process with the overlay, the compositor and the capture session, so
//! a rise seen from outside has to be correlated with counters taken inside, in the same frame
//! (docs/39 §3). The FFI is declared here rather than by widening `windows-sys`'s feature set: this
//! file exists in measurement builds only, and a dependency feature is crate-wide.

use core::ffi::c_void;

#[repr(C)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn K32GetProcessMemoryInfo(
        process: *mut c_void,
        counters: *mut ProcessMemoryCounters,
        size: u32,
    ) -> i32;
}

/// This process's working set in bytes, or `None` if the query failed.
///
/// `-1` is the pseudo-handle for the current process, which is why nothing is opened or closed
/// here.
pub(crate) fn working_set_bytes() -> Option<usize> {
    let mut counters = ProcessMemoryCounters {
        cb: size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: the pseudo-handle needs no cleanup, and the structure is `repr(C)` with the size and
    // layout the API requires, passed by pointer for the duration of the call.
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            (-1i64) as *mut c_void,
            &mut counters,
            size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    (ok != 0).then_some(counters.working_set_size)
}

/// Test evidence uses PROCESS_MEMORY_COUNTERS_EX's appended PrivateUsage field;
/// keep a single FFI declaration and pass its documented base layout.
#[cfg(test)]
pub(crate) fn private_bytes() -> Option<usize> {
    #[repr(C)]
    struct Extended {
        base: ProcessMemoryCounters,
        private: usize,
    }
    // Both structures contain integers only; all-zero is valid before the API
    // fills it. The cb and actual buffer size include the appended EX field.
    let mut value: Extended = unsafe { std::mem::zeroed() };
    value.base.cb = size_of::<Extended>() as u32;
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            (-1isize) as *mut c_void,
            &mut value.base,
            size_of::<Extended>() as u32,
        )
    };
    (ok != 0).then_some(value.private)
}

/// Diagnostic only: distinguish process/driver background handles from handles
/// owned by the prototype. Queries this process only, and never closes a handle.
/// The native snapshot and object-type structures follow winternl's ABI.
#[cfg(test)]
pub(crate) fn handle_types() -> std::collections::BTreeMap<String, usize> {
    #[repr(C)]
    struct Entry {
        handle: usize,
        handle_count: usize,
        pointer_count: usize,
        access: u32,
        type_index: u32,
        attributes: u32,
        reserved: u32,
    }
    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *const u16,
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationProcess(
            process: *mut c_void,
            class: u32,
            buffer: *mut c_void,
            size: u32,
            returned: *mut u32,
        ) -> i32;
        fn NtQueryObject(
            handle: *mut c_void,
            class: u32,
            buffer: *mut c_void,
            size: u32,
            returned: *mut u32,
        ) -> i32;
    }
    // usize storage guarantees alignment for both native structures.
    let mut snapshot = vec![0usize; 4096];
    let mut returned = 0;
    let mut status = unsafe {
        NtQueryInformationProcess(
            (-1isize) as *mut c_void,
            51, // ProcessHandleInformation
            snapshot.as_mut_ptr().cast(),
            size_of_val(snapshot.as_slice()) as u32,
            &mut returned,
        )
    };
    if status < 0 && returned as usize > size_of_val(snapshot.as_slice()) {
        assert!(returned < 16 * 1_048_576, "unexpected handle snapshot size");
        snapshot.resize((returned as usize).div_ceil(size_of::<usize>()), 0);
        status = unsafe {
            NtQueryInformationProcess(
                (-1isize) as *mut c_void,
                51,
                snapshot.as_mut_ptr().cast(),
                size_of_val(snapshot.as_slice()) as u32,
                &mut returned,
            )
        };
    }
    assert!(status >= 0, "handle snapshot failed: {status:#x}");
    let count = snapshot[0];
    assert!(2 * size_of::<usize>() + count * size_of::<Entry>() <= returned as usize);
    let entries =
        unsafe { std::slice::from_raw_parts(snapshot.as_ptr().add(2).cast::<Entry>(), count) };
    let mut counts = std::collections::BTreeMap::new();
    let mut buffer = vec![0usize; 512];
    for entry in entries {
        let status = unsafe {
            NtQueryObject(
                entry.handle as *mut c_void,
                2, // ObjectTypeInformation: no object-name query or file I/O.
                buffer.as_mut_ptr().cast(),
                size_of_val(buffer.as_slice()) as u32,
                &mut returned,
            )
        };
        let name = if status >= 0 {
            let value = unsafe { &*buffer.as_ptr().cast::<UnicodeString>() };
            let start = buffer.as_ptr() as usize;
            let pointer = value.buffer as usize;
            assert!(value.length % 2 == 0 && pointer >= start);
            assert!(pointer + value.length as usize <= start + size_of_val(buffer.as_slice()));
            let text =
                unsafe { std::slice::from_raw_parts(value.buffer, value.length as usize / 2) };
            String::from_utf16_lossy(text)
        } else {
            // A background thread may close a handle after the snapshot.
            format!("unavailable:{status:#x}")
        };
        *counts.entry(name).or_insert(0) += 1;
    }
    counts
}

/// The working set formatted for a log line, in mebibytes.
pub(crate) fn working_set_mb() -> String {
    match working_set_bytes() {
        Some(bytes) => format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)),
        None => "unavailable".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wrong struct layout would still return a number, so the shape is checked by asking for a
    /// plausible one: a running test process has megabytes of working set, not zero and not a
    /// number larger than any real machine's memory.
    #[test]
    fn the_query_returns_this_process_working_set() {
        let bytes = working_set_bytes().expect("the query must succeed for this process");
        assert!(private_bytes().unwrap() > 1024 * 1024);
        assert!(
            bytes > 1024 * 1024,
            "working set {bytes} is implausibly small"
        );
        assert!(
            bytes < 1024usize * 1024 * 1024 * 1024,
            "working set {bytes} is implausibly large"
        );
    }
}
