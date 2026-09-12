use crate::cli::{Parser, Plan, Step};
use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const IDLE: u32 = 0x0000_0040;
const BELOW_NORMAL: u32 = 0x0000_4000;
const NORMAL: u32 = 0x0000_0020;
const ABOVE_NORMAL: u32 = 0x0000_8000;
const HIGH: u32 = 0x0000_0080;

pub fn priority_flags(p: &str) -> u32 {
    match p {
        "idle" => IDLE,
        "below" => BELOW_NORMAL,
        "above" => ABOVE_NORMAL,
        "high" => HIGH,
        _ => NORMAL,
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: u32,
    pub pct: f64,
    pub frame: u64,
    pub total: u64,
    pub fps: f64,
    pub eta_s: f64,
    pub elapsed_s: f64,
    pub kbps: f64,
    pub size_mb: f64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JobState {
    pub id: u32,
    pub state: String, // queued|running|done|failed|cancelled
    pub title: String,
    pub output: String,
    pub msg: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub id: u32,
    pub line: String,
}

/// 任务事件（由 commands.rs 桥接到 Tauri event）。
pub enum Ev {
    Progress(Progress),
    State(JobState),
    Log(LogLine),
    QueueIdle { last_failed: bool },
}

pub type Emitter = Box<dyn Fn(Ev) + Send + Sync>;

/// 运行期设置快照（提交时确定）。
#[derive(Clone, Debug, Default)]
pub struct RunOpts {
    pub priority: String,
    pub delete_temp: bool,
}

struct QueueItem {
    id: u32,
    plan: Plan,
    opts: RunOpts,
}

struct Shared {
    queue: Mutex<VecDeque<QueueItem>>,
    cv: Condvar,
    next_id: AtomicU32,
    emitter: Arc<Emitter>,
    cancel: Arc<AtomicBool>,
    dead: Arc<AtomicBool>,
    running_pid: Mutex<Option<u32>>,
    /// 正在运行的任务 id；0 = 无
    running_id: AtomicU32,
}

trait EmitLog {
    fn emitter_log(&self, id: u32, line: &str);
}

impl EmitLog for Arc<Shared> {
    fn emitter_log(&self, id: u32, line: &str) {
        (self.emitter)(Ev::Log(LogLine { id, line: line.to_string() }));
    }
}

pub struct JobManager {
    shared: Arc<Shared>,
}

impl JobManager {
    pub fn new(emitter: Arc<Emitter>) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
            next_id: AtomicU32::new(1),
            emitter,
            cancel: Arc::new(AtomicBool::new(false)),
            dead: Arc::new(AtomicBool::new(false)),
            running_pid: Mutex::new(None),
            running_id: AtomicU32::new(0),
        });
        let sh = shared.clone();
        std::thread::Builder::new()
            .name("job-worker".into())
            .spawn(move || worker_loop(sh))
            .expect("启动任务线程失败");
        JobManager { shared }
    }

    pub fn submit(&self, plan: Plan, opts: RunOpts) -> u32 {
        let id = self.shared.next_id.fetch_add(1, Ordering::SeqCst);
        (self.shared.emitter)(Ev::State(JobState {
            id,
            state: "queued".into(),
            title: plan.title.clone(),
            output: plan.output.clone(),
            msg: String::new(),
        }));
        self.shared.queue.lock().unwrap().push_back(QueueItem { id, plan, opts });
        self.shared.cv.notify_one();
        id
    }

    /// 取消：排队中的按 id 移除；运行中的杀整个进程树。
    pub fn cancel(&self, id: u32) {
        {
            let mut q = self.shared.queue.lock().unwrap();
            if let Some(pos) = q.iter().position(|it| it.id == id) {
                q.remove(pos);
                drop(q);
                (self.shared.emitter)(Ev::State(JobState {
                    id,
                    state: "cancelled".into(),
                    title: String::new(),
                    output: String::new(),
                    msg: "任务已从队列移除".into(),
                }));
                return;
            }
        }
        if self.shared.running_id.load(Ordering::SeqCst) == id {
            self.shared.cancel.store(true, Ordering::SeqCst);
            self.shared.dead.store(true, Ordering::SeqCst);
            if let Some(pid) = *self.shared.running_pid.lock().unwrap() {
                kill_tree(pid);
            }
        }
    }
}

