use maruko_core::cli::{self, audio::AudioJob, common as clicommon, video::VideoJob};
use maruko_core::gpu::GpuDetector;
use maruko_core::jobs::{cancel_shutdown, Emitter, Ev, JobManager, RunOpts};
use maruko_core::locator::{self, Tools};
use maruko_core::mediainfo;
use maruko_core::settings::{self, Settings};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter as _, Manager, State, Theme};

pub const VERSION: &str = "1.1.0";
/// 默认更新接口：本仓库 GitHub Releases（设置页可自定义，兼容旧 {version,url} 协议）
pub const DEFAULT_UPDATE_API: &str = "https://api.github.com/repos/118coder/maruko-toolbox-v2/releases/latest";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct AppState {
    pub settings: Mutex<Settings>,
    /// 工具链快照：设置页改目录/ffmpeg 后由 save_settings 同步重解析
    pub tools: Mutex<Tools>,
    pub gpu: Arc<GpuDetector>,
    pub jobs: JobManager,
}

impl AppState {
    pub fn new(emitter: Arc<Emitter>) -> Self {
        let s = settings::load();
        let tools = locator::resolve(&s);
        AppState {
            settings: Mutex::new(s),
            tools: Mutex::new(tools),
            gpu: Arc::new(GpuDetector::new()),
            jobs: JobManager::new(emitter),
        }
    }

    pub fn tools(&self) -> Tools {
        self.tools.lock().unwrap().clone()
    }
}

fn temp_dir() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
        .map(|d| d.join("temp"))
        .filter(|p| std::fs::create_dir_all(p).is_ok());
    base.unwrap_or_else(|| {
        let p = std::env::temp_dir().join("MarukoV2");
        let _ = std::fs::create_dir_all(&p);
        p
    })
}

fn unique_temp(base: &PathBuf) -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 + d.as_secs())
        .unwrap_or(0);
    format!("{}\\job-{}-{}", base.to_string_lossy(), std::process::id(), n % 100000)
}

fn run_opts(s: &Settings) -> RunOpts {
    RunOpts { priority: s.x264_priority.clone(), delete_temp: s.delete_temp }
}

fn cmd_nowin(exe: &str) -> std::process::Command {
    let mut c = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = &mut c;
    c
}

