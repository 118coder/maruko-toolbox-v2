use super::{split_args, Parser, Step};
use serde::{Deserialize, Serialize};

/// 音频压制任务。
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioJob {
    pub input: String,
    pub output: String,
    pub encoder: String,   // neroaac | qaac | fdkaac | lame | flac | ffmpeg_aac
    pub mode: String,      // bitrate | custom
    pub bitrate: u32,      // Kbps
    pub custom: String,    // 自定义参数
}

impl Default for AudioJob {
    fn default() -> Self {
        AudioJob {
            input: String::new(),
            output: String::new(),
            encoder: "neroaac".into(),
            mode: "bitrate".into(),
            bitrate: 128,
            custom: String::new(),
        }
    }
}

pub fn output_ext(encoder: &str) -> &'static str {
    match encoder {
        "lame" => "mp3",
        "flac" => "flac",
        _ => "m4a",
    }
}

/// 音频压制计划：ffmpeg 解码 wav 管道 → 编码器（与原版管线一致）。
/// `temp_base` 用于输出文件自动命名（output 为空时）。
pub fn build_steps(job: &AudioJob, tools_dir: &str, ffmpeg: &str, _temp_base: &str) -> Result<Vec<Step>, String> {
    if job.input.is_empty() {
        return Err("未选择输入音频".into());
    }
    let output = if job.output.is_empty() {
        super::auto_output(&job.input, output_ext(&job.encoder), "")
    } else {
        job.output.clone()
    };
    let mut steps = Vec::new();

    if job.encoder == "ffmpeg_aac" {
        if ffmpeg.is_empty() {
            return Err("未找到 ffmpeg".into());
        }
        let mut s = Step::new("音频编码 ffmpeg AAC", ffmpeg, vec![
            "-hide_banner".into(), "-y".into(), "-i".into(), job.input.clone(),
            "-vn".into(),
            "-c:a".into(), "aac".into(),
            "-b:a".into(), format!("{}k", job.bitrate),
            output.clone(),
        ]);
        s.parser = Parser::Ffmpeg { duration: 0.0 };
        steps.push(s);
        return Ok(steps);
    }

    let tool = match job.encoder.as_str() {
        "neroaac" => "neroAacEnc.exe",
        "qaac" => "qaac.exe",
        "fdkaac" => "fdkaac.exe",
        "lame" => "lame.exe",
        "flac" => "flac.exe",
        other => return Err(format!("未知音频编码器：{}", other)),
    };
    let tool_path = std::path::Path::new(tools_dir).join(tool).to_string_lossy().to_string();

    // 第 1 步：解码为 wav 管道
    let mut dec = Step::new("音频解码 (ffmpeg)", ffmpeg, vec![
        "-hide_banner".into(), "-y".into(), "-i".into(), job.input.clone(),
        "-vn".into(), "-f".into(), "wav".into(), "pipe:1".into(),
    ]);
    dec.parser = Parser::Ffmpeg { duration: 0.0 };
    steps.push(dec);

    // 第 2 步：编码器（stdin = 上一步 stdout）
    let custom = job.mode == "custom" && !job.custom.trim().is_empty();
    let mut args: Vec<String> = Vec::new();
    let mut env_path = vec![tools_dir.to_string()];
    match job.encoder.as_str() {
        "neroaac" => {
            args.extend(["-if".into(), "-".into(), "-of".into(), output.clone()]);
            if custom {
                args.extend(split_args(&job.custom));
            } else {
                // ≤64Kbps 用 HE，其余 LC（对齐原版 preset 习惯）
                args.push(if job.bitrate <= 64 { "-he".into() } else { "-lc".into() });
                args.extend(["-br".into(), format!("{}", job.bitrate * 1000)]);
            }
        }
        "qaac" => {
            // qaac 依赖 qtfiles 里的 Apple DLL
            env_path.push(std::path::Path::new(tools_dir).join("qtfiles").to_string_lossy().to_string());
            args.push("--ignorelength".into());
            args.extend(["-o".into(), output.clone(), "-".into()]);
            if custom {
                args.splice(1..1, split_args(&job.custom));
            } else {
                args.splice(1..1, vec!["-c".into(), job.bitrate.to_string()]);
            }
        }
        "fdkaac" => {
            args.extend(["-o".into(), output.clone(), "-".into()]);
            if custom {
                args.splice(0..0, split_args(&job.custom));
            } else {
                args.splice(0..0, vec!["-b".into(), job.bitrate.to_string()]);
            }
        }
        "lame" => {
            args.extend(["--silent".into()]);
            if custom {
                args.extend(split_args(&job.custom));
            } else {
                args.extend(["-b".into(), job.bitrate.to_string()]);
            }
            args.extend(["-".into(), output.clone()]);
        }
        "flac" => {
            args.extend(["-s".into()]);
            if custom {
                args.extend(split_args(&job.custom));
            }
            args.extend(["-o".into(), output.clone(), "-".into()]);
        }
        _ => unreachable!(),
    }
    let title = format!("音频编码 {}", tool.trim_end_matches(".exe"));
    let mut enc = Step::new(&title, &tool_path, args);
    enc.stdin_from_prev = true;
    enc.env_path = env_path;
    steps.push(enc);
    Ok(steps)
}