pub fn kill_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags_cfg(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

pub fn cancel_shutdown() {
    let _ = Command::new("shutdown")
        .arg("/a")
        .creation_flags_cfg(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

trait CreationFlagsCfg {
    fn creation_flags_cfg(&mut self, flags: u32) -> &mut Self;
}

#[cfg(windows)]
impl CreationFlagsCfg for Command {
    fn creation_flags_cfg(&mut self, flags: u32) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(flags)
    }
}

#[cfg(not(windows))]
impl CreationFlagsCfg for Command {
    fn creation_flags_cfg(&mut self, _flags: u32) -> &mut Self {
        self
    }
}

fn emit_state(em: &Arc<Emitter>, id: u32, state: &str, plan: &Plan, msg: &str) {
    em(Ev::State(JobState {
        id,
        state: state.into(),
        title: plan.title.clone(),
        output: plan.output.clone(),
        msg: msg.into(),
    }));
}

fn worker_loop(sh: Arc<Shared>) {
    loop {
        let item = {
            let mut q = sh.queue.lock().unwrap();
            loop {
                if let Some(it) = q.pop_front() {
                    break it;
                }
                q = sh.cv.wait(q).unwrap();
            }
        };
        let id = item.id;
        let plan = item.plan;
        let opts = item.opts;
        sh.cancel.store(false, Ordering::SeqCst);
        sh.dead.store(false, Ordering::SeqCst);
        sh.running_id.store(id, Ordering::SeqCst);
        emit_state(&sh.emitter, id, "running", &plan, "");
        crate::logs::write(&format!("任务 #{} 开始：{}", id, plan.title));

        let ok = run_plan(id, &plan, &sh, &opts);
        let state = if sh.cancel.load(Ordering::SeqCst) {
            "cancelled"
        } else if ok {
            "done"
        } else {
            "failed"
        };
        emit_state(&sh.emitter, id, state, &plan, "");
        crate::logs::write(&format!("任务 #{} 结束：{}", id, state));

        // 临时文件清理
        if opts.delete_temp && state != "cancelled" {
            for step in &plan.steps {
                for t in &step.temp_outputs {
                    let _ = std::fs::remove_file(t);
                }
            }
        }
        sh.running_id.store(0, Ordering::SeqCst);

        let failed = state == "failed";
        if sh.queue.lock().unwrap().is_empty() {
            (sh.emitter)(Ev::QueueIdle { last_failed: failed });
            if plan.shutdown_after && !failed {
                arm_shutdown(&sh.emitter);
            }
        }
    }
}

fn arm_shutdown(em: &Arc<Emitter>) {
    let spawned = Command::new("shutdown")
        .args(["/s", "/t", "60"])
        .creation_flags_cfg(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok();
    crate::logs::write("自动关机已布置（60 秒）");
    if spawned {
        em(Ev::State(JobState {
            id: 0,
            state: "shutdown".into(),
            title: "自动关机".into(),
            output: String::new(),
            msg: "60 秒后关机，可在进度窗点击「取消关机」撤销".into(),
        }));
    }
}

fn run_plan(id: u32, plan: &Plan, sh: &Arc<Shared>, opts: &RunOpts) -> bool {
    // 按 stdin_from_prev 把步骤切成若干"管道链"，链内并发启动（shell 语义），链间串行
    let mut i = 0;
    while i < plan.steps.len() {
        let mut j = i + 1;
        while j < plan.steps.len() && plan.steps[j].stdin_from_prev {
            j += 1;
        }
        if !run_chain(id, &plan.steps[i..j], sh, opts) {
            return false;
        }
        i = j;
    }
    true
}

fn run_chain(id: u32, steps: &[Step], sh: &Arc<Shared>, opts: &RunOpts) -> bool {
    use std::process::{Child, ChildStdout};
    let n = steps.len();
    if n == 1 {
        return run_single(id, &steps[0], None, sh, opts);
    }
    sh.emitter_log(id, &format!("▶ 管道链：{} 步并发启动", n));

    let dead: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let mut children: Vec<Child> = Vec::new();
    let mut stderr_handles: Vec<std::thread::JoinHandle<()>> = Vec::new();
    let mut prev_stdout: Option<ChildStdout> = None;

    for (k, step) in steps.iter().enumerate() {
        let mut cmd = Command::new(&step.exe);
        cmd.args(&step.args);
        if k == 0 {
            cmd.stdin(Stdio::null());
        } else {
            let prev = prev_stdout.take().expect("链上 stdout 缺失");
            cmd.stdin(Stdio::from(prev));
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        if !step.env_path.is_empty() {
            let old = std::env::var("PATH").unwrap_or_default();
            let mut np = step.env_path.join(";");
            np.push(';');
            np.push_str(&old);
            cmd.env("PATH", np);
        }
        cmd.creation_flags_cfg(CREATE_NO_WINDOW | priority_flags(&opts.priority));
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                sh.emitter_log(id, &format!("✗ 启动失败：{}（{}）", e, step.exe));
                for c in &children {
                    kill_tree(c.id());
                }
                return false;
            }
        };
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let em = sh.emitter.clone();
        let parser = step.parser.clone();
        let t0 = Instant::now();
        let dead_err = dead.clone();
        stderr_handles.push(std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for chunk in ChunkIter::new(reader) {
                if dead_err.load(Ordering::SeqCst) {
                    return;
                }
                for line in chunk.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Some(mut p) = parse_progress(&parser, &line, t0.elapsed().as_secs_f64()) {
                        p.id = id;
                        em(Ev::Progress(p));
                    }
                    em(Ev::Log(LogLine { id, line: line.to_string() }));
                }
            }
        }));

        if k + 1 < n {
            // 中间步骤：stdout 留给下一步的 stdin
            prev_stdout = Some(stdout);
        } else {
            // 最后一步：stdout 由日志线程排空
            let dead_out = dead.clone();
            let em2 = sh.emitter.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    if dead_out.load(Ordering::SeqCst) {
                        return;
                    }
                    match line {
                        Ok(l) if !l.trim().is_empty() => em2(Ev::Log(LogLine { id, line: l })),
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            });
        }
        children.push(child);
    }

    let pids: Vec<u32> = children.iter().map(|c| c.id()).collect();
    *sh.running_pid.lock().unwrap() = Some(pids[0]);

    let mut all_ok = true;
    for (idx, child) in children.iter_mut().enumerate() {
        let st = loop {
            match child.try_wait() {
                Ok(Some(st)) => break Some(st),
                Ok(None) => {
                    if sh.cancel.load(Ordering::SeqCst) {
                        for pid in &pids {
                            kill_tree(*pid);
                        }
                        let _ = child.wait();
                        break None;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(150));
                }
                Err(_) => break None,
            }
        };
        match st {
            Some(s) if s.success() => {}
            _ => {
                if !sh.cancel.load(Ordering::SeqCst) {
                    sh.emitter_log(id, &format!("✗ [{}] 失败（退出码非 0）", steps[idx].title));
                }
                all_ok = false;
                break;
            }
        }
    }

    dead.store(true, Ordering::SeqCst);
    for h in stderr_handles {
        let _ = h.join();
    }
    *sh.running_pid.lock().unwrap() = None;
    all_ok
}

