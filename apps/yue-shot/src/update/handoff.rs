//! The same binary runs from an isolated copy, waits for normal parent exit, then installs.
use super::*;
use std::{
    os::windows::{ffi::OsStrExt, process::CommandExt},
    process::{Command, Stdio},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    System::{Registry::*, Threading::*},
};

const NO_WINDOW: u32 = 0x08000000;
const SYNCHRONIZE_PROCESS: u32 = 0x00100000;
const SYNCHRONIZE_EVENT: u32 = 0x00100000;
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn handle(value: HANDLE) -> Result<Handle> {
    if value.is_null() {
        Err(format!(
            "准备更新进程失败：{}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(Handle(value))
    }
}
fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    parent: u32,
    executable: PathBuf,
    data: PathBuf,
    channel: String,
    ready_event: String,
}

fn registered_executable(channel: &str) -> Result<PathBuf> {
    if !matches!(channel, "dev" | "beta" | "stable") {
        return Err("更新渠道无效".into());
    }
    let key = wide(std::ffi::OsStr::new(&format!(
        "Software\\Yue\\com.seeyuer.yueshot.{channel}.x64"
    )));
    let mut buffer = vec![0u16; 32768];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            null(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 {
        return Err("当前程序未登记为此渠道的安装版本，请使用安装包安装后再更新".into());
    }
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .ok_or("安装位置登记无效")?;
    use std::os::windows::ffi::OsStringExt;
    let path = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]));
    path.join("YueShot.exe")
        .canonicalize()
        .map_err(|error| format!("已安装程序不可用：{error}"))
}

pub(crate) struct Prepared(String);
impl Prepared {
    pub(crate) fn commit(self) -> Result<()> {
        let name = wide(std::ffi::OsStr::new(&self.0));
        let event = handle(unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) })?;
        if unsafe { SetEvent(event.0) } == 0 {
            return Err("无法提交更新任务".into());
        }
        Ok(())
    }
}
pub(crate) fn prepare(staged: &Staged, channel: &str, data: &Path) -> Result<Prepared> {
    let attempt = unique_directory(&staged.directory.join("handoffs"))?;
    write_new(&attempt.join("latest.json"), &staged.offer.bytes)?;
    write_new(
        &attempt.join("latest.json.minisig"),
        staged.offer.signature.as_bytes(),
    )?;
    fs::copy(
        staged.directory.join("installer.exe"),
        attempt.join("installer.exe"),
    )
    .map_err(|error| format!("准备更新安装包失败：{error}"))?;
    let executable = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|error| format!("读取当前程序路径失败：{error}"))?;
    if executable != registered_executable(channel)? {
        return Err("当前运行的不是此渠道已登记的安装版本，已拒绝覆盖其他安装".into());
    }
    // Authentication is repeated by the helper. No installer is launched by the GUI process.
    let helper = attempt.join("updater-host.exe");
    let source = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&executable)
        .map_err(|error| format!("读取更新辅助程序失败：{error}"))?;
    let mut source = source;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&helper)
        .map_err(|error| format!("创建更新辅助程序失败：{error}"))?;
    std::io::copy(&mut source, &mut output)
        .and_then(|_| output.sync_all())
        .map_err(|error| format!("复制更新辅助程序失败：{error}"))?;
    drop(output);
    // The app-local VC runtime must follow the executable; never copy a DLL from System32.
    let runtime = executable
        .parent()
        .ok_or("安装位置无效")?
        .join("vcruntime140.dll");
    if runtime.exists() {
        fs::copy(&runtime, attempt.join("vcruntime140.dll"))
            .map_err(|error| format!("复制更新运行库失败：{error}"))?;
    }
    let identity = format!(
        "{}|{}|{:?}",
        attempt.display(),
        std::process::id(),
        SystemTime::now()
    );
    let event_name = format!(
        "Local\\YueShot-Update-{}",
        hex(&Sha256::digest(identity.as_bytes()))
    );
    let event_wide = wide(std::ffi::OsStr::new(&event_name));
    let event = handle(unsafe { CreateEventW(null(), 1, 0, event_wide.as_ptr()) })?;
    let job = Job {
        parent: std::process::id(),
        executable,
        data: data.to_owned(),
        channel: channel.into(),
        ready_event: event_name,
    };
    let job_path = attempt.join("handoff.json");
    write_new(
        &job_path,
        &serde_json::to_vec(&job).map_err(|error| error.to_string())?,
    )?;
    let _child = Command::new(&helper)
        .arg("--apply-update")
        .arg(&job_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(NO_WINDOW)
        .spawn()
        .map_err(|error| format!("启动更新辅助进程失败：{error}"))?;
    if unsafe { WaitForSingleObject(event.0, 30_000) } != WAIT_OBJECT_0 {
        return Err("更新辅助进程未就绪，YueShot 将继续运行".into());
    }
    Ok(Prepared(format!("{}-commit", job.ready_event)))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|error| format!("读取更新任务失败：{error}"))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("更新任务文件过大".into());
    }
    Ok(bytes)
}

