//! One writer per canonical data root; a same-user/session pipe transfers launch intent.
//! Kept product-private until another product supplies a real consumer.
use crate::startup::{Launch, ProductPaths};
use std::{
    fs::{File, OpenOptions, TryLockError},
    io,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    ptr::{null, null_mut},
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::*,
    System::{
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        Pipes::*,
        RemoteDesktop::ProcessIdToSessionId,
        Threading::{
            CreateEventW, GetCurrentProcess, GetCurrentProcessId, INFINITE, OpenProcessToken,
            SetEvent, WaitForMultipleObjects, WaitForSingleObject,
        },
    },
    UI::WindowsAndMessaging::AllowSetForegroundWindow,
};

const IO_TIMEOUT: u32 = 2000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const RECEIPT: u8 = 0xAC;

/// The lock file stays in place; the OS releases the byte-range lock on process exit/crash.
/// Do not unlink it: doing so could give a third process a different lock inode/handle.
pub(crate) struct DataLock(File);
impl DataLock {
    pub(crate) fn acquire(paths: &ProductPaths) -> Result<Option<Self>, String> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(paths.root.join(".yueshot.lock"))
            .map_err(|e| format!("open data directory lock: {e}"))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self(file))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(format!("lock data directory: {error}")),
        }
    }
}
impl Drop for DataLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Command {
    Show = 0,
    Capture = 1,
    Background = 2,
}
impl Command {
    pub(crate) fn from_launch(launch: Launch) -> Option<Self> {
        match launch {
            Launch::Show => Some(Self::Show),
            Launch::Capture => Some(Self::Capture),
            Launch::Background => Some(Self::Background),
            _ => None,
        }
    }
    fn encode(self) -> [u8; 8] {
        [b'Y', b'S', b'H', b'T', 1, self as u8, 0, 0]
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 8 || bytes[..5] != *b"YSHT\x01" || bytes[6..] != [0, 0] {
            return None;
        }
        match bytes[5] {
            0 => Some(Self::Show),
            1 => Some(Self::Capture),
            2 => Some(Self::Background),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum Event {
    Command(Command),
    Failed(String),
}

/// The app owns the server, not any window. Shutdown cancels pending I/O before joining.
pub(crate) struct Instance {
    stop: Arc<OwnedHandle>,
    thread: Option<JoinHandle<()>>,
}
impl Instance {
    pub(crate) fn start(
        paths: &ProductPaths,
    ) -> Result<(Self, async_channel::Receiver<Event>), String> {
        let name = pipe_name(paths)?;
        // Create the endpoint before returning, so a secondary can queue intent during UI startup.
        let pipe = create_pipe(&name).map_err(|e| format!("create instance pipe: {e}"))?;
        let stop = Arc::new(event().map_err(|e| e.to_string())?);
        let thread_stop = stop.clone();
        let session = session_id()?;
        let (sender, receiver) = async_channel::bounded(32);
        let thread = thread::Builder::new()
            .name("yueshot-instance".into())
            .spawn(move || {
                if let Err(error) = serve(&pipe, &thread_stop, session, &sender) {
                    let message = format!("单实例命令接收已停止：{error}；请退出后重启 YueShot");
                    eprintln!("[yue-shot][instance] {message}");
                    let _ = sender.try_send(Event::Failed(message));
                }
            })
            .map_err(|e| format!("start instance thread: {e}"))?;
        Ok((
            Self {
                stop,
                thread: Some(thread),
            },
            receiver,
        ))
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            SetEvent(raw(&self.stop));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) fn forward(paths: &ProductPaths, command: Command) -> Result<(), String> {
    let name = pipe_name(paths)?;
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    let pipe = loop {
        match open_pipe(&name) {
            Ok(pipe) => break pipe,
            Err(error)
                if matches!(error.raw_os_error(), Some(code) if code == ERROR_FILE_NOT_FOUND as i32 || code == ERROR_PIPE_BUSY as i32)
                    && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                return Err(format!(
                    "数据目录已被占用，无法向当前用户/会话中的同渠道实例转交命令（{error}）；不启动第二个写入者"
                ));
            }
        }
    };
    // Permit the owner to activate its existing window when this launch has foreground rights.
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(raw(&pipe), &mut pid) } == 0 {
        return Err(format!(
            "verify instance process: {}",
            io::Error::last_os_error()
        ));
    }
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(pid, &mut session) } == 0 || session != session_id()? {
        return Err("existing instance belongs to another Windows session".into());
    }
    unsafe {
        AllowSetForegroundWindow(pid);
    }
    transact(&pipe, &command.encode()).map_err(|e| format!("transfer instance command: {e}"))
}

fn transact(pipe: &OwnedHandle, request: &[u8]) -> io::Result<()> {
    let stop = event()?;
    write(pipe, &stop, request)?;
    let mut response = [0xff];
    if read(pipe, &stop, &mut response)? != 1 {
        return Err(io::Error::other("invalid instance acknowledgement"));
    }
    write(pipe, &stop, &[RECEIPT])?;
    match response[0] {
        0 => Ok(()),
        1 => Err(io::Error::other(
            "instance command queue is full or shutting down",
        )),
        _ => Err(io::Error::other("instance rejected the command protocol")),
    }
}

fn serve(
    pipe: &OwnedHandle,
    stop: &OwnedHandle,
    session: u32,
    sender: &async_channel::Sender<Event>,
) -> io::Result<()> {
    while unsafe { WaitForSingleObject(raw(stop), 0) } != WAIT_OBJECT_0 {
        match operation(pipe, stop, INFINITE, true, |ov| unsafe {
            ConnectNamedPipe(raw(pipe), ov)
        }) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(()),
            Err(error) if matches!(error.raw_os_error(), Some(code) if code == ERROR_NO_DATA as i32 || code == ERROR_BROKEN_PIPE as i32 || code == ERROR_PIPE_NOT_CONNECTED as i32) =>
            {
                // The client can exit between CreateFile and our ConnectNamedPipe call.
                unsafe {
                    DisconnectNamedPipe(raw(pipe));
                }
                continue;
            }
            Err(error) => return Err(error),
        }
        // A peer may disappear, send bad/oversized data, or stop reading. Only its transaction
        // fails; the persistent endpoint continues serving the next launch.
        let _ = receive(pipe, stop, session, sender);
        unsafe {
            DisconnectNamedPipe(raw(pipe));
        }
    }
    Ok(())
}