// ---------- 设置 / 工具 ----------

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, app: AppHandle, new_settings: Settings) -> Result<Settings, String> {
    let tools_changed;
    {
        let mut st = state.settings.lock().unwrap();
        tools_changed = st.tools_dir != new_settings.tools_dir
            || st.ffmpeg_path != new_settings.ffmpeg_path
            || st.enable_x265 != new_settings.enable_x265;
        *st = new_settings;
        settings::save(&st)?;
    }
    if tools_changed {
        // 同步重解析轻量部分（毫秒级），GPU 等深度探测交给后台
        let s = state.settings.lock().unwrap().clone();
        *state.tools.lock().unwrap() = locator::resolve(&s);
        spawn_tools_refresh(app.clone(), &state);
    }
    if let Some(w) = app.get_webview_window("main") {
        let dark = state.settings.lock().unwrap().dark;
        let _ = w.set_theme(Some(if dark { Theme::Dark } else { Theme::Light }));
    }
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
pub fn reset_settings(state: State<AppState>) -> Settings {
    let mut st = state.settings.lock().unwrap();
    *st = Settings::default();
    let _ = settings::save(&st);
    st.clone()
}

#[tauri::command]
pub fn get_tools_info(app: AppHandle, state: State<AppState>) -> Tools {
    // 立即返回轻量快照（纯文件系统检查，毫秒级），主窗不等待任何子进程。
    // GPU 试编码 / 专业编码器清单 / 特性检测由后台完成并经 tools://ready 推送。
    let tools = state.tools();
    spawn_tools_refresh(app, &state);
    tools
}

/// 深度探测去重标志
static TOOLS_REFRESHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 后台深度探测：GPU 试编码（并行 + 磁盘缓存）→ ffmpeg 专业编码器清单 → libx264/subtitles 特性。
/// 完成后更新主快照并发 tools://ready 事件；不阻塞任何命令。
fn spawn_tools_refresh(app: AppHandle, state: &State<AppState>) {
    use std::sync::atomic::Ordering;
    if TOOLS_REFRESHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let gpu = state.gpu.clone();
    let settings = state.settings.lock().unwrap().clone();
    std::thread::spawn(move || {
        let mut tools = locator::resolve(&settings);
        tools.gpu_info = gpu.detect(&tools.ffmpeg);
        // Voukoder 风格专业编码器：解析 ffmpeg -encoders 过滤精选清单
        if !tools.ffmpeg.is_empty() {
            if let Ok(o) = cmd_nowin(&tools.ffmpeg).args(["-hide_banner", "-encoders"]).output() {
                let txt = String::from_utf8_lossy(&o.stdout);
                let curated: &[(&str, &str)] = &[
                    ("libx265", "现代版 x265（Voukoder 风格预设）"),
                    ("libx264", "现代版 x264（Voukoder 风格预设）"),
                    ("prores_ks", "ProRes 422（专业剪辑）"),
                    ("cfhd", "CineForm（专业剪辑）"),
                    ("ffv1", "FFV1（无损归档）"),
                    ("utvideo", "UtVideo（无损）"),
                    ("libvpx-vp9", "VP9（webm）"),
                ];
                for (id, label) in curated {
                    if txt.contains(&format!(" {}", id)) {
                        tools.ffmpeg_encoders.push(locator::EncoderEntry {
                            id: id.to_string(),
                            label: label.to_string(),
                            gpu: false,
                        });
                    }
                }
            }
        }
        let features = detect_features(&tools.ffmpeg);
        // 更新主快照后推送
        if let Some(st) = app.try_state::<AppState>() {
            *st.tools.lock().unwrap() = tools.clone();
        }
        let _ = app.emit("tools://ready", ToolsReadyPayload { tools, features });
        maruko_core::logs::write("工具链深度探测完成（tools://ready 已推送）");
        TOOLS_REFRESHING.store(false, Ordering::SeqCst);
    });
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ToolsReadyPayload {
    tools: Tools,
    features: LibCheck,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LibCheck {
    pub libx264: bool,
    pub subtitles_filter: bool,
}

/// libx264 探测结果缓存（ffmpeg 路径为键）：一图流/旋转等命令不再反复 spawn -encoders
static LIBX264_CACHE: OnceLock<Mutex<Option<(String, bool)>>> = OnceLock::new();

fn cached_libx264(ffmpeg: &str) -> bool {
    if ffmpeg.is_empty() {
        return false;
    }
    let cache = LIBX264_CACHE.get_or_init(|| Mutex::new(None));
    {
        let g = cache.lock().unwrap();
        if let Some((path, ok)) = g.as_ref() {
            if path == ffmpeg {
                return *ok;
            }
        }
    }
    let ok = cmd_nowin(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.contains(" libx264 ")))
        .unwrap_or(false);
    *cache.lock().unwrap() = Some((ffmpeg.to_string(), ok));
    ok
}

fn detect_features(ffmpeg: &str) -> LibCheck {
    if ffmpeg.is_empty() {
        return LibCheck { libx264: false, subtitles_filter: false };
    }
    let libx264 = cached_libx264(ffmpeg);
    let mut subtitles_filter = false;
    if let Ok(o) = cmd_nowin(ffmpeg).args(["-hide_banner", "-filters"]).output() {
        subtitles_filter = String::from_utf8_lossy(&o.stdout).lines().any(|l| l.contains(" subtitles "));
    }
    LibCheck { libx264, subtitles_filter }
}

#[tauri::command]
pub fn check_ffmpeg_features(state: State<AppState>) -> LibCheck {
    detect_features(&state.tools().ffmpeg)
}

// ---------- 对话框 / 打开 ----------

#[derive(serde::Deserialize, Clone)]
pub struct DialogFilter {
    pub name: String,
    pub exts: Vec<String>,
}

#[tauri::command]
pub async fn pick_file(app: AppHandle, title: String, filters: Vec<DialogFilter>, save: Option<bool>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    let mut builder = app.dialog().file().set_title(&title);
    if !filters.is_empty() {
        let name = filters.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join("/");
        let exts: Vec<&str> = filters.iter().flat_map(|f| f.exts.iter().map(|s| s.as_str())).collect();
        builder = builder.add_filter(&name, &exts);
    }
    if save.unwrap_or(false) {
        builder.save_file(move |p| {
            let _ = tx.send(p.map(|v| v.simplified().to_string()));
        });
    } else {
        builder.pick_file(move |p| {
            let _ = tx.send(p.map(|v| v.simplified().to_string()));
        });
    }
    rx.recv().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle, title: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    app.dialog()
        .file()
        .set_title(&title)
        .pick_folder(move |p| {
            let _ = tx.send(p.map(|v| v.simplified().to_string()));
        });
    rx.recv().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn file_exists(path: String) -> bool {
    PathBuf::from(&path).exists()
}

#[tauri::command]
pub fn open_path(path: String, reveal: Option<bool>) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("路径不存在：{}", path));
    }
    let mut c = std::process::Command::new("explorer");
    if reveal.unwrap_or(false) {
        c.arg(format!("/select,{}", path));
    } else {
        c.arg(&path);
    }
    c.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// 用系统默认浏览器打开 URL（帮助页开源地址等；仅接受 http/https）。
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!("仅支持 http/https 链接：{}", url));
    }
    std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- MediaInfo ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfoResult {
    pub text: String,
    pub source: String,
    pub error: String,
}

#[tauri::command]
pub fn mediainfo_analyze(state: State<AppState>, path: String) -> MediaInfoResult {
    if !PathBuf::from(&path).is_file() {
        return MediaInfoResult { text: String::new(), source: "none".into(), error: format!("文件不存在：{}", path) };
    }
    let tools = state.tools();
    match mediainfo::analyze(
        &path,
        if tools.mediainfo_x64.is_empty() { None } else { Some(&tools.mediainfo_x64) },
        if tools.ffprobe.is_empty() { None } else { Some(&tools.ffprobe) },
    ) {
        Ok((text, src)) => MediaInfoResult { text, source: src.to_string(), error: String::new() },
        Err(e) => MediaInfoResult { text: String::new(), source: "none".into(), error: e },
    }
}

// ---------- 视频压制 ----------

fn probe_video(state: &AppState, input: &str) -> Result<mediainfo::ProbeInfo, String> {
    let ff = &state.tools().ffprobe;
    if ff.is_empty() {
        return Ok(mediainfo::ProbeInfo {
            duration: 0.0,
            fps: String::new(),
            has_audio: true,
            width: 0,
            height: 0,
            vcodec: String::new(),
            acodec: String::new(),
        });
    }
    mediainfo::probe(ff, input)
}

