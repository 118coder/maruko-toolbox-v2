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
    pub enc_options: std::collections::BTreeMap<String, String>, // 编码器专属参数（Voukoder 风格面板）
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
            enc_options: Default::default(),
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
    // 新工具链是单 exe 多位深库（x265 4.x multilib / x264 t_mod (8&10)-bit），
    // 位深不再由 exe 变体决定，需显式选择；放在 extra 之前，用户自定义参数可覆盖
    if job.encoder.contains("10bit") {
        if x264 {
            a.extend(["--output-depth".into(), "10".into()]);
        } else {
            a.extend(["-D".into(), "10".into()]);
        }
    } else if !x264 && job.encoder.contains("12bit") {
        a.extend(["-D".into(), "12".into()]);
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
    // Voukoder 风格专业编码器（ffmpeg 后端）
    match id {
        "prores_ks" => "prores",
        "cfhd" => "cineform",
        "ffv1" => "ffv1",
        "utvideo" => "utvideo",
        "libvpx-vp9" => "vp9",
        "libx265" => "x265m",
        "libx264" => "x264m",
        _ => "",
    }
}

/// 走 ffmpeg 直编管线的编码器（GPU + 专业/无损编码器），与 AVS 软编管线区分。
pub fn is_ffmpeg_encoder(id: &str) -> bool {
    is_gpu_encoder(id)
        || matches!(
            id,
            "prores_ks" | "cfhd" | "ffv1" | "utvideo" | "libvpx-vp9" | "libx265" | "libx264"
        )
}

