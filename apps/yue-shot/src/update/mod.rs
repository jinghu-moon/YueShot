//! Product-private authenticated updates. No network access until the user requests it.
mod handoff;
mod http;
mod view;
pub(super) use handoff::apply;
pub(super) use handoff::prepare;
pub(super) use view::{InstallUpdate, UpdateView};

use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const REPOSITORY: &str = "https://github.com/jinghu-moon/YueShot/releases/download/";
const MAX_MANIFEST: u64 = 32 * 1024;
const MAX_INSTALLER: u64 = 512 * 1024 * 1024;
const KEYS: &str = include_str!("../../packaging/update-keys.json");
pub(super) type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    product: String,
    channel: String,
    architecture: String,
    version: String,
    expires_unix: u64,
    installer_url: String,
    installer_size: u64,
    installer_sha256: String,
    executable_sha256: String,
}

#[derive(Clone, Debug)]
pub(super) struct Offer {
    manifest: Manifest,
    bytes: Vec<u8>,
    signature: String,
}
impl Offer {
    pub(super) fn version(&self) -> &str {
        &self.manifest.version
    }
}

#[derive(Clone, Debug)]
pub(super) struct Staged {
    directory: PathBuf,
    offer: Offer,
}

fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| "系统时间无效，无法检查更新".into())
}

fn trusted_keys(channel: &str) -> Result<Vec<String>> {
    let keys: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(KEYS).map_err(|error| format!("更新公钥配置无效：{error}"))?;
    let keys = keys.get(channel).cloned().unwrap_or_default();
    if keys.is_empty() {
        return Err("此渠道尚未配置更新公钥，暂不能在线更新".into());
    }
    for key in &keys {
        PublicKey::from_base64(key).map_err(|error| format!("更新公钥无效：{error}"))?;
    }
    Ok(keys)
}

fn authenticate(
    bytes: &[u8],
    signature: &str,
    keys: &[String],
    channel: &str,
    current: &str,
    time: u64,
) -> Result<Offer> {
    if bytes.len() as u64 > MAX_MANIFEST || signature.len() > 4096 {
        return Err("更新清单过大".into());
    }
    let signature_value = Signature::decode(signature).map_err(|_| "更新签名格式无效")?;
    if !keys.iter().any(|key| {
        PublicKey::from_base64(key)
            .and_then(|key| key.verify(bytes, &signature_value, false))
            .is_ok()
    }) {
        return Err("更新签名验证失败，已拒绝此更新".into());
    }
    // Parse only authenticated bytes. Identity, target and version are all signed.
    let manifest: Manifest =
        serde_json::from_slice(bytes).map_err(|error| format!("更新清单无效：{error}"))?;
    if manifest.format != 1
        || manifest.product != "YueShot"
        || manifest.channel != channel
        || manifest.architecture != "x64"
    {
        return Err("更新产品、渠道或架构不匹配".into());
    }
    let version = Version::parse(&manifest.version).map_err(|_| "更新版本无效")?;
    let current = Version::parse(current).map_err(|_| "当前版本无效")?;
    if !version.build.is_empty() || (channel == "stable" && !version.pre.is_empty()) {
        return Err("更新版本不适用于当前渠道".into());
    }
    if version <= current {
        return Err("更新版本没有高于当前版本，已拒绝降级或重复安装".into());
    }
    if manifest.expires_unix <= time {
        return Err("更新清单已过期，请稍后重试".into());
    }
    if manifest.installer_size == 0 || manifest.installer_size > MAX_INSTALLER {
        return Err("更新安装包大小无效".into());
    }
    if !is_hash(&manifest.installer_sha256) || !is_hash(&manifest.executable_sha256) {
        return Err("更新文件校验值无效".into());
    }
    let suffix = manifest
        .installer_url
        .strip_prefix(REPOSITORY)
        .ok_or("更新安装包不来自 YueShot 发布仓库")?;
    if suffix.split('/').count() != 2
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._/".contains(&byte))
        || !suffix.ends_with(".exe")
        || suffix.contains("..")
    {
        return Err("更新安装包地址无效".into());
    }
    Ok(Offer {
        manifest,
        bytes: bytes.to_vec(),
        signature: signature.into(),
    })
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[(byte >> 4) as usize]));
        text.push(char::from(DIGITS[(byte & 15) as usize]));
    }
    text
}