fn avs_dir(tools: &Tools) -> String {
    if tools.tools_dir.is_empty() {
        String::new()
    } else {
        PathBuf::from(&tools.tools_dir).join("avs").to_string_lossy().to_string()
    }
}

fn opt_str(s: &str) -> Option<&str> {
    if s.is_empty() { None } else { Some(s) }
}

/// 批量内嵌字幕：同目录同名 + 语言后缀 + .ass/.srt/.ssa。
pub fn resolve_batch_subtitle(video: &str, suffix: &str) -> Option<String> {
    if suffix.is_empty() || suffix == "none" {
        return None;
    }
    let p = PathBuf::from(video);
    let stem = p.file_stem()?.to_string_lossy().to_string();
    let dir = p.parent()?;
    for ext in ["ass", "srt", "ssa"] {
        let cand = dir.join(format!("{}.{}.{}", stem, suffix, ext));
        if cand.is_file() {
            return Some(cand.to_string_lossy().to_string());
        }
    }
    None
}

/// 组装 VideoCtx：探测源、定输出、写字幕、生成/写入 AVS。
fn prepare_video_job(
    state: &AppState,
    job: &mut VideoJob,
    subtitle_override: Option<String>,
) -> Result<(cli::video::VideoCtx, bool, f64), String> {
    let settings = state.settings.lock().unwrap().clone();
    let tools = state.tools();
    let info = probe_video(state, &job.input)?;

    // 输出命名：空 → 源目录/名+编码器标签+容器（测试.mp4 → 测试x264.mp4）；
    // 非空 → 防覆盖冲突（存在则 _2/_3）
    if job.output.is_empty() {
        job.output = cli::tagged_output(
            &job.input,
            &job.container,
            cli::video::encoder_tag(&job.encoder),
        );
    } else {
        job.output = cli::deconflict(&job.output);
    }
    if let Some(suffix) = subtitle_override {
        job.subtitle = resolve_batch_subtitle(&job.input, &suffix).unwrap_or_default();
    }
    let gpu = cli::video::is_ffmpeg_encoder(&job.encoder);
    if !gpu && tools.tools_dir.is_empty() {
        return Err("未找到工具链目录，软编码不可用（设置页指定工具链目录）".into());
    }

    let temp_base = unique_temp(&temp_dir());
    let audio_none = job.audio_mode == "none" || (!info.has_audio && job.audio_mode != "encode");
    let mut audio_temp: Option<String> = None;
    if !gpu && !audio_none {
        let ext = if job.audio_mode == "copy" {
            if job.container == "mkv" {
                "mka".to_string()
            } else {
                match info.acodec.as_str() {
                    "aac" => "m4a".to_string(),
                    "mp3" => "mp3".to_string(),
                    other => {
                        if other.is_empty() { "m4a".to_string() } else { "mka".to_string() }
                    }
                }
            }
        } else {
            cli::audio::output_ext(&job.audio_encoder).to_string()
        };
        audio_temp = Some(format!("{}.{}", temp_base, ext));
    }

    let mut avs_path = String::new();
    if !gpu {
        let script = if job.avs_script.trim().is_empty() {
            let demuxer = match job.demuxer.as_str() {
                "ffms2" => maruko_core::avs::Demuxer::Ffms2,
                _ => maruko_core::avs::Demuxer::Lsmash,
            };
            let filters = job.avs_filters.clone().unwrap_or(maruko_core::avs::AvsFilters {
                resize: if !job.keep_res && job.width > 0 && job.height > 0 {
                    Some((job.width, job.height))
                } else {
                    None
                },
                ..Default::default()
            });
            maruko_core::avs::build_script(
                &job.input,
                opt_str(&job.subtitle),
                demuxer,
                &filters,
                &tools.tools_dir,
                &avs_dir(&tools),
                "",
                job.start_frame,
                job.frames,
                info.width,
                info.height,
            )
        } else {
            job.avs_script.clone()
        };
        avs_path = format!("{}.avs", temp_base);
        maruko_core::avs::write_script_ansi(&avs_path, &script).map_err(|e| format!("写入 AVS 脚本失败：{}", e))?;
    }

    let ctx = cli::video::VideoCtx {
        tools_dir: tools.tools_dir.clone(),
        ffmpeg: tools.ffmpeg.clone(),
        avs_path,
        temp_base,
        audio_temp,
        src_fps: info.fps.clone(),
        priority: settings.x264_priority.clone(),
        threads: settings.x264_threads,
        extra: settings.x264_extra.clone(),
        // NVENC 硬解直通准入：NVDEC 可解 h264/hevc/av1 主流形态；罕见不支持的源
        // 由 encode 阶段的 Plan.fallback 自动回退软解，无需预检
        hwaccel: gpu && cli::video::hwaccel_eligible(&job.encoder, &info.vcodec, job.subtitle.is_empty()),
    };
    Ok((ctx, info.has_audio, info.duration))
}

/// GPU 任务：硬解主计划 + 同参软解回退计划（NVDEC 不支持的源自动重试）。
fn gpu_plan_with_fallback(job: &VideoJob, ctx: &cli::video::VideoCtx, has_audio: bool, duration: f64) -> Result<cli::Plan, String> {
    let mut plan = cli::video::build_gpu_plan(job, ctx, has_audio, duration)?;
    if ctx.hwaccel {
        let mut sw_ctx = ctx.clone();
        sw_ctx.hwaccel = false;
        let mut fb = cli::video::build_gpu_plan(job, &sw_ctx, has_audio, duration)?;
        fb.title = format!("{}（软解回退）", fb.title);
        plan.fallback = Some(Box::new(fb));
    }
    Ok(plan)
}

