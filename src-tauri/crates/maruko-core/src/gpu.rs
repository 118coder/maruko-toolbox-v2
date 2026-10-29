use crate::locator::GpuEntry;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Candidate {
    id: &'static str,
    label: &'static str,
    api: &'static str,
}

/// 逐 API 逐编码器的候选清单。
const CANDIDATES: &[Candidate] = &[
    Candidate { id: "h264_nvenc", label: "H.264 NVENC (GPU)", api: "NVENC" },
    Candidate { id: "hevc_nvenc", label: "HEVC NVENC (GPU)", api: "NVENC" },
    Candidate { id: "av1_nvenc", label: "AV1 NVENC (GPU)", api: "NVENC" },
    Candidate { id: "h264_qsv", label: "H.264 QuickSync (GPU)", api: "QSV" },
    Candidate { id: "hevc_qsv", label: "HEVC QuickSync (GPU)", api: "QSV" },
    Candidate { id: "av1_qsv", label: "AV1 QuickSync (GPU)", api: "QSV" },
    Candidate { id: "h264_amf", label: "H.264 AMF (GPU)", api: "AMF" },
    Candidate { id: "hevc_amf", label: "HEVC AMF (GPU)", api: "AMF" },
    Candidate { id: "av1_amf", label: "AV1 AMF (GPU)", api: "AMF" },
];

/// 对单个硬件编码器做 0.3s 试编码，exit 0 = 可用。
/// 编码器列表虽在 ffmpeg -encoders 里，但驱动缺失/不支持时照样报错，必须实测。
fn probe(ffmpeg: &Path, id: &str) -> bool {
    let mut cmd = Command::new(ffmpeg);
    cmd.args([
        "-hide_banner", "-v", "error",
        "-f", "lavfi", "-i", "color=c=black:s=256x256:d=0.3:r=25",
        "-c:v", id, "-f", "null", "-",
    ]);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.spawn() {
        Ok(mut child) => {
            // 最多等 8 秒，超时杀掉
            let deadline = std::time::Instant::now() + Duration::from_secs(8);
            loop {
                match child.try_wait() {
                    Ok(Some(st)) => return st.success(),
                    Ok(None) => {
                        if std::time::Instant::now() > deadline {
                            let _ = child.kill();
                            return false;
                        }
                        std::thread::sleep(Duration::from_millis(120));
                    }
                    Err(_) => return false,
                }
            }
        }
        Err(_) => false,
    }
}

/// 带缓存的探测器（ffmpeg 路径变化时失效）。
/// 两级缓存：内存（进程内）→ 磁盘（跨启动，按 ffmpeg 路径 + mtime 校验）。
pub struct GpuDetector {
    cache: Mutex<Option<(String, Vec<GpuEntry>)>>,
}

impl GpuDetector {
    pub fn new() -> Self {
        GpuDetector { cache: Mutex::new(None) }
    }

    pub fn detect(&self, ffmpeg: &str) -> Vec<GpuEntry> {
        if ffmpeg.is_empty() {
            return Vec::new();
        }
        if let Some((path, entries)) = self.cache.lock().unwrap().clone() {
            if path == ffmpeg {
                return entries;
            }
        }
        let entries = match read_disk_cache(ffmpeg) {
            Some(cached) => {
                crate::logs::write("GPU 探测命中磁盘缓存（ffmpeg 未变化，跳过试编码）");
                cached
            }
            None => {
                let e = detect_parallel(Path::new(ffmpeg));
                write_disk_cache(ffmpeg, &e);
                e
            }
        };
        *self.cache.lock().unwrap() = Some((ffmpeg.to_string(), entries.clone()));
        entries
    }
}

/// 并发探测全部候选（9 个 ffmpeg 进程同时起）。
/// 串行逐个试编码约 4.5s，是启动慢的主因；并发后 ≈ 最慢一个的耗时（约 0.5s）。
fn detect_parallel(ff: &Path) -> Vec<GpuEntry> {
    let handles: Vec<_> = CANDIDATES
        .iter()
        .map(|c| {
            let ff = ff.to_path_buf();
            std::thread::spawn(move || {
                let ok = probe(&ff, c.id);
                (c.id.to_string(), c.label.to_string(), c.api.to_string(), ok)
            })
        })
        .collect();
    let mut entries = Vec::new();
    for h in handles {
        if let Ok((id, label, api, ok)) = h.join() {
            crate::logs::write(&format!("GPU 探测 {} -> {}", id, if ok { "OK" } else { "不可用" }));
            entries.push(GpuEntry { id, label, api, ok });
        }
    }
    entries
}

