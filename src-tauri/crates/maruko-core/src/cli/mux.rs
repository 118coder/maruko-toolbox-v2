use super::video::{mkvmerge_path, mp4box_path};
use super::{Parser, Plan, Step};

/// 合并为 MP4（MP4Box）或「替换音频」（ffmpeg 等效实现，见 D8）。
pub fn build_mux_mp4(
    video: &str,
    audio: Option<&str>,
    output: &str,
    fps: &str,       // "auto" | "23.976" | ...
    par: &str,       // "auto" | "1:1" | "16:9" ...
    replace_audio: bool,
    ffmpeg: &str,
    tools_dir: &str,
) -> Result<Plan, String> {
    if video.is_empty() {
        return Err("未选择视频".into());
    }
    if replace_audio {
        let a = audio.ok_or("替换音频需要选择音频文件")?;
        let mut s = Step::new("替换音频 (ffmpeg)", ffmpeg, vec![
            "-hide_banner".into(), "-y".into(),
            "-i".into(), video.to_string(),
            "-i".into(), a.to_string(),
            "-map".into(), "0:v:0".into(),
            "-map".into(), "1:a:0".into(),
            "-c".into(), "copy".into(),
            output.to_string(),
        ]);
        s.parser = Parser::Ffmpeg { duration: 0.0 };
        return Ok(Plan {
            title: "替换音频".into(),
            output: output.to_string(),
            steps: vec![s],
            shutdown_after: false,
        });
    }
    let mut args: Vec<String> = vec!["-add".into(), video.to_string()];
    if fps != "auto" && !fps.is_empty() {
        // 视频为文件时一般无需指定；仅当用户显式选择才覆盖
        args.push("-fps".into());
        args.push(fps.to_string());
    }
    if let Some(a) = audio {
        args.extend(["-add".into(), a.to_string()]);
    }
    if par != "auto" && !par.is_empty() {
        args.push("-par".into());
        args.push(format!("1={}", par));
    }
    args.extend(["-new".into(), output.to_string()]);
    Ok(Plan {
        title: "封装 MP4".into(),
        output: output.to_string(),
        steps: vec![Step::new("封装 MP4 (MP4Box)", &mp4box_path(tools_dir), args)],
        shutdown_after: false,
    })
}

/// 合并为 MKV（mkvmerge）。
pub fn build_mux_mkv(
    video: &str,
    audio: Option<&str>,
    subtitle: Option<&str>,
    output: &str,
    tools_dir: &str,
) -> Result<Plan, String> {
    if video.is_empty() {
        return Err("未选择视频".into());
    }
    let mut args: Vec<String> = vec!["-o".into(), output.to_string()];
    args.push(video.to_string());
    if let Some(a) = audio {
        if !a.is_empty() {
            args.push(a.to_string());
        }
    }
    if let Some(s) = subtitle {
        if !s.is_empty() {
            args.push("--language".into());
            args.push("0:und".into());
            args.push(s.to_string());
        }
    }
    let mut step = Step::new("封装 MKV (mkvmerge)", &mkvmerge_path(tools_dir), args);
    step.env_path.push(tools_dir.to_string());
    Ok(Plan {
        title: "封装 MKV".into(),
        output: output.to_string(),
        steps: vec![step],
        shutdown_after: false,
    })
}

/// 封装转换：容器转换 + 音频重编码为 AAC（flv 必需）。
pub fn build_convert(
    input: &str,
    output: &str,
    audio_encoder: &str, // aac | copy
    ffmpeg: &str,
) -> Result<Plan, String> {
    if ffmpeg.is_empty() {
        return Err("未找到 ffmpeg".into());
    }
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-y".into(), "-i".into(), input.to_string()];
    args.extend(["-c:v".into(), "copy".into()]);
    if audio_encoder == "copy" {
        args.extend(["-c:a".into(), "copy".into()]);
    } else {
        args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()]);
    }
    args.extend(["-map_metadata".into(), "0".into(), "-y".into(), output.to_string()]);
    let mut s = Step::new("封装转换", ffmpeg, args);
    s.parser = Parser::Ffmpeg { duration: 0.0 };
    Ok(Plan {
        title: "封装转换".into(),
        output: output.to_string(),
        steps: vec![s],
        shutdown_after: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mux_mp4_basic() {
        let p = build_mux_mp4("v.mp4", Some("a.m4a"), "o.mp4", "auto", "auto", false, "ff", "T:\\").unwrap();
        let args = &p.steps[0].args;
        assert!(args.windows(2).any(|w| w[0] == "-add" && w[1] == "v.mp4"));
        assert!(args.windows(2).any(|w| w[0] == "-add" && w[1] == "a.m4a"));
        assert!(args.windows(2).any(|w| w[0] == "-new" && w[1] == "o.mp4"));
        assert!(!args.contains(&"-par".to_string()));
    }

    #[test]
    fn test_mux_mp4_par_fps() {
        let p = build_mux_mp4("v.mp4", None, "o.mp4", "29.97", "16:9", false, "ff", "T:\\").unwrap();
        let args = &p.steps[0].args;
        assert!(args.contains(&"-fps".to_string()) && args.contains(&"29.97".to_string()));
        assert!(args.contains(&"-par".to_string()) && args.contains(&"1=16:9".to_string()));
    }

    #[test]
    fn test_replace_audio() {
        let p = build_mux_mp4("v.mp4", Some("a.m4a"), "o.mp4", "auto", "auto", true, "ff", "T:\\").unwrap();
        let joined = p.steps[0].args.join(" ");
        assert!(joined.contains("-map 0:v:0 -map 1:a:0 -c copy"));
    }

    #[test]
    fn test_mux_mkv() {
        let p = build_mux_mkv("v.mkv", Some("a.m4a"), Some("s.ass"), "o.mkv", "T:\\").unwrap();
        let args = &p.steps[0].args;
        assert!(args.windows(2).any(|w| w[0] == "-o" && w[1] == "o.mkv"));
        assert!(args.contains(&"v.mkv".to_string()) && args.contains(&"a.m4a".to_string()) && args.contains(&"s.ass".to_string()));
    }

    #[test]
    fn test_convert() {
        let p = build_convert("in.mp4", "out.flv", "aac", "ff").unwrap();
        let joined = p.steps[0].args.join(" ");
        assert!(joined.contains("-c:v copy -c:a aac -b:a 192k"));
        let p2 = build_convert("in.mp4", "out.mkv", "copy", "ff").unwrap();
        assert!(p2.steps[0].args.contains(&"copy".to_string()));
    }
}
