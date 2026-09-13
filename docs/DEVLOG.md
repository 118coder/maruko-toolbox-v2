# 开发日志

## 2026-09-12

### 环境勘测
- 本机工具链：Node 24 / Rust 1.96（MSVC+GNU 双装）/ Python 3.12 / ffmpeg 7.1.1（PATH，含 nvenc）
- **GPU 实测**：NVENC 可用（N 卡），QSV/AMF 不可用 → 应用内 GPU 探测必须"试编码"而非看编码器列表
- 原版小丸 r236 安装于 `E:\Program Files (x86)\MarukoToolbox`，tools\ 全套工具可执行（x265 2.1、mkvmerge 9.5、avs4x26x 0.10）
- 提取原版默认参数：preset.xml（x264 default 预设、音频预设）+ xiaowan.exe.Config（CRF 23.5、字幕后缀 none,sc,tc,chs,cht,en,jp 等）

### 环境坑位与解法（重要，换机器构建前必读）
1. **VS 2022 残缺安装**：cl.exe/lib 在但 `VC\Tools\MSVC\14.37.32822\include` 全空（无 stdarg.h 等）→ cc-rs 编 C 依赖必炸，vcvars64.bat 自身报"找不到路径"。无管理员权限无法静默修复 → **改用 windows-gnu 工具链**（rustup override 已设置），C 依赖交给 `C:\mingw64\mingw64` 的 gcc
2. **windres 不认中文路径**：tauri-winres 用 dunce::canonicalize 把图标路径还原成真实中文路径，windres 打不开 → **图标放到 ASCII 绝对路径** `E:\maruko-build\icons\`（tauri.conf.json 引用），.rc 里的中文版本信息字符串（带 `#pragma code_page(65001)`）windres 可正常处理
3. **cargo target 含中文路径会让部分工具出错** → `CARGO_TARGET_DIR=E:\maruko-build`
4. **构建入口统一为 `devbuild.bat`**（追加 mingw64 到 PATH 尾部——放前面会劫持测试进程的 DLL 导致 STATUS_ENTRYPOINT_NOT_FOUND；subst X: 映射已不需要但保留目录结构）
5. **GUI 测试二进制起不来**（tauri→webview2 api-set 静态导入）→ 单元测试全部放在 `maruko-core` 子 crate（无 tauri 依赖），`cargo test -p maruko-core`

### 架构落地
- `crates/maruko-core`：CLI 生成器（x264/x265/2Pass/ffmpeg GPU/音频管道/封装/抽取/一图流/截取/旋转）、AVS 脚本生成、任务队列（进度解析 x264/x265/ffmpeg）、工具链定位（设置→注册表→常见路径）、GPU 探测（试编码 0.3s）、MediaInfo.dll x64 FFI + ffprobe 回退、设置持久化（便携优先）、日志
- 50 个单元测试全过（crop 语法、2Pass 参数、码控映射、进度解析等）
- GUI 壳 `src-tauri/src`：commands.rs（34 个命令）+ lib.rs（托盘/启动画面/关窗行为/主题）

### 修复记录
- crop 解析 `-0` 丢符号导致边距/尺寸模式误判 → 按原字符串符号判断
- x264/x265 进度行 fps/kbps 取错 token（值在前一 token）→ 修正
- 解析出的 eta 被估算值覆盖 → 仅缺省时估算

### 联调与冒烟测试（全部通过 ✅）
- 视觉走查：九个标签页逐页截图核对，布局与原版一致、现代皮肤生效、启动画面/托盘正常
- 发现并修复：前端字段名与 serde camelCase 不一致（audioEncoders/avsPlugins/gpuInfo/toolsDir）
- 发现并修复：MediaInfo 页文本域高度塌陷（.fill 布局类）
- **x265 软编码全链路实测**：AVS 生成 → avs4x26x → x265 CRF 23.5 → MP4Box 封装 → test_2.mp4 ✅（HEVC 320x240 2s + AAC 复制）
- **GPU 全链路实测**：hevc_nvenc（运行时探测后出现在编码器下拉）→ test_3.mp4 ✅
- **音频全链路实测**：ffmpeg 解码 → NeroAAC LC 128K → tone.m4a ✅
- **MediaInfo 实测**：64 位 MediaInfo.dll FFI 输出经典 Inform 文本 ✅

