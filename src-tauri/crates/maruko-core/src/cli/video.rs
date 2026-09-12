use super::{split_args, Parser, Plan, Step};
use serde::{Deserialize, Serialize};

/// 视频压制任务（与前端字段一一对应，camelCase）。
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct VideoJob {
    pub input: String,
    pub output: String,
    pub encoder: String,       // 软编 exe 名 或 ffmpeg 编码器 id（GPU）
    pub demuxer: String,       // auto | lsmash | ffms2
    pub mode: String,          // crf | bitrate | 2pass
    pub crf: f64,
    pub bitrate: u32,
    pub start_frame: u32,
    pub frames: u32,           // 0 = 到结尾
    pub width: u32,
    pub height: u32,
    pub keep_res: bool,
    pub audio_mode: String,    // copy | encode | none
    pub audio_encoder: String,
    pub audio_bitrate: u32,
    pub subtitle: String,
    pub container: String,     // mp4 | mkv
    pub avs_script: String,    // 非空 = 原样使用（AVS 页）
    pub avs_filters: Option<crate::avs::AvsFilters>, // AVS 页结构化滤镜
    pub shutdown_after: bool,
}

impl Default for VideoJob {
    fn default() -> Self {
        VideoJob {
            input: String::new(),
            output: String::new(),
            encoder: String::new(),
            demuxer: "auto".into(),
            mode: "crf".into(),
            crf: 23.5,
            bitrate: 800,
            start_frame: 0,
            frames: 0,
            width: 0,
            height: 0,
            keep_res: true,
            audio_mode: "copy".into(),
            audio_encoder: "neroaac".into(),
            audio_bitrate: 128,
            subtitle: String::new(),
            container: "mp4".into(),
            avs_script: String::new(),
            avs_filters: None,
            shutdown_after: false,
        }
    }
}

/// 原版 preset.xml 的 x264 default 预设（不含 --vf 部分，分辨率由 AVS 层处理）。
pub const DEFAULT_X264_PRESET: &[&str] = &[
    "--preset", "8", "-r", "6", "-b", "6", "-i", "1", "--scenecut", "60",
    "-f", "1:1", "--qcomp", "0.5", "--psy-rd", "0.3:0",
    "--aq-mode", "2", "--aq-strength", "0.8",
];

/// 命令层准备好的上下文（路径均已探测/写入完成）。
#[derive(Debug, Clone, Default)]
pub struct VideoCtx {
    pub tools_dir: String,
    pub ffmpeg: String,
    /// 已写好的 AVS 临时脚本路径（软编路径必需）
    pub avs_path: String,
    /// 临时文件基名（不含扩展名），如 ...\tmp\job-1
    pub temp_base: String,
    /// 音频临时文件（mode != none 时非空，扩展名已定）
    pub audio_temp: Option<String>,
    /// 源视频帧率（ffprobe r_frame_rate，如 "24000/1001"），raw 流封装时需要
    pub src_fps: String,
    pub priority: String,
    pub threads: u32,
    pub extra: String,
}

pub fn is_gpu_encoder(id: &str) -> bool {
    id.starts_with("h264_") || id.starts_with("hevc_") || id.starts_with("av1_")
}

pub fn is_x264(id: &str) -> bool {
    id.starts_with("x264")
}

/// 软编码参数（x264/x265 通用骨架）。
pub fn encode_args(job: &VideoJob, ctx: &VideoCtx, pass: Option<u32>, x264: bool) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    match job.mode.as_str() {
        "crf" => {
            a.push("--crf".into());
            a.push(fmt_crf(job.crf));
            if x264 && ctx.extra.trim().is_empty() {
                a.extend(DEFAULT_X264_PRESET.iter().map(|s| s.to_string()));
            }
        }
        "bitrate" => {
            a.push("--bitrate".into());
            a.push(job.bitrate.to_string());
        }
        "2pass" => {
            a.push("--bitrate".into());
            a.push(job.bitrate.to_string());
            a.push("--pass".into());
            a.push(pass.unwrap_or(1).to_string());
            a.push("--stats".into());
            a.push(format!("{}.stat", ctx.temp_base));
            if pass == Some(1) && x264 {
                a.push("--slow-firstpass".into());
            }
        }
        _ => {}
    }
    if ctx.threads > 0 {
        if x264 {
            a.push("--threads".into());
        } else {
            a.push("--pool".into());
        }
        a.push(ctx.threads.to_string());
    }
    if !ctx.extra.trim().is_empty() {
        a.extend(split_args(&ctx.extra));
    }
    a
}

