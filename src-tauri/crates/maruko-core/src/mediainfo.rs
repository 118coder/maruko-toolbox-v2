use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn to_wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

unsafe fn wide_to_string(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *p.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(p, len);
    String::from_utf16_lossy(slice)
}

/// 用 MediaInfo.dll (x64) 读取标准 Inform 文本（与 MediaInfo GUI 文本视图一致）。
pub fn analyze_with_dll(dll_path: &Path, file: &str) -> Result<String, String> {
    if !dll_path.is_file() {
        return Err(format!("MediaInfo.dll 不存在：{}", dll_path.display()));
    }
    unsafe {
        let lib = libloading::Library::new(dll_path).map_err(|e| format!("加载 MediaInfo.dll 失败：{}", e))?;
        type NewFn = unsafe extern "system" fn() -> *mut c_void;
        type OpenFn = unsafe extern "system" fn(*mut c_void, *const u16) -> usize;
        type InformFn = unsafe extern "system" fn(*mut c_void, usize) -> *const u16;
        type CloseFn = unsafe extern "system" fn(*mut c_void);
        type DeleteFn = unsafe extern "system" fn(*mut c_void);
        let new_fn: libloading::Symbol<NewFn> = lib.get(b"MediaInfo_New").map_err(|e| e.to_string())?;
        let open_fn: libloading::Symbol<OpenFn> = lib.get(b"MediaInfo_Open").map_err(|e| e.to_string())?;
        let inform_fn: libloading::Symbol<InformFn> = lib.get(b"MediaInfo_Inform").map_err(|e| e.to_string())?;
        let close_fn: libloading::Symbol<CloseFn> = lib.get(b"MediaInfo_Close").map_err(|e| e.to_string())?;
        let delete_fn: libloading::Symbol<DeleteFn> = lib.get(b"MediaInfo_Delete").map_err(|e| e.to_string())?;
        let handle = new_fn();
        if handle.is_null() {
            return Err("MediaInfo_New 失败".into());
        }
        let opened = open_fn(handle, to_wide(file).as_ptr());
        let result = if opened == 0 {
            Err(format!("MediaInfo 无法解析此文件：{}", file))
        } else {
            let p = inform_fn(handle, 0);
            Ok(wide_to_string(p))
        };
        close_fn(handle);
        delete_fn(handle);
        result
    }
}

/// ffprobe 回退：合成 MediaInfo 风格的文本。
pub fn analyze_with_ffprobe(ffprobe: &str, file: &str) -> Result<String, String> {
    let mut cmd = Command::new(ffprobe);
    cmd.args(["-v", "error", "-show_format", "-show_streams", "-of", "json", file]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(std::process::Stdio::null());
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("ffprobe 失败：{}", String::from_utf8_lossy(&out.stderr)));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let fmt = &v["format"];
    let mut s = String::new();
    s.push_str("General\n");
    let f = fmt.as_object().cloned().unwrap_or_default();
    s.push_str(&format!("Complete name                    : {}\n", file));
    for (k, label) in [
        ("format_name", "Format"),
        ("duration", "Duration"),
        ("size", "File size"),
        ("bit_rate", "Overall bit rate"),
    ] {
        if let Some(val) = f.get(k).and_then(|x| x.as_str()) {
            s.push_str(&format!("{:<33}: {}\n", label, prettify(k, val)));
        }
    }
    for (i, st) in v["streams"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let ct = st["codec_type"].as_str().unwrap_or("");
        let idx = st["index"].as_i64().unwrap_or(i as i64);
        s.push_str(&format!("\n{}\n", if ct == "video" { "Video" } else { "Audio" }));
        s.push_str(&format!("ID                               : {}{}\n", idx, if ct == "video" { "-1" } else { "-2" }));
        for (k, label) in [
            ("codec_name", "Format"),
            ("width", "Width"),
            ("height", "Height"),
            ("r_frame_rate", "Frame rate"),
            ("sample_rate", "Sampling rate"),
            ("channels", "Channel(s)"),
            ("bit_rate", "Bit rate"),
        ] {
            if let Some(val) = st.get(k).and_then(|x| x.as_str().or(x.as_i64().map(|_| "".into())).or(None)) {
                s.push_str(&format!("{:<33}: {}\n", label, prettify(k, val)));
            } else if let Some(num) = st.get(k).and_then(|x| x.as_i64()) {
                s.push_str(&format!("{:<33}: {}\n", label, num));
            }
        }
    }
    Ok(s)
}

