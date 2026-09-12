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

/// 音频压制计划：ffmpeg 解码到临时 wav → 编码器从文件读取。
/// 不用 stdin 管道：ffmpeg 7.x 的 WAV muxer 写管道收尾时报 EINVAL（数据已写完但退出码非 0），
/// 老编码器的 stdin 兼容性也参差——文件中转最稳（见 DEVLOG）。
pub fn build_steps(job: &AudioJob, tools_dir: &str, ffmpeg: &str, temp_base: &str) -> Result<Vec<Step>, String> {
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

    // 第 1 步：解码为临时 wav 文件
    let wav_tmp = format!("{}.wav", temp_base);
    let mut dec = Step::new("音频解码 (ffmpeg)", ffmpeg, vec![
        "-hide_banner".into(), "-y".into(), "-i".into(), job.input.clone(),
        "-vn".into(), "-f".into(), "wav".into(), wav_tmp.clone(),
    ]);
    dec.parser = Parser::Ffmpeg { duration: 0.0 };
    dec.temp_outputs.push(wav_tmp.clone());
    steps.push(dec);

    // 第 2 步：编码器读临时 wav
    let custom = job.mode == "custom" && !job.custom.trim().is_empty();
    let mut args: Vec<String> = Vec::new();
    let mut env_path = vec![tools_dir.to_string()];
    match job.encoder.as_str() {
        "neroaac" => {
            // neroAacEnc 要求 [options] 在 -if/-of 之前
            if custom {
                args.extend(split_args(&job.custom));
            } else {
                // ≤64Kbps 用 HE，其余 LC（对齐原版 preset 习惯）
                args.push(if job.bitrate <= 64 { "-he".into() } else { "-lc".into() });
                args.extend(["-br".into(), format!("{}", job.bitrate * 1000)]);
            }
            args.extend(["-if".into(), wav_tmp.clone(), "-of".into(), output.clone()]);
        }
        "qaac" => {
            // qaac 依赖 qtfiles 里的 Apple DLL
            env_path.push(std::path::Path::new(tools_dir).join("qtfiles").to_string_lossy().to_string());
            args.push("--ignorelength".into());
            if custom {
                args.extend(split_args(&job.custom));
            } else {
                args.extend(["-c".into(), job.bitrate.to_string()]);
            }
            args.extend([wav_tmp.clone(), "-o".into(), output.clone()]);
        }
        "fdkaac" => {
            if custom {
                args.extend(split_args(&job.custom));
            } else {
                args.extend(["-b".into(), job.bitrate.to_string()]);
            }
            args.extend(["-o".into(), output.clone(), wav_tmp.clone()]);
        }
        "lame" => {
            args.push("--silent".into());
            if custom {
                args.extend(split_args(&job.custom));
            } else {
                args.extend(["-b".into(), job.bitrate.to_string()]);
            }
            args.extend([wav_tmp.clone(), output.clone()]);
        }
        "flac" => {
            args.push("-s".into());
            if custom {
                args.extend(split_args(&job.custom));
            }
            args.extend(["-f".into(), wav_tmp.clone(), "-o".into(), output.clone()]);
        }
        _ => unreachable!(),
    }
    let title = format!("音频编码 {}", tool.trim_end_matches(".exe"));
    let mut enc = Step::new(&title, &tool_path, args);
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
    // NeroAAC / qaac / fdkaac：先合并到临时 wav 文件，再从文件编码
    let wav_tmp = format!("{}_merged.wav", output.trim_end_matches(output_ext(encoder)));
    let mut dec = Step::new("音频合并 (ffmpeg)", ffmpeg, args);
    dec.args.push(wav_tmp.clone());
    dec.parser = Parser::Ffmpeg { duration: 0.0 };
    dec.temp_outputs.push(wav_tmp.clone());

    let sub_job = AudioJob {
        input: wav_tmp.clone(),
        output: output.to_string(),
        encoder: encoder.to_string(),
        mode: "bitrate".into(),
        bitrate,
        custom: String::new(),
    };
    let mut steps = vec![dec];
    // 从 wav 文件走编码（build_steps 会先解码——但输入已是 wav，
    // 为避免重复解码，这里直接取它的编码步）
    let mut enc_steps = build_steps(&sub_job, tools_dir, ffmpeg, "")?;
    if let Some(enc) = enc_steps.pop() {
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
        let st = build_steps(&j, "T:\\", "T:\\ffmpeg.exe", "C:\\tmp\\j1").unwrap();
        assert_eq!(st.len(), 2);
        // 解码到临时 wav
        assert!(st[0].args.contains(&"C:\\tmp\\j1.wav".to_string()));
        assert!(st[0].args.contains(&"-f".to_string()) && st[0].args.contains(&"wav".to_string()));
        // 编码器从临时 wav 读，参数在前
        let joined = st[1].args.join(" ");
        assert!(joined.contains("-lc -br 128000 -if C:\\tmp\\j1.wav -of o.m4a"));
        assert!(!st[1].stdin_from_prev);
        // 临时 wav 会被清理
        assert!(st[0].temp_outputs.contains(&"C:\\tmp\\j1.wav".to_string()));
    }

    #[test]
    fn test_neroaac_he() {
        let j = AudioJob { input: "a.wav".into(), output: "o.m4a".into(), bitrate: 48, ..Default::default() };
        let st = build_steps(&j, "T:\\", "T:\\ffmpeg.exe", "C:\\tmp\\j1").unwrap();
        assert!(st[1].args.contains(&"-he".to_string()));
    }

    #[test]
    fn test_qaac_env() {
        let j = AudioJob { input: "a.wav".into(), output: "o.m4a".into(), encoder: "qaac".into(), bitrate: 256, ..Default::default() };
        let st = build_steps(&j, "T:\\", "T:\\ffmpeg.exe", "C:\\tmp\\j1").unwrap();
        assert!(st[1].env_path.iter().any(|p| p.ends_with("qtfiles")));
        let joined = st[1].args.join(" ");
        assert!(joined.contains("-c 256"));
        assert!(joined.contains("C:\\tmp\\j1.wav -o o.m4a"));
    }

    #[test]
    fn test_custom_mode() {
        let j = AudioJob { input: "a.wav".into(), output: "o.m4a".into(), encoder: "fdkaac".into(), mode: "custom".into(), custom: "-m 3".into(), ..Default::default() };
        let st = build_steps(&j, "T:\\", "T:\\ffmpeg.exe", "C:\\tmp\\j1").unwrap();
        assert!(st[1].args.contains(&"-m".to_string()));
    }

    #[test]
    fn test_auto_ext() {
        assert_eq!(output_ext("lame"), "mp3");
        assert_eq!(output_ext("flac"), "flac");
        assert_eq!(output_ext("neroaac"), "m4a");
    }

    #[test]
    fn test_merge_steps() {
        let inputs = vec!["a.m4a".to_string(), "b.m4a".to_string()];
        let st = build_merge_steps(&inputs, "o.m4a", "neroaac", 128, "T:\\", "T:\\ffmpeg.exe").unwrap();
        // 合并到临时 wav + nero 从文件编码
        assert!(st[0].args.contains(&"-filter_complex".to_string()));
        let joined = st[1].args.join(" ");
        assert!(joined.contains("-br 128000"));
    }
}