/// 编码器命名标签（对齐原版：测试.mp4 → 测试x264.mp4 / 测试x265.mp4）。
pub fn encoder_tag(id: &str) -> &'static str {
    if id.starts_with("x264") {
        return "x264";
    }
    if id.starts_with("x265") {
        return "x265";
    }
    if id.contains("nvenc") {
        return "nvenc";
    }
    if id.contains("qsv") {
        return "qsv";
    }
    if id.contains("amf") {
        return "amf";
    }
    ""
}

pub fn fmt_crf(crf: f64) -> String {
    if (crf - crf.round()).abs() < f64::EPSILON {
        format!("{}", crf.round() as i64)
    } else {
        format!("{}", crf)
    }
}

/// 软编码完整计划：[(音频压制/抽取)] → avs4x26x 编码 (1Pass 或 2Pass) → 封装。
pub fn build_soft_plan(job: &VideoJob, ctx: &VideoCtx) -> Result<Plan, String> {
    if job.input.is_empty() {
        return Err("未选择输入视频".into());
    }
    if ctx.avs_path.is_empty() {
        return Err("内部错误：缺少 AVS 脚本".into());
    }
    let x264 = is_x264(&job.encoder);
    // 注意：不在此处做文件存在性检查（保持纯函数）；路径有效性由 locator 与运行期保证
    let enc_exe = std::path::Path::new(&ctx.tools_dir).join(&job.encoder);
    let raw_ext = if x264 { "264" } else { "hevc" };
    let raw = format!("{}.{}", ctx.temp_base, raw_ext);
    let output = job.output.clone();
    let mut steps: Vec<Step> = Vec::new();

    // 音频步骤
    match job.audio_mode.as_str() {
        "encode" => {
            let at = ctx.audio_temp.clone().ok_or("内部错误：缺少音频临时路径")?;
            let aj = super::audio::AudioJob {
                input: job.input.clone(),
                output: at.clone(),
                encoder: job.audio_encoder.clone(),
                mode: "bitrate".into(),
                bitrate: job.audio_bitrate,
                custom: String::new(),
            };
            steps.extend(super::audio::build_steps(&aj, &ctx.tools_dir, &ctx.ffmpeg, &ctx.temp_base)?);
        }
        "copy" => {
            let at = ctx.audio_temp.clone().ok_or("内部错误：缺少音频临时路径")?;
            let mut s = Step::new("抽取音频流", &ctx.ffmpeg, vec![
                "-hide_banner".into(), "-y".into(), "-i".into(), job.input.clone(),
                "-map".into(), "0:a:0".into(), "-c".into(), "copy".into(), at.clone(),
            ]);
            s.temp_outputs.push(at.clone());
            steps.push(s);
        }
        _ => {}
    }

    // 编码步骤（avs4x26x 管道）
    let bin_flag = "--x26x-binary";
    let tool = if x264 { "x264" } else { "x265" };
    let mk_encode_step = |pass: Option<u32>, title: String| -> Step {
        let mut args: Vec<String> = vec![bin_flag.into(), enc_exe.to_string_lossy().to_string()];
        args.extend(encode_args(job, ctx, pass, x264));
        args.push("-o".into());
        args.push(raw.clone());
        args.push(ctx.avs_path.clone());
        let mut s = Step::new(&title, /* avs4x26x 占位，下方替换 */ "", args);
        s.exe = std::path::Path::new(&ctx.tools_dir).join("avs4x26x.exe").to_string_lossy().to_string();
        s.env_path.push(ctx.tools_dir.clone());
        s.temp_outputs.push(raw.clone());
        s.parser = if x264 { Parser::X264 } else { Parser::X265 };
        s
    };

    match job.mode.as_str() {
        "2pass" => {
            steps.push(mk_encode_step(Some(1), format!("视频编码 {} 第一遍", tool)));
            steps.push(mk_encode_step(Some(2), format!("视频编码 {} 第二遍", tool)));
        }
        m => {
            let name = if m == "bitrate" { "平均码率" } else { "CRF" };
            steps.push(mk_encode_step(None, format!("视频编码 {}（{}）", tool, name)));
        }
    }

    // 封装步骤
    if job.container == "mkv" {
        let mut args: Vec<String> = vec!["-o".into(), output.clone()];
        if !ctx.src_fps.is_empty() {
            args.push("--default-duration".into());
            args.push(format!("0:{}", ctx.src_fps));
        }
        args.push(raw.clone());
        if let Some(at) = &ctx.audio_temp {
            args.push(at.clone());
        }
        let mut s = Step::new("封装 MKV", &mkvmerge_path(&ctx.tools_dir), args);
        s.env_path.push(ctx.tools_dir.clone());
        steps.push(s);
    } else {
        let mut args: Vec<String> = vec!["-add".into()];
        if ctx.src_fps.is_empty() {
            args.push(raw.clone());
        } else {
            args.push(format!("{}:fps={}", raw, fps_decimal(&ctx.src_fps)));
        }
        if let Some(at) = &ctx.audio_temp {
            args.push("-add".into());
            args.push(at.clone());
        }
        args.push("-new".into());
        args.push(output.clone());
        steps.push(Step::new("封装 MP4", &mp4box_path(&ctx.tools_dir), args));
    }

    Ok(Plan {
        title: format!("压制 {}", std::path::Path::new(&job.input)
            .file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()),
        output,
        steps,
        shutdown_after: job.shutdown_after,
    })
}

