//! Per-user Windows login registration. Windows is the source of truth, not settings.json.
//! The product identity includes the channel and canonical data root.
use crate::startup::{Channel, ProductPaths};
use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path};
use windows_sys::Win32::{Foundation::*, System::Registry::*};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVAL: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const MAX_VALUE_BYTES: u32 = 64 * 1024;

/// Uninstall owns startup entries pointing to this executable, including custom data roots.
/// It neither creates a data directory nor removes another installation/channel's entry.
pub(super) fn remove_installation_startup(channel: Channel) -> Result<(), String> {
    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| e.to_string())?;
    remove_owned_startup(RUN, APPROVAL, channel, &exe)
}
fn remove_owned_startup(
    run: &str,
    approval: &str,
    channel: Channel,
    exe: &Path,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStringExt as _;
    let mut key = std::ptr::null_mut();
    let code = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(run).as_ptr(),
            0,
            KEY_QUERY_VALUE | KEY_WOW64_64KEY,
            &mut key,
        )
    };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(code, "读取卸载拥有的启动项")?;
    let key = Key(key);
    let prefix = format!("com.seeyuer.yueshot.{}.", channel.name());
    let mut names = Vec::new();
    // Snapshot names before deleting: deleting during enumeration skips shifted indexes.
    for index in 0..4096 {
        let mut name = [0u16; 16384];
        let mut len = name.len() as u32;
        let code = unsafe {
            RegEnumValueW(
                key.0,
                index,
                name.as_mut_ptr(),
                &mut len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if code == ERROR_NO_MORE_ITEMS {
            break;
        }
        check(code, "枚举卸载拥有的启动项")?;
        let name = String::from_utf16(&name[..len as usize]).map_err(|e| e.to_string())?;
        if name.starts_with(&prefix) {
            names.push(name);
        }
        if index == 4095 {
            return Err("启动项数量超过卸载检查上限".into());
        }
    }
    for name in names {
        let registry = Registry::new(run, approval, &name)?;
        let Some(value) = registry.read(&registry.run)? else {
            continue;
        };
        if value.kind != REG_SZ {
            continue;
        }
        let Ok(words) = utf16(&value.bytes) else {
            continue;
        };
        let mut count = 0;
        let argv = unsafe {
            windows_sys::Win32::UI::Shell::CommandLineToArgvW(words.as_ptr(), &mut count)
        };
        if argv.is_null() {
            return Err("无法解析启动命令".into());
        }
        let args: Vec<_> = unsafe { std::slice::from_raw_parts(argv, count as usize) }
            .iter()
            .map(|&ptr| {
                let mut len = 0;
                while unsafe { *ptr.add(len) } != 0 {
                    len += 1;
                }
                std::ffi::OsString::from_wide(unsafe { std::slice::from_raw_parts(ptr, len) })
            })
            .collect();
        unsafe {
            LocalFree(argv.cast());
        }
        if args.len() != 4 || args[1] != "--background" || args[2] != "--data-dir" {
            continue;
        }
        let command_exe = Path::new(&args[0]);
        let root = Path::new(&args[3]);
        if !command_exe.is_absolute() || !root.is_absolute() {
            continue;
        }
        let Ok(command_exe) = command_exe.canonicalize() else {
            continue;
        };
        let Ok(root) = root.canonicalize() else {
            continue;
        };
        if command_exe.to_string_lossy().to_lowercase() != exe.to_string_lossy().to_lowercase()
            || crate::product::instance_id(channel, &root) != name
        {
            continue;
        }
        // Do not delete if another process changed the registration while we inspected it.
        let current = registry.read(&registry.run)?;
        if current.is_some_and(|v| v.kind == value.kind && v.bytes == value.bytes) {
            registry.remove()?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Missing,
    Registered,
    BlockedByWindows,
    WindowsStatusUnknown,
    DifferentCommand,
}
impl Status {
    pub(crate) fn is_registered(&self) -> bool {
        !matches!(self, Self::Missing)
    }
    pub(crate) fn message(&self) -> &'static str {
        match self {
            Self::Missing => "未注册登录启动",
            Self::Registered => "已注册登录启动，将在后台运行",
            Self::BlockedByWindows => {
                "已注册，但 Windows 已禁用此启动项；请在 Windows 设置的“启动应用”中启用"
            }
            Self::WindowsStatusUnknown => {
                "已注册；Windows 启动许可状态无法识别，请在“启动应用”中检查"
            }
            Self::DifferentCommand => "启动项与当前程序位置不一致，请更新启动项",
        }
    }
}

/// Injectable boundary keeps UI tests away from the real Windows login keys.
pub(crate) trait LoginStartup: Send + Sync {
    fn status(&self) -> Result<Status, String>;
    fn set_registered(&self, enabled: bool) -> Result<Status, String>;
}

pub(crate) struct Registration {
    registry: Registry,
    command: Vec<u16>,
}
impl Registration {
    pub(crate) fn remove_for(paths: &ProductPaths) -> Result<(), String> {
        let registry = Registry::new(RUN, APPROVAL, &paths.instance_id)?;
        registry.remove()?;
        if registry.read(&registry.run)?.is_some() {
            return Err("Windows 登录启动移除后校验失败".into());
        }
        Ok(())
    }
    pub(crate) fn current(paths: &ProductPaths) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| format!("读取程序位置失败：{e}"))?;
        Self::new(
            Registry::new(RUN, APPROVAL, &paths.instance_id)?,
            &exe,
            &paths.root,
        )
    }
    fn new(registry: Registry, exe: &Path, data: &Path) -> Result<Self, String> {
        if !exe.is_absolute() || !data.is_absolute() {
            return Err("登录启动需要绝对路径".into());
        }
        let mut command = quote(exe.as_os_str())?;
        command.extend(" --background --data-dir ".encode_utf16());
        command.extend(quote(data.as_os_str())?);
        command.push(0);
        // Windows Run explicitly limits command lines to 260 UTF-16 characters.
        if command.len() - 1 > 260 {
            return Err("程序或数据路径过长，无法注册 Windows 登录启动（最多 260 个字符）".into());
        }
        Ok(Self { registry, command })
    }
}
impl LoginStartup for Registration {
    fn status(&self) -> Result<Status, String> {
        let Some(value) = self.registry.read(&self.registry.run)? else {
            return Ok(Status::Missing);
        };
        if value.kind != REG_SZ || utf16(&value.bytes)? != self.command {
            return Ok(Status::DifferentCommand);
        }
        let approval = self.registry.read(&self.registry.approval)?;
        Ok(match approval {
            None => Status::Registered,
            // StartupApproved is Windows-owned. Observe recognized states only; never rewrite
            // this value to circumvent the user's Task Manager/startup-apps decision.
            Some(value) if value.kind == REG_BINARY && value.bytes.len() >= 12 => {
                match u32::from_le_bytes(value.bytes[..4].try_into().unwrap()) {
                    2 | 6 => Status::Registered,
                    3 | 7 => Status::BlockedByWindows,
                    _ => Status::WindowsStatusUnknown,
                }
            }
            Some(_) => Status::WindowsStatusUnknown,
        })
    }
    fn set_registered(&self, enabled: bool) -> Result<Status, String> {
        if enabled {
            let bytes: Vec<_> = self
                .command
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect();
            self.registry.write(&bytes)?;
        } else {
            self.registry.remove()?;
        }
        let status = self.status()?;
        if status.is_registered() != enabled || status == Status::DifferentCommand {
            return Err("Windows 登录启动写入后校验失败，请刷新后重试".into());
        }
        Ok(status)
    }
}