fn file_name(p: &str) -> String {
    PathBuf::from(p)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string())
}

#[tauri::command]
pub fn encode_video(state: State<AppState>, mut job: VideoJob) -> Result<u32, String> {
    let gpu = cli::video::is_ffmpeg_encoder(&job.encoder);
    let (ctx, has_audio, duration) = prepare_video_job(&state, &mut job, None)?;
    let plan = if gpu {
        gpu_plan_with_fallback(&job, &ctx, has_audio, duration)?
    } else {
        cli::video::build_soft_plan(&job, &ctx)?
    };
    let opts = run_opts(&state.settings.lock().unwrap());
    maruko_core::logs::write(&format!("提交视频任务（encoder={} mode={} input={}）", job.encoder, job.mode, job.input));
    Ok(state.jobs.submit(plan, opts))
}

#[tauri::command]
pub fn encode_video_batch(state: State<AppState>, mut jobs: Vec<VideoJob>, subSuffix: String) -> Result<Vec<u32>, String> {
    let opts = run_opts(&state.settings.lock().unwrap());
    let mut ids = Vec::new();
    for job in jobs.iter_mut() {
        let gpu = cli::video::is_ffmpeg_encoder(&job.encoder);
        let (ctx, has_audio, duration) = prepare_video_job(&state, job, Some(subSuffix.clone()))?;
        let plan = if gpu {
            gpu_plan_with_fallback(job, &ctx, has_audio, duration)?
        } else {
            cli::video::build_soft_plan(job, &ctx)?
        };
        ids.push(state.jobs.submit(plan, opts.clone()));
    }
    maruko_core::logs::write(&format!("提交批量视频任务 {} 条", ids.len()));
    Ok(ids)
}

// ---------- 音频 ----------

#[tauri::command]
pub fn encode_audio(state: State<AppState>, job: AudioJob) -> Result<u32, String> {
    let tools = state.tools().clone();
    let temp_base = unique_temp(&temp_dir());
    let steps = cli::audio::build_steps(&job, &tools.tools_dir, &tools.ffmpeg, &temp_base)?;
    let plan = cli::Plan {
        title: format!("音频压制 {}", file_name(&job.input)),
        output: job.output.clone(),
        steps,
        shutdown_after: false,
        fallback: None,
        };
    let opts = run_opts(&state.settings.lock().unwrap());
    maruko_core::logs::write(&format!("提交音频任务（{} → {}）", job.input, job.output));
    Ok(state.jobs.submit(plan, opts))
}

#[tauri::command]
pub fn encode_audio_batch(state: State<AppState>, jobs: Vec<AudioJob>) -> Result<Vec<u32>, String> {
    let tools = state.tools().clone();
    let opts = run_opts(&state.settings.lock().unwrap());
    let mut ids = Vec::new();
    for job in &jobs {
        let temp_base = unique_temp(&temp_dir());
        let steps = cli::audio::build_steps(job, &tools.tools_dir, &tools.ffmpeg, &temp_base)?;
        let plan = cli::Plan {
            title: format!("音频压制 {}", file_name(&job.input)),
            output: job.output.clone(),
            steps,
            shutdown_after: false,
            fallback: None,
            };
        ids.push(state.jobs.submit(plan, opts.clone()));
    }
    Ok(ids)
}

#[tauri::command]
pub fn merge_audio(state: State<AppState>, inputs: Vec<String>, encoder: String, bitrate: u32) -> Result<u32, String> {
    let tools = state.tools().clone();
    if inputs.len() < 2 {
        return Err("合并至少需要两个音频文件".into());
    }
    let ext = cli::audio::output_ext(&encoder);
    let out = cli::auto_output(&inputs[0], &format!("merged.{}", ext), "");
    let steps = cli::audio::build_merge_steps(&inputs, &out, &encoder, bitrate, &tools.tools_dir, &tools.ffmpeg)?;
    let plan = cli::Plan { title: "合并音频".into(), output: out, steps, shutdown_after: false, fallback: None };
    let opts = run_opts(&state.settings.lock().unwrap());
    Ok(state.jobs.submit(plan, opts))
}

// ---------- 封装 ----------

#[tauri::command]
pub fn mux_mp4(
    state: State<AppState>,
    video: String,
    audio: Option<String>,
    output: String,
    fps: String,
    par: String,
    replace_audio: bool,
) -> Result<u32, String> {
    let tools = state.tools().clone();
    let out = if output.is_empty() { cli::auto_output(&video, "mp4", "") } else { output };
    let plan = cli::mux::build_mux_mp4(&video, audio.as_deref(), &out, &fps, &par, replace_audio, &tools.ffmpeg, &tools.tools_dir)?;
    let opts = run_opts(&state.settings.lock().unwrap());
    Ok(state.jobs.submit(plan, opts))
}

#[tauri::command]
pub fn mux_mkv(state: State<AppState>, video: String, audio: Option<String>, subtitle: Option<String>, output: String) -> Result<u32, String> {
    let tools = state.tools().clone();
    let out = if output.is_empty() { cli::auto_output(&video, "mkv", "") } else { output };
    let plan = cli::mux::build_mux_mkv(&video, audio.as_deref(), subtitle.as_deref(), &out, &tools.tools_dir)?;
    let opts = run_opts(&state.settings.lock().unwrap());
    Ok(state.jobs.submit(plan, opts))
}

#[derive(serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConvertJob {
    pub input: String,
    pub output: String,
    pub audio_encoder: String,
}