fn receive(
    pipe: &OwnedHandle,
    stop: &OwnedHandle,
    session: u32,
    sender: &async_channel::Sender<Event>,
) -> io::Result<()> {
    let mut peer_session = u32::MAX;
    if unsafe { GetNamedPipeClientSessionId(raw(pipe), &mut peer_session) } == 0
        || peer_session != session
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "different Windows session",
        ));
    }
    let mut bytes = [0; 8];
    // Message mode rejects oversized frames with ERROR_MORE_DATA before dispatch.
    let count = read(pipe, stop, &mut bytes)?;
    let status = match Command::decode(&bytes[..count as usize]) {
        Some(command) => {
            if sender.try_send(Event::Command(command)).is_ok() {
                0
            } else {
                1
            }
        }
        None => 2,
    };
    write(pipe, stop, &[status])?;
    // DisconnectNamedPipe discards unread data. A bounded receipt handshake keeps the ack
    // available until the client consumes it, without blocking in FlushFileBuffers.
    let mut receipt = [0];
    if read(pipe, stop, &mut receipt)? != 1 || receipt != [RECEIPT] {
        return Err(io::Error::other("missing instance receipt"));
    }
    Ok(())
}

fn pipe_name(paths: &ProductPaths) -> Result<Vec<u16>, String> {
    Ok(wide(&format!(
        r"\\.\pipe\{}.session-{}",
        paths.instance_id,
        session_id()?
    )))
}
fn session_id() -> Result<u32, String> {
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 {
        Err(format!(
            "read Windows session: {}",
            io::Error::last_os_error()
        ))
    } else {
        Ok(session)
    }
}
fn create_pipe(name: &[u16]) -> io::Result<OwnedHandle> {
    let descriptor = pipe_security()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    owned(unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            8,
            8,
            IO_TIMEOUT,
            &attributes,
        )
    })
}
fn open_pipe(name: &[u16]) -> io::Result<OwnedHandle> {
    owned(unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            null_mut(),
        )
    })
}