fn run_single(id: u32, step: &Step, stdin: Option<std::process::ChildStdout>, sh: &Arc<Shared>, opts: &RunOpts) -> bool {
    let mut cmd = Command::new(&step.exe);
    cmd.args(&step.args);
    if let Some(prev) = stdin {
        cmd.stdin(Stdio::from(prev));
    } else {
        cmd.stdin(Stdio::null());
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    if !step.env_path.is_empty() {
        let old = std::env::var("PATH").unwrap_or_default();
        let mut np = step.env_path.join(";");
        np.push(';');
        np.push_str(&old);
        cmd.env("PATH", np);
    }
    cmd.creation_flags_cfg(CREATE_NO_WINDOW | priority_flags(&opts.priority));
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            (sh.emitter)(Ev::Log(LogLine { id, line: format!("✗ 启动失败：{}（{}）", e, step.exe) }));
            return false;
        }
    };
    *sh.running_pid.lock().unwrap() = Some(child.id());
    let pid = child.id();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let dead = Arc::new(AtomicBool::new(false));
    let dead_err = dead.clone();
    let em1 = sh.emitter.clone();
    let parser = step.parser.clone();
    let t0 = Instant::now();
    let h_err = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for chunk in ChunkIter::new(reader) {
            if dead_err.load(Ordering::SeqCst) {
                return;
            }
            for line in chunk.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Some(mut p) = parse_progress(&parser, &line, t0.elapsed().as_secs_f64()) {
                    p.id = id;
                    em1(Ev::Progress(p));
                }
                em1(Ev::Log(LogLine { id, line: line.to_string() }));
            }
        }
    });
    let dead2 = dead.clone();
    let em2 = sh.emitter.clone();
    let h_out = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if dead2.load(Ordering::SeqCst) {
                return;
            }
            match line {
                Ok(l) if !l.trim().is_empty() => em2(Ev::Log(LogLine { id, line: l })),
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if sh.cancel.load(Ordering::SeqCst) {
                    kill_tree(pid);
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            Err(_) => break None,
        }
    };
    dead.store(true, Ordering::SeqCst);
    let _ = h_err.join();
    let _ = h_out.join();
    *sh.running_pid.lock().unwrap() = None;
    match status {
        Some(st) if st.success() => {
            (sh.emitter)(Ev::Log(LogLine { id, line: format!("✓ [{}] 完成", step.title) }));
            true
        }
        _ => {
            if !sh.cancel.load(Ordering::SeqCst) {
                (sh.emitter)(Ev::Log(LogLine { id, line: format!("✗ [{}] 失败（退出码非 0），查看上方日志定位原因", step.title) }));
            }
            false
        }
    }
}

