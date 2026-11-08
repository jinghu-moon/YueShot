//! Windows notification-area/message-loop mechanism. Products own command IDs and menus.
#![cfg(windows)]
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey},
        Shell::*,
        WindowsAndMessaging::*,
    },
};

#[derive(Clone, Debug)]
pub struct MenuItem {
    id: u32,
    label: String,
}
impl MenuItem {
    pub fn new(id: u32, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    command: u32,
    modifiers: u32,
    key: u32,
}
impl Hotkey {
    pub fn command(&self) -> u32 {
        self.command
    }
    pub fn modifiers(&self) -> u32 {
        self.modifiers
    }
    pub fn virtual_key(&self) -> u32 {
        self.key
    }
    pub fn new(command: u32, modifiers: u32, key: u32) -> Self {
        Self {
            command,
            modifiers,
            key,
        }
    }
}
#[derive(Clone)]
pub struct TrayOptions {
    identity: String,
    title: String,
    menu: Vec<MenuItem>,
    hotkeys: Vec<Hotkey>,
}
impl TrayOptions {
    pub fn new(identity: impl Into<String>, title: impl Into<String>, menu: Vec<MenuItem>) -> Self {
        Self {
            identity: identity.into(),
            title: title.into(),
            menu,
            hotkeys: Vec::new(),
        }
    }
    pub fn with_hotkeys(mut self, hotkeys: Vec<Hotkey>) -> Self {
        self.hotkeys = hotkeys;
        self
    }
    pub fn with_menu(mut self, menu: Vec<MenuItem>) -> Self {
        self.menu = menu;
        self
    }
    fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        if self.identity.is_empty() || self.identity.contains('\0') || self.title.contains('\0') {
            return Err("invalid tray identity/title".into());
        }
        for item in &self.menu {
            if item.id == 0 || item.id > 0xbfff || !ids.insert(item.id) || item.label.contains('\0')
            {
                return Err("invalid or duplicate menu command".into());
            }
        }
        let mut chords = std::collections::HashSet::new();
        let mut hotkey_ids = std::collections::HashSet::new();
        for key in &self.hotkeys {
            if !ids.contains(&key.command)
                || !hotkey_ids.insert(key.command)
                || !chords.insert((key.modifiers & !MOD_NOREPEAT, key.key))
                || key.key == 0
                || key.key > 255
                || key.modifiers & !(0xf | MOD_NOREPEAT) != 0
            {
                return Err("invalid or duplicate hotkey".into());
            }
        }
        Ok(())
    }
}