pub(super) fn check(channel: &str) -> Result<Option<Offer>> {
    let keys = trusted_keys(channel)?;
    check_with(channel, &keys, env!("CARGO_PKG_VERSION"), now()?, http::get)
}

fn check_with(
    channel: &str,
    keys: &[String],
    current: &str,
    time: u64,
    mut fetch: impl FnMut(&str, u64) -> Result<Vec<u8>>,
) -> Result<Option<Offer>> {
    let base = format!("{REPOSITORY}yueshot-updater-{channel}/latest.json");
    let bytes = fetch(&base, MAX_MANIFEST)?;
    let signature = String::from_utf8(fetch(&format!("{base}.minisig"), 4096)?)
        .map_err(|_| "更新签名编码无效")?;
    let offer = authenticate(&bytes, &signature, keys, channel, "0.0.0", time)?;
    let current = Version::parse(current).map_err(|_| "当前版本无效")?;
    let version = Version::parse(offer.version()).map_err(|_| "更新版本无效")?;
    Ok((version > current).then_some(offer))
}

fn unique_directory(root: &Path) -> Result<PathBuf> {
    fs::create_dir_all(root).map_err(|error| format!("创建更新目录失败：{error}"))?;
    for counter in 0..100 {
        let path = root.join(format!("{}-{}-{counter}", now()?, std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("创建更新暂存目录失败：{error}")),
        }
    }
    Err("无法分配更新暂存目录".into())
}

pub(super) fn download(offer: Offer, data: &Path) -> Result<Staged> {
    stage_with(offer, data, http::download)
}

fn stage_with(
    offer: Offer,
    data: &Path,
    fetch: impl FnOnce(&str, u64, &mut File) -> Result<()>,
) -> Result<Staged> {
    let directory = unique_directory(&data.join("updates"))?;
    let result = (|| {
        write_new(&directory.join("latest.json"), &offer.bytes)?;
        write_new(
            &directory.join("latest.json.minisig"),
            offer.signature.as_bytes(),
        )?;
        let partial = directory.join("installer.partial");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|error| format!("创建更新文件失败：{error}"))?;
        fetch(
            &offer.manifest.installer_url,
            offer.manifest.installer_size,
            &mut file,
        )?;
        file.sync_all()
            .map_err(|error| format!("写入更新失败：{error}"))?;
        drop(file);
        verify_file(
            &partial,
            &offer.manifest.installer_sha256,
            Some(offer.manifest.installer_size),
        )?;
        fs::rename(partial, directory.join("installer.exe"))
            .map_err(|error| format!("提交更新文件失败：{error}"))?;
        Ok(Staged {
            directory: directory.clone(),
            offer,
        })
    })();
    if result.is_err() {
        // This directory was created exclusively for this request; no user data lives in it.
        let _ = fs::remove_dir_all(&directory);
    }
    result
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("创建更新文件失败：{error}"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("写入更新文件失败：{error}"))
}

fn file_hash(file: &mut File) -> Result<(String, u64)> {
    let mut digest = Sha256::new();
    let mut count = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("读取更新文件失败：{error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        count += read as u64;
    }
    Ok((hex(&digest.finalize()), count))
}

/// A read-only handle denying writes/deletes remains alive through installer completion.
fn verified_file(path: &Path, hash: &str, size: Option<u64>) -> Result<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .map_err(|error| format!("打开更新文件失败：{error}"))?;
    let (actual, length) = file_hash(&mut file)?;
    if actual != hash || size.is_some_and(|expected| length != expected) {
        return Err("更新文件校验失败，已拒绝安装".into());
    }
    Ok(file)
}
fn verify_file(path: &Path, hash: &str, size: Option<u64>) -> Result<()> {
    verified_file(path, hash, size).map(drop)
}

#[cfg(test)]
mod tests;