#[tauri::command]
pub fn convert_mux(state: State<AppState>, jobs: Vec<ConvertJob>) -> Result<Vec<u32>, String> {
    let tools = state.tools().clone();
    let opts = run_opts(&state.settings.lock().unwrap());
    let mut ids = Vec::new();
    for j in &jobs {
        let plan = cli::mux::build_convert(&j.input, &j.output, &j.audio_encoder, &tools.ffmpeg)?;
        ids.push(state.jobs.submit(plan, opts.clone()));
    }
    Ok(ids)
}

// ---------- 抽取 ----------

#[tauri::command]
pub fn extract_common(state: State<AppState>, path: String, kind: String, index: u32) -> Result<u32, String> {
    let tools = state.tools().clone();
    if tools.ffmpeg.is_empty() {
        return Err("未找到 ffmpeg".into());
    }
    let info = probe_video(&state, &path)?;
    let plan = if kind == "video" {
        let ext = match info.vcodec.as_str() {
            "h264" => "264",
            "hevc" => "hevc",
            _ => "mkv",
        };
        let out = cli::auto_output(&path, ext, "");
        cli::extract::build_common_video(&path, &out, &info.vcodec, &tools.ffmpeg)?
    } else {
        let base_ext = cli::extract::audio_ext(&info.acodec);
        let ext = if index > 0 { format!("{}.{}", index, base_ext) } else { base_ext.to_string() };
        let out = cli::auto_output(&path, &ext, "");
        cli::extract::build_common_audio(&path, &out, index, &tools.ffmpeg)?
    };
    let opts = run_opts(&state.settings.lock().unwrap());
    Ok(state.jobs.submit(plan, opts))
}

#[tauri::command]
pub fn extract_flv(state: State<AppState>, path: String, kind: String) -> Result<String, String> {
    let tools = state.tools().clone();
    // 优先 FLVExtractCL（输出到源目录，同步快速执行），失败回退 ffmpeg 任务
    let cl_plan = cli::extract::build_flv(&path, &kind, &tools.tools_dir);
    if let Ok(p) = &cl_plan {
        if !tools.tools_dir.is_empty() {
            if let Ok(st) = cmd_nowin(&p.steps[0].exe).args(&p.steps[0].args).status() {
                if st.success() {
                    maruko_core::logs::write("FLV 抽取完成（FLVExtractCL）");
                    return Ok("ok".into());
                }
            }
        }
    }
    if tools.ffmpeg.is_empty() {
        return Err("FLVExtractCL 与 ffmpeg 均不可用".into());
    }
    let info = probe_video(&state, &path)?;
    let plan = if kind == "video" {
        let ext = match info.vcodec.as_str() {
            "h264" => "264",
            "hevc" => "hevc",
            _ => "mkv",
        };
        let out = cli::auto_output(&path, ext, "");
        cli::extract::build_common_video(&path, &out, &info.vcodec, &tools.ffmpeg)?
    } else {
        let out = cli::auto_output(&path, cli::extract::audio_ext(&info.acodec), "");
        cli::extract::build_common_audio(&path, &out, 0, &tools.ffmpeg)?
    };
    let opts = run_opts(&state.settings.lock().unwrap());
    Ok(state.jobs.submit(plan, opts).to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MkvTrack {
    pub index: u32,
    pub kind: String,
    pub codec: String,
    pub lang: String,
    pub name: String,
}

#[tauri::command]
pub fn identify_mkv(state: State<AppState>, path: String) -> Result<Vec<MkvTrack>, String> {
    let tools = state.tools().clone();
    let mkvmerge = cli::video::mkvmerge_path(&tools.tools_dir);
    let out = cmd_nowin(&mkvmerge).args(["-J", &path]).output().map_err(|e| e.to_string())?;
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("mkvmerge -J 解析失败：{}", e))?;
    let mut tracks = Vec::new();
    for t in v["tracks"].as_array().cloned().unwrap_or_default() {
        tracks.push(MkvTrack {
            index: t["id"].as_u64().unwrap_or(0) as u32,
            kind: t["type"].as_str().unwrap_or("").to_string(),
            codec: t["properties"]["codec_name"].as_str().or(t["codec"].as_str()).unwrap_or("").to_string(),
            lang: t["properties"]["language"].as_str().unwrap_or("und").to_string(),
            name: t["properties"]["track_name"].as_str().unwrap_or("").to_string(),
        });
    }
    Ok(tracks)
}

#[tauri::command]
pub fn extract_mkv_track(state: State<AppState>, path: String, track: u32, ext: String) -> Result<u32, String> {
    let tools = state.tools().clone();
    let out = cli::auto_output(&path, &ext, "");
    let plan = cli::extract::build_mkv_track(&path, track, &out, &tools.tools_dir)?;
    let opts = run_opts(&state.settings.lock().unwrap());
    Ok(state.jobs.submit(plan, opts))
}

// ---------- 常用 ----------

#[tauri::command]
pub fn one_pic(state: State<AppState>, mut job: clicommon::OnePicJob) -> Result<u32, String> {
    let tools = state.tools().clone();
    if job.output.is_empty() {
        job.output = cli::auto_output(&job.image, "mp4", "");
    }
    let feat = check_libx264(&tools.ffmpeg);
    let plan = cli::common::build_one_pic(&job, &tools.ffmpeg, &tools.tools_dir, feat)?;
    let opts = run_opts(&state.settings.lock().unwrap());
    Ok(state.jobs.submit(plan, opts))
}

#[tauri::command]
pub fn cut_video(state: State<AppState>, mut job: clicommon::CutJob) -> Result<u32, String> {
    let tools = state.tools().clone();
    if job.output.is_empty() {
        let ext = PathBuf::from(&job.input)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "mp4".into());
        job.output = cli::auto_output(&job.input, &ext, "");
    }
    let plan = cli::common::build_cut(&job, &tools.ffmpeg)?;
    let opts = run_opts(&state.settings.lock().unwrap());
    Ok(state.jobs.submit(plan, opts))
}