### 冒烟测试揪出的三个深层 bug（均已修复 + 回归测试）
1. **neroAacEnc 参数顺序**：要求 `[options] -if -of`，不能颠倒
2. **ffmpeg 7.x WAV muxer 写管道收尾必报 EINVAL**（数据完整但退出码非 0）→ 音频管线改为"临时 wav 文件中转"，所有编码器统一文件模式
3. **任务执行器管道死锁**：原实现等上一步退出才把 stdout 接给下一步，而下一步不读则上一步永远退不出 → 重写为 shell 语义：链内进程并发启动、stderr 各自解析进度、链间串行

### 其他改进
- 任务日志全部持久化到 logs/maruko-v2.log（对齐原版日志习惯，排障利器）

### UX 迭代（用户反馈）
1. **输出自动命名**：文件拖入/选入后，输出框自动填 源目录/名+编码器标签+容器（测试.mp4 + x265 → 测试x265.mp4；音频 → 测试nero.m4a；一图流/截取/旋转/封装同理）。手动选过输出后不再覆盖，换输入或编码器时自动刷新；后端对重名自动 _2/_3 防覆盖（tagged_output/deconflict，含单元测试）
2. **内嵌进度条**：压制时批量压制上方常驻一条粘性进度条（任务名+进度+百分比+详情/取消）；点「收起」只收起详情弹窗，进度条不再消失，点「详情」随时展开
3. **保持原分辨率锁定宽高**：勾选时宽度/高度输入框禁用（置灰）

### 启动卡死 bug 诊断（diagnosing-bugs 流程）
- 症状：应用卡在启动画面，Rust 日志正常但前端 main.js 从未执行
- 反馈环：debug_loop.sh 自动 启动→等18s→grep 日志判定 RED/GREEN，并在链路每环埋 [DEBUG-m1] 标记
- 定位：index.html 加载 OK、Tauri IPC 注入 OK、main.js 模块从未执行 → 窗口级 error 监听捕获 `SyntaxError: Identifier 'out' has already been declared @ video.js:111`
- 根因：行级补丁把批量提交块改坏（重复 const out + 丢失 return buildJob）
- 修复 + 回归缝隙：所有 ui/js 文件纳入 node --check 语法检查；埋点全部清理

### Voukoder 技术移植（V2.1）
用户反馈 Voukoder 导出"更小更快"，解剖其源码（EOL，GPL v2）找到三个技术来源：
1. FFmpeg libavcodec 内置的**新版 x265**（4.1 vs 原版工具链的 2.1/2016）
2. **iAvoe 社区调参预设**（preset slow + aq-mode 4 + aq-motion + bframes 11 + rdoq 2 + psy-rd 等）
3. **10bit 编码**（yuv420p10le，压缩效率更高）+ MP4 faststart

实施（不复制 GPL 代码，预设参数值为 x265 公开 CLI 事实并注明来源）：
- 新编码器条目"现代版 x265 / x264"（ffmpeg libx265/libx264 后端），五套 iAvoe 预设下拉（通用 8/10bit、动漫 10bit、电影级高压缩 8/10bit、快速），切换预设自动填 CRF
- NVENC 参数面板（预设 p1-p7 / Multipass / 空间AQ / 前瞻），白名单透传
- 音频 AC3/E-AC3/Opus；封装转换加 mov/webm/mpegts
- E2E 实测：ProRes mov（apcn）✅、现代 x265 通用 10bit（yuv420p10le）✅

## 2026-09-14

### 工具链现代化（v2.4）：2016 版 → 2025/2026 版

背景：完整包里的 tools\ 除 ffmpeg 外全部是原版小丸 2016-10-23 的产物。用户提出更新工具链以提升渲染性能。

**替换清单**（新 tools-modern 目录 = 原版 tools 副本 + 逐项替换，原版安装目录不动）：