struct LocalAllocation(*mut std::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn pipe_security() -> io::Result<LocalAllocation> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = owned(token)?;
    let mut length = 0;
    unsafe {
        GetTokenInformation(raw(&token), TokenUser, null_mut(), 0, &mut length);
    }
    if length < std::mem::size_of::<TOKEN_USER>() as u32 {
        return Err(io::Error::last_os_error());
    }
    // usize alignment also satisfies TOKEN_USER; the SID points inside this allocation.
    let mut buffer = vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            raw(&token),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut sid = null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _sid_allocation = LocalAllocation(sid.cast());
    let mut end = 0;
    while unsafe { *sid.add(end) } != 0 {
        end += 1;
    }
    let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(sid, end) });
    // Protected DACL: current user and SYSTEM, no inherited Everyone/network grants.
    let sddl = wide(&format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid})"));
    let mut descriptor = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(LocalAllocation(descriptor))
}

fn read(pipe: &OwnedHandle, stop: &OwnedHandle, bytes: &mut [u8]) -> io::Result<u32> {
    operation(pipe, stop, IO_TIMEOUT, false, |ov| unsafe {
        ReadFile(
            raw(pipe),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            null_mut(),
            ov,
        )
    })
}
fn write(pipe: &OwnedHandle, stop: &OwnedHandle, bytes: &[u8]) -> io::Result<()> {
    let count = operation(pipe, stop, IO_TIMEOUT, false, |ov| unsafe {
        WriteFile(
            raw(pipe),
            bytes.as_ptr(),
            bytes.len() as u32,
            null_mut(),
            ov,
        )
    })?;
    if count != bytes.len() as u32 {
        return Err(io::Error::other("incomplete pipe write"));
    }
    Ok(())
}
fn operation(
    pipe: &OwnedHandle,
    stop: &OwnedHandle,
    timeout: u32,
    connected_is_success: bool,
    start: impl FnOnce(*mut OVERLAPPED) -> BOOL,
) -> io::Result<u32> {
    let ready = event()?;
    let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
    ov.hEvent = raw(&ready);
    let mut count = 0;
    // ov, the event and caller-owned byte buffer remain live until I/O completes or is drained.
    if start(&mut ov) != 0 {
        if unsafe { GetOverlappedResult(raw(pipe), &ov, &mut count, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        return Ok(count);
    }
    let error = io::Error::last_os_error();
    if connected_is_success && error.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
        return Ok(0);
    }
    if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
        return Err(error);
    }
    let handles = [raw(stop), raw(&ready)];
    let waited = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, timeout) };
    if waited == WAIT_OBJECT_0 + 1 {
        if unsafe { GetOverlappedResult(raw(pipe), &ov, &mut count, 0) } != 0 {
            return Ok(count);
        }
        return Err(io::Error::last_os_error());
    }
    let error = match waited {
        WAIT_OBJECT_0 => io::Error::new(io::ErrorKind::Interrupted, "instance is shutting down"),
        WAIT_TIMEOUT => io::Error::new(io::ErrorKind::TimedOut, "instance pipe timed out"),
        _ => io::Error::last_os_error(),
    };
    // Cancel and wait before stack/buffer destruction, including the cancellation race.
    unsafe {
        CancelIoEx(raw(pipe), &ov);
        GetOverlappedResult(raw(pipe), &ov, &mut count, 1);
    }
    Err(error)
}
fn event() -> io::Result<OwnedHandle> {
    owned(unsafe { CreateEventW(null(), 1, 0, null()) })
}
fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}
fn raw(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle()
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startup::{Channel, StartupOptions};
    use std::{
        path::PathBuf,
        process::{Child, Command as ProcessCommand, Stdio},
    };

    struct Sandbox(PathBuf);
    impl Sandbox {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            Self(std::env::temp_dir().join(format!(
                "yueshot-instance-{}-{}-{}",
                std::process::id(),
                yue_shot_model::unix_time_ms(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            )))
        }
        fn paths(&self, channel: Channel) -> ProductPaths {
            ProductPaths::resolve(
                &StartupOptions {
                    data_dir: Some(self.0.clone()),
                    ..Default::default()
                },
                channel,
                None,
            )
            .unwrap()
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    struct TestChild(Child);
    impl Drop for TestChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn child(sandbox: &Sandbox, mode: &str) -> TestChild {
        TestChild(
            ProcessCommand::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "instance::tests::instance_subprocess",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("YUESHOT_INSTANCE_TEST_ROOT", &sandbox.0)
                .env("YUESHOT_INSTANCE_TEST_MODE", mode)
                .stdin(Stdio::piped())
                .spawn()
                .unwrap(),
        )
    }
    fn expect_command(receiver: &async_channel::Receiver<Event>, expected: Command) {
        match receiver
            .try_recv()
            .expect("accepted command must be queued")
        {
            Event::Command(actual) => assert_eq!(actual, expected),
            Event::Failed(error) => panic!("{error}"),
        }
    }
    fn wait_child(child: &mut TestChild) -> std::process::ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "instance child timed out");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn protocol_accepts_only_fixed_versioned_commands() {
        for command in [Command::Show, Command::Capture, Command::Background] {
            assert_eq!(Command::decode(&command.encode()), Some(command));
        }
        for bytes in [
            b"".as_slice(),
            b"YSHT\x01\x00\x00",
            b"YSHT\x02\x00\x00\x00",
            b"YSHT\x01\x03\x00\x00",
            b"YSHT\x01\x00\x01\x00",
            b"YSHT\x01\x00\x00\x00x",
        ] {
            assert_eq!(Command::decode(bytes), None);
        }
        for launch in [
            Launch::Help,
            Launch::Version,
            Launch::Smoke,
            Launch::RemoveStartup,
            Launch::Backup,
            Launch::Restore,
            Launch::ExportDiagnostics,
            Launch::ProductInfo,
            Launch::UninstallCleanup,
        ] {
            assert!(Command::from_launch(launch).is_none());
        }
    }

    #[test]
    fn data_root_lock_excludes_other_channels_and_releases_without_unlinking() {
        let sandbox = Sandbox::new();
        let dev = sandbox.paths(Channel::Dev);
        let beta = sandbox.paths(Channel::Beta);
        let lock = DataLock::acquire(&dev).unwrap().unwrap();
        assert!(DataLock::acquire(&dev).unwrap().is_none());
        assert!(DataLock::acquire(&beta).unwrap().is_none());
        assert!(std::fs::remove_file(dev.root.join(".yueshot.lock")).is_err());
        drop(lock);
        assert!(DataLock::acquire(&beta).unwrap().is_some());
        assert!(dev.root.join(".yueshot.lock").exists());
    }

    #[test]
    fn commands_transfer_over_real_pipes_and_shutdown_releases_the_endpoint() {
        let sandbox = Sandbox::new();
        let paths = sandbox.paths(Channel::Dev);
        let _lock = DataLock::acquire(&paths).unwrap().unwrap();
        let (owner, receiver) = Instance::start(&paths).unwrap();
        assert!(Instance::start(&paths).is_err());
        for command in [Command::Show, Command::Capture, Command::Background] {
            forward(&paths, command).unwrap();
            expect_command(&receiver, command);
        }
        let start = Instant::now();
        drop(owner);
        assert!(start.elapsed() < Duration::from_secs(1));
        let (owner, receiver) = Instance::start(&paths).unwrap();
        forward(&paths, Command::Show).unwrap();
        expect_command(&receiver, Command::Show);
        drop(owner);
    }

    #[test]
    fn bad_and_oversized_clients_do_not_poison_the_next_launch() {
        let sandbox = Sandbox::new();
        let paths = sandbox.paths(Channel::Dev);
        let (_owner, receiver) = Instance::start(&paths).unwrap();
        for request in [b"YSHT\x02\x00\x00\x00".as_slice(), b"YSHT\x01\x00\x00\x00x"] {
            let pipe = connect_test_client(&paths);
            assert!(transact(&pipe, request).is_err());
            assert!(receiver.is_empty());
            drop(pipe);
            forward(&paths, Command::Show).unwrap();
            expect_command(&receiver, Command::Show);
        }
    }
    #[test]
    fn clients_that_exit_before_sending_do_not_stop_the_server() {
        let sandbox = Sandbox::new();
        let paths = sandbox.paths(Channel::Dev);
        let (_owner, receiver) = Instance::start(&paths).unwrap();
        for _ in 0..16 {
            drop(connect_test_client(&paths));
        }
        forward(&paths, Command::Show).unwrap();
        expect_command(&receiver, Command::Show);
    }
    fn connect_test_client(paths: &ProductPaths) -> OwnedHandle {
        let name = pipe_name(paths).unwrap();
        let deadline = Instant::now() + CONNECT_TIMEOUT;
        loop {
            match open_pipe(&name) {
                Ok(pipe) => return pipe,
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Err(error) => panic!("{error}"),
            }
        }
    }

    #[test]
    fn full_queue_rejects_a_launch_and_recovers_after_drain() {
        let sandbox = Sandbox::new();
        let paths = sandbox.paths(Channel::Dev);
        let (_owner, receiver) = Instance::start(&paths).unwrap();
        for _ in 0..32 {
            forward(&paths, Command::Background).unwrap();
        }
        assert!(
            forward(&paths, Command::Capture)
                .unwrap_err()
                .contains("queue")
        );
        for _ in 0..32 {
            expect_command(&receiver, Command::Background);
        }
        forward(&paths, Command::Capture).unwrap();
        expect_command(&receiver, Command::Capture);
    }

    #[test]
    fn idle_peer_is_timed_out_and_pending_io_is_cancelled_on_shutdown() {
        let sandbox = Sandbox::new();
        let paths = sandbox.paths(Channel::Dev);
        let (owner, receiver) = Instance::start(&paths).unwrap();
        let _idle = connect_test_client(&paths);
        forward(&paths, Command::Show).unwrap();
        expect_command(&receiver, Command::Show);
        let _idle_again = connect_test_client(&paths);
        let start = Instant::now();
        drop(owner);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "shutdown must cancel pending reads"
        );
    }

    #[test]
    fn a_second_process_transfers_capture_without_becoming_a_writer() {
        let sandbox = Sandbox::new();
        let paths = sandbox.paths(Channel::Dev);
        let _lock = DataLock::acquire(&paths).unwrap().unwrap();
        let (_owner, receiver) = Instance::start(&paths).unwrap();
        let mut child = child(&sandbox, "forward");
        assert!(wait_child(&mut child).success());
        expect_command(&receiver, Command::Capture);
    }

    #[test]
    fn isolated_roots_can_run_independently_and_crash_releases_the_writer_lock() {
        let first = Sandbox::new();
        let second = Sandbox::new();
        let paths = first.paths(Channel::Dev);
        let independent = second.paths(Channel::Dev);
        let _independent_lock = DataLock::acquire(&independent).unwrap().unwrap();
        let (_owner, receiver) = Instance::start(&independent).unwrap();
        let mut child = child(&first, "hold");
        let deadline = Instant::now() + Duration::from_secs(8);
        while !first.0.join("ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "lock holder exited early"
            );
            assert!(
                Instant::now() < deadline,
                "lock holder did not become ready"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert!(DataLock::acquire(&paths).unwrap().is_none());
        forward(&independent, Command::Show).unwrap();
        expect_command(&receiver, Command::Show);
        // Only this disposable test helper is killed: this proves crash recovery, not normal exit.
        child.0.kill().unwrap();
        assert!(!wait_child(&mut child).success());
        assert!(DataLock::acquire(&paths).unwrap().is_some());
    }

    #[test]
    fn instance_subprocess() {
        let Some(root) = std::env::var_os("YUESHOT_INSTANCE_TEST_ROOT") else {
            return;
        };
        let paths = ProductPaths::resolve(
            &StartupOptions {
                data_dir: Some(root.into()),
                ..Default::default()
            },
            Channel::Dev,
            None,
        )
        .unwrap();
        match std::env::var("YUESHOT_INSTANCE_TEST_MODE")
            .unwrap()
            .as_str()
        {
            "forward" => {
                assert!(DataLock::acquire(&paths).unwrap().is_none());
                forward(&paths, Command::Capture).unwrap();
            }
            "hold" => {
                let _lock = DataLock::acquire(&paths).unwrap().unwrap();
                std::fs::write(paths.root.join("ready"), b"ready").unwrap();
                let mut byte = [0];
                std::io::Read::read_exact(&mut std::io::stdin(), &mut byte).unwrap();
            }
            other => panic!("unknown subprocess mode {other}"),
        }
    }
}