/// 合并批量列表中的音频（concat 重编码到所选编码器/码率）。
pub fn build_merge_steps(
    inputs: &[String],
    output: &str,
    encoder: &str,
    bitrate: u32,
    tools_dir: &str,
    ffmpeg: &str,
) -> Result<Vec<Step>, String> {
    if inputs.len() < 2 {
        return Err("合并至少需要两个音频文件".into());
    }
    if ffmpeg.is_empty() {
        return Err("未找到 ffmpeg".into());
    }
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-y".into()];
    for f in inputs {
        args.extend(["-i".into(), f.clone()]);
    }
    args.extend([
        "-filter_complex".into(),
        format!("concat=n={}:v=0:a=1", inputs.len()),
        "-vn".into(),
    ]);
    if encoder == "lame" {
        args.extend(["-c:a".into(), "libmp3lame".into(), "-b:a".into(), format!("{}k", bitrate)]);
        args.push(output.to_string());
        let mut s = Step::new("合并音频 (MP3)", ffmpeg, args);
        s.parser = Parser::Ffmpeg { duration: 0.0 };
        return Ok(vec![s]);
    }
    if encoder == "flac" {
        args.extend(["-c:a".into(), "flac".into()]);
        args.push(output.to_string());
        let mut s = Step::new("合并音频 (FLAC)", ffmpeg, args);
        s.parser = Parser::Ffmpeg { duration: 0.0 };
        return Ok(vec![s]);
    }
    if encoder == "ffmpeg_aac" {
        args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), format!("{}k", bitrate)]);
        args.push(output.to_string());
        let mut s = Step::new("合并音频 (AAC)", ffmpeg, args);
        s.parser = Parser::Ffmpeg { duration: 0.0 };
        return Ok(vec![s]);
    }
    // NeroAAC / qaac / fdkaac 走 wav 管道
    args.extend(["-f".into(), "wav".into(), "pipe:1".into()]);
    let mut dec = Step::new("音频解码 (ffmpeg)", ffmpeg, args);
    dec.parser = Parser::Ffmpeg { duration: 0.0 };
    let sub_job = AudioJob {
        input: "(merge)".into(),
        output: output.to_string(),
        encoder: encoder.to_string(),
        mode: "bitrate".into(),
        bitrate,
        custom: String::new(),
    };
    let mut steps = vec![dec];
    let mut enc_steps = build_steps(&sub_job, tools_dir, ffmpeg, "")?;
    // 丢弃解码步（input 是占位符），只留编码步
    if let Some(enc) = enc_steps.pop() {
        let mut enc = enc;
        enc.stdin_from_prev = true;
        steps.push(enc);
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neroaac_steps() {
        let j = AudioJob { input: "a.wav".into(), output: "o.m4a".into(), bitrate: 128, ..Default::default() };
        let st = build_steps(&j, "T:\\", "T:\\ffmpeg.exe", "").unwrap();
        assert_eq!(st.len(), 2);
        assert!(st[0].args.contains(&"pipe:1".to_string()));
        assert!(st[1].stdin_from_prev);
        let joined = st[1].args.join(" ");
        assert!(joined.contains("-lc -br 128000"));
        assert!(joined.contains("-if - -of o.m4a"));
    }

    #[test]
    fn test_neroaac_he() {
        let j = AudioJob { input: "a.wav".into(), output: "o.m4a".into(), bitrate: 48, ..Default::default() };
        let st = build_steps(&j, "T:\\", "T:\\ffmpeg.exe", "").unwrap();
        assert!(st[1].args.contains(&"-he".to_string()));
    }

    #[test]
    fn test_qaac_env() {
        let j = AudioJob { input: "a.wav".into(), output: "o.m4a".into(), encoder: "qaac".into(), bitrate: 256, ..Default::default() };
        let st = build_steps(&j, "T:\\", "T:\\ffmpeg.exe", "").unwrap();
        assert!(st[1].env_path.iter().any(|p| p.ends_with("qtfiles")));
        assert!(st[1].args.contains(&"-c".to_string()) && st[1].args.contains(&"256".to_string()));
    }

    #[test]
    fn test_custom_mode() {
        let j = AudioJob { input: "a.wav".into(), output: "o.m4a".into(), encoder: "fdkaac".into(), mode: "custom".into(), custom: "-m 3".into(), ..Default::default() };
        let st = build_steps(&j, "T:\\", "T:\\ffmpeg.exe", "").unwrap();
        assert!(st[1].args.contains(&"-m".to_string()));
    }

    #[test]
    fn test_auto_ext() {
        assert_eq!(output_ext("lame"), "mp3");
        assert_eq!(output_ext("flac"), "flac");
        assert_eq!(output_ext("neroaac"), "m4a");
    }
}
