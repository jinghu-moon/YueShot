use super::*;
use std::time::{Duration, Instant};

#[test]
fn tooltip_and_windows_strings_are_terminated() {
    assert_eq!(wide("A中"), vec![0x41, 0x4e2d, 0]);
    let mut small = [0xffffu16; 4];
    write_wide_buffer(&mut small, "a very long tooltip");
    assert_eq!(small, ['a' as u16, ' ' as u16, 'v' as u16, 0]);
    write_wide_buffer(&mut [], "empty");
}

#[test]
fn invalid_options_are_rejected_before_starting_a_thread() {
    let duplicate = TrayOptions::new(
        "test",
        "test",
        vec![MenuItem::new(1, "A"), MenuItem::new(1, "B")],
    );
    assert!(duplicate.validate().is_err());
    for hotkeys in [
        vec![Hotkey::new(2, 0, 0x70)],
        vec![Hotkey::new(1, 0x100, 0x70)],
        vec![Hotkey::new(1, 0, 0)],
    ] {
        assert!(
            TrayOptions::new("test", "test", vec![MenuItem::new(1, "A")])
                .with_hotkeys(hotkeys)
                .validate()
                .is_err()
        );
    }
}

#[test]
fn repeated_health_failures_publish_once_and_recovery_clears_them() {
    let (sender, receiver) = async_channel::unbounded();
    let state = WindowState {
        events: Arc::new(move |event| {
            sender.try_send(event).unwrap();
        }),
        menu: std::cell::RefCell::new(vec![]),
        icon: unsafe { std::mem::zeroed() },
        taskbar_created: 0,
        hotkeys: std::cell::RefCell::new(vec![]),
        requests: async_channel::bounded(1).1,
        registered: std::cell::RefCell::new(vec![]),
        status: Arc::new(Mutex::new(TrayStatus::default())),
        stop: Arc::new(AtomicBool::new(false)),
        failed_icon_attempts: std::cell::Cell::new(0),
    };
    let failure = TrayStatus {
        running: true,
        available: false,
        error: Some("unavailable".into()),
        failures: vec![HotkeyFailure {
            command: 1,
            error: 1409,
        }],
    };
    state.publish(failure.clone());
    state.publish(failure.clone());
    let TrayEvent::Status(first) = receiver.try_recv().unwrap() else {
        panic!("status expected");
    };
    assert_eq!(first, failure);
    assert!(
        receiver.try_recv().is_err(),
        "timer retries must not flood the owner"
    );
    let restored = TrayStatus {
        running: true,
        available: true,
        error: None,
        failures: vec![],
    };
    state.publish(restored.clone());
    assert_eq!(*state.status.lock().unwrap(), restored);
    let TrayEvent::Status(last) = receiver.try_recv().unwrap() else {
        panic!("status expected");
    };
    assert!(last.is_available());
    assert!(last.error().is_none());
    assert!(last.hotkey_failures().is_empty());
}

