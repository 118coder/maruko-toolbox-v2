pub mod audio;
pub mod common;
pub mod extract;
pub mod mux;
pub mod video;

/// 进度解析器类型（jobs.rs 据此解析 stderr）。
#[derive(Debug, Clone)]
pub enum Parser {
    None,
    X264,
    X265,
    Ffmpeg { duration: f64 },
}

/// 计划中的一步。exe/args 均为最终值，jobs.rs 只负责执行与进度解析。
#[derive(Debug, Clone)]
pub struct Step {
    pub title: String,
    pub exe: String,
    pub args: Vec<String>,
    pub parser: Parser,
    /// stdin 接上一步的 stdout（音频管道用）
    pub stdin_from_prev: bool,
    /// 需要前置到子进程 PATH 的目录（qaac 的 qtfiles 等）
    pub env_path: Vec<String>,
    /// 成功后清理的临时文件
    pub temp_outputs: Vec<String>,
}

impl Step {
    pub fn new(title: &str, exe: &str, args: Vec<String>) -> Step {
        Step {
            title: title.to_string(),
            exe: exe.to_string(),
            args,
            parser: Parser::None,
            stdin_from_prev: false,
            env_path: Vec::new(),
            temp_outputs: Vec::new(),
        }
    }
}

/// 一个完整任务（可含多步，串行执行）。
#[derive(Debug, Clone)]
pub struct Plan {
    pub title: String,
    pub output: String,
    pub steps: Vec<Step>,
    pub shutdown_after: bool,
}

/// 类 shell 的参数拆分（支持引号），用于"自定义命令行"。
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote: Option<char> = None;
    let mut has_token = false;
    for c in s.chars() {
        match in_quote {
            Some(q) => {
                if c == q {
                    in_quote = None;
                } else {
                    cur.push(c);
                }
            }
            None => match c {
                '"' | '\'' => {
                    in_quote = Some(c);
                    has_token = true;
                }
                c if c.is_whitespace() => {
                    if has_token {
                        out.push(std::mem::take(&mut cur));
                        has_token = false;
                    }
                }
                c => {
                    cur.push(c);
                    has_token = true;
                }
            },
        }
    }
    if has_token {
        out.push(cur);
    }
    out
}

/// 带编码器标签的输出命名：源目录/名.扩展名 → 源目录/名 + 标签 + .扩展名
/// （对齐原版：测试.mp4 + x264 → 测试x264.mp4），重名追加 _2/_3…
pub fn tagged_output(input: &str, ext: &str, tag: &str) -> String {
    let p = std::path::Path::new(input);
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let mut candidate = dir.join(format!("{}{}.{}", stem, tag, ext));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{}{}_{}.{}", stem, tag, n, ext));
        n += 1;
    }
    candidate.to_string_lossy().to_string()
}

/// 已有明确输出路径时防覆盖：存在则追加 _2/_3…
pub fn deconflict(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let p = std::path::Path::new(path);
    if !p.exists() {
        return path.to_string();
    }
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = p.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
    let mut n = 2;
    loop {
        let cand = dir.join(format!("{}_{}.{}", stem, n, ext));
        if !cand.exists() {
            return cand.to_string_lossy().to_string();
        }
        n += 1;
    }
}

/// 输出文件自动命名：源目录同名 + 新扩展名，重名追加 _2/_3…（fs 部分，供 commands 调用）。
pub fn auto_output(input: &str, ext: &str, tag: &str) -> String {
    let p = std::path::Path::new(input);
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let suffix = if tag.is_empty() { String::new() } else { format!("_{}", tag) };
    let mut candidate = dir.join(format!("{}{}.{}", stem, suffix, ext));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{}{}_{}.{}", stem, suffix, n, ext));
        n += 1;
    }
    candidate.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_args() {
        assert_eq!(split_args("--crf 23.5 --tune anime"), vec!["--crf", "23.5", "--tune", "anime"]);
        assert_eq!(split_args("--stats \"my file.stat\" -p2"), vec!["--stats", "my file.stat", "-p2"]);
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("  a  'b c'  "), vec!["a", "b c"]);
    }

    #[test]
    fn test_auto_output() {
        assert_eq!(auto_output("D:\\a\\v.mp4", "mkv", ""), "D:\\a\\v.mkv");
    }
}