#[derive(Serialize, Deserialize)]
struct GpuCacheFile {
    ffmpeg: String,
    mtime: u64,
    entries: Vec<GpuEntry>,
}

fn appdata_cache_path() -> Option<PathBuf> {
    let p = std::env::var("APPDATA").ok().map(|d| PathBuf::from(d).join("MarukoV2").join("gpu_cache.json"))?;
    let _ = std::fs::create_dir_all(p.parent()?);
    Some(p)
}

fn exe_cache_path() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("gpu_cache.json")))
}

/// mtime 粒度 2 秒（FAT32 为 2s，NTFS 为 100ns；用户换 ffmpeg 是低频事件，2s 足够）
fn ffmpeg_mtime(ffmpeg: &str) -> Option<u64> {
    let m = std::fs::metadata(ffmpeg).ok()?.modified().ok()?;
    m.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() / 2 * 2)
}

fn read_disk_cache(ffmpeg: &str) -> Option<Vec<GpuEntry>> {
    let mut txt = None;
    if let Some(p) = exe_cache_path() {
        txt = std::fs::read_to_string(p).ok();
    }
    if txt.is_none() {
        txt = appdata_cache_path().and_then(|p| std::fs::read_to_string(p).ok());
    }
    let v: GpuCacheFile = serde_json::from_str(txt.as_deref()?).ok()?;
    if v.ffmpeg != ffmpeg || Some(v.mtime) != ffmpeg_mtime(ffmpeg) {
        return None;
    }
    Some(v.entries)
}

fn write_disk_cache(ffmpeg: &str, entries: &[GpuEntry]) {
    let mtime = match ffmpeg_mtime(ffmpeg) {
        Some(m) => m,
        None => return,
    };
    let v = match serde_json::to_string(&GpuCacheFile { ffmpeg: ffmpeg.to_string(), mtime, entries: entries.to_vec() }) {
        Ok(t) => t,
        Err(_) => return,
    };
    // exe 旁优先（便携习惯）；失败（如 Program Files 权限）回退 %APPDATA%
    if let Some(p) = exe_cache_path() {
        if std::fs::write(&p, &v).is_ok() {
            return;
        }
    }
    if let Some(p) = appdata_cache_path() {
        let _ = std::fs::write(p, &v);
    }
}

/// 硬件编码器的码控参数（纯函数，供 cli::video 使用）。
/// 追加到 -c:v <id> 之后；crf_mode: true=CRF(质量优先) false=码率模式。
pub fn rate_ctl_args(encoder: &str, crf: f64, bitrate_kbps: u32) -> Vec<String> {
    let id = encoder.to_lowercase();
    if id.contains("nvenc") {
        if crf_mode_is_quality(encoder, crf, bitrate_kbps > 0) {
            // NVENC 的 CQ 模式
            vec!["-rc".into(), "vbr".into(), "-cq".into(), crf.to_string(), "-b:v".into(), "0".into()]
        } else {
            vec!["-rc".into(), "vbr".into(), "-b:v".into(), format!("{}k", bitrate_kbps)]
        }
    } else if id.contains("qsv") {
        if bitrate_kbps == 0 {
            vec!["-global_quality".into(), format!("{}", crf.round() as i64)]
        } else {
            vec!["-rc".into(), "vbr".into(), "-b:v".into(), format!("{}k", bitrate_kbps)]
        }
    } else if id.contains("amf") {
        if bitrate_kbps == 0 {
            vec![
                "-rc".into(), "cqp".into(),
                "-qp_i".into(), format!("{}", crf.round() as i64),
                "-qp_p".into(), format!("{}", crf.round() as i64),
                "-qp_b".into(), format!("{}", crf.round() as i64 + 2),
            ]
        } else {
            vec!["-rc".into(), "vbr".into(), "-b:v".into(), format!("{}k", bitrate_kbps)]
        }
    } else {
        Vec::new()
    }
}

fn crf_mode_is_quality(_encoder: &str, _crf: f64, bitrate_set: bool) -> bool {
    !bitrate_set
}