| 组件 | 旧 | 新 | 来源 |
| --- | --- | --- | --- |
| x265 64 位三件（8/10/12bit 文件名） | x265 2.5 | **4.3**（multilib 单 exe 三份同内容，从 y4m/raw 位深参数选库） | jpsdr/x265 r4.3.0.13 (GCC 13.1 winthread) |
| x265-8bit[gcc].exe（32 位名） | x265 2.5 x86 | 4.3 x86 8bit（官方 4.3 仍有 32 位 build） | 同上 |
| x264 四件（32/64 × 8/10bit 文件名） | ~r2700 | **r3214 t_mod**（(8 & 10)-bit 双库单 exe） | jpsdr/x264 r3214 (winthread) |
| mkvmerge / mkvextract / mkvinfo | MKVToolNix 9.x | **v101.0**（CLI 静态链接，独立运行已验证） | mkvtoolnix.download 官方 |
| MediaInfo.dll（根级 + x64\ 两处） | 0.7.x (2016) | **25.09** | mediaarea.net（用户系统 System32 已有同版 DLL，直接拷贝） |
| ffmpeg.exe / ffprobe.exe | （打包时顶替） | 7.1（pack_full.py 逻辑不变） | C:\Program Files\FFmpeg |
| MP4Box.exe / mmg.exe | 0.6.x (2016) | **保留旧版**（见下） | - |

**不更新项与理由**：AviSynth.dll + avs\ 滤镜群（32 位 2.6 生态，换 AviSynth+ 需全系 64 位滤镜，多数不存在；喂帧不是编码瓶颈）；avs4x26x（纯管道转发，无性能影响）；neroAacEnc（Nero 上游 2012 年停更，无新版）；qaac/fdkaac/lame/flac（收益极小，本轮未动）。

**MP4Box 未换的原因**：GPAC Windows 构建的全部官方渠道（download.gpac.io、download.tsi.telecom-paristech.fr、files.gpac.io、GitHub releases 无资产）在本机网络环境均不可达，videohelp/free-codecs 转载站无直链。实测 2016 版 MP4Box 封装新 x265 4.3 流正常。后续如需更换：开代理后从 https://download.gpac.io/gpac/release/ 取新版替换 tools\MP4Box.exe 即可（程序按文件名调用，无需改代码）。

**代码适配**（新旧版本差异，video.rs）：
1. 位深注入：新版编码器是"单 exe 多位深库"，位深不再由 exe 变体决定 → encode_args() 按 encoder 文件名注入 `-D 10/12`（x265 multilib）或 `--output-depth 10`（x264 t_mod）；放在 extra 之前，用户自定义参数可覆盖。avs4x26x 对 HBD AVS 自动传 `--input-depth 10`，与 `-D 10` 协同（raw 形态实测 yuv420p10le）
2. mkvmerge `--default-duration 0:24000/1001` 被 v101 拒绝（要求时间单位后缀）→ 改为 `0:24000/1001fps`（分数+fps，精确帧率）

**下载渠道备忘**（GitHub 直连可通但 git push 被重置；bitbucket/mkvtoolnix.download/mediaarea 直连正常）：jpsdr 构建发布于 GitHub Releases；MKVToolNix 官方 7z 命名 `mkvtoolnix-64-bit-<ver>.7z`；Windows 自带 `C:\Windows\System32\tar.exe`（bsdtar）可解 7z，Git Bash 的 GNU tar 不行。

**E2E 验证矩阵**（tools-modern 实测）：avs4x26x+新x265 8bit（真实 AVS 管线，ColorBars 48帧）✅ yuv420p；10bit y4m `-D 10` ✅ yuv420p10le；raw 10bit（avs4x26x HBD 形态）✅；x265 2pass ✅；x264 10bit `--output-depth 10` ✅ yuv420p10le；mkvmerge v101 封装 ✅ 帧率保持 24000/1001；MP4Box(2016)+新流 ✅；MediaInfo 25.09 C API（ctypes 模拟 mediainfo.rs 调用序列）✅ 正确识别 HEVC Main 10；单元测试 63 个全过（含新增 test_soft_plan_depth_args）。
