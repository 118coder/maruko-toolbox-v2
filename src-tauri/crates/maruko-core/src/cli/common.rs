use super::video::{mp4box_path, fps_decimal};
use super::{Parser, Plan, Step};
use serde::{Deserialize, Serialize};

/// 一图流：图片 + 音频 → 视频。
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct OnePicJob {
    pub image: String,
    pub audio: String,
    pub output: String,
    pub audio_bitrate: u32,
    pub fps: f64,
    pub crf: f64,
    pub duration: u32,   // 秒
    pub copy_audio: bool,
}

impl Default for OnePicJob {
    fn default() -> Self {
        OnePicJob {
            image: String::new(),
            audio: String::new(),
            output: String::new(),
            audio_bitrate: 128,
            fps: 1.0,
            crf: 24.0,
            duration: 120,
            copy_audio: false,
        }
    }
}

/// 截取（流复制）。
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct CutJob {
    pub input: String,
    pub output: String,
    pub start: String, // HH:MM:SS
    pub end: String,
}

impl Default for CutJob {
    fn default() -> Self {
        CutJob { input: String::new(), output: String::new(), start: "00:00:00".into(), end: "00:00:20".into() }
    }
}

/// 旋转（Transpose）。
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct RotateJob {
    pub input: String,
    pub output: String,
    pub transpose: String, // transpose0..3 | hflip | vflip
}

impl Default for RotateJob {
    fn default() -> Self {
        RotateJob { input: String::new(), output: String::new(), transpose: "1".into() }
    }
}

fn encoder_chain_args(crf: f64) -> Vec<String> {
    vec!["-crf".into(), super::video::fmt_crf(crf), "-preset".into(), "medium".into()]
}

/// 一图流计划。`libx264_ok` = 本机 ffmpeg 含 libx264。
pub fn build_one_pic(job: &OnePicJob, ffmpeg: &str, tools_dir: &str, libx264_ok: bool) -> Result<Plan, String> {
    if job.image.is_empty() || job.audio.is_empty() {
        return Err("一图流需要图片和音频".into());
    }
    if ffmpeg.is_empty() {
        return Err("未找到 ffmpeg".into());
    }
    let fps = format!("{}", job.fps);
    let dur = job.duration.to_string();
    if libx264_ok {
        let mut args: Vec<String> = vec![
            "-hide_banner".into(), "-y".into(),
            "-loop".into(), "1".into(),
            "-framerate".into(), fps.clone(),
            "-i".into(), job.image.clone(),
            "-i".into(), job.audio.clone(),
            "-r".into(), fps.clone(),
            "-t".into(), dur.clone(),
            "-c:v".into(), "libx264".into(),
        ];
        args.extend(encoder_chain_args(job.crf));
        args.extend(["-pix_fmt".into(), "yuv420p".into()]);
        if job.copy_audio {
            args.extend(["-c:a".into(), "copy".into()]);
        } else {
            args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), format!("{}k", job.audio_bitrate)]);
        }
        args.extend(["-shortest".into(), job.output.clone()]);
        let mut s = Step::new("一图流", ffmpeg, args);
        s.parser = Parser::Ffmpeg { duration: job.duration as f64 };
        return Ok(Plan { title: "一图流".into(), output: job.output.clone(), steps: vec![s], shutdown_after: false });
    }
    // 回退：y4m 管道 → 原版 x264 → MP4Box 封装
    let mut steps = Vec::new();
    let mut dec = Step::new("一图流·生成视频流", ffmpeg, vec![
        "-hide_banner".into(), "-y".into(),
        "-loop".into(), "1".into(), "-framerate".into(), fps.clone(),
        "-t".into(), dur.clone(), "-i".into(), job.image.clone(),
        "-f".into(), "y4m".into(), "pipe:1".into(),
    ]);
    dec.parser = Parser::Ffmpeg { duration: job.duration as f64 };
    steps.push(dec);
    let x264 = std::path::Path::new(tools_dir).join("x264_64-8bit.exe").to_string_lossy().to_string();
    let raw = format!("{}.264", job.output.trim_end_matches(".mp4"));
    let mut enc = Step::new("一图流·x264 编码", &x264, vec![
        "--crf".into(), super::video::fmt_crf(job.crf), "--y4m".into(),
        "-o".into(), raw.clone(), "-".into(),
    ]);
    enc.stdin_from_prev = true;
    enc.parser = Parser::X264;
    enc.temp_outputs.push(raw.clone());
    steps.push(enc);
    let mut mux = Step::new("一图流·封装", &mp4box_path(tools_dir), vec![
        "-add".into(), raw.clone(),
        "-add".into(), job.audio.clone(),
        "-new".into(), job.output.clone(),
    ]);
    mux.env_path.push(tools_dir.to_string());
    steps.push(mux);
    Ok(Plan { title: "一图流".into(), output: job.output.clone(), steps, shutdown_after: false })
}

