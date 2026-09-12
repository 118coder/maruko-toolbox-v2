//! 小丸工具箱V2现代版 · 纯逻辑核心
//!
//! 本 crate 不依赖任何 UI/进程框架，只做三类事：
//! 1. CLI 生成器（纯函数：任务结构体 → argv）+ AVS 脚本生成
//! 2. 任务调度（进程队列、进度解析）
//! 3. 环境探测（工具链定位、GPU 编码器探测、MediaInfo FFI、设置持久化、日志）
//!
//! 解耦规则：本 crate 与上层 GUI 壳（src-tauri 根包）之间唯一的接口是
//! 结构体参数与 `Plan/Step`，见 docs/ARCHITECTURE.md。

pub mod avs;
pub mod cli;
pub mod gpu;
pub mod jobs;
pub mod locator;
pub mod logs;
pub mod mediainfo;
pub mod settings;
