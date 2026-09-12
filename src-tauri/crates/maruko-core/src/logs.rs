use chrono::Local;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG_LOCK: Mutex<()> = Mutex::new(());

fn log_dir() -> PathBuf {
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        let p = dir.join("logs");
        if fs::create_dir_all(&p).is_ok() {
            return p;
        }
    }
    let p = std::env::var("APPDATA")
        .map(|d| PathBuf::from(d).join("MarukoV2").join("logs"))
        .unwrap_or_else(|_| PathBuf::from("logs"));
    let _ = fs::create_dir_all(&p);
    p
}

pub fn log_file() -> PathBuf {
    log_dir().join("maruko-v2.log")
}

pub fn write(msg: &str) {
    let _g = LOG_LOCK.lock().unwrap();
    let path = log_file();
    // 超过 5MB 轮转为 .old
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > 5 * 1024 * 1024 {
            let _ = fs::rename(&path, path.with_extension("log.old"));
        }
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{}] {}", Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
    }
}

pub fn read_all() -> Vec<String> {
    let mut out = Vec::new();
    let path = log_file();
    if let Ok(txt) = fs::read_to_string(&path) {
        out.extend(txt.lines().map(|s| s.to_string()));
    }
    let old = path.with_extension("log.old");
    if let Ok(txt) = fs::read_to_string(&old) {
        out.splice(..0, txt.lines().map(|s| s.to_string()));
    }
    out
}

pub fn delete_all() {
    let _g = LOG_LOCK.lock().unwrap();
    let path = log_file();
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(path.with_extension("log.old"));
}
