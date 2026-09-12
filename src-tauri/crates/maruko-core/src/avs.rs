/// AVS 脚本生成（纯函数，不碰文件系统）。
/// 顺序（见 docs/DECISIONS.md D6）：Source → Crop → LanczosResize → AddBorders
/// → Tweak → Levels → Sharpen → Undot → Trim → TextSub

#[derive(Debug, Clone, Default)]
pub struct AvsFilters {
    /// (色度 hue, 饱和度 sat, 亮度 bright, 对比度 cont)
    pub tweak: Option<(f64, f64, f64, f64)>,
    /// (宽, 高)
    pub resize: Option<(u32, u32)>,
    /// (左, 上, 右, 下) 黑边
    pub borders: Option<(u32, u32, u32, u32)>,
    /// 用户输入的裁剪串（"l,t,-r,-b" 或 "l,t,w,h" 或 "l-t"）
    pub crop: Option<String>,
    /// (起始帧, 结束帧)；结束帧 0 = 不限
    pub trim: Option<(u32, u32)>,
    /// Levels 亮度 gamma
    pub levels: Option<f64>,
    /// Sharpen 锐化
    pub sharpen: Option<f64>,
    pub undot: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Demuxer {
    Lsmash,
    Ffms2,
}

fn num(v: f64) -> String {
    if (v - v.round()).abs() < f64::EPSILON {
        format!("{}", v.round() as i64)
    } else {
        format!("{}", v)
    }
}

/// Crop 输入解析：
/// - "8,0,-8,0"  → 左8 上0 右(边)8 下(边)0
/// - "8,0,960,540" → 左8 上0 目标宽960 高540（需要源宽高）
/// - "8-0"       → 左8 上0（右下不裁）
pub fn crop_line(input: &str, src_w: u32, src_h: u32) -> Option<String> {
    let s = input.trim().replace('，', ",");
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    let parse = |v: &str| -> Option<i64> { v.parse::<i64>().ok() };
    match parts.len() {
        4 => {
            let l = parse(parts[0])?;
            let t = parse(parts[1])?;
            let (l, t) = (l.max(0) as u32, t.max(0) as u32);
            let (r, b) = (parse(parts[2])?, parse(parts[3])?);
            // 以符号判断模式（-0 也算边距）：带负号 = 边距，全正 = 目标宽高
            let margin_mode = parts[2].trim_start().starts_with('-')
                || parts[3].trim_start().starts_with('-');
            let (w, h) = if margin_mode {
                (
                    src_w.saturating_sub(l).saturating_sub(r.unsigned_abs() as u32),
                    src_h.saturating_sub(t).saturating_sub(b.unsigned_abs() as u32),
                )
            } else {
                // 正数 = 目标宽高；源尺寸未知时无法计算
                if src_w == 0 || src_h == 0 {
                    return None;
                }
                (r.max(0) as u32, b.max(0) as u32)
            };
            if w == 0 || h == 0 {
                return None;
            }
            Some(format!("Crop({}, {}, {}, {})", l, t, w, h))
        }
        2 => {
            let l = parse(parts[0])?.max(0) as u32;
            let t = parse(parts[1])?.max(0) as u32;
            let w = src_w.saturating_sub(l);
            let h = src_h.saturating_sub(t);
            if w == 0 || h == 0 {
                return None;
            }
            Some(format!("Crop({}, {}, {}, {})", l, t, w, h))
        }
        1 => {
            // "8-0" 形式：左-上
            let ab: Vec<&str> = s.split('-').collect();
            if ab.len() == 2 {
                let l = parse(ab[0])?.max(0) as u32;
                let t = parse(ab[1])?.max(0) as u32;
                let w = src_w.saturating_sub(l);
                let h = src_h.saturating_sub(t);
                if w == 0 || h == 0 {
                    return None;
                }
                Some(format!("Crop({}, {}, {}, {})", l, t, w, h))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 生成完整 AVS 脚本。`extra_lines` 为外置滤镜插入的行。
/// `src_w/src_h` 为源分辨率（探测失败传 0，此时仅支持边距式裁剪的左/上）。
pub fn build_script(
    video_path: &str,
    subtitle_path: Option<&str>,
    demuxer: Demuxer,
    filters: &AvsFilters,
    tools_dir: &str,
    avs_dir: &str, // tools\avs（含 plugins 子目录）
    extra_lines: &str,
    start_frame: u32,
    frames: u32,
    src_w: u32,
    src_h: u32,
) -> String {
    let mut s = String::new();
    let plugins = format!("{}\\plugins", avs_dir);
    match demuxer {
        Demuxer::Lsmash => {
            s.push_str(&format!("LoadPlugin(\"{}\\LSMASHSource.dll\")\n", plugins.replace('\\', "\\\\")));
            s.push_str(&format!("LWLibavVideoSource(\"{}\", cache = false)\n", video_path.replace('"', "")));
        }
        Demuxer::Ffms2 => {
            s.push_str(&format!("LoadPlugin(\"{}\\ffms2.dll\")\n", plugins.replace('\\', "\\\\")));
            s.push_str(&format!("FFVideoSource(\"{}\", threads = 1)\n", video_path.replace('"', "")));
        }
    }
    // Crop（先裁剪再缩放）
    if let Some(c) = &filters.crop {
        if let Some(line) = crop_line(c, src_w, src_h) {
            s.push_str(&line);
            s.push('\n');
        }
    }
    if let Some((w, h)) = filters.resize {
        if w > 0 && h > 0 {
            s.push_str(&format!("LanczosResize({}, {})\n", w, h));
        }
    }
    if let Some((l, t, r, b)) = filters.borders {
        if l + t + r + b > 0 {
            s.push_str(&format!("AddBorders({}, {}, {}, {})\n", l, t, r, b));
        }
    }
    if let Some((hue, sat, bright, cont)) = filters.tweak {
        s.push_str(&format!(
            "Tweak(hue={}, sat={}, bright={}, cont={})\n",
            num(hue), num(sat), num(bright), num(cont)
        ));
    }
    if let Some(g) = filters.levels {
        s.push_str(&format!("Levels(0, {}, 255, 0, 255)\n", num(g)));
    }
    if let Some(a) = filters.sharpen {
        s.push_str(&format!("Sharpen({})\n", num(a)));
    }
    if filters.undot {
        s.push_str("Undot()\n");
    }
    // Trim：起始帧 + 编码帧数
    if let Some((a, b)) = filters.trim {
        if b > a {
            s.push_str(&format!("Trim({}, {})\n", a, b));
        }
    } else if start_frame > 0 || frames > 0 {
        if frames > 0 {
            s.push_str(&format!("Trim({}, {})\n", start_frame, start_frame + frames - 1));
        } else if start_frame > 0 {
            s.push_str(&format!("Trim({}, 0)\n", start_frame));
        }
    }
    // 外置滤镜行
    if !extra_lines.trim().is_empty() {
        s.push_str(extra_lines.trim());
        s.push('\n');
    }
    // 字幕烧录（最后）
    if let Some(sub) = subtitle_path {
        if !sub.is_empty() {
            if !tools_dir.is_empty() {
                s.push_str(&format!("LoadPlugin(\"{}\\VSFilter.dll\")\n", tools_dir.replace('\\', "\\\\")));
            }
            s.push_str(&format!("TextSub(\"{}\")\n", sub.replace('"', "")));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crop_margins() {
        assert_eq!(crop_line("8,0,-8,0", 1920, 1080).unwrap(), "Crop(8, 0, 1904, 1080)");
    }
    #[test]
    fn test_crop_sizes() {
        assert_eq!(crop_line("8,0,960,540", 1920, 1080).unwrap(), "Crop(8, 0, 960, 540)");
    }
    #[test]
    fn test_crop_dash() {
        assert_eq!(crop_line("8-0", 1920, 1080).unwrap(), "Crop(8, 0, 1912, 1080)");
    }
    #[test]
    fn test_crop_full_syntax() {
        // 左,上,-右,-下 全裁
        assert_eq!(crop_line("0,140,0,-140", 1920, 1080).unwrap(), "Crop(0, 140, 1920, 800)");
    }

    #[test]
    fn test_script_minimal() {
        let f = AvsFilters::default();
        let s = build_script("D:\\v.mp4", None, Demuxer::Lsmash, &f, "T:\\", "T:\\avs", "", 0, 0, 1920, 1080);
        assert!(s.contains("LWLibavVideoSource(\"D:\\v.mp4\", cache = false)"));
        assert!(s.starts_with("LoadPlugin("));
    }

    #[test]
    fn test_script_trim_and_sub() {
        let f = AvsFilters { undot: true, ..Default::default() };
        let s = build_script("v.mp4", Some("s.ass"), Demuxer::Ffms2, &f, "T:\\", "T:\\avs", "Deblock()", 100, 50, 0, 0);
        assert!(s.contains("FFVideoSource(\"v.mp4\", threads = 1)"));
        assert!(s.contains("Trim(100, 149)"));
        assert!(s.contains("Undot()"));
        assert!(s.contains("TextSub(\"s.ass\")"));
        assert!(s.contains("Deblock()"));
        // 字幕必须在最后
        assert!(s.trim_end().ends_with("TextSub(\"s.ass\")"));
    }

    #[test]
    fn test_crop_with_real_dims() {
        let f = AvsFilters { crop: Some("0,140,0,-140".into()), ..Default::default() };
        let s = build_script("v.mp4", None, Demuxer::Lsmash, &f, "T:\\", "T:\\avs", "", 0, 0, 1920, 1080);
        assert!(s.contains("Crop(0, 140, 1920, 800)"));
    }
}
