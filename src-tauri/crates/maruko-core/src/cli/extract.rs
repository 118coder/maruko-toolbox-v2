use super::video::mkvextract_path;
use super::{Parser, Plan, Step};
use std::path::Path;

/// 按音频 codec 决定抽取扩展名。
pub fn audio_ext(codec: &str) -> &'static str {
    match codec {
        "aac" => "aac",
        "mp3" => "mp3",
        "flac" => "flac",
        "vorbis" => "ogg",
        "opus" => "opus",
        _ => "mka",
    }
}

/// 按视频 codec 决定抽取方式：h264/hevc → 裸流（annexb），其余 → mka。
pub fn build_common_video(input: &str, out: &str, codec: &str, ffmpeg: &str) -> Result<Plan, String> {
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-y".into(), "-i".into(), input.to_string(), "-map".into(), "0:v:0".into(), "-c".into(), "copy".into()];
    match codec {
        "h264" => {
            args.extend(["-bsf:v".into(), "h264_mp4toannexb".into(), "-f".into(), "h264".into()]);
        }
        "hevc" => {
            args.extend(["-bsf:v".into(), "hevc_mp4toannexb".into(), "-f".into(), "hevc".into()]);
        }
        _ => {
            args.extend(["-f".into(), "matroska".into()]);
        }
    }
    args.push(out.to_string());
    let mut s = Step::new("抽取视频", ffmpeg, args);
    s.parser = Parser::Ffmpeg { duration: 0.0 };
    Ok(Plan { title: "抽取视频".into(), output: out.to_string(), steps: vec![s], shutdown_after: false })
}

pub fn build_common_audio(input: &str, out: &str, index: u32, ffmpeg: &str) -> Result<Plan, String> {
    let args = vec![
        "-hide_banner".into(), "-y".into(), "-i".into(), input.to_string(),
        "-map".into(), format!("0:a:{}", index),
        "-c".into(), "copy".into(),
        out.to_string(),
    ];
    let mut s = Step::new("抽取音频", ffmpeg, args);
    s.parser = Parser::Ffmpeg { duration: 0.0 };
    Ok(Plan { title: "抽取音频".into(), output: out.to_string(), steps: vec![s], shutdown_after: false })
}

/// FLV 抽取（FLVExtractCL 输出到源文件目录，与原版一致）。
pub fn build_flv(input: &str, kind: &str, tools_dir: &str) -> Result<Plan, String> {
    let exe = Path::new(tools_dir).join("FLVExtractCL.exe").to_string_lossy().to_string();
    let mut args = vec!["-o".into()];
    args.push(match kind {
        "video" => "-v".into(),
        _ => "-a".into(),
    });
    args.push(input.to_string());
    Ok(Plan {
        title: format!("抽取FLV{}", if kind == "video" { "视频" } else { "音频" }),
        output: String::new(),
        steps: vec![Step::new("FLVExtractCL", &exe, args)],
        shutdown_after: false,
    })
}

/// MKV 轨道抽取（mkvextract）。track 为 mkvmerge -J 输出的轨道序号。
pub fn build_mkv_track(input: &str, track: u32, out: &str, tools_dir: &str) -> Result<Plan, String> {
    let args = vec![
        "tracks".into(),
        input.to_string(),
        format!("{}:{}", track, out),
    ];
    let mut s = Step::new("抽取MKV轨道", &mkvextract_path(tools_dir), args);
    s.env_path.push(tools_dir.to_string());
    Ok(Plan { title: format!("抽取MKV轨道 {}", track), output: out.to_string(), steps: vec![s], shutdown_after: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_video_h264() {
        let p = build_common_video("in.mp4", "o.264", "h264", "ff").unwrap();
        let joined = p.steps[0].args.join(" ");
        assert!(joined.contains("-bsf:v h264_mp4toannexb -f h264"));
    }

    #[test]
    fn test_common_video_other() {
        let p = build_common_video("in.mkv", "o.mka", "vp9", "ff").unwrap();
        assert!(p.steps[0].args.contains(&"-f".to_string()) && p.steps[0].args.contains(&"matroska".to_string()));
    }

    #[test]
    fn test_common_audio() {
        let p = build_common_audio("in.mkv", "o.aac", 1, "ff").unwrap();
        assert!(p.steps[0].args.contains(&"0:a:1".to_string()));
    }

    #[test]
    fn test_flv() {
        let p = build_flv("in.flv", "video", "T:\\").unwrap();
        let a = &p.steps[0].args;
        assert!(a.contains(&"-v".to_string()) && a.contains(&"-o".to_string()));
        let p2 = build_flv("in.flv", "audio", "T:\\").unwrap();
        assert!(p2.steps[0].args.contains(&"-a".to_string()));
    }

    #[test]
    fn test_mkv_track() {
        let p = build_mkv_track("in.mkv", 2, "o.aac", "T:\\").unwrap();
        assert!(p.steps[0].args.contains(&"2:o.aac".to_string()));
    }

    #[test]
    fn test_audio_ext() {
        assert_eq!(audio_ext("aac"), "aac");
        assert_eq!(audio_ext("mp3"), "mp3");
        assert_eq!(audio_ext("dts"), "mka");
    }
}