/// 编码器专属参数 → ffmpeg 参数（Voukoder 风格面板注入点）。
pub fn encoder_option_args(encoder: &str, options: &std::collections::BTreeMap<String, String>, crf: f64) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    let opt = |k: &str| options.get(k).cloned().unwrap_or_default();
    match encoder {
        // ProRes：质量 qscale 1-31（越低越好）+ profile
        "prores_ks" => {
            if let Some(q) = options.get("qscale") {
                if let Ok(n) = q.parse::<i64>() {
                    if (1..=31).contains(&n) {
                        a.extend(["-qscale:v".into(), n.to_string()]);
                    }
                }
            }
            match opt("profile").as_str() {
                "proxy" | "lt" | "standard" | "hq" | "4444" | "4444xq" => {
                    a.extend(["-profile:v".into(), opt("profile")]);
                }
                _ => {}
            }
        }
        // CineForm：质量档位
        "cfhd" => match opt("quality").as_str() {
            "film1" | "film2" | "film3" | "film4" | "film5" => {
                a.extend(["-quality".into(), opt("quality")]);
            }
            _ => {}
        },
        // FFV1 无损：熵编码器
        "ffv1" => match opt("coder").as_str() {
            "0" | "1" => {
                a.extend(["-coder".into(), opt("coder")]);
            }
            _ => {}
        },
        // UtVideo 无损：帧内预测方向
        "utvideo" => match opt("pred").as_str() {
            "none" | "left" | "gradient" | "median" => {
                a.extend(["-pred".into(), opt("pred")]);
            }
            _ => {}
        },
        // VP9：CRF 恒定质量（-b:v 0）或目标码率 + 速度档
        "libvpx-vp9" => {
            if crf > 0.0 {
                a.extend(["-crf".into(), fmt_crf(crf), "-b:v".into(), "0".into()]);
            }
            match opt("deadline").as_str() {
                "good" | "best" | "realtime" => {
                    a.extend(["-deadline".into(), opt("deadline")]);
                }
                _ => {}
            }
            if let Ok(n) = opt("cpu_used").parse::<i64>() {
                if (0..=63).contains(&n) {
                    a.extend(["-cpu-used".into(), n.to_string()]);
                }
            }
        }
        // 现代版 x265/x264（ffmpeg 内置库，Voukoder/iAvoe 预设走这里）
        "libx265" | "libx264" => {
            let ps_key = if encoder == "libx265" { "x265-params" } else { "x264-params" };
            for (k, v) in options {
                match k.as_str() {
                    "preset" if !v.is_empty() => a.extend(["-preset".into(), v.clone()]),
                    k2 if k2 == ps_key && !v.is_empty() => a.extend([("-".to_string() + ps_key), v.clone()]),
                    _ => {}
                }
            }
        }
        // NVENC 参数面板白名单透传（预设/Multipass/空间AQ/Lookahead）
        "h264_nvenc" | "hevc_nvenc" | "av1_nvenc" => {
            for (k, v) in options {
                if v.is_empty() {
                    continue;
                }
                if matches!(k.as_str(), "preset" | "multipass" | "spatial-aq" | "lookahead") {
                    a.extend(["-".to_string() + k, v.clone()]);
                }
            }
        }
        _ => {}
    }
    a
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
            // mkvmerge 新版（v70+）要求 --default-duration 带单位，"24000/1001fps" 分数+fps 兼容精确帧率
            args.push("--default-duration".into());
            args.push(format!("0:{}fps", ctx.src_fps));
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
    let pix_fmt = job.enc_options.get("pix_fmt").cloned().unwrap_or_else(|| "yuv420p".into());
    vf.push(format!("format={}", pix_fmt));

    let mut args: Vec<String> = vec![
        "-hide_banner".into(), "-y".into(), "-i".into(), job.input.clone(),
        "-vf".into(), vf.join(","),
        "-c:v".into(), job.encoder.clone(),
    ];
    let bitrate_mode = job.mode == "bitrate" || job.mode == "2pass";
    match job.encoder.as_str() {
        // 现代版 x265/x264：CRF/码率由主面板控制，预设参数来自 enc_options
        "libx265" | "libx264" => {
            if bitrate_mode {
                args.extend(["-b:v".into(), format!("{}k", job.bitrate)]);
            } else {
                args.extend(["-crf".into(), fmt_crf(job.crf)]);
            }
            args.extend(encoder_option_args(&job.encoder, &job.enc_options, 0.0));
        }
        "libvpx-vp9" => {
            if bitrate_mode {
                args.extend(["-b:v".into(), format!("{}k", job.bitrate)]);
            }
            args.extend(encoder_option_args("libvpx-vp9", &job.enc_options, if bitrate_mode { 0.0 } else { job.crf }));
        }
        "prores_ks" | "cfhd" | "ffv1" | "utvideo" => {
            // 固定质量/无损：参数来自 enc_options 面板
            args.extend(encoder_option_args(&job.encoder, &job.enc_options, 0.0));
        }
        _ => {
            args.extend(crate::gpu::rate_ctl_args(
                &job.encoder,
                job.crf,
                if bitrate_mode { job.bitrate } else { 0 },
            ));
            // NVENC 参数面板白名单透传
            args.extend(encoder_option_args(&job.encoder, &job.enc_options, 0.0));
        }
    }
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
    let kind_label = if crate::cli::video::is_gpu_encoder(&job.encoder) { "GPU" } else { "ffmpeg" };
    let mut step = Step::new(&format!("视频编码（{}）", kind_label), &ctx.ffmpeg, args);
    step.parser = Parser::Ffmpeg { duration };
    Ok(Plan {
        title: format!("压制 {}（{}）", std::path::Path::new(&job.input)
            .file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), kind_label),
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
    fn test_soft_plan_depth_args() {
        // 新工具链单 exe 多位深库：10bit/12bit 需显式注入位深参数
        let mut j = job();
        j.encoder = "x265_64-10bit[gcc].exe".into();
        let p = build_soft_plan(&j, &ctx()).unwrap();
        assert!(p.steps[1].args.join(" ").contains("-D 10"));

        j.encoder = "x265_64-12bit[gcc].exe".into();
        let p = build_soft_plan(&j, &ctx()).unwrap();
        assert!(p.steps[1].args.join(" ").contains("-D 12"));

        j.encoder = "x264_64-10bit.exe".into();
        let p = build_soft_plan(&j, &ctx()).unwrap();
        assert!(p.steps[1].args.join(" ").contains("--output-depth 10"));

        // 8bit 走默认，不注入
        j.encoder = "x265_64-8bit[gcc].exe".into();
        let p = build_soft_plan(&j, &ctx()).unwrap();
        assert!(!p.steps[1].args.join(" ").contains("-D"));

        // extra 自定义参数在位深参数之后，可覆盖（重复参数取后者）
        let mut c = ctx();
        c.extra = "-D 8".into();
        j.encoder = "x265_64-10bit[gcc].exe".into();
        let p = build_soft_plan(&j, &c).unwrap();
        let joined = p.steps[1].args.join(" ");
        assert!(joined.contains("-D 10"));
        assert!(joined.rfind("-D 8").unwrap() > joined.rfind("-D 10").unwrap());
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
        assert!(mux.args.contains(&"0:24000/1001fps".to_string()));
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
    fn test_is_ffmpeg_encoder() {
        assert!(is_ffmpeg_encoder("h264_nvenc"));
        assert!(is_ffmpeg_encoder("prores_ks"));
        assert!(is_ffmpeg_encoder("libvpx-vp9"));
        assert!(!is_ffmpeg_encoder("x265_64-8bit[gcc].exe"));
    }

    #[test]
    fn test_encoder_option_args_prores() {
        let mut o = std::collections::BTreeMap::new();
        o.insert("profile".into(), "hq".into());
        o.insert("qscale".into(), "9".into());
        let a = encoder_option_args("prores_ks", &o, 0.0);
        assert!(a.contains(&"-profile:v".to_string()) && a.contains(&"hq".to_string()));
        assert!(a.contains(&"-qscale:v".to_string()) && a.contains(&"9".to_string()));
    }

    #[test]
    fn test_encoder_option_args_vp9() {
        let mut o = std::collections::BTreeMap::new();
        o.insert("deadline".into(), "good".into());
        o.insert("cpu_used".into(), "3".into());
        let a = encoder_option_args("libvpx-vp9", &o, 30.0);
        let j = a.join(" ");
        assert!(j.contains("-crf 30 -b:v 0"));
        assert!(j.contains("-deadline good -cpu-used 3"));
    }

    #[test]
    fn test_encoder_option_args_lossless() {
        let mut o = std::collections::BTreeMap::new();
        o.insert("pred".into(), "median".into());
        let a = encoder_option_args("utvideo", &o, 0.0);
        assert!(a.contains(&"-pred".to_string()) && a.contains(&"median".to_string()));
    }

    #[test]
    fn test_encoder_option_args_libx265() {
        let mut o = std::collections::BTreeMap::new();
        o.insert("preset".into(), "slow".into());
        o.insert("x265-params".into(), "aq-mode=4:bframes=11".into());
        o.insert("pix_fmt".into(), "yuv420p10le".into());
        let a = encoder_option_args("libx265", &o, 0.0);
        let j = a.join(" ");
        assert!(j.contains("-preset slow"));
        assert!(j.contains("-x265-params aq-mode=4:bframes=11"));
        // pix_fmt 不在此处（由 vf format 滤镜处理）
        assert!(!j.contains("yuv420p10le"));
    }

    #[test]
    fn test_encoder_option_args_nvenc_passthrough() {
        let mut o = std::collections::BTreeMap::new();
        o.insert("preset".into(), "p6".into());
        o.insert("multipass".into(), "fullres".into());
        o.insert("spatial-aq".into(), "1".into());
        o.insert("lookahead".into(), "32".into());
        o.insert("evil".into(), "arg".into());
        let a = encoder_option_args("hevc_nvenc", &o, 0.0);
        let j = a.join(" ");
        // BTreeMap 按字母序输出，逐项断言
        assert!(j.contains("-preset p6"));
        assert!(j.contains("-multipass fullres"));
        assert!(j.contains("-spatial-aq 1"));
        assert!(j.contains("-lookahead 32"));
        assert!(!j.contains("evil"));
    }

    #[test]
    fn test_gpu_plan_pix_fmt() {
        let mut j = job();
        j.encoder = "libx265".into();
        j.mode = "crf".into();
        let mut c = ctx();
        c.audio_temp = None;
        let mut o = std::collections::BTreeMap::new();
        o.insert("pix_fmt".into(), "yuv420p10le".into());
        o.insert("preset".into(), "slow".into());
        o.insert("x265-params".into(), "aq-mode=4:bframes=11".into());
        j.enc_options = o;
        let p = build_gpu_plan(&j, &c, false, 0.0).unwrap();
        let joined = p.steps[0].args.join(" ");
        assert!(joined.contains("format=yuv420p10le"));
        assert!(joined.contains("-preset slow -x265-params"));
        assert!(joined.contains("-crf 23.5"));
    }

    #[test]
    fn test_encoder_tag_pro() {
        assert_eq!(encoder_tag("prores_ks"), "prores");
        assert_eq!(encoder_tag("cfhd"), "cineform");
        assert_eq!(encoder_tag("libvpx-vp9"), "vp9");
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