fn await_status(
    receiver: &async_channel::Receiver<TrayEvent>,
    predicate: impl Fn(&TrayStatus) -> bool,
) -> TrayStatus {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        assert!(
            Instant::now() < deadline,
            "tray status did not arrive within 12 seconds"
        );
        if let Ok(TrayEvent::Status(status)) = receiver.try_recv() {
            if predicate(&status) {
                return status;
            }
        } else {
            assert!(
                Instant::now() < deadline,
                "tray status did not arrive within 12 seconds"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
#[ignore = "creates its own icon and hidden native window; no desktop input or Explorer restart"]
fn the_icon_can_be_created_and_taken_away() {
    let (sender, receiver) = async_channel::unbounded();
    let mut tray = Tray::start(
        TrayOptions::new(
            format!("Yue.TrayRecoveryTest.{}", std::process::id()),
            "Tray recovery test",
            vec![],
        ),
        sender,
    )
    .unwrap();
    await_status(&receiver, |status| status.is_available());
    assert!(tray.is_available());
    // Delete only this test's icon, fail two add attempts, then pass the real notification
    // through wndproc. Explorer and the user's applications are left untouched.
    assert_ne!(
        unsafe {
            PostThreadMessageW(
                tray.thread_id.load(Ordering::Acquire),
                WM_TEST_LOSE_ICON,
                0,
                0,
            )
        },
        0
    );
    await_status(&receiver, |status| !status.is_available());
    assert!(!tray.is_available());
    await_status(&receiver, |status| status.is_available());
    assert!(tray.is_available());
    tray.retry().unwrap();
    tray.shutdown();
    tray.shutdown();
    assert!(!tray.is_available());
    assert!(tray.retry().is_err());
    // Class, window and icon cleanup must allow the same identity to start again.
    let mut next = Tray::start(
        TrayOptions::new(
            format!("Yue.TrayRecoveryTest.{}", std::process::id()),
            "Tray recovery test",
            vec![],
        ),
        async_channel::unbounded().0,
    )
    .unwrap();
    assert!(next.is_available());
    next.shutdown();
}

#[test]
#[ignore = "temporarily reserves an unused Ctrl+Alt+Shift function-key chord on this thread"]
fn occupied_hotkey_recovers_without_unregistering_working_bindings() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_SHIFT};
    let modifiers = MOD_CONTROL | MOD_ALT | MOD_SHIFT;
    let free_keys: Vec<_> = (0x7bu32..=0x87).filter(|key| unsafe { RegisterHotKey(std::ptr::null_mut(), *key as i32, modifiers, *key) } != 0).take(2).collect();
    struct Reservations(Vec<u32>);
    impl Drop for Reservations {
        fn drop(&mut self) {
            for key in &self.0 {
                unsafe {
                    UnregisterHotKey(std::ptr::null_mut(), *key as i32);
                }
            }
        }
    }
    let reservations = Reservations(free_keys);
    assert_eq!(reservations.0.len(), 2, "need two available test chords");
    let occupied = reservations.0[0];
    let working = reservations.0[1];
    unsafe {
        UnregisterHotKey(std::ptr::null_mut(), working as i32);
    }
    let (sender, receiver) = async_channel::unbounded();
    let mut tray = Tray::start(
        TrayOptions::new(
            format!("Yue.TrayHotkeyTest.{}", std::process::id()),
            "Tray hotkey test",
            vec![MenuItem::new(1, "First"), MenuItem::new(2, "Second")],
        )
        .with_hotkeys(vec![
            Hotkey::new(1, modifiers, occupied),
            Hotkey::new(2, modifiers, working),
        ]),
        sender,
    )
    .unwrap();
    let first = await_status(&receiver, |status| {
        status.is_available() && !status.hotkey_failures().is_empty()
    });
    assert_eq!(first.hotkey_failures().len(), 1);
    assert_eq!(first.hotkey_failures()[0].command(), 1);
    assert_eq!(
        first.hotkey_failures()[0].error(),
        ERROR_HOTKEY_ALREADY_REGISTERED
    );
    unsafe {
        UnregisterHotKey(std::ptr::null_mut(), occupied as i32);
    }
    tray.retry().unwrap();
    await_status(&receiver, |status| {
        status.is_available() && status.hotkey_failures().is_empty()
    });
    for key in [occupied, working] {
        assert_eq!(
            unsafe { RegisterHotKey(std::ptr::null_mut(), key as i32, modifiers, key) },
            0,
            "both bindings stay owned by the tray"
        );
    }
    tray.shutdown();
    for key in [occupied, working] {
        assert_ne!(
            unsafe { RegisterHotKey(std::ptr::null_mut(), key as i32, modifiers, key) },
            0,
            "shutdown releases each binding"
        );
    }
}

#[test]
#[ignore = "reserves isolated Ctrl+Alt+Shift function keys and creates a test icon"]
fn reconfiguration_reserves_before_commit_and_preserves_old_bindings_on_failure() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_SHIFT};
    let mods = MOD_ALT | MOD_CONTROL | MOD_SHIFT;
    let keys: Vec<u32> = (0x7bu32..=0x87)
        .filter(|key| unsafe { RegisterHotKey(std::ptr::null_mut(), *key as i32, mods, *key) } != 0)
        .take(3)
        .collect();
    struct Reservations(Vec<u32>);
    impl Drop for Reservations {
        fn drop(&mut self) {
            for key in &self.0 {
                unsafe {
                    UnregisterHotKey(std::ptr::null_mut(), *key as i32);
                }
            }
        }
    }
    let reservations = Reservations(keys);
    assert_eq!(reservations.0.len(), 3);
    let [first, second, occupied] = reservations.0.as_slice() else {
        panic!()
    };
    unsafe {
        UnregisterHotKey(std::ptr::null_mut(), *first as i32);
        UnregisterHotKey(std::ptr::null_mut(), *second as i32);
    }
    let menu = vec![MenuItem::new(1, "First"), MenuItem::new(2, "Second")];
    let mut tray = Tray::start_with_handler(
        TrayOptions::new(
            format!("Yue.TrayConfigureTest.{}", std::process::id()),
            "Configure test",
            menu.clone(),
        )
        .with_hotkeys(vec![Hotkey::new(1, mods, *first)]),
        Arc::new(|_| {}),
    )
    .unwrap();
    let committed = Arc::new(AtomicBool::new(false));
    let flag = committed.clone();
    let failure = tray
        .configure(
            menu.clone(),
            vec![
                Hotkey::new(1, mods, *second),
                Hotkey::new(2, mods, *occupied),
            ],
            move || {
                flag.store(true, Ordering::Release);
                Ok(())
            },
        )
        .unwrap()
        .recv_blocking()
        .unwrap();
    assert!(failure.is_err());
    assert!(
        !committed.load(Ordering::Acquire),
        "a conflict must not write settings"
    );
    assert_eq!(
        unsafe { RegisterHotKey(std::ptr::null_mut(), *first as i32, mods, *first) },
        0,
        "old chord retained"
    );
    assert_ne!(
        unsafe { RegisterHotKey(std::ptr::null_mut(), *second as i32, mods, *second) },
        0,
        "partial candidate released"
    );
    unsafe {
        UnregisterHotKey(std::ptr::null_mut(), *second as i32);
    }
    let failure = tray
        .configure(menu.clone(), vec![Hotkey::new(1, mods, *second)], || {
            Err("injected settings write failure".into())
        })
        .unwrap()
        .recv_blocking()
        .unwrap();
    assert!(failure.unwrap_err().contains("settings write failure"));
    assert_eq!(
        unsafe { RegisterHotKey(std::ptr::null_mut(), *first as i32, mods, *first) },
        0
    );
    let flag = committed.clone();
    tray.configure(
        menu.clone(),
        vec![Hotkey::new(1, mods, *second)],
        move || {
            flag.store(true, Ordering::Release);
            Ok(())
        },
    )
    .unwrap()
    .recv_blocking()
    .unwrap()
    .unwrap();
    assert!(committed.load(Ordering::Acquire));
    assert_ne!(
        unsafe { RegisterHotKey(std::ptr::null_mut(), *first as i32, mods, *first) },
        0,
        "successful commit releases old chord"
    );
    assert_eq!(
        unsafe { RegisterHotKey(std::ptr::null_mut(), *second as i32, mods, *second) },
        0,
        "new chord is owned"
    );
    tray.configure(menu, vec![Hotkey::new(2, mods, *second)], || Ok(()))
        .unwrap()
        .recv_blocking()
        .unwrap()
        .unwrap();
    assert!(
        tray.status().hotkey_failures().is_empty(),
        "changing command for an existing chord does not release it"
    );
    tray.shutdown();
}
