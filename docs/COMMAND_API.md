# 命令 API（前端 ↔ 后端）

前端与后端之间全部通信 = `invoke(cmd, args)` + 事件。下表为全集；新增命令必须先补此文档。

## 系统 / 设置

| 命令 | 参数 | 返回 | 说明 |
| --- | --- | --- | --- |
| `ui_ready` | – | null | 前端加载完成 → 后端关闭启动画面、显示主窗口 |
| `get_settings` | – | `Settings` | 读配置（无则默认值） |
| `save_settings` | `settings: Settings` | `Settings` | 保存并返回规范化的配置 |
| `reset_settings` | – | `Settings` | 还原默认设置 |
| `get_tools_info` | – | `ToolsInfo` | 工具链定位结果 + 编码器清单 + GPU 探测 + AVS 插件清单 |
| `pick_file` | `title, filters[{name,exts}], save?` | `string?` | 系统文件对话框（单选） |
| `pick_folder` | `title` | `string?` | 文件夹对话框 |
| `open_path` | `path, reveal?` | null | 打开文件/目录（reveal=资源管理器定位） |
| `check_update` | – | `{ok, current, latest, msg}` | 检查更新（可配置 URL，失败返回 ok:false） |
| `get_logs` | – | `string[]` | 读取日志（最近的在后） |
| `open_logs_dir` | – | null | 打开日志目录 |
| `delete_logs` | – | null | 删除全部日志 |
| `set_dark` | `dark: bool` | null | 同步窗口主题（标题栏颜色） |

## 媒体信息 / AVS

| 命令 | 参数 | 返回 |
| --- | --- | --- |
| `mediainfo_analyze` | `path` | `{text, source: "mediainfo"\|"ffprobe", error?}` |
| `avs_preview` | `job: VideoJob` | `{ok, path?（临时 mp4）, error?}` 渲染 ≤96 帧预览 |
| `save_avs` | `path, content` | null |

## 任务（全部进入串行队列）

| 命令 | 参数 | 返回 |
| --- | --- | --- |
| `encode_video` | `job: VideoJob` | `job_id` |
| `encode_video_batch` | `jobs: VideoJob[]` | `[job_id]` |
| `encode_audio` | `job: AudioJob` | `job_id` |
| `encode_audio_batch` | `jobs: AudioJob[]` | `[job_id]` |
| `merge_audio` | `inputs: string[]` | `job_id` |
| `mux_mp4` | `video, audio, out, fps, par, replace_audio` | `job_id` |
| `mux_mkv` | `video, audio, subtitle, out` | `job_id` |
| `convert_mux` | `jobs: ConvertJob[]`（AAC编码器/输出格式/输出目录） | `[job_id]` |
| `extract_common` | `path, kind: "video"\|"audio", index, out_dir` | `job_id` |
| `extract_flv` | `path, kind: "video"\|"audio"` | `job_id` |
| `extract_mkv_track` | `path, track: number` | `job_id` |
| `one_pic` | `job: OnePicJob` | `job_id` |
| `cut_video` | `job: CutJob` | `job_id` |
| `rotate_video` | `job: RotateJob` | `job_id` |
| `job_cancel` | `id` | null（排队的移除，运行中的杀进程） |
| `shutdown_cancel` | – | null（`shutdown /a`） |

## 事件（Rust → 前端）

| 事件 | 载荷 |
| --- | --- |
| `job://progress` | `{id, pct, frame, total, fps, eta_s, elapsed_s, kbps, size_mb}` |
| `job://state` | `{id, state: "queued"\|"running"\|"done"\|"failed"\|"cancelled", title, output?, msg?}` |
| `job://log` | `{id, line}` |
| `job://queue_idle` | `{last_failed: bool}`（批量队列清空，自动关机倒计时从此开始） |
| `shutdown://armed` | `{seconds}` |

## 核心结构体

```ts
VideoJob {
  input, output,            // 输出为空 → 源目录同名.容器
  encoder,                  // "x265_64-8bit[gcc].exe" | "h264_nvenc" | ...
  demuxer,                  // "auto" | "lsmash" | "ffms2"
  mode: "crf"|"bitrate"|"2pass",
  crf: f64, bitrate: number,
  start_frame: number, frames: number,   // 编码帧数 0 = 到结尾
  width: number, height: number, keep_res: bool,
  audio_mode: "copy"|"encode"|"none",
  audio_encoder: string, audio_bitrate: number,   // audio_mode=encode 时
  subtitle: string,                     // 内嵌字幕（烧录）
  priority: "idle"|"below"|"normal"|"above"|"high",
  threads: number,                      // 0 = auto
  extra_args: string,                   // x264/x265 自定义命令行
  container: "mp4"|"mkv",               // 批量模式用
  avs_script: string,                   // AVS 页粘贴的脚本（非空则原样使用）
}
```

`AudioJob / OnePicJob / CutJob / RotateJob / ConvertJob` 见 `src-tauri/src/cli/*.rs` 中的结构体定义（文档与代码同步维护）。
