//! Reproduce observations in the original fontdb 0.24.0 source, not a security gate.
//! The runner supplies an untouched, checksum-verified copy of the published crate.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

#[path = "fontdb-0.24.0/src/ttf_parser/mod.rs"]
#[allow(dead_code, unused_imports)]
mod embedded;

struct WarningCounter;
static WARNINGS: AtomicUsize = AtomicUsize::new(0);
impl log::Log for WarningCounter {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if record.level() == log::Level::Warn {
            WARNINGS.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn flush(&self) {}
}

fn collection(count: u32) -> Vec<u8> {
    [
        b"ttcf".as_slice(),
        &0x0001_0000u32.to_be_bytes(),
        &count.to_be_bytes(),
    ]
    .concat()
}

fn name_table(count: u16, name_length: u16) -> Vec<u8> {
    let storage_offset = 6usize + usize::from(count) * 12;
    let mut bytes = Vec::new();
    for value in [0, count, u16::try_from(storage_offset).unwrap_or(0)] {
        bytes.extend(value.to_be_bytes());
    }
    for index in 0..count {
        // One family record and one PostScript record; unused records have ID 0.
        let id = match index {
            0 => 1,
            1 => 6,
            _ => 0,
        };
        for value in [3u16, 1, 0x0409, id, name_length, 0] {
            bytes.extend(value.to_be_bytes());
        }
    }
    bytes.extend((0..name_length).map(|index| if index % 2 == 0 { 0 } else { b'A' }));
    bytes
}

fn font_with_name(table: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    bytes.extend(b"name");
    bytes.extend(0u32.to_be_bytes());
    bytes.extend(28u32.to_be_bytes());
    bytes.extend(u32::try_from(table.len()).unwrap().to_be_bytes());
    bytes.extend(table);
    bytes
}

fn main() {
    log::set_logger(&WarningCounter).unwrap();
    log::set_max_level(log::LevelFilter::Warn);
    let scratch = std::path::PathBuf::from(std::env::args_os().nth(1).unwrap());
    std::fs::create_dir_all(&scratch).unwrap();
    println!("debug_assertions={}", cfg!(debug_assertions));
    if std::env::args_os().nth(2).as_deref() == Some(std::ffi::OsStr::new("--system-probe")) {
        // The parent sets SYSTEMROOT/USERPROFILE only for this child. No real
        // system/user font directory is written or scanned by this probe.
        assert!(cfg!(debug_assertions));
        let panicked = std::panic::catch_unwind(|| {
            fontdb::Database::new().load_system_fonts();
        })
        .is_err();
        assert!(panicked);
        println!("isolated_system_font_scan_panicked={panicked}");
        return;
    }

    for length in [65534u16, 65535] {
        let bytes = font_with_name(&name_table(2, length));
        let mut db = fontdb::Database::new();
        db.load_font_data(bytes);
        let face = db.faces().next().expect("max-length name accepted");
        assert_eq!(face.families[0].0.len(), 32767);
        println!("name_bytes={length} decoded_units=32767 panic=false");
    }
    let bytes = name_table(u16::MAX, 2);
    let names = embedded::name::Table::parse(&bytes).unwrap().names;
    let mut iter = names.into_iter();
    assert_eq!(iter.by_ref().count(), 65535);
    assert!(iter.next().is_none());
    let mut db = fontdb::Database::new();
    db.load_font_data(font_with_name(&bytes));
    assert_eq!(db.faces().count(), 1);
    println!("name_records=65535 parsed=true iterator_terminated=true");

    // This helper can overflow in isolation, but name strings in the consumer
    // contain at most floor(u16::MAX / 2) entries, not u16::MAX entries.
    let helper_data = vec![0; 65535 * 2];
    let helper_panicked = std::panic::catch_unwind(|| {
        let mut iter = embedded::LazyArray16::<u16>::new(&helper_data).into_iter();
        for _ in 0..65535 {
            assert!(iter.next().is_some());
        }
        assert!(iter.next().is_none());
    })
    .is_err();
    assert_eq!(helper_panicked, cfg!(debug_assertions));
    println!(
        "isolated_helper_65535_entries_panicked={helper_panicked} product_name_path_reachable=false"
    );

    for count in [0u32, 1, 1000, 100000] {
        let bytes = collection(count);
        for memory in [false, true] {
            WARNINGS.store(0, Ordering::Relaxed);
            let mut db = fontdb::Database::new();
            let start = Instant::now();
            if memory {
                db.load_font_data(bytes.clone());
            } else {
                let file = scratch.join("truncated.ttc");
                std::fs::write(&file, &bytes).unwrap();
                db.load_font_file(&file).unwrap();
            }
            assert_eq!(db.faces().count(), 0);
            let warnings = WARNINGS.load(Ordering::Relaxed);
            assert_eq!(warnings, count as usize);
            println!(
                "truncated_ttc_bytes=12 count={count} memory={memory} warnings={warnings} elapsed_us={}",
                start.elapsed().as_micros()
            );
        }
    }

    let bytes = collection(0x4000_0000);
    let raw_panicked = std::panic::catch_unwind(|| embedded::RawFace::parse(&bytes, 0)).is_err();
    assert_eq!(raw_panicked, cfg!(debug_assertions));
    println!("huge_ttc_count=1073741824 raw_parser_panicked={raw_panicked}");
    if cfg!(debug_assertions) {
        // No preallocation on the file path: this reaches the panic safely.
        // Do not run the full-count loop in release or the huge memory path.
        let file = scratch.join("huge-count.ttc");
        std::fs::write(&file, &bytes).unwrap();
        let file_panicked = std::panic::catch_unwind(|| {
            fontdb::Database::new().load_font_file(&file).unwrap();
        })
        .is_err();
        assert!(file_panicked);
        println!("huge_ttc_file_public_api_panicked={file_panicked}");
        std::fs::remove_file(file).unwrap();
    }

    // Small deterministic malformed-input sweep, not a fuzzing claim. Constrain
    // collection counts to avoid intentionally triggering the unbounded loop.
    let mut state = 0x59ab31cdu32;
    for len in 0..256 {
        let mut bytes = Vec::new();
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            bytes.push(state as u8);
        }
        if len >= 4 {
            bytes[..4].copy_from_slice(&[0, 1, 0, 0]);
        }
        fontdb::Database::new().load_font_data(bytes);
    }
    println!("deterministic_malformed_samples=256 panics=0");
    println!("REVIEW_REPRODUCED=true RELEASE_BLOCKER_REMAINS=true");
}