fn prettify(key: &str, val: &str) -> String {
    match key {
        "duration" => format!("{} s", (val.parse::<f64>().unwrap_or(0.0) * 1000.0).round() / 1000.0),
        "size" => match val.parse::<f64>() {
            Ok(n) => format!("{:.1} MiB", n / 1048576.0),
            Err(_) => val.to_string(),
        },
        "bit_rate" => match val.parse::<f64>() {
            Ok(n) => format!("{} b/s", n as i64),
            Err(_) => val.to_string(),
        },
        _ => val.to_string(),
    }
}

/// 统一入口：dll 优先，ffprobe 回退。
pub fn analyze(
    file: &str,
    dll: Option<&str>,
    ffprobe: Option<&str>,
) -> Result<(String, &'static str), String> {
    if let Some(d) = dll {
        if let Ok(t) = analyze_with_dll(Path::new(d), file) {
            return Ok((t, "mediainfo"));
        }
    }
    if let Some(f) = ffprobe {
        if let Ok(t) = analyze_with_ffprobe(f, file) {
            return Ok((t, "ffprobe"));
        }
    }
    Err("MediaInfo.dll 与 ffprobe 均不可用".into())
}

/// 用 ffprobe 读取视频信息：时长、帧率、是否含音频、分辨率、codec（供压制管线用）。
pub struct ProbeInfo {
    pub duration: f64,
    pub fps: String,
    pub has_audio: bool,
    pub width: u32,
    pub height: u32,
    pub vcodec: String,
    pub acodec: String,
}

pub fn probe(ffprobe: &str, file: &str) -> Result<ProbeInfo, String> {
    let mut cmd = Command::new(ffprobe);
    cmd.args(["-v", "error", "-select_streams", "v:0", "-show_entries",
        "stream=width,height,r_frame_rate,codec_name:format=duration", "-of", "json", file]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(std::process::Stdio::null());
    let out = cmd.output().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe 解析失败：{}", e))?;
    let st = &v["streams"][0];
    let mut info = ProbeInfo {
        duration: v["format"]["duration"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
        fps: st["r_frame_rate"].as_str().unwrap_or("").to_string(),
        has_audio: false,
        width: st["width"].as_u64().unwrap_or(0) as u32,
        height: st["height"].as_u64().unwrap_or(0) as u32,
        vcodec: st["codec_name"].as_str().unwrap_or("").to_string(),
        acodec: String::new(),
    };
    // 音频流
    let mut cmd2 = Command::new(ffprobe);
    cmd2.args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=codec_name", "-of", "json", file]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd2.creation_flags(CREATE_NO_WINDOW);
    }
    cmd2.stdin(std::process::Stdio::null());
    if let Ok(out2) = cmd2.output() {
        if let Ok(v2) = serde_json::from_slice::<serde_json::Value>(&out2.stdout) {
            if v2["streams"].as_array().map(|a| !a.is_empty()).unwrap_or(false) {
                info.has_audio = true;
                info.acodec = v2["streams"][0]["codec_name"].as_str().unwrap_or("").to_string();
            }
        }
    }
    Ok(info)
}

pub fn dll_path_from(tools: &crate::locator::Tools) -> Option<PathBuf> {
    if tools.mediainfo_x64.is_empty() {
        None
    } else {
        Some(PathBuf::from(&tools.mediainfo_x64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wide_roundtrip() {
        let w = to_wide("abc");
        assert_eq!(unsafe { wide_to_string(w.as_ptr()) }, "abc");
    }

    #[test]
    fn test_prettify() {
        assert_eq!(prettify("size", "1048576"), "1.0 MiB");
        assert_eq!(prettify("duration", "1.5"), "1.5 s");
    }
}
