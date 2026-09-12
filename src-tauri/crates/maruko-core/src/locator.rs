use serde::Serialize;
use std::path::{Path, PathBuf};

/// 工具链定位结果（一次性发给前端）。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    pub tools_dir: String,          // 原版 tools 目录（可能为空）
    pub ffmpeg: String,             // 实际采用的 ffmpeg（GPU 编码用）
    pub ffprobe: String,
    pub mediainfo_x64: String,
    pub vsfilter: String,
    pub encoders: Vec<EncoderEntry>, // 视频编码器下拉（软编 + GPU）
    pub audio_encoders: Vec<AudioEnc>,
    pub avs_plugins: Vec<String>,   // tools\avs\plugins 下可用滤镜
    pub gpu_info: Vec<GpuEntry>,
    pub notes: Vec<String>,         // 定位过程中的提示（UI 显示）
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EncoderEntry {
    pub id: String,     // exe 文件名（软编）或 ffmpeg 编码器名（GPU）
    pub label: String,  // 下拉框显示
    pub gpu: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AudioEnc {
    pub id: String,     // neroaac|qaac|fdkaac|lame|flac|ffmpeg_aac
    pub label: String,
    pub available: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GpuEntry {
    pub id: String,
    pub label: String,
    pub api: String,
    pub ok: bool,
}

/// 原版 x264/x265 全部变体（存在才列出）。
pub const SOFT_ENCODERS: &[&str] = &[
    "x265_64-8bit[gcc].exe",
    "x265_64-10bit[gcc].exe",
    "x265_64-12bit[gcc].exe",
    "x265-8bit[gcc].exe",
    "x264_64-8bit.exe",
    "x264_64-10bit.exe",
    "x264_32-8bit.exe",
    "x264_32-10bit.exe",
];

const REG_UNINSTALL: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{7EEB2DD0-264B-4340-B6BE-F05D99066D3C}";

/// 按设置 → 注册表 → 常见路径 的顺序定位原版 tools 目录。
pub fn find_tools_dir(settings_dir: &str) -> Option<PathBuf> {
    if !settings_dir.trim().is_empty() {
        let p = PathBuf::from(settings_dir.trim());
        if p.join("x265_64-8bit[gcc].exe").is_file() || p.join("mkvmerge.exe").is_file() {
            return Some(p);
        }
    }
    // 注册表卸载项 InstallLocation
    if let Ok(hk) = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE).open_subkey(REG_UNINSTALL) {
        if let Ok(loc) = hk.get_value::<String, _>("InstallLocation") {
            let p = PathBuf::from(&loc).join("tools");
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    let candidates = [
        r"E:\Program Files (x86)\MarukoToolbox\tools",
        r"C:\Program Files (x86)\MarukoToolbox\tools",
        r"D:\Program Files (x86)\MarukoToolbox\tools",
        r"E:\Program Files\MarukoToolbox\tools",
        r"C:\Program Files\MarukoToolbox\tools",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// GPU 用 ffmpeg：设置路径 → PATH → 原版 tools（2016 版仅作最后兜底）。
pub fn find_ffmpeg(settings_path: &str, tools_dir: Option<&Path>) -> Option<PathBuf> {
    if !settings_path.trim().is_empty() {
        let p = PathBuf::from(settings_path.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(out) = which("ffmpeg") {
        return Some(out);
    }
    if let Some(t) = tools_dir {
        let p = t.join("ffmpeg.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(';') {
        let p = PathBuf::from(dir).join(format!("{}.exe", name));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub fn find_ffprobe(tools_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = which("ffprobe") {
        return Some(p);
    }
    if let Some(t) = tools_dir {
        let p = t.join("ffprobe.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// 解析完整工具链（除 GPU 探测外不做耗时操作）。
pub fn resolve(settings: &crate::settings::Settings) -> Tools {
    let mut notes = Vec::new();
    let tools_dir = find_tools_dir(&settings.tools_dir);
    let ffmpeg = find_ffmpeg(&settings.ffmpeg_path, tools_dir.as_deref());
    let ffprobe = find_ffprobe(tools_dir.as_deref());
    let mediainfo_x64 = tools_dir
        .as_ref()
        .map(|t| t.join("x64").join("MediaInfo.dll"))
        .filter(|p| p.is_file());
    let vsfilter = tools_dir
        .as_ref()
        .map(|t| t.join("VSFilter.dll"))
        .filter(|p| p.is_file());

    match &tools_dir {
        Some(t) => notes.push(format!("工具链目录：{}", t.display())),
        None => notes.push("未找到原版工具链目录，请在设置页手动指定（软编码不可用）".into()),
    }
    match &ffmpeg {
        Some(f) => notes.push(format!("ffmpeg（GPU 编码用）：{}", f.display())),
        None => notes.push("未找到 ffmpeg，GPU 编码与部分容器操作不可用".into()),
    }

    // 软编码器枚举
    let mut encoders = Vec::new();
    if let Some(t) = &tools_dir {
        for name in SOFT_ENCODERS {
            if t.join(name).is_file() {
                let is_x265 = name.starts_with("x265");
                if is_x265 && !settings.enable_x265 {
                    continue;
                }
                encoders.push(EncoderEntry {
                    id: name.to_string(),
                    label: name.trim_end_matches(".exe").to_string(),
                    gpu: false,
                });
            }
        }
    }
    if encoders.is_empty() {
        notes.push("未发现 x264/x265 软编码器".into());
    }

    // 音频编码器可用性
    let has = |n: &str| tools_dir.as_ref().map(|t| t.join(n).is_file()).unwrap_or(false);
    let audio_encoders = vec![
        AudioEnc { id: "neroaac".into(), label: "NeroAAC".into(), available: has("neroAacEnc.exe") },
        AudioEnc { id: "qaac".into(), label: "qaac (Apple AAC)".into(), available: has("qaac.exe") },
        AudioEnc { id: "fdkaac".into(), label: "fdkaac".into(), available: has("fdkaac.exe") },
        AudioEnc { id: "lame".into(), label: "LAME (MP3)".into(), available: has("lame.exe") },
        AudioEnc { id: "flac".into(), label: "FLAC (无损)".into(), available: has("flac.exe") },
        AudioEnc { id: "ffmpeg_aac".into(), label: "ffmpeg AAC".into(), available: ffmpeg.is_some() },
    ];

    // AVS 插件枚举
    let mut avs_plugins = Vec::new();
    if let Some(t) = &tools_dir {
        let pd = t.join("avs").join("plugins");
        if let Ok(rd) = std::fs::read_dir(&pd) {
            let mut names: Vec<String> = rd
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| {
                    let l = n.to_lowercase();
                    l.ends_with(".dll") || l.ends_with(".avsi")
                })
                .collect();
            names.sort();
            avs_plugins = names;
        }
    }

    Tools {
        tools_dir: tools_dir.map(|t| t.to_string_lossy().to_string()).unwrap_or_default(),
        ffmpeg: ffmpeg.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        ffprobe: ffprobe.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        mediainfo_x64: mediainfo_x64.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        vsfilter: vsfilter.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        encoders,
        audio_encoders,
        avs_plugins,
        gpu_info: Vec::new(), // 由 gpu.rs 探测后填充
        notes,
    }
}

/// 给定工具目录 + 编码器 id，返回 exe 全路径（软编码）。
pub fn encoder_exe(tools_dir: &str, id: &str) -> PathBuf {
    Path::new(tools_dir).join(id)
}