/// Runs before GPUI and before the data lock. It may only install after the parent terminates.
pub(crate) fn apply(path: &Path) -> Result<()> {
    let channel = crate::startup::Channel::build()?.name();
    let keys = trusted_keys(channel)?;
    let installation = registered_executable(channel)?;
    apply_authorized(path, channel, &keys, &installation)
}

fn apply_authorized(
    path: &Path,
    channel: &str,
    keys: &[String],
    installation: &Path,
) -> Result<()> {
    if !path.is_absolute() {
        return Err("更新任务路径必须为绝对路径".into());
    }
    let directory = path.parent().ok_or("更新任务路径无效")?;
    let job: Job = serde_json::from_slice(&read_bounded(path, MAX_MANIFEST)?)
        .map_err(|error| format!("更新任务无效：{error}"))?;
    if job.channel != channel || !job.data.is_absolute() || job.parent == std::process::id() {
        return Err("更新任务身份无效".into());
    }
    let current = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|error| error.to_string())?;
    if current
        != directory
            .join("updater-host.exe")
            .canonicalize()
            .map_err(|error| error.to_string())?
    {
        return Err("更新任务必须由独立的辅助程序执行".into());
    }
    let bytes = read_bounded(&directory.join("latest.json"), MAX_MANIFEST)?;
    let signature = String::from_utf8(read_bounded(&directory.join("latest.json.minisig"), 4096)?)
        .map_err(|_| "更新签名无效")?;
    let offer = authenticate(
        &bytes,
        &signature,
        keys,
        channel,
        env!("CARGO_PKG_VERSION"),
        now()?,
    )?;
    if job.executable != installation {
        return Err("安装位置已改变，更新已取消".into());
    }
    let old_hash =
        file_hash(&mut File::open(&job.executable).map_err(|error| error.to_string())?)?.0;
    verify_file(&current, &old_hash, None)?;
    let installer = directory.join("installer.exe");
    // Keep this handle until installation has exited, eliminating the verify/execute replacement gap.
    let _installer_guard = verified_file(
        &installer,
        &offer.manifest.installer_sha256,
        Some(offer.manifest.installer_size),
    )?;
    let parent = handle(unsafe {
        OpenProcess(
            SYNCHRONIZE_PROCESS | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            job.parent,
        )
    })?;
    let mut image = vec![0u16; 32768];
    let mut length = image.len() as u32;
    if unsafe { QueryFullProcessImageNameW(parent.0, 0, image.as_mut_ptr(), &mut length) } == 0 {
        return Err("无法确认更新来源进程".into());
    }
    use std::os::windows::ffi::OsStringExt;
    let parent_path = PathBuf::from(std::ffi::OsString::from_wide(&image[..length as usize]));
    if parent_path
        .canonicalize()
        .map_err(|error| error.to_string())?
        != job.executable
    {
        return Err("更新来源进程与已安装程序不匹配".into());
    }
    let name = wide(std::ffi::OsStr::new(&job.ready_event));
    let ready =
        handle(unsafe { OpenEventW(EVENT_MODIFY_STATE | SYNCHRONIZE_EVENT, 0, name.as_ptr()) })?;
    let commit_name = wide(std::ffi::OsStr::new(&format!("{}-commit", job.ready_event)));
    let commit = handle(unsafe { CreateEventW(null(), 1, 0, commit_name.as_ptr()) })?;
    if unsafe { SetEvent(ready.0) } == 0 {
        return Err("通知更新准备结果失败".into());
    }
    if unsafe { WaitForSingleObject(commit.0, 30_000) } != WAIT_OBJECT_0 {
        return Err("更新未获提交，已取消".into());
    }
    if unsafe { WaitForSingleObject(parent.0, 120_000) } != WAIT_OBJECT_0 {
        return Err("YueShot 未正常退出，更新已取消".into());
    }
    // All application shutdown observers, GPU cleanup and the data lock have now finished.
    let outcome = install(&job, &installer, &offer.manifest);
    let receipt = serde_json::json!({
        "format": 1, "version": offer.version(), "channel": channel,
        "status": if outcome.is_ok() { "installed" } else { "failed" },
        "error": outcome.as_ref().err(),
    });
    let recorded = (|| {
        write_new(
            &directory.join("result.json"),
            &serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
        )?;
        let last = job.data.join("updates");
        fs::create_dir_all(&last).map_err(|error| format!("创建更新记录目录失败：{error}"))?;
        let temporary = last.join(format!("result-{}.partial", std::process::id()));
        write_new(
            &temporary,
            &serde_json::to_vec(&receipt).map_err(|error| error.to_string())?,
        )?;
        fs::rename(&temporary, last.join("last-result.json"))
            .map_err(|error| format!("记录更新结果失败：{error}"))
    })();
    // Record the outcome before restarting, so the next process can display a failure.
    if outcome.is_ok() || verify_file(&job.executable, &old_hash, None).is_ok() {
        Command::new(&job.executable)
            .args(["--show", "--data-dir"])
            .arg(&job.data)
            .creation_flags(NO_WINDOW)
            .spawn()
            .map_err(|error| format!("重启 YueShot 失败：{error}"))?;
    }
    outcome.and(recorded)
}