/// Always quote and implement CommandLineToArgvW's backslash/quote rules losslessly.
fn quote(value: &OsStr) -> Result<Vec<u16>, String> {
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for word in value.encode_wide() {
        if word == 0 {
            return Err("启动路径不能包含空字符".into());
        }
        if word == b'\\' as u16 {
            slashes += 1;
        } else {
            result.extend(std::iter::repeat_n(
                b'\\' as u16,
                if word == b'"' as u16 {
                    slashes * 2 + 1
                } else {
                    slashes
                },
            ));
            slashes = 0;
            result.push(word);
        }
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
    Ok(result)
}
fn utf16(bytes: &[u8]) -> Result<Vec<u16>, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("Windows 启动项字符串长度无效".into());
    }
    let words: Vec<_> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect();
    if words.last() != Some(&0) || words[..words.len() - 1].contains(&0) {
        return Err("Windows 启动项字符串格式无效".into());
    }
    Ok(words)
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
struct Registry {
    run: Vec<u16>,
    approval: Vec<u16>,
    name: Vec<u16>,
}
struct Value {
    kind: u32,
    bytes: Vec<u8>,
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
impl Registry {
    fn new(run: &str, approval: &str, name: &str) -> Result<Self, String> {
        if name.is_empty() || [run, approval, name].iter().any(|s| s.contains('\0')) {
            return Err("登录启动注册身份无效".into());
        }
        Ok(Self {
            run: wide(run),
            approval: wide(approval),
            name: wide(name),
        })
    }
    fn read(&self, path: &[u16]) -> Result<Option<Value>, String> {
        let mut key = std::ptr::null_mut();
        let code = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                KEY_QUERY_VALUE | KEY_WOW64_64KEY,
                &mut key,
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(code, "读取登录启动注册表")?;
        let key = Key(key);
        // A concurrent Windows settings update may change its length between the two queries.
        // Retry a small bounded number of times; never truncate or accept unbounded data.
        for _ in 0..3 {
            let mut kind = 0;
            let mut len = 0;
            let code = unsafe {
                RegQueryValueExW(
                    key.0,
                    self.name.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    std::ptr::null_mut(),
                    &mut len,
                )
            };
            if code == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            check(code, "读取登录启动项大小")?;
            if len > MAX_VALUE_BYTES {
                return Err("Windows 登录启动项过大，未读取或修改".into());
            }
            let mut bytes = vec![0; len as usize];
            let code = unsafe {
                RegQueryValueExW(
                    key.0,
                    self.name.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    bytes.as_mut_ptr(),
                    &mut len,
                )
            };
            if code == ERROR_MORE_DATA {
                continue;
            }
            if code == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            check(code, "读取登录启动项")?;
            bytes.truncate(len as usize);
            return Ok(Some(Value { kind, bytes }));
        }
        Err("Windows 登录启动项正在变化，请重试".into())
    }
    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        let mut key = std::ptr::null_mut();
        check(
            unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    self.run.as_ptr(),
                    0,
                    std::ptr::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_SET_VALUE | KEY_WOW64_64KEY,
                    std::ptr::null(),
                    &mut key,
                    std::ptr::null_mut(),
                )
            },
            "打开登录启动项",
        )?;
        let key = Key(key);
        check(
            unsafe {
                RegSetValueExW(
                    key.0,
                    self.name.as_ptr(),
                    0,
                    REG_SZ,
                    bytes.as_ptr(),
                    bytes.len() as u32,
                )
            },
            "注册登录启动",
        )
    }
    fn remove(&self) -> Result<(), String> {
        let mut key = std::ptr::null_mut();
        let code = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                self.run.as_ptr(),
                0,
                KEY_SET_VALUE | KEY_WOW64_64KEY,
                &mut key,
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(code, "打开登录启动项")?;
        let key = Key(key);
        let code = unsafe { RegDeleteValueW(key.0, self.name.as_ptr()) };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(code, "移除登录启动")
    }
}
fn check(code: u32, action: &str) -> Result<(), String> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!(
            "{action}失败：{}",
            std::io::Error::from_raw_os_error(code as i32)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uninstall_cleans_only_this_executable_and_channel_for_all_data_roots() {
        let sandbox = Sandbox::new();
        let root = std::env::temp_dir().join(format!(
            "yueshot-uninstall-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(root.join("data-a")).unwrap();
        std::fs::create_dir_all(root.join("data-b")).unwrap();
        std::fs::write(root.join("YueShot.exe"), b"fixture").unwrap();
        std::fs::write(root.join("other.exe"), b"fixture").unwrap();
        let exe = root.join("YueShot.exe").canonicalize().unwrap();
        let other = root.join("other.exe").canonicalize().unwrap();
        let a = root.join("data-a").canonicalize().unwrap();
        let b = root.join("data-b").canonicalize().unwrap();
        let fixtures = [
            (Channel::Beta, &a, &exe, true),
            (Channel::Beta, &b, &exe, true),
            (Channel::Dev, &a, &exe, false),
            (Channel::Stable, &a, &exe, false),
            (Channel::Beta, &root.canonicalize().unwrap(), &other, false),
        ];
        let mut registrations = Vec::new();
        for (channel, data, path, removed) in fixtures {
            let name = crate::product::instance_id(channel, data);
            let registration = Registration::new(sandbox.registry(&name), path, data).unwrap();
            assert_eq!(
                registration.set_registered(true).unwrap(),
                Status::Registered
            );
            // Windows-owned disabled state must not be rewritten or removed by uninstall.
            sandbox.value(
                true,
                &name,
                REG_BINARY,
                &[3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            );
            registrations.push((registration, removed));
        }
        let malformed = sandbox.registration("com.seeyuer.yueshot.beta.not-a-real-data-identity");
        malformed.set_registered(true).unwrap();
        remove_owned_startup(
            &format!(r"{}\Run", sandbox.root),
            &format!(r"{}\Approved", sandbox.root),
            Channel::Beta,
            &exe,
        )
        .unwrap();
        for (registration, removed) in registrations {
            assert_eq!(
                registration
                    .registry
                    .read(&registration.registry.run)
                    .unwrap()
                    .is_none(),
                removed
            );
            assert!(
                registration
                    .registry
                    .read(&registration.registry.approval)
                    .unwrap()
                    .is_some()
            );
        }
        assert_ne!(malformed.status().unwrap(), Status::Missing);
        std::fs::remove_dir_all(&root).unwrap();
    }
    struct Sandbox {
        root: String,
    }
    impl Sandbox {
        fn new() -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            Self {
                root: format!(
                    r"Software\YueShot\AutostartTests\{}-{stamp}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ),
            }
        }
        fn registry(&self, name: &str) -> Registry {
            Registry::new(
                &format!(r"{}\Run", self.root),
                &format!(r"{}\Approved", self.root),
                name,
            )
            .unwrap()
        }
        fn registration(&self, name: &str) -> Registration {
            Registration::new(
                self.registry(name),
                Path::new(r"C:\Test 程序\YueShot.exe"),
                Path::new(r"C:\Data space\悦"),
            )
            .unwrap()
        }
        fn value(&self, approved: bool, name: &str, kind: u32, bytes: &[u8]) {
            let registry = self.registry(name);
            let path = if approved {
                &registry.approval
            } else {
                &registry.run
            };
            let mut key = std::ptr::null_mut();
            check(
                unsafe {
                    RegCreateKeyExW(
                        HKEY_CURRENT_USER,
                        path.as_ptr(),
                        0,
                        std::ptr::null(),
                        REG_OPTION_NON_VOLATILE,
                        KEY_SET_VALUE | KEY_WOW64_64KEY,
                        std::ptr::null(),
                        &mut key,
                        std::ptr::null_mut(),
                    )
                },
                "create test key",
            )
            .unwrap();
            let key = Key(key);
            // RegSetValueExW may append a trailing NUL to an unterminated REG_SZ based
            // on the adjacent memory. The native counted-byte API is required to seed
            // exact corrupt on-disk data without a nondeterministic fixture repair.
            let name = UNICODE_STRING {
                Length: ((registry.name.len() - 1) * 2).try_into().unwrap(),
                MaximumLength: (registry.name.len() * 2).try_into().unwrap(),
                Buffer: registry.name.as_ptr().cast_mut(),
            };
            let status = unsafe {
                windows_sys::Wdk::System::Registry::NtSetValueKey(
                    key.0,
                    &name,
                    0,
                    kind,
                    bytes.as_ptr().cast(),
                    bytes.len() as u32,
                )
            };
            assert!(status >= 0, "seed exact test value failed: 0x{status:08x}");
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            // Deliberately isolated test subtree. Never write/delete the real Windows Run key.
            assert!(self.root.starts_with(r"Software\YueShot\AutostartTests\"));
            unsafe {
                RegDeleteTreeW(HKEY_CURRENT_USER, wide(&self.root).as_ptr());
            }
        }
    }
    #[test]
    fn login_command_roundtrips_unicode_spaces_quotes_and_trailing_slashes() {
        use windows_sys::Win32::UI::Shell::CommandLineToArgvW;
        for (exe, data) in [
            (r"C:\Test 程序\YueShot.exe", r"C:\Data space\悦"),
            (r"C:\YueShot.exe", r"C:\"),
            (r"C:\YueShot.exe", r"\\?\D:\test space\悦\"),
        ] {
            let sandbox = Sandbox::new();
            let registration =
                Registration::new(sandbox.registry("command"), Path::new(exe), Path::new(data))
                    .unwrap();
            let mut count = 0;
            let args = unsafe { CommandLineToArgvW(registration.command.as_ptr(), &mut count) };
            assert!(!args.is_null());
            assert_eq!(count, 4);
            let actual: Vec<_> = unsafe { std::slice::from_raw_parts(args, count as usize) }
                .iter()
                .map(|pointer| {
                    let mut len = 0;
                    while unsafe { *pointer.add(len) } != 0 {
                        len += 1;
                    }
                    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(*pointer, len) })
                })
                .collect();
            unsafe {
                LocalFree(args.cast());
            }
            assert_eq!(actual, [exe, "--background", "--data-dir", data]);
        }
        // Embedded quotes are not valid in file names, but the encoder must still be correct.
        assert_eq!(
            quote(OsStr::new("a\\\"b")).unwrap(),
            "\"a\\\\\\\"b\"".encode_utf16().collect::<Vec<_>>()
        );
        assert!(quote(OsStr::new("bad\0path")).is_err());
    }
    #[test]
    fn registry_read_register_remove_and_identities_are_isolated() {
        let sandbox = Sandbox::new();
        let dev = sandbox.registration("yue.dev.root-a");
        let other = sandbox.registration("yue.beta.root-b");
        assert_eq!(dev.status().unwrap(), Status::Missing);
        assert_eq!(dev.set_registered(true).unwrap(), Status::Registered);
        assert_eq!(other.status().unwrap(), Status::Missing);
        assert_eq!(other.set_registered(true).unwrap(), Status::Registered);
        assert_eq!(dev.set_registered(false).unwrap(), Status::Missing);
        assert_eq!(dev.set_registered(false).unwrap(), Status::Missing);
        assert_eq!(other.status().unwrap(), Status::Registered);
    }
    #[test]
    fn windows_block_is_reported_and_never_overwritten() {
        let sandbox = Sandbox::new();
        let registration = sandbox.registration("blocked");
        for (code, wanted) in [
            (2u32, Status::Registered),
            (3, Status::BlockedByWindows),
            (6, Status::Registered),
            (7, Status::BlockedByWindows),
            (42, Status::WindowsStatusUnknown),
        ] {
            let bytes: Vec<_> = code.to_le_bytes().into_iter().chain([0u8; 8]).collect();
            sandbox.value(true, "blocked", REG_BINARY, &bytes);
            assert_eq!(registration.set_registered(true).unwrap(), wanted);
            assert_eq!(
                registration
                    .registry
                    .read(&registration.registry.approval)
                    .unwrap()
                    .unwrap()
                    .bytes,
                bytes
            );
            registration.set_registered(false).unwrap();
            assert_eq!(
                registration
                    .registry
                    .read(&registration.registry.approval)
                    .unwrap()
                    .unwrap()
                    .bytes,
                bytes
            );
        }
    }
    #[test]
    fn moved_executable_requires_an_explicit_registration_update() {
        let sandbox = Sandbox::new();
        let old = sandbox.registration("moved");
        old.set_registered(true).unwrap();
        let new = Registration::new(
            sandbox.registry("moved"),
            Path::new(r"D:\New folder\YueShot.exe"),
            Path::new(r"C:\Data space\悦"),
        )
        .unwrap();
        assert_eq!(new.status().unwrap(), Status::DifferentCommand);
        assert_eq!(old.status().unwrap(), Status::Registered);
        assert_eq!(new.set_registered(true).unwrap(), Status::Registered);
        assert_eq!(old.status().unwrap(), Status::DifferentCommand);
        assert_eq!(new.set_registered(false).unwrap(), Status::Missing);
    }
    #[test]
    fn invalid_types_strings_and_oversized_values_never_look_enabled() {
        let sandbox = Sandbox::new();
        let registration = sandbox.registration("invalid");
        sandbox.value(false, "invalid", REG_BINARY, b"fake command");
        assert_eq!(registration.status().unwrap(), Status::DifferentCommand);
        for bytes in [
            vec![0],
            vec![65, 0],
            vec![0, 0, 65, 0, 0, 0],
            vec![0; MAX_VALUE_BYTES as usize + 2],
        ] {
            sandbox.value(false, "invalid", REG_SZ, &bytes);
            let status = registration.status();
            assert!(
                status.is_err(),
                "invalid input {bytes:?} produced {status:?}; read back {:?}",
                registration
                    .registry
                    .read(&registration.registry.run)
                    .ok()
                    .flatten()
                    .map(|v| (v.kind, v.bytes))
            );
            assert_eq!(
                registration
                    .registry
                    .read(&registration.registry.run)
                    .ok()
                    .flatten()
                    .map(|v| v.bytes),
                if bytes.len() <= MAX_VALUE_BYTES as usize {
                    Some(bytes)
                } else {
                    None
                }
            );
        }
        assert!(
            Registration::new(
                sandbox.registry("long"),
                Path::new(&format!(r"C:\{}\YueShot.exe", "a".repeat(260))),
                Path::new(r"C:\data")
            )
            .is_err()
        );
        assert!(
            Registration::new(
                sandbox.registry("relative"),
                Path::new("YueShot.exe"),
                Path::new(r"C:\data")
            )
            .is_err()
        );
    }
}
