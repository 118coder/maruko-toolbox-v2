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