fn install(job: &Job, installer: &Path, manifest: &Manifest) -> Result<()> {
    let install_directory = job.executable.parent().ok_or("安装位置无效")?;
    let mut destination = std::ffi::OsString::from("/D=");
    destination.push(install_directory);
    let result = Command::new(installer)
        .args(["/S", "/NS"])
        .arg(destination)
        .creation_flags(NO_WINDOW)
        .status();
    match result {
        Ok(status) if status.success() => {
            verify_file(&job.executable, &manifest.executable_sha256, None)
                .map_err(|error| format!("安装后程序校验失败：{error}"))?;
            let output = Command::new(&job.executable)
                .arg("--product-info")
                .creation_flags(NO_WINDOW)
                .output()
                .map_err(|error| format!("检查更新版本失败：{error}"))?;
            let info: serde_json::Value =
                serde_json::from_slice(&output.stdout).map_err(|_| "安装后程序身份无效")?;
            if !output.status.success()
                || info["product"] != "YueShot"
                || info["version"] != manifest.version
                || info["channel"] != manifest.channel
                || info["architecture"] != "x64"
            {
                return Err("安装后程序的产品、版本、渠道或架构不匹配".into());
            }
            Ok(())
        }
        Ok(status) => Err(format!(
            "更新安装失败（退出码 {:?}），请查看更新记录",
            status.code()
        )),
        Err(error) => Err(format!("无法启动更新安装：{error}")),
    }
}

pub(super) fn last_result(data: &Path) -> Option<String> {
    let bytes = read_bounded(&data.join("updates/last-result.json"), MAX_MANIFEST).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    match value["status"].as_str()? {
        "installed" => Some(format!("上次已安装更新 {}", value["version"].as_str()?)),
        "failed" => Some(format!("上次更新失败：{}", value["error"].as_str()?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
