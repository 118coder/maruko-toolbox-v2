# 设计决策记录

用户已确认：纯桌面 exe（Tauri 2）· GPU 用于视频压制 · 布局保持 + 视觉现代化 · 九个标签页一次性交付 · 边做边写文档 · 代码强解耦 · 名称「小丸工具箱V2现代版」。以下为用户休息期间自行定夺的默认决策。

| # | 决策 | 理由 |
| --- | --- | --- |
| D1 | Tauri 2 + WebView2，前端原生 ES Modules 无打包器，`withGlobalTauri` 注入 | 体积小、构建链简单；UI 本身由 WebView2 GPU 合成（满足"现代化+GPU渲染"的界面部分） |
| D2 | 工具链复用原版 `E:\Program Files (x86)\MarukoToolbox\tools`；定位顺序：设置内手动路径 → 注册表卸载项 → 常见路径 → PATH。设置页可改 | 用户机器上原版工具链完好，压制行为与原版逐位一致 |
| D3 | GPU 编码基于 **ffmpeg**（探测顺序：PATH → 原版 tools → 设置内自定义路径），运行时对 nvenc/qsv/amf 逐一试编码 0.3s 探测可用性，仅把可用的加入编码器下拉框 | 原版 x265 [gcc] 构建无 NVENC 支持；2016 版 ffmpeg 对现代驱动兼容差；本机实测 NVENC 可用、QSV/AMF 不可用，故必须运行时探测而非静态假设 |
| D4 | GPU 编码器码控映射：NVENC→`-rc vbr -cq`、QSV→`-global_quality`(ICQ)、AMF→`-rc cqp`；「2Pass」对 GPU 编码器禁用（置灰） | 各硬件 API 的 CRF 语义不同，2Pass 在消费级硬件编码器上收益极小且实现分裂 |
| D5 | x264 默认参数沿用原版 preset.xml 的 `default` 预设（`--preset 8 -r 6 -b 6 -i 1 --scenecut 60 -f 1:1 --qcomp 0.5 --psy-rd 0.3:0 --aq-mode 2 --aq-strength 0.8`），`--crf` 由界面注入；x265 默认仅注入 `--crf`；分辨率由界面控制（保持原分辨率时不加 resize） | 与原版 r236 行为对齐（preset.xml + xiaowan.exe.Config 实测值） |
| D6 | AVS 生成顺序：Source → Crop → LanczosResize → AddBorders → Tweak → Levels → Sharpen → Undot → Trim → TextSub（字幕最后烧录）；脚本框非空时原样使用 | 烧字幕放最后避免被裁剪/缩放影响；与原版可直观对齐 |
| D7 | AVS 预览 = 用 avs4x26x + x264 ultrafast 渲染 ≤96 帧生成临时 mp4，在应用内 `<video>` 播放；「设置→预览播放器」仍可指定外部播放器接管 | 不依赖用户装有 AviSynth 兼容播放器；WebView2 内播 mp4 天然 GPU 解码 |
| D8 | 「替换音频」用 ffmpeg 等效实现（`-map 0:v:0 -map 1:a -c copy`）；FLV 抽取优先用原版 FLVExtractCL，调用失败自动回退 ffmpeg | ffmpeg 语义更可控；保持功能等价 |
| D9 | MediaInfo 标签页：优先 `tools\x64\MediaInfo.dll`（64 位，匹配本进程）libloading FFI；加载失败回退 ffprobe 合成同格式文本 | 原版仅带 32 位 MediaInfo.dll，64 位进程不能加载，必须用 x64 版 |
| D10 | 配置存储：exe 旁 `config.json`（便携优先），写失败回退 `%APPDATA%\MarukoV2\config.json` | 与原版 preset.xml 便携习惯一致 |
| D11 | 自动关机：批量完成后 `shutdown /s /t 60`，界面弹 60 秒倒计时可取消（`shutdown /a`） | 原版直接关机无缓冲，此处加了撤销窗口更安全 |
| D12 | 「检查更新」：GET 设置内 URL 的版本 JSON `{version,url}`，与内置版本比较；默认 URL 指向本项目占位接口，失败时如实显示"检查失败（离线）"，不伪造结果 | 原版的 WebService 接口早已失效，诚实降级 |
| D13 | 「帮助」页文案自行撰写（使用说明 + 关于 + 各工具链致谢），不复制原版 help.rtf 内容 | 版权考虑 |
| D14 | 输出文件命名：源目录同名 + 新容器扩展名；若重名自动追加 `_2`、`_3`… | 对齐原版行为且避免覆盖 |
| D15 | 单实例锁未实现（低价值，记录在案）；托盘模式开启时点关闭 = 隐藏到托盘 | 原版托盘模式行为 |
| D16 | 批量队列串行执行（同一时刻一个压制任务），与原版一致；取消 = 杀进程树 | 小丸原版即串行；并行会造成 GPU/CPU 争抢 |
| D17 | 界面语言提供 简体中文 / English；原版的 ja-JP、zh-TW 资源不在首版范围（框架支持后续补字典） | 控制一次性交付范围 |
| D18 | GPU 模式下 AVS 页滤镜不生效，仅支持 缩放 / 裁剪(trim) / 烧字幕(subtitles 滤镜，检测可用才启用) | AVS 依赖 AviSynth 管线（CPU），与 ffmpeg 硬编管线不兼容，如实降级并在 UI 提示 |
| D19 | 「常用→旋转」用 ffmpeg + transpose 重编码（x264 CRF 23.5）；「一图流」优先 ffmpeg libx264，缺失时回退 管道→原版 x264 | 单进程方案更稳；回退保证纯原版工具链也能跑 |
| D20 | 窗口尺寸 640×640（原版 624×583），最小 580×540，可缩放；控件布局按截图 1:1 复刻但用现代间距令牌 | 现代化要求 + 高分屏可读性 |

## 与原版交互细节的忠实度说明

- 起始帧/编码帧数 → AVS `Trim(start, start+N-1)` 实现（与 avs4x26x 管线一致）
- x264 线程数 → `--threads`，x265 → `--pool`
- 优先级 → 进程创建标志（IDLE/BELOW_NORMAL/NORMAL/ABOVE_NORMAL/HIGH）+ CREATE_NO_WINDOW
- 分离器 auto → 优先 LSMASH（LWLibavVideoSource），失败回退 FFMS2；下拉可强制指定
- 内嵌字幕语言后缀下拉：`none, sc, tc, chs, cht, en, jp`（取自原版配置 SubLanguageExtension）
- 音频码率下拉值：32/48/64/80/96/112/128/160/192/224/256/320 Kbps；NeroAAC ≤64K 自动用 HE 模式（对齐 preset.xml 预设）
