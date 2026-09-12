# 架构

## 总览

```
┌────────────────────────── WebView2 (GPU 合成) ──────────────────────────┐
│  ui/index.html                                                          │
│  ├─ css/theme.css        设计令牌（明/暗主题 CSS 变量）                    │
│  ├─ js/components.js     通用控件（NumBox/FileField/ProgressModal/Toast）│
│  ├─ js/bridge.js         ★唯一允许 import window.__TAURI__ 的模块         │
│  ├─ js/i18n.js           界面语言字典（zh-CN / en）                       │
│  ├─ js/state.js          全局状态 + 设置持久化（防抖保存）                 │
│  └─ js/tabs/*.js         九个标签页模块，互不引用，只依赖 components/bridge│
└───────────────▲────────────────────────────┬───────────────────────────┘
        invoke / event（JSON）                 │
┌───────────────┴────────────────────────────▼───────────────────────────┐
│  src-tauri/src                                                          │
│  ├─ main.rs / lib.rs     入口：托盘、启动画面切换、插件注册                │
│  ├─ commands.rs          ★命令薄层：参数校验 → 调用下层 → 事件桥接        │
│  ├─ settings.rs          配置持久化（便携优先，回退 %APPDATA%）           │
│  ├─ locator.rs           工具链定位（设置路径 → 注册表 → 常见路径 → PATH）│
│  ├─ gpu.rs               硬件编码器运行时探测（试编码 0.3s）              │
│  ├─ cli/*                ★★纯函数 CLI 生成器（无 IO，#[cfg(test)] 全覆盖）│
│  ├─ avs.rs               AVS 脚本生成（纯函数）                          │
│  ├─ jobs.rs              任务队列：串行执行、进度解析、取消、自动关机      │
│  ├─ mediainfo.rs         MediaInfo.dll (x64) libloading FFI + ffprobe 回退│
│  └─ logs.rs              日志写入 / 查看 / 删除                           │
└────────────────────────────────────────────────────────────────────────┘
                                     │ spawn（CREATE_NO_WINDOW + 优先级类别）
                     原版 tools\：x264 / x265 / avs4x26x / ffmpeg / MP4Box /
                     mkvmerge / mkvextract / FLVExtractCL / neroAacEnc / …
```

## 解耦规则（强制）

1. **前端只经 bridge.js 触达 Tauri**：任何 tab 模块不得直接使用 `window.__TAURI__`。替换运行时（如换成浏览器 + 本地服务）只需重写 bridge.js。
2. **后端命令薄层**：commands.rs 只做参数校验与组装，不写业务逻辑；业务在 cli/* 与 jobs.rs。
3. **CLI 生成器是纯函数**：`cli/*` 输入结构体、输出 `Vec<String>`（argv），不碰文件系统与进程，全部有单元测试。真实进程执行只发生在 jobs.rs。
4. **任务状态单向**：jobs.rs → mpsc channel → commands.rs 桥 → Tauri event `job://progress` / `job://state` / `job://log` → 前端渲染。前端不轮询。
5. **工具定位单一来源**：所有需要 exe 路径的模块一律向 locator.rs 要，不自行拼路径。

## 关键数据流

### 视频压制（含 GPU）
```
前端 VideoJob → commands::encode_video
  → cli::video::build_avs_script()      # 生成/复用 AVS（软编码路径）
  → cli::video::build_encode_argv()     # x265/x264/nvenc 各自的 argv
  → cli::video::build_mux_argv()        # MP4Box/mkvmerge 封装 argv
  → jobs::submit(计划: [avs]→[encode]→[mux]) → 串行执行 → 进度事件
```
GPU 路径不走 AVS：`cli::video::build_gpu_argv()` 直接 ffmpeg 滤镜链（scale/trim/subtitles）。

### 进度解析（jobs::progress）
- x264/x265：解析 stderr `\r` 分片中的 `a/b frames` 与 `xx.xx%`
- ffmpeg：解析 `time=` 并结合 ffprobe 时长换算百分比
- 全部原始行同步转发 `job://log` 供日志窗查看