/// 截取计划（流复制）。
pub fn build_cut(job: &CutJob, ffmpeg: &str) -> Result<Plan, String> {
    if job.input.is_empty() {
        return Err("未选择视频".into());
    }
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-y".into()];
    if !job.start.is_empty() {
        args.extend(["-ss".into(), job.start.clone()]);
    }
    if !job.end.is_empty() {
        args.extend(["-to".into(), job.end.clone()]);
    }
    args.extend([
        "-i".into(), job.input.clone(),
        "-c".into(), "copy".into(),
        "-avoid_negative_ts".into(), "make_zero".into(),
        job.output.clone(),
    ]);
    let mut s = Step::new("截取", ffmpeg, args);
    s.parser = Parser::Ffmpeg { duration: 0.0 };
    Ok(Plan { title: "截取".into(), output: job.output.clone(), steps: vec![s], shutdown_after: false })
}

/// 旋转计划。`libx264_ok` = 本机 ffmpeg 含 libx264，否则管道回退到原版 x264。
pub fn build_rotate(job: &RotateJob, ffmpeg: &str, _tools_dir: &str, libx264_ok: bool) -> Result<Plan, String> {
    if job.input.is_empty() {
        return Err("未选择视频".into());
    }
    let vf = match job.transpose.as_str() {
        "hflip" => "hflip".to_string(),
        "vflip" => "vflip".to_string(),
        n if n.chars().all(|c| c.is_ascii_digit()) => format!("transpose={}", n),
        other => return Err(format!("未知旋转方式：{}", other)),
    };
    if !libx264_ok {
        return Err("本机 ffmpeg 缺少 libx264，旋转功能不可用".into());
    }
    let mut args: Vec<String> = vec![
        "-hide_banner".into(), "-y".into(), "-i".into(), job.input.clone(),
        "-vf".into(), vf,
        "-c:v".into(), "libx264".into(),
    ];
    args.extend(encoder_chain_args(23.5));
    args.extend(["-c:a".into(), "copy".into(), job.output.clone()]);
    let mut s = Step::new("旋转", ffmpeg, args);
    s.parser = Parser::Ffmpeg { duration: 0.0 };
    Ok(Plan { title: "旋转".into(), output: job.output.clone(), steps: vec![s], shutdown_after: false })
}

/// MediaInfo 探测出的 fps 分数转 MP4Box 用小数。
pub fn fps_for_mp4box(rate: &str) -> String {
    fps_decimal(rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one_pic_libx264() {
        let j = OnePicJob {
            image: "i.png".into(), audio: "a.m4a".into(), output: "o.mp4".into(),
            fps: 1.0, crf: 24.0, duration: 120, audio_bitrate: 128, copy_audio: true,
        };
        let p = build_one_pic(&j, "ff", "T:\\", true).unwrap();
        let joined = p.steps[0].args.join(" ");
        assert!(joined.contains("-loop 1 -framerate 1"));
        assert!(joined.contains("-c:v libx264 -crf 24"));
        assert!(joined.contains("-c:a copy"));
        assert!(joined.contains("-shortest"));
    }

    #[test]
    fn test_one_pic_pipe_fallback() {
        let j = OnePicJob { image: "i.png".into(), audio: "a.m4a".into(), output: "o.mp4".into(), ..Default::default() };
        let p = build_one_pic(&j, "ff", "T:\\", false).unwrap();
        assert_eq!(p.steps.len(), 3);
        assert!(p.steps[1].stdin_from_prev);
        assert!(p.steps[1].exe.ends_with("x264_64-8bit.exe"));
    }

    #[test]
    fn test_cut() {
        let j = CutJob { input: "in.mp4".into(), output: "o.mp4".into(), start: "00:00:00".into(), end: "00:00:20".into() };
        let p = build_cut(&j, "ff").unwrap();
        let joined = p.steps[0].args.join(" ");
        assert!(joined.contains("-ss 00:00:00 -to 00:00:20 -i in.mp4 -c copy"));
    }

    #[test]
    fn test_rotate() {
        let j = RotateJob { input: "in.mp4".into(), output: "o.mp4".into(), transpose: "2".into() };
        let p = build_rotate(&j, "ff", "T:\\", true).unwrap();
        assert!(p.steps[0].args.contains(&"transpose=2".to_string()));
        let j2 = RotateJob { input: "in.mp4".into(), output: "o.mp4".into(), transpose: "hflip".into() };
        let p2 = build_rotate(&j2, "ff", "T:\\", true).unwrap();
        assert!(p2.steps[0].args.contains(&"hflip".to_string()));
    }

    #[test]
    fn test_fps_for_mp4box() {
        assert_eq!(fps_for_mp4box("30000/1001"), "29.970");
    }
}
