//! Native downloads inherit macOS network/proxy settings without copying proxy credentials.
use block2::{DynBlock, RcBlock};
use objc2::{
    define_class, msg_send, rc::Retained, runtime::ProtocolObject, AllocAnyThread, AnyThread,
    ClassType,
};
use objc2_foundation::*;
use std::{
    io::Read,
    sync::mpsc,
    time::{Duration, Instant},
};

#[link(name = "CFNetwork", kind = "framework")]
unsafe extern "C" {
    fn CFNetworkCopySystemProxySettings() -> *mut NSDictionary;
}

pub fn allowed_url(url: &str) -> bool {
    url == "https://codex-pets.net" || url.starts_with("https://codex-pets.net/")
}
fn http_result(status: isize) -> Result<(), String> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(format!("网站下载失败：HTTP {status}"))
    }
}
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind=AnyThread]
    struct DownloadPolicy;
    unsafe impl NSObjectProtocol for DownloadPolicy {}
    unsafe impl NSURLSessionDelegate for DownloadPolicy {}
    unsafe impl NSURLSessionTaskDelegate for DownloadPolicy {
        #[unsafe(method(URLSession:task:willPerformHTTPRedirection:newRequest:completionHandler:))]
        unsafe fn redirect(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionTask,
            _response: &NSHTTPURLResponse,
            request: &NSURLRequest,
            completion: &DynBlock<dyn Fn(*mut NSURLRequest)>,
        ) {
            let valid = request
                .URL()
                .and_then(|url| url.absoluteString())
                .is_some_and(|url| allowed_url(&url.to_string()));
            completion.call((if valid {
                request as *const NSURLRequest as *mut NSURLRequest
            } else {
                std::ptr::null_mut()
            },));
        }
    }
);
pub fn download(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    if !allowed_url(url) {
        return Err("素材下载地址必须来自 codex-pets.net".into());
    }
    unsafe {
        let url = NSURL::URLWithString(&NSString::from_str(url)).ok_or("素材链接无效")?;
        let config = NSURLSessionConfiguration::ephemeralSessionConfiguration();
        // Menu-bar apps may have no proxy environment. Explicitly snapshot the current
        // system HTTP/SOCKS/PAC configuration for every download, including later changes.
        // CFDictionary and NSDictionary are toll-free bridged; the Copy result is owned.
        if let Some(settings) = Retained::from_raw(CFNetworkCopySystemProxySettings()) {
            config.setConnectionProxyDictionary(Some(&settings));
        }
        config.setTimeoutIntervalForRequest(15.);
        config.setTimeoutIntervalForResource(40.);
        let policy: Retained<DownloadPolicy> = msg_send![DownloadPolicy::alloc(), init];
        let session = NSURLSession::sessionWithConfiguration_delegate_delegateQueue(
            &config,
            Some(ProtocolObject::from_ref(&*policy)),
            None,
        );
        let (tx, rx) = mpsc::channel();
        let completion = RcBlock::new(
            move |location: *mut NSURL, response: *mut NSURLResponse, error: *mut NSError| {
                let result = objc2::rc::autoreleasepool(|_| -> Result<Vec<u8>, String> {
                    if let Some(error) = error.as_ref() {
                        return Err(format!(
                            "系统网络下载失败：{} ({})：{}",
                            error.domain(),
                            error.code(),
                            error.localizedDescription()
                        ));
                    }
                    let response = response.as_ref().ok_or("网站没有返回响应")?;
                    if !response.isKindOfClass(NSHTTPURLResponse::class()) {
                        return Err("网站没有返回 HTTP 响应".into());
                    }
                    let status: isize = msg_send![response, statusCode];
                    http_result(status)?;
                    if !response
                        .URL()
                        .and_then(|u| u.absoluteString())
                        .is_some_and(|u| allowed_url(&u.to_string()))
                    {
                        return Err("素材下载重定向到了不支持的网站".into());
                    }
                    let path = location
                        .as_ref()
                        .and_then(|url| url.path())
                        .ok_or("下载没有生成有效文件")?;
                    let mut data = vec![];
                    let file = std::fs::File::open(path.to_string())
                        .map_err(|e| format!("读取下载文件失败：{e}"))?;
                    if file.metadata().map_err(|e| e.to_string())?.len() > limit as u64 {
                        return Err("素材文件超过大小限制".into());
                    }
                    file.take(limit as u64 + 1)
                        .read_to_end(&mut data)
                        .map_err(|e| e.to_string())?;
                    if data.len() > limit {
                        return Err("素材文件超过大小限制".into());
                    }
                    Ok(data)
                });
                let _ = tx.send(result);
            },
        );
        let task = session.downloadTaskWithURL_completionHandler(&url, &completion);
        task.resume();
        let start = Instant::now();
        let result = loop {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(result) => break result,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break Err("系统下载任务提前结束".into())
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if task.countOfBytesReceived() > limit as i64
                || task.countOfBytesExpectedToReceive() > limit as i64
            {
                task.cancel();
                break Err("素材文件超过大小限制".into());
            }
            if start.elapsed() > Duration::from_secs(45) {
                task.cancel();
                break Err("系统网络下载超时，请检查 macOS 代理设置或稍后重试".into());
            }
        };
        session.invalidateAndCancel();
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_community_https_origin_is_allowed() {
        assert!(allowed_url("https://codex-pets.net/assets/pets/a.webp"));
        for url in [
            "http://codex-pets.net/a",
            "https://codex-pets.net.evil/a",
            "https://codex-pets.net@evil/a",
            "file:///tmp/a",
            "https://127.0.0.1/a",
        ] {
            assert!(!allowed_url(url));
        }
    }
    #[test]
    fn http_errors_remain_actionable() {
        assert!(http_result(200).is_ok());
        assert!(http_result(404).unwrap_err().contains("404"));
        assert!(http_result(503).unwrap_err().contains("503"));
        assert!(http_result(302).is_err());
    }
}
