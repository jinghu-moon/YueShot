//! System WinHTTP supplies proxy handling and certificate verification. Never disable TLS checks.
use super::{Duration, Result};
use std::{
    ffi::c_void,
    io::Write,
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::Networking::WinHttp::*;

struct Handle(*mut c_void);
impl Handle {
    fn new(value: *mut c_void) -> Result<Self> {
        if value.is_null() {
            Err(format!(
                "连接更新服务器失败：{}",
                std::io::Error::last_os_error()
            ))
        } else {
            Ok(Self(value))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn checked(value: i32) -> Result<()> {
    if value == 0 {
        Err(format!("读取更新失败：{}", std::io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

pub(super) fn get(url: &str, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    transfer(url, limit, &mut bytes)?;
    Ok(bytes)
}
pub(super) fn download(url: &str, size: u64, output: &mut impl Write) -> Result<()> {
    let actual = transfer(url, size, output)?;
    if actual != size {
        return Err("更新下载不完整，请重试".into());
    }
    Ok(())
}
fn transfer(address: &str, limit: u64, output: &mut impl Write) -> Result<u64> {
    let url = url::Url::parse(address).map_err(|_| "更新地址无效")?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err("更新连接必须使用 HTTPS".into());
    }
    let host = wide(url.host_str().ok_or("更新地址没有主机名")?);
    let mut resource = url.path().to_owned();
    if let Some(query) = url.query() {
        resource.push('?');
        resource.push_str(query);
    }
    let resource = wide(&resource);
    let start = Instant::now();
    unsafe {
        let session = Handle::new(WinHttpOpen(
            wide("YueShot/updates").as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        ))?;
        checked(WinHttpSetTimeouts(
            session.0, 15_000, 15_000, 15_000, 30_000,
        ))?;
        let connection = Handle::new(WinHttpConnect(
            session.0,
            host.as_ptr(),
            url.port_or_known_default().ok_or("更新端口无效")?,
            0,
        ))?;
        let request = Handle::new(WinHttpOpenRequest(
            connection.0,
            wide("GET").as_ptr(),
            resource.as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        let policy = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
        checked(WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            (&policy as *const u32).cast(),
            4,
        ))?;
        checked(WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0))?;
        checked(WinHttpReceiveResponse(request.0, null_mut()))?;
        let mut status = 0u32;
        let mut length = 4;
        checked(WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast(),
            &mut length,
            null_mut(),
        ))?;
        if status != 200 {
            return Err(format!("更新服务器返回 HTTP {status}，请稍后重试"));
        }
        let mut buffer = [0u8; 64 * 1024];
        let mut total = 0;
        loop {
            if start.elapsed() > Duration::from_secs(300) {
                return Err("更新下载超时，请重试".into());
            }
            let mut read = 0;
            checked(WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            ))?;
            if read == 0 {
                break;
            }
            total += u64::from(read);
            if total > limit {
                return Err("更新响应超过允许大小".into());
            }
            output
                .write_all(&buffer[..read as usize])
                .map_err(|error| format!("写入更新文件失败：{error}"))?;
        }
        Ok(total)
    }
}