#[tauri::command]
pub fn rotate_video(state: State<AppState>, mut job: clicommon::RotateJob) -> Result<u32, String> {
    let tools = state.tools().clone();
    if job.output.is_empty() {
        job.output = cli::auto_output(&job.input, "mp4", "rotated");
    }
    let feat = check_libx264(&tools.ffmpeg);
    let plan = cli::common::build_rotate(&job, &tools.ffmpeg, &tools.tools_dir, feat)?;
    let opts = run_opts(&state.settings.lock().unwrap());
    Ok(state.jobs.submit(plan, opts))
}

fn check_libx264(ffmpeg: &str) -> bool {
    cached_libx264(ffmpeg)
}

// ---------- AVS ----------

#[tauri::command]
pub fn save_avs(path: String, content: String) -> Result<(), String> {
    maruko_core::avs::write_script_ansi(&path, &content).map_err(|e| format!("保存 AVS 失败：{}", e))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvsPreviewResult {
    pub ok: bool,
    pub path: String,
    pub error: String,
}

/// AVS 预览：x264 ultrafast 渲染 ≤96 帧到临时 mp4。
#[tauri::command]
pub fn avs_preview(state: State<AppState>, mut job: VideoJob) -> AvsPreviewResult {
    let tools = state.tools().clone();
    let info = match probe_video(&state, &job.input) {
        Ok(i) => i,
        Err(e) => return AvsPreviewResult { ok: false, path: String::new(), error: format!("探测源失败：{}", e) },
    };
    if tools.tools_dir.is_empty() {
        return AvsPreviewResult { ok: false, path: String::new(), error: "未找到工具链目录".into() };
    }
    let temp_base = unique_temp(&temp_dir());
    let script = if job.avs_script.trim().is_empty() {
        let demuxer = match job.demuxer.as_str() {
            "ffms2" => maruko_core::avs::Demuxer::Ffms2,
            _ => maruko_core::avs::Demuxer::Lsmash,
        };
        maruko_core::avs::build_script(
            &job.input,
            opt_str(&job.subtitle),
            demuxer,
            &maruko_core::avs::AvsFilters::default(),
            &tools.tools_dir,
            &avs_dir(&tools),
            "",
            job.start_frame,
            job.frames,
            info.width,
            info.height,
        )
    } else {
        job.avs_script.clone()
    };
    let avs_path = format!("{}.avs", temp_base);
    let out = format!("{}_preview.mp4", temp_base);
    if let Err(e) = maruko_core::avs::write_script_ansi(&avs_path, &script) {
        return AvsPreviewResult { ok: false, path: String::new(), error: e };
    }
    let x264 = PathBuf::from(&tools.tools_dir).join("x264_64-8bit.exe");
    let avs4x26x = PathBuf::from(&tools.tools_dir).join("avs4x26x.exe");
    let mut cmd = cmd_nowin(&avs4x26x.to_string_lossy());
    cmd.env("PATH", format!("{};{}", tools.tools_dir, std::env::var("PATH").unwrap_or_default()));
    cmd.args([
        "--x26x-binary".to_string(),
        x264.to_string_lossy().to_string(),
        "--preset".to_string(),
        "ultrafast".to_string(),
        "--crf".to_string(),
        "18".to_string(),
        "--frames".to_string(),
        "96".to_string(),
        "-o".to_string(),
        out.clone(),
        avs_path.clone(),
    ]);
    let st = cmd.status();
    match st {
        Ok(s) if s.success() => AvsPreviewResult { ok: true, path: out, error: String::new() },
        Ok(_) => AvsPreviewResult { ok: false, path: String::new(), error: "预览渲染失败，详见日志".into() },
        Err(e) => AvsPreviewResult { ok: false, path: String::new(), error: e.to_string() },
    }
}

// ---------- 任务控制 / 关机 / 日志 / 更新 ----------

#[tauri::command]
pub fn job_cancel(state: State<AppState>, id: u32) {
    state.jobs.cancel(id);
    cancel_shutdown();
}

#[tauri::command]
pub fn shutdown_cancel_cmd() {
    cancel_shutdown();
}

#[tauri::command]
pub fn get_logs() -> Vec<String> {
    maruko_core::logs::read_all()
}

#[tauri::command]
pub fn open_logs_dir() -> Result<(), String> {
    let dir = maruko_core::logs::log_file().parent().unwrap().to_path_buf();
    std::process::Command::new("explorer").arg(dir).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_logs() {
    maruko_core::logs::delete_all();
}

// ---------- 更新（GitHub Releases 检查 / 下载 / 应用） ----------

use maruko_core::update::UpdateAsset;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    pub ok: bool,
    pub current: String,
    pub latest: String,
    pub msg: String,
    /// 轻量更新包（仅 exe+dll，供一键更新）
    pub update_asset: Option<UpdateAsset>,
    /// 完整版 zip（含工具链）
    pub full_asset: Option<UpdateAsset>,
    pub html_url: String,
}

/// HTTP GET，带代理回退链：直连 → 环境代理 → 本地常见代理端口（7890/7897）。
/// 目标是 GitHub 在部分网络下直连不稳；哪条路成功记录到日志。
fn http_get(url: &str, timeout: std::time::Duration) -> Result<ureq::Response, String> {
    let env_proxy = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()));
    let mut candidates: Vec<Option<String>> = vec![None, env_proxy.clone()];
    for p in ["http://127.0.0.1:7890", "http://127.0.0.1:7897"] {
        if env_proxy.as_deref() != Some(p) {
            candidates.push(Some(p.into()));
        }
    }
    let mut last_err = String::new();
    for via in &candidates {
        let mut builder = ureq::AgentBuilder::new();
        if let Some(p) = via {
            match ureq::Proxy::new(p) {
                Ok(proxy) => builder = builder.proxy(proxy),
                Err(_) => continue,
            }
        }
        let res = builder
            .build()
            .get(url)
            .timeout(timeout)
            .set("User-Agent", "maruko-v2-updater")
            .call();
        match res {
            Ok(r) => {
                maruko_core::logs::write(&format!("更新请求成功（{}）", if via.is_none() { "直连" } else { via.as_deref().unwrap_or("") }));
                return Ok(r);
            }
            Err(e) => {
                last_err = format!("{}（经 {}）", e, if via.is_none() { "直连" } else { via.as_deref().unwrap_or("") });
                maruko_core::logs::write(&format!("更新请求失败：{}", last_err));
            }
        }
    }
    Err(last_err)
}