/// GPU（ffmpeg 硬编）计划：单步完成（含音频与滤镜链）。
pub fn build_gpu_plan(job: &VideoJob, ctx: &VideoCtx, has_audio: bool, duration: f64) -> Result<Plan, String> {
    if job.input.is_empty() {
        return Err("未选择输入视频".into());
    }
    if ctx.ffmpeg.is_empty() {
        return Err("未找到 ffmpeg，GPU 编码不可用".into());
    }
    let mut vf: Vec<String> = Vec::new();
    if job.start_frame > 0 || job.frames > 0 {
        let end = if job.frames > 0 { job.start_frame + job.frames } else { 0 };
        vf.push(if end > 0 {
            format!("trim=start_frame={}:end_frame={}", job.start_frame, end)
        } else {
            format!("trim=start_frame={}", job.start_frame)
        });
        vf.push("setpts=PTS-STARTPTS".into());
    }
    if !job.keep_res && job.width > 0 && job.height > 0 {
        vf.push(format!("scale={}:{},flags=lanczos", job.width, job.height));
    }
    if !job.subtitle.is_empty() {
        vf.push(format!("subtitles='{}'", escape_filter_path(&job.subtitle)));
    }
    vf.push("format=yuv420p".into());

    let mut args: Vec<String> = vec![
        "-hide_banner".into(), "-y".into(), "-i".into(), job.input.clone(),
        "-vf".into(), vf.join(","),
        "-c:v".into(), job.encoder.clone(),
    ];
    let bitrate_mode = job.mode == "bitrate" || job.mode == "2pass";
    args.extend(crate::gpu::rate_ctl_args(
        &job.encoder,
        job.crf,
        if bitrate_mode { job.bitrate } else { 0 },
    ));
    match job.audio_mode.as_str() {
        "copy" if has_audio => {
            args.extend(["-map".into(), "0:v:0".into(), "-map".into(), "0:a:0".into(), "-c:a".into(), "copy".into()]);
        }
        "encode" if has_audio => {
            args.extend([
                "-map".into(), "0:v:0".into(), "-map".into(), "0:a:0".into(),
                "-c:a".into(), "aac".into(), "-b:a".into(), format!("{}k", job.audio_bitrate),
            ]);
        }
        _ => args.push("-an".into()),
    }
    if job.container == "mp4" {
        args.extend(["-movflags".into(), "+faststart".into()]);
    }
    args.push(job.output.clone());
    let mut step = Step::new("视频编码 (GPU)", &ctx.ffmpeg, args);
    step.parser = Parser::Ffmpeg { duration };
    Ok(Plan {
        title: format!("压制 {}（GPU）", std::path::Path::new(&job.input)
            .file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()),
        output: job.output.clone(),
        steps: vec![step],
        shutdown_after: job.shutdown_after,
    })
}