/// 把 stderr 流按 \r / \n 分片（编码器用 \r 原地刷新进度）。
struct ChunkIter<R: std::io::Read> {
    r: BufReader<R>,
    pending: Vec<u8>,
    done: bool,
}

impl<R: std::io::Read> ChunkIter<R> {
    fn new(r: BufReader<R>) -> Self {
        ChunkIter { r, pending: Vec::new(), done: false }
    }
}

impl<R: std::io::Read> Iterator for ChunkIter<R> {
    type Item = String;
    fn next(&mut self) -> Option<String> {
        if self.done {
            return None;
        }
        let mut out: Vec<u8> = std::mem::take(&mut self.pending);
        let mut byte = [0u8; 1];
        loop {
            use std::io::Read;
            match self.r.read(&mut byte) {
                Ok(0) => {
                    self.done = true;
                    break;
                }
                Ok(_) => {
                    let b = byte[0];
                    if b == b'\r' || b == b'\n' {
                        self.pending.clear();
                        break;
                    }
                    out.push(b);
                }
                Err(_) => {
                    self.done = true;
                    break;
                }
            }
        }
        if out.is_empty() && self.done {
            return None;
        }
        Some(String::from_utf8_lossy(&out).to_string())
    }
}

/// 解析单行进度（纯函数）。
pub fn parse_progress(parser: &Parser, line: &str, elapsed: f64) -> Option<Progress> {
    let mut p = Progress {
        id: 0,
        pct: 0.0,
        frame: 0,
        total: 0,
        fps: 0.0,
        eta_s: 0.0,
        elapsed_s: elapsed,
        kbps: 0.0,
        size_mb: 0.0,
    };
    match parser {
        Parser::None => return None,
        Parser::X264 => {
            let a: Vec<&str> = line.split_whitespace().collect();
            for (i, w) in a.iter().enumerate() {
                if *w == "frames:" && i + 1 < a.len() {
                    let fr: Vec<&str> = a[i + 1].split('/').collect();
                    if fr.len() == 2 {
                        p.frame = fr[0].parse().unwrap_or(0);
                        p.total = fr[1].parse().unwrap_or(0);
                    }
                }
                if *w == "fps," && i > 0 {
                    p.fps = a[i - 1].parse().unwrap_or(0.0);
                }
                if *w == "kbps," && i > 0 {
                    p.kbps = a[i - 1].parse().unwrap_or(0.0);
                }
                if *w == "eta" && i + 1 < a.len() {
                    p.eta_s = hms_to_secs_f(a[i + 1]);
                }
            }
        }
        Parser::X265 => {
            let a: Vec<&str> = line.split_whitespace().collect();
            for (i, w) in a.iter().enumerate() {
                if w.ends_with("frames,") && i > 0 {
                    let fr: Vec<&str> = a[i - 1].split('/').collect();
                    if fr.len() == 2 {
                        p.frame = fr[0].parse().unwrap_or(0);
                        p.total = fr[1].parse().unwrap_or(0);
                    }
                }
                if *w == "fps," && i > 0 {
                    p.fps = a[i - 1].parse().unwrap_or(0.0);
                }
                if *w == "kb/s" && i > 0 {
                    p.kbps = a[i - 1].parse().unwrap_or(0.0);
                }
            }
        }
        Parser::Ffmpeg { duration } => {
            let t = line.split("time=").nth(1)?;
            let t = t.split_whitespace().next().unwrap_or("");
            let secs = hms_to_secs_f(t);
            p.frame = (secs * 1000.0) as u64;
            if *duration > 0.0 {
                p.total = (*duration * 1000.0) as u64;
                p.pct = (secs / duration * 100.0).min(100.0);
                if p.pct > 0.1 && elapsed > 0.0 {
                    p.eta_s = elapsed * (100.0 - p.pct) / p.pct;
                }
                p.fps = if elapsed > 0.0 { secs / elapsed } else { 0.0 };
            } else {
                return Some(p);
            }
        }
    }
    if p.total > 0 && p.pct == 0.0 {
        p.pct = (p.frame as f64 / p.total as f64 * 100.0).min(100.0);
        // 未解析到 eta 时用 fps 估算
        if p.eta_s == 0.0 && p.fps > 0.0 && p.frame < p.total {
            p.eta_s = (p.total - p.frame) as f64 / p.fps;
        }
    }
    if p.total == 0 && p.pct == 0.0 && p.frame == 0 {
        return None;
    }
    Some(p)
}