#[tauri::command]
pub async fn check_update(state: State<'_, AppState>) -> Result<UpdateResult, String> {
    let mut url = state.settings.lock().unwrap().update_url.clone();
    if url.trim().is_empty() {
        url = DEFAULT_UPDATE_API.into();
    }
    let resp = tauri::async_runtime::spawn_blocking(move || http_get(&url, std::time::Duration::from_secs(12)))
        .await
        .map_err(|e| e.to_string())?;
    let mut resp = resp?;
    let v: serde_json::Value = resp.into_json().map_err(|e| format!("解析失败：{}", e))?;

    let (latest, update_asset, full_asset, html_url);
    if v["tag_name"].is_string() {
        // GitHub Releases 协议
        latest = v["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
        let (u, f) = maruko_core::update::pick_assets(v["assets"].as_array().map(|a| a.as_slice()).unwrap_or(&[]));
        update_asset = u;
        full_asset = f;
        html_url = v["html_url"].as_str().unwrap_or("").to_string();
    } else if v["version"].is_string() {
        // 兼容旧简易协议 {version, url}
        latest = v["version"].as_str().unwrap_or("").to_string();
        update_asset = None;
        full_asset = None;
        html_url = v["url"].as_str().unwrap_or("").to_string();
    } else {
        return Ok(UpdateResult {
            ok: false, current: VERSION.into(), latest: String::new(),
            msg: "接口返回异常".into(), update_asset: None, full_asset: None, html_url: String::new(),
        });
    }

    let (msg, ok) = match maruko_core::update::version_cmp(&latest, VERSION) {
        std::cmp::Ordering::Greater => (format!("发现新版本 {}", latest), true),
        std::cmp::Ordering::Equal => ("当前已是最新版本".to_string(), true),
        std::cmp::Ordering::Less => (format!("当前版本 {} 较新（接口为 {}）", VERSION, latest), true),
    };
    maruko_core::logs::write(&format!("检查更新：当前 {}，接口 {}，{}", VERSION, latest, msg));
    Ok(UpdateResult { ok, current: VERSION.into(), latest, msg, update_asset, full_asset, html_url })
}

static UPDATE_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub fn update_cancel() {
    UPDATE_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn update_work_dir() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.join("temp").join("update")))
        .filter(|p| std::fs::create_dir_all(p).is_ok());
    base.unwrap_or_else(|| {
        let p = std::env::temp_dir().join("MarukoV2-update");
        let _ = std::fs::create_dir_all(&p);
        p
    })
}

/// 流式下载更新包到本地，发 update://progress 事件（received/total/pct）。
/// 返回 zip 落盘路径。
#[tauri::command]
pub async fn download_update(app: AppHandle, url: String, name: String) -> Result<String, String> {
    use tauri::Emitter as _;
    UPDATE_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    let dest = update_work_dir().join(if name.is_empty() { "MarukoV2-update.zip".to_string() } else { name });
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 连接 15s 超时；下载体不限总时长（大文件慢网）
        let mut resp = http_get(&url, std::time::Duration::from_secs(15))?;
        let total = resp
            .header("Content-Length")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let mut f = std::fs::File::create(&dest).map_err(|e| format!("创建文件失败：{}", e))?;
        let mut reader = resp.into_reader();
        let mut buf = [0u8; 64 * 1024];
        let mut received: u64 = 0;
        let mut last_emit = std::time::Instant::now() - std::time::Duration::from_secs(1);
        loop {
            if UPDATE_CANCEL.load(std::sync::atomic::Ordering::SeqCst) {
                drop(f);
                let _ = std::fs::remove_file(&dest);
                return Err("已取消下载".into());
            }
            let n = reader.read(&mut buf).map_err(|e| format!("下载中断：{}", e))?;
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n]).map_err(|e| format!("写入失败：{}", e))?;
            received += n as u64;
            if last_emit.elapsed() >= std::time::Duration::from_millis(300) {
                last_emit = std::time::Instant::now();
                let pct = if total > 0 { received as f64 / total as f64 * 100.0 } else { 0.0 };
                let _ = app2.emit(
                    "update://progress",
                    serde_json::json!({ "received": received, "total": total, "pct": pct, "phase": "download" }),
                );
            }
        }
        let _ = app2.emit(
            "update://progress",
            serde_json::json!({ "received": received, "total": total, "pct": 100.0, "phase": "download" }),
        );
        maruko_core::logs::write(&format!("更新包下载完成：{}（{} MB）", dest.display(), received / 1024 / 1024));
        Ok(dest.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

use std::io::{Read as _, Write as _};

/// 自更新批处理：等本进程退出 → 覆盖 → 重启 → 清理 + 自删（中文路径走 GBK 落盘）
fn write_batch_ansi(path: &PathBuf, content: &str) -> Result<(), String> {
    maruko_core::avs::write_text_ansi(&path.to_string_lossy(), content)
}

/// 应用更新：解压 → 写自更新批处理（等本进程退出 → 覆盖 exe/dll → 重启 → 清理自删）→ 退出本程序。
#[tauri::command]
pub async fn apply_update(app: AppHandle, zip_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || apply_update_blocking(&zip_path))
        .await
        .map_err(|e| e.to_string())??;
    maruko_core::logs::write("更新已布置：程序即将重启完成替换");
    app.exit(0);
    #[allow(unreachable_code)]
    Ok(())
}