/// ffmpeg 滤镜参数转义（subtitles 路径等）。
pub fn escape_filter_path(p: &str) -> String {
    p.replace('\\', "/").replace(':', "\\:").replace('\'', "\\'")
}

pub fn mkvmerge_path(tools_dir: &str) -> String {
    std::path::Path::new(tools_dir).join("mkvmerge.exe").to_string_lossy().to_string()
}
pub fn mp4box_path(tools_dir: &str) -> String {
    std::path::Path::new(tools_dir).join("MP4Box.exe").to_string_lossy().to_string()
}
pub fn mkvextract_path(tools_dir: &str) -> String {
    std::path::Path::new(tools_dir).join("mkvextract.exe").to_string_lossy().to_string()
}

/// "24000/1001" → "23.976"；"25" → "25"
pub fn fps_decimal(rate: &str) -> String {
    if let Some((a, b)) = rate.split_once('/') {
        let (a, b) = (a.trim().parse::<f64>(), b.trim().parse::<f64>());
        if let (Ok(a), Ok(b)) = (a, b) {
            if b != 0.0 {
                return format!("{:.3}", a / b);
            }
        }
        return "25".into();
    }
    rate.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> VideoJob {
        VideoJob {
            input: "D:\\in.mkv".into(),
            output: "D:\\out.mp4".into(),
            encoder: "x265_64-8bit[gcc].exe".into(),
            crf: 23.5,
            ..Default::default()
        }
    }

    fn ctx() -> VideoCtx {
        VideoCtx {
            tools_dir: "T:\\".into(),
            ffmpeg: "T:\\ffmpeg.exe".into(),
            avs_path: "C:\\tmp\\a.avs".into(),
            temp_base: "C:\\tmp\\j1".into(),
            audio_temp: Some("C:\\tmp\\j1.m4a".into()),
            src_fps: "24000/1001".into(),
            ..Default::default()
        }
    }

    #[test]
    fn test_soft_plan_x265_crf() {
        let p = build_soft_plan(&job(), &ctx()).unwrap();
        assert_eq!(p.steps.len(), 3); // 抽取音频 → 编码 → 封装
        let enc = &p.steps[1];
        assert!(enc.exe.ends_with("avs4x26x.exe"));
        let joined = enc.args.join(" ");
        assert!(joined.contains("--x26x-binary"));
        assert!(joined.contains("--crf 23.5"));
        assert!(joined.contains("-o C:\\tmp\\j1.hevc"));
        let mux = &p.steps[2];
        assert!(mux.exe.ends_with("MP4Box.exe"));
        assert!(mux.args.contains(&"C:\\tmp\\j1.hevc:fps=23.976".to_string()));
    }

    #[test]
    fn test_soft_plan_x264_default_preset() {
        let mut j = job();
        j.encoder = "x264_64-8bit.exe".into();
        let p = build_soft_plan(&j, &ctx()).unwrap();
        let enc = &p.steps[1];
        assert!(enc.args.join(" ").contains("--preset 8 -r 6 -b 6"));
        assert!(enc.args.contains(&"C:\\tmp\\j1.264".to_string()));
    }

    #[test]
    fn test_soft_plan_2pass() {
        let mut j = job();
        j.mode = "2pass".into();
        j.bitrate = 900;
        j.audio_mode = "none".into();
        let p = build_soft_plan(&j, &ctx()).unwrap();
        assert_eq!(p.steps.len(), 3); // pass1 pass2 mux
        let s1 = p.steps[0].args.join(" ");
        let s2 = p.steps[1].args.join(" ");
        assert!(s1.contains("--pass 1") && s1.contains("--bitrate 900"));
        assert!(s2.contains("--pass 2"));
    }

    #[test]
    fn test_soft_plan_2pass_x264_slow_firstpass() {
        let mut j = job();
        j.encoder = "x264_64-8bit.exe".into();
        j.mode = "2pass".into();
        j.audio_mode = "none".into();
        let p = build_soft_plan(&j, &ctx()).unwrap();
        assert!(p.steps[0].args.contains(&"--slow-firstpass".to_string()));
    }

    #[test]
    fn test_soft_plan_extra_overrides_preset() {
        let mut j = job();
        j.encoder = "x264_64-8bit.exe".into();
        let mut c = ctx();
        c.extra = "--tune animation".into();
        let p = build_soft_plan(&j, &c).unwrap();
        let joined = p.steps[1].args.join(" ");
        assert!(joined.contains("--tune animation"));
        assert!(!joined.contains("--preset 8"));
    }

    #[test]
    fn test_soft_plan_threads() {
        let mut j = job();
        let mut c = ctx();
        c.threads = 8;
        let p = build_soft_plan(&j, &c).unwrap();
        assert!(p.steps[1].args.contains(&"--pool".to_string()));
        j.encoder = "x264_64-8bit.exe".into();
        let p = build_soft_plan(&j, &c).unwrap();
        assert!(p.steps[1].args.contains(&"--threads".to_string()));
    }

    #[test]
    fn test_soft_plan_mkv() {
        let mut j = job();
        j.container = "mkv".into();
        let p = build_soft_plan(&j, &ctx()).unwrap();
        let mux = p.steps.last().unwrap();
        assert!(mux.exe.ends_with("mkvmerge.exe"));
        assert!(mux.args.contains(&"--default-duration".to_string()));
        assert!(mux.args.contains(&"0:24000/1001".to_string()));
    }

    #[test]
    fn test_gpu_plan() {
        let mut j = job();
        j.encoder = "hevc_nvenc".into();
        j.subtitle = "D:\\s ass.ass".into();
        let c = ctx();
        let p = build_gpu_plan(&j, &c, true, 60.0).unwrap();
        assert_eq!(p.steps.len(), 1);
        let a = &p.steps[0].args;
        let joined = a.join(" ");
        assert!(joined.contains("-c:v hevc_nvenc -rc vbr -cq 23.5 -b:v 0"));
        assert!(joined.contains("subtitles='D\\:/s ass.ass'"));
        assert!(joined.contains("-map 0:a:0"));
        assert!(a.contains(&"-movflags".to_string()));
    }

    #[test]
    fn test_gpu_plan_bitrate_and_trim() {
        let mut j = job();
        j.encoder = "h264_nvenc".into();
        j.mode = "bitrate".into();
        j.bitrate = 4500;
        j.start_frame = 100;
        j.frames = 200;
        j.audio_mode = "none".into();
        j.container = "mkv".into();
        let p = build_gpu_plan(&j, &ctx(), false, 0.0).unwrap();
        let joined = p.steps[0].args.join(" ");
        assert!(joined.contains("-b:v 4500k"));
        assert!(joined.contains("trim=start_frame=100:end_frame=300,setpts=PTS-STARTPTS"));
        assert!(joined.contains("-an"));
        assert!(!joined.contains("faststart"));
    }

    #[test]
    fn test_fps_decimal() {
        assert_eq!(fps_decimal("24000/1001"), "23.976");
        assert_eq!(fps_decimal("25"), "25");
    }

    #[test]
    fn test_encoder_tag() {
        assert_eq!(encoder_tag("x264_64-8bit.exe"), "x264");
        assert_eq!(encoder_tag("x265_64-10bit[gcc].exe"), "x265");
        assert_eq!(encoder_tag("hevc_nvenc"), "nvenc");
        assert_eq!(encoder_tag("h264_qsv"), "qsv");
        assert_eq!(encoder_tag("hevc_amf"), "amf");
    }

    #[test]
    fn test_fmt_crf() {
        assert_eq!(fmt_crf(23.5), "23.5");
        assert_eq!(fmt_crf(24.0), "24");
    }
}