fn hms_to_secs_f(s: &str) -> f64 {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        3 => {
            parts[0].parse::<f64>().unwrap_or(0.0) * 3600.0
                + parts[1].parse::<f64>().unwrap_or(0.0) * 60.0
                + parts[2].parse::<f64>().unwrap_or(0.0)
        }
        2 => parts[0].parse::<f64>().unwrap_or(0.0) * 60.0 + parts[1].parse::<f64>().unwrap_or(0.0),
        _ => s.parse().unwrap_or(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_x265() {
        let p = parse_progress(&Parser::X265, "  240/1440 frames, 45.67 fps, 2340.5 kb/s", 5.0).unwrap();
        assert_eq!(p.frame, 240);
        assert_eq!(p.total, 1440);
        assert!((p.fps - 45.67).abs() < 0.01);
        assert!((p.pct - 16.666).abs() < 0.1);
        assert!(p.eta_s > 0.0);
    }

    #[test]
    fn test_parse_x264() {
        let p = parse_progress(
            &Parser::X264,
            "frames: 120/1440 (8%) 34.5 fps, 45.6 kbps, eta 0:00:36",
            3.0,
        )
        .unwrap();
        assert_eq!(p.frame, 120);
        assert_eq!(p.total, 1440);
        assert!((p.fps - 34.5).abs() < 0.01);
        assert!((p.kbps - 45.6).abs() < 0.01);
        assert!((p.eta_s - 36.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_ffmpeg() {
        let p = parse_progress(&Parser::Ffmpeg { duration: 100.0 }, "frame= 250 fps=50 q=-1.0 time=00:00:50.00", 1.0).unwrap();
        assert!((p.pct - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_ffmpeg_no_duration() {
        let p = parse_progress(&Parser::Ffmpeg { duration: 0.0 }, "frame= 10 time=00:00:01.00", 1.0).unwrap();
        assert_eq!(p.pct, 0.0);
    }

    #[test]
    fn test_parse_none() {
        assert!(parse_progress(&Parser::None, "anything", 1.0).is_none());
    }

    #[test]
    fn test_priority_flags() {
        assert_eq!(priority_flags("normal"), NORMAL);
        assert_eq!(priority_flags("idle"), IDLE);
        assert_eq!(priority_flags("below"), BELOW_NORMAL);
        assert_eq!(priority_flags("above"), ABOVE_NORMAL);
        assert_eq!(priority_flags("high"), HIGH);
    }

    #[test]
    fn test_hms() {
        assert!((hms_to_secs_f("0:00:36") - 36.0).abs() < 0.001);
        assert!((hms_to_secs_f("00:01:02.5") - 62.5).abs() < 0.001);
    }
}