/// Product commands run synchronously on the tray thread; health changes use the same sink.
#[derive(Debug, Clone)]
pub enum TrayEvent {
    Command(TrayCommand),
    Status(TrayStatus),
}
#[derive(Debug, Clone, Copy)]
pub struct TrayCommand {
    command: u32,
    position: (i32, i32),
}
impl TrayCommand {
    pub fn command(&self) -> u32 {
        self.command
    }
    pub fn position(&self) -> (i32, i32) {
        self.position
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyFailure {
    command: u32,
    error: u32,
}
impl HotkeyFailure {
    pub fn command(&self) -> u32 {
        self.command
    }
    pub fn error(&self) -> u32 {
        self.error
    }
}
/// A current snapshot, never an optimistic indication that a thread merely exists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayStatus {
    running: bool,
    available: bool,
    error: Option<String>,
    failures: Vec<HotkeyFailure>,
}
impl TrayStatus {
    pub fn is_running(&self) -> bool {
        self.running
    }
    pub fn is_available(&self) -> bool {
        self.available
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn hotkey_failures(&self) -> &[HotkeyFailure] {
        &self.failures
    }
}
pub struct Tray {
    thread_id: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<TrayStatus>>,
    thread: Option<JoinHandle<()>>,
    configure: async_channel::Sender<Configuration>,
}
impl Tray {
    pub fn start(
        options: TrayOptions,
        events: async_channel::Sender<TrayEvent>,
    ) -> Result<Self, String> {
        Self::start_with_handler(
            options,
            Arc::new(move |event| {
                let _ = events.try_send(event);
            }),
        )
    }
    /// The handler must not block. Commands can snapshot transient OS state before queueing.
    pub fn start_with_handler(
        options: TrayOptions,
        events: Arc<dyn Fn(TrayEvent) + Send + Sync>,
    ) -> Result<Self, String> {
        options.validate()?;
        let (ready, result) = mpsc::sync_channel(1);
        let thread_id = Arc::new(AtomicU32::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(TrayStatus::default()));
        let (configure, requests) = async_channel::bounded(1);
        let id = thread_id.clone();
        let shared_status = status.clone();
        let stopping = stop.clone();
        let thread = thread::Builder::new()
            .name(format!("{}-tray", options.identity))
            .spawn(move || {
                run(
                    options,
                    events,
                    ready,
                    id,
                    shared_status,
                    stopping,
                    requests,
                )
            })
            .map_err(|e| e.to_string())?;
        match result.recv() {
            Ok(Ok(())) => Ok(Self {
                thread_id,
                stop,
                status,
                thread: Some(thread),
                configure,
            }),
            failed => {
                let _ = thread.join();
                Err(match failed {
                    Ok(Err(error)) => error,
                    _ => "tray stopped during startup".into(),
                })
            }
        }
    }
    pub fn status(&self) -> TrayStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn is_available(&self) -> bool {
        self.status().is_available()
    }
    /// Failed registrations only; already-working hotkeys remain registered throughout retry.
    pub fn retry(&self) -> Result<(), String> {
        if self.stop.load(Ordering::Acquire) {
            return Err("tray thread has stopped".into());
        }
        if unsafe { PostThreadMessageW(self.thread_id.load(Ordering::Acquire), WM_RETRY, 0, 0) }
            == 0
        {
            return Err(format!(
                "queue tray retry: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
    /// Acquire candidate bindings before committing settings. On reservation or commit failure
    /// all previous registrations and menu remain active. The commit runs on the service
    /// thread and must not access UI or re-enter this service.
    pub fn configure(
        &self,
        menu: Vec<MenuItem>,
        hotkeys: Vec<Hotkey>,
        commit: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) -> Result<async_channel::Receiver<Result<(), String>>, String> {
        TrayOptions::new("validation", "validation", menu.clone())
            .with_hotkeys(hotkeys.clone())
            .validate()?;
        if self.stop.load(Ordering::Acquire) {
            return Err("tray thread has stopped".into());
        }
        let (reply, result) = async_channel::bounded(1);
        self.configure
            .try_send(Configuration {
                menu,
                hotkeys,
                commit: Box::new(commit),
                reply,
            })
            .map_err(|_| "已有快捷键设置正在提交，请稍后重试".to_string())?;
        // The timer also drains requests, so a failed wakeup never loses an accepted transaction.
        unsafe {
            PostThreadMessageW(self.thread_id.load(Ordering::Acquire), WM_CONFIGURE, 0, 0);
        }
        Ok(result)
    }
    pub fn shutdown(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.stop.store(true, Ordering::Release);
            // The timer is also a stop fallback if posting fails, including a nested menu loop.
            unsafe {
                PostThreadMessageW(self.thread_id.load(Ordering::Acquire), WM_STOP, 0, 0);
            }
            let _ = thread.join();
        }
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        self.shutdown();
    }
}
const WM_CALLBACK: u32 = WM_APP + 1;
const WM_STOP: u32 = WM_APP + 2;
const WM_RETRY: u32 = WM_APP + 3;
const WM_CONFIGURE: u32 = WM_APP + 5;
const RECOVERY_TIMER: usize = 1;
#[cfg(test)]
const WM_TEST_LOSE_ICON: u32 = WM_APP + 4;
struct Configuration {
    menu: Vec<MenuItem>,
    hotkeys: Vec<Hotkey>,
    commit: Box<dyn FnOnce() -> Result<(), String> + Send>,
    reply: async_channel::Sender<Result<(), String>>,
}
#[derive(Clone, Copy)]
struct Registration {
    key: Hotkey,
    id: i32,
}
struct WindowState {
    events: Arc<dyn Fn(TrayEvent) + Send + Sync>,
    menu: std::cell::RefCell<Vec<MenuItem>>,
    icon: NOTIFYICONDATAW,
    taskbar_created: u32,
    hotkeys: std::cell::RefCell<Vec<Hotkey>>,
    registered: std::cell::RefCell<Vec<Registration>>,
    requests: async_channel::Receiver<Configuration>,
    status: Arc<Mutex<TrayStatus>>,
    stop: Arc<AtomicBool>,
    #[cfg(test)]
    failed_icon_attempts: std::cell::Cell<u32>,
}
impl WindowState {
    fn publish(&self, next: TrayStatus) {
        let changed = {
            let mut current = self.status.lock().unwrap_or_else(|e| e.into_inner());
            if *current == next {
                false
            } else {
                *current = next.clone();
                true
            }
        };
        if changed {
            (self.events)(TrayEvent::Status(next));
        }
    }
    fn recover(&self, recreate_icon: bool) {
        let mut next = self
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        next.running = true;
        if recreate_icon || !next.available {
            next.available = self.add_icon();
            next.error = (!next.available)
                .then(|| "Windows refused the notification-area icon; retrying".into());
        }
        next.failures.clear();
        for hotkey in self.hotkeys.borrow().iter() {
            if self
                .registered
                .borrow()
                .iter()
                .any(|entry| entry.key == *hotkey)
            {
                continue;
            }
            let registration = {
                let registered = self.registered.borrow();
                self.register_key(*hotkey, &registered)
            };
            match registration {
                Ok(entry) => {
                    self.registered.borrow_mut().push(entry);
                }
                Err(code) => next.failures.push(HotkeyFailure {
                    command: hotkey.command,
                    error: code,
                }),
            }
        }
        self.publish(next);
    }
    fn register_key(&self, key: Hotkey, occupied: &[Registration]) -> Result<Registration, u32> {
        let id = (0x1000..0x2000)
            .find(|id| !occupied.iter().any(|entry| entry.id == *id))
            .ok_or(ERROR_NO_SYSTEM_RESOURCES)?;
        if unsafe {
            RegisterHotKey(
                std::ptr::null_mut(),
                id,
                key.modifiers | MOD_NOREPEAT,
                key.key,
            )
        } == 0
        {
            return Err(unsafe { GetLastError() });
        }
        Ok(Registration { key, id })
    }
    fn configure(&self) {
        while let Ok(request) = self.requests.try_recv() {
            if request.reply.is_closed() {
                continue;
            }
            let previous = self.registered.borrow().clone();
            let mut reserved = previous.clone();
            let mut desired = Vec::new();
            let mut failure = None;
            for key in &request.hotkeys {
                // Reuse a chord even when commands are swapped; ownership never leaves this thread.
                if let Some(entry) = previous
                    .iter()
                    .find(|entry| entry.key.key == key.key && entry.key.modifiers == key.modifiers)
                {
                    desired.push(Registration {
                        key: *key,
                        id: entry.id,
                    });
                } else {
                    match self.register_key(*key, &reserved) {
                        Ok(entry) => {
                            reserved.push(entry);
                            desired.push(entry);
                        }
                        Err(error) => {
                            failure = Some(format!(
                                "快捷键注册失败（命令 {}，Windows 错误 {error}）；原绑定保持有效",
                                key.command
                            ));
                            break;
                        }
                    }
                }
            }
            let result = match failure {
                Some(error) => Err(error),
                None if request.reply.is_closed() => Err("快捷键提交已取消".into()),
                None => (request.commit)(),
            };
            if result.is_ok() {
                for entry in &previous {
                    if !desired.iter().any(|next| next.id == entry.id) {
                        unsafe {
                            UnregisterHotKey(std::ptr::null_mut(), entry.id);
                        }
                    }
                }
                *self.registered.borrow_mut() = desired;
                *self.hotkeys.borrow_mut() = request.hotkeys;
                *self.menu.borrow_mut() = request.menu;
                self.recover(false);
            } else {
                for entry in &reserved {
                    if !previous.iter().any(|old| old.id == entry.id) {
                        unsafe {
                            UnregisterHotKey(std::ptr::null_mut(), entry.id);
                        }
                    }
                }
            }
            let _ = request.reply.try_send(result);
        }
    }
    fn add_icon(&self) -> bool {
        #[cfg(test)]
        if self.failed_icon_attempts.get() > 0 {
            self.failed_icon_attempts
                .set(self.failed_icon_attempts.get() - 1);
            return false;
        }
        // A duplicate TaskbarCreated notification may arrive after the icon was restored.
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &self.icon) != 0
                || Shell_NotifyIconW(NIM_MODIFY, &self.icon) != 0
        }
    }
    fn send(&self, command: u32) {
        let mut point = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut point);
        }
        (self.events)(TrayEvent::Command(TrayCommand {
            command,
            position: (point.x, point.y),
        }));
    }
}
extern "system" fn wndproc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WindowState;
    if message == WM_DESTROY {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            PostQuitMessage(0);
        }
        return 0;
    }
    if !pointer.is_null() {
        // run owns this Box until the message loop and any nested popup loop have returned.
        let state = unsafe { &*pointer };
        if message == state.taskbar_created {
            state.recover(true);
            return 0;
        }
        if message == WM_TIMER && wparam == RECOVERY_TIMER {
            if state.stop.load(Ordering::Acquire) {
                unsafe {
                    EndMenu();
                    PostQuitMessage(0);
                }
            } else {
                state.configure();
                state.recover(false);
            }
            return 0;
        }
        if message == WM_CALLBACK && matches!((lparam as u32) & 0xffff, WM_LBUTTONUP | WM_RBUTTONUP)
        {
            let items = state.menu.borrow().clone();
            if let Some(id) = unsafe { show_menu(hwnd, &items) } {
                state.send(id);
            }
            return 0;
        }
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
unsafe fn show_menu(hwnd: HWND, items: &[MenuItem]) -> Option<u32> {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return None;
    }
    for item in items {
        let label = wide(&item.label);
        if unsafe { AppendMenuW(menu, MF_STRING, item.id as usize, label.as_ptr()) } == 0 {
            unsafe {
                DestroyMenu(menu);
            }
            return None;
        }
    }
    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
    }
    let chosen = unsafe {
        TrackPopupMenuEx(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY,
            point.x,
            point.y,
            hwnd,
            std::ptr::null(),
        )
    } as u32;
    unsafe {
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
    }
    items.iter().any(|item| item.id == chosen).then_some(chosen)
}
fn run(
    options: TrayOptions,
    events: Arc<dyn Fn(TrayEvent) + Send + Sync>,
    ready: mpsc::SyncSender<Result<(), String>>,
    thread_id: Arc<AtomicU32>,
    status: Arc<Mutex<TrayStatus>>,
    stop: Arc<AtomicBool>,
    requests: async_channel::Receiver<Configuration>,
) {
    let mut state = WindowState {
        events,
        menu: std::cell::RefCell::new(options.menu.clone()),
        icon: unsafe { std::mem::zeroed() },
        taskbar_created: 0,
        hotkeys: std::cell::RefCell::new(options.hotkeys.clone()),
        requests,
        registered: std::cell::RefCell::new(Vec::new()),
        status,
        stop,
        #[cfg(test)]
        failed_icon_attempts: std::cell::Cell::new(0),
    };
    let class_name = wide(&format!("{}.Tray", options.identity));
    let title = wide(&options.title);
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        let _ = ready.send(Err(format!(
            "register tray window: {}",
            std::io::Error::last_os_error()
        )));
        return;
    }
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        let error = std::io::Error::last_os_error();
        unsafe {
            UnregisterClassW(class_name.as_ptr(), instance);
        }
        let _ = ready.send(Err(format!("tray window: {error}")));
        return;
    }
    let (icon_handle, owned_icon) = unsafe { tray_icon() };
    state.icon.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    state.icon.hWnd = hwnd;
    state.icon.uID = 1;
    state.icon.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    state.icon.uCallbackMessage = WM_CALLBACK;
    state.icon.hIcon = icon_handle;
    write_wide_buffer(&mut state.icon.szTip, &options.title);
    state.taskbar_created = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
    let state = Box::new(state);
    unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            (&*state as *const WindowState) as isize,
        );
    }
    let initialized = if state.taskbar_created == 0 || icon_handle.is_null() {
        Err("initialize tray icon/restart notification failed".into())
    } else if unsafe { SetTimer(hwnd, RECOVERY_TIMER, 2000, None) } == 0 {
        Err(format!(
            "create tray recovery timer: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    };
    let mut loop_error = None;
    match initialized {
        Ok(()) => {
            thread_id.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
            state.recover(true);
            let _ = ready.send(Ok(()));
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while !state.stop.load(Ordering::Acquire) {
                let result = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
                if result == 0 {
                    break;
                }
                if result == -1 {
                    loop_error = Some(format!(
                        "tray message loop: {}",
                        std::io::Error::last_os_error()
                    ));
                    break;
                }
                if message.message == WM_STOP {
                    break;
                }
                if message.message == WM_CONFIGURE {
                    state.configure();
                    continue;
                }
                if message.message == WM_RETRY {
                    state.recover(false);
                    continue;
                }
                #[cfg(test)]
                if message.message == WM_TEST_LOSE_ICON {
                    unsafe {
                        Shell_NotifyIconW(NIM_DELETE, &state.icon);
                    }
                    state.failed_icon_attempts.set(2);
                    unsafe {
                        SendMessageW(hwnd, state.taskbar_created, 0, 0);
                    }
                    continue;
                }
                if message.message == WM_HOTKEY {
                    let command = state
                        .registered
                        .borrow()
                        .iter()
                        .find(|entry| entry.id == message.wParam as i32)
                        .map(|entry| entry.key.command);
                    if let Some(command) = command {
                        state.send(command);
                    }
                    continue;
                }
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
        Err(error) => {
            let _ = ready.send(Err(error));
        }
    }
    state.stop.store(true, Ordering::Release);
    state.publish(TrayStatus {
        running: false,
        available: false,
        error: Some(loop_error.unwrap_or_else(|| "tray thread stopped".into())),
        failures: Vec::new(),
    });
    unsafe {
        KillTimer(hwnd, RECOVERY_TIMER);
        for id in state.registered.borrow().iter() {
            UnregisterHotKey(std::ptr::null_mut(), id.id);
        }
        Shell_NotifyIconW(NIM_DELETE, &state.icon);
        DestroyWindow(hwnd);
        if owned_icon {
            DestroyIcon(icon_handle);
        }
        UnregisterClassW(class_name.as_ptr(), instance);
    }
}
/// The application's own icon, falling back to the generic one.
unsafe fn tray_icon() -> (HICON, bool) {
    if let Ok(exe) = std::env::current_exe() {
        let path = wide(&exe.to_string_lossy());
        let mut small: HICON = std::ptr::null_mut();
        // SAFETY: `path` is NUL-terminated and both out-pointers are valid.
        let extracted =
            unsafe { ExtractIconExW(path.as_ptr(), 0, std::ptr::null_mut(), &mut small, 1) };
        if extracted > 0 && !small.is_null() {
            return (small, true);
        }
    }
    // SAFETY: a stock icon takes no pointers; a null instance means "system stock".
    (
        unsafe {
            LoadIconW(
                std::ptr::null_mut::<std::ffi::c_void>() as HINSTANCE,
                IDI_APPLICATION,
            )
        },
        false,
    )
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn write_wide_buffer(buffer: &mut [u16], value: &str) {
    if buffer.is_empty() {
        return;
    }
    let encoded: Vec<u16> = value
        .encode_utf16()
        .take(buffer.len().saturating_sub(1))
        .collect();
    buffer[..encoded.len()].copy_from_slice(&encoded);
    buffer[encoded.len()] = 0;
}

#[cfg(test)]
mod tests;