fn apply_update_blocking(zip_path: &str) -> Result<(), String> {
    let zip = PathBuf::from(zip_path);
    if !zip.is_file() {
        return Err(format!("更新包不存在：{}", zip_path));
    }
    let extract_dir = zip.parent().unwrap().join(format!("{}_x", zip.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default()));
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir).map_err(|e| format!("创建解压目录失败：{}", e))?;

    // 解压：优先系统自带 bsdtar（支持 zip），失败回退 PowerShell Expand-Archive
    let tar_ok = cmd_nowin("tar")
        .args(["-xf", zip_path, "-C"])
        .arg(&extract_dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !tar_ok {
        let ps = format!(
            "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            zip_path,
            extract_dir.display()
        );
        let ps_ok = cmd_nowin("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ps_ok {
            return Err("解压更新包失败（tar 与 PowerShell 均不可用）".into());
        }
    }

    // 找新 exe（zip 内平铺：exe + WebView2Loader.dll）
    let mut new_exe: Option<PathBuf> = None;
    let mut new_dll: Option<PathBuf> = None;
    for e in std::fs::read_dir(&extract_dir).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        match p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default().as_str() {
            "exe" if new_exe.is_none() => new_exe = Some(p),
            "dll" if p.file_name().map(|n| n.eq_ignore_ascii_case("WebView2Loader.dll")).unwrap_or(false) => new_dll = Some(p),
            _ => {}
        }
    }
    let new_exe = new_exe.ok_or("更新包内未找到 exe")?;
    let cur_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let cur_pid = std::process::id();

    // 自更新批处理：等本进程退出 → 覆盖 → 重启 → 清理 + 自删
    let bat = update_work_dir().join("update.bat");
    let script = format!(
        "@echo off\r\n:waitloop\r\ntasklist /FI \"PID eq {pid}\" | find \"{pid}\" >NUL\r\nif not errorlevel 1 (\r\n  ping -n 2 127.0.0.1 >NUL\r\n  goto waitloop\r\n)\r\ncopy /y \"{new}\" \"{cur}\"\r\n{dll_copy}start \"\" \"{cur}\"\r\nrd /s /q \"{ext}\"\r\ndel \"{zipfile}\"\r\ndel \"%~f0\"\r\n",
        pid = cur_pid,
        new = new_exe.display(),
        cur = cur_exe.display(),
        dll_copy = new_dll.as_ref().map(|d| format!("copy /y \"{}\" \"{}\"\r\n", d.display(), cur_exe.with_file_name("WebView2Loader.dll").display())).unwrap_or_default(),
        ext = extract_dir.display(),
        zipfile = zip.display(),
    );
    write_batch_ansi(&bat, &script).map_err(|e| format!("写更新脚本失败：{}", e))?;
    cmd_nowin("cmd")
        .args(["/C"])
        .arg(&bat)
        .spawn()
        .map_err(|e| format!("启动更新脚本失败：{}", e))?;
    Ok(())
}

// ---------- 事件桥 / 启动 ----------

pub fn make_emitter(app: AppHandle) -> Arc<Emitter> {
    Arc::new(Box::new(move |ev: Ev| {
        let r = match ev {
            Ev::Progress(p) => app.emit("job://progress", p),
            Ev::State(s) => {
                maruko_core::logs::write(&format!("[任务 #{}] {} ({})", s.id, s.state, s.title));
                app.emit("job://state", s)
            }
            Ev::Log(l) => {
                maruko_core::logs::write(&format!("[任务 #{}] {}", l.id, l.line));
                app.emit("job://log", l)
            }
            Ev::QueueIdle { last_failed } => app.emit("job://queue_idle", serde_json::json!({ "lastFailed": last_failed })),
        };
        if let Err(e) = r {
            maruko_core::logs::write(&format!("事件发送失败：{}", e));
        }
    }))
}

#[tauri::command]
pub fn log_frontend(message: String) {
    maruko_core::logs::write(&message);
}

#[tauri::command]
pub fn ui_ready(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    if let Some(w) = app.get_webview_window("splash") {
        let _ = w.close();
    }
}
