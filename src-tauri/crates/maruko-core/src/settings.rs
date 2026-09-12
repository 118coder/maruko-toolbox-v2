use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 全部设置项（camelCase 直传前端）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub language: String,       // "zh-CN" | "en"
    pub splash: bool,           // 显示启动画面
    pub tray_mode: bool,        // 托盘模式（关闭=隐藏到托盘）
    pub dark: bool,             // 深色模式（V2 新增）
    pub x264_priority: String,  // idle|below|normal|above|high
    pub x264_threads: u32,      // 0 = auto
    pub x264_extra: String,     // x264/x265 自定义命令行（覆盖内置参数）
    pub enable_x265: bool,      // 启用x265（保留原版开关）
    pub preview_player: String, // 外部预览播放器路径
    pub delete_temp: bool,      // 退出/任务后删除临时文件
    pub check_update: bool,
    pub tools_dir: String,      // 工具链目录（空 = 自动定位）
    pub ffmpeg_path: String,    // GPU 编码用 ffmpeg（空 = 自动）
    pub update_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "zh-CN".into(),
            splash: true,
            tray_mode: false,
            dark: false,
            x264_priority: "normal".into(),
            x264_threads: 0,
            x264_extra: String::new(),
            enable_x265: true,
            preview_player: String::new(),
            delete_temp: true,
            check_update: true,
            tools_dir: String::new(),
            ffmpeg_path: String::new(),
            update_url: String::new(),
        }
    }
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.to_path_buf())
}

fn appdata_path() -> Option<PathBuf> {
    dirs_appdata().map(|d| d.join("MarukoV2").join("config.json"))
}

fn dirs_appdata() -> Option<PathBuf> {
    std::env::var("APPDATA").ok().map(PathBuf::from)
}

/// 便携优先：exe 旁 config.json，其次 %APPDATA%\MarukoV2\config.json。
pub fn config_path() -> Option<PathBuf> {
    if let Some(dir) = exe_dir() {
        let p = dir.join("config.json");
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(p) = appdata_path() {
        if p.exists() {
            return Some(p);
        }
    }
    None
}

pub fn load() -> Settings {
    if let Some(p) = config_path() {
        if let Ok(txt) = fs::read_to_string(&p) {
            if let Ok(s) = serde_json::from_str::<Settings>(&txt) {
                return s;
            }
        }
    }
    Settings::default()
}

/// 写入 exe 旁；失败（如 Program Files 无权限）则写 %APPDATA%，返回实际路径。
pub fn save(s: &Settings) -> Result<PathBuf, String> {
    let txt = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    if let Some(dir) = exe_dir() {
        let p = dir.join("config.json");
        if fs::write(&p, &txt).is_ok() {
            return Ok(p);
        }
    }
    let p = appdata_path().ok_or("无法确定配置目录")?;
    fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&p, &txt).map_err(|e| e.to_string())?;
    Ok(p)
}
