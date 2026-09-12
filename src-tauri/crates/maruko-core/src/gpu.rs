use crate::locator::GpuEntry;
use std::path::Path;
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
        let ff = Path::new(ffmpeg);
        let mut entries = Vec::new();
        for c in CANDIDATES {
            let ok = probe(ff, c.id);
            crate::logs::write(&format!("GPU 探测 {} -> {}", c.id, if ok { "OK" } else { "不可用" }));
            entries.push(GpuEntry {
                id: c.id.to_string(),
                label: c.label.to_string(),
                api: c.api.to_string(),
                ok,
            });
        }
        *self.cache.lock().unwrap() = Some((ffmpeg.to_string(), entries.clone()));
        entries
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
