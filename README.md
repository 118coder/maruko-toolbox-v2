# 小丸工具箱V2现代版

复刻经典视频压制工具「小丸工具箱 r236」的完整界面与功能，采用现代化技术栈重构：

- **界面**：Tauri 2 + WebView2（GPU 合成渲染），布局与原版一致，视觉现代化（扁平、深色模式、高分屏清晰字体）
- **GPU 压制**：自动探测 NVENC / QSV / AMF 硬件编码器（基于 ffmpeg），与原版 x264/x265 软编码并存于编码器下拉框
- **工具链**：直接复用原版小丸工具箱 `tools\` 目录中的 x264 / x265 / ffmpeg / MP4Box / mkvmerge / mkvextract / FLVExtractCL / neroAacEnc / qaac / fdkaac / lame / flac / MediaInfo.dll / AviSynth 全套滤镜
- **零耦合架构**：前端 ↔ 后端仅通过一条命令 API 通信；命令行参数生成全部为纯函数并有单元测试

## 九个标签页

视频压制（CRF/自定义/2Pass、批量、内嵌字幕）、音频压制（NeroAAC/qaac/fdkaac/lame/flac、批量、合并）、常用（一图流、截取、旋转）、封装（合并 MP4/MKV、封装转换）、抽取（所有格式/FLV/MKV 轨道）、AVS（滤镜脚本生成与预览、avs4x26x 压制）、MediaInfo（64 位 MediaInfo.dll 直读）、设置（界面/编码器/功能）、帮助。

## 构建与运行

```bash
# 依赖：Node 18+、Rust (stable-msvc)、Windows 10/11（WebView2 系统自带）
npm install          # 安装 @tauri-apps/cli
npx tauri dev        # 开发调试
npx tauri build --no-bundle   # 发布构建，产物在 src-tauri/target/release/
```

产物为单文件 exe（前端资源已内嵌）。首次启动会自动定位原版小丸工具箱的 `tools\` 目录（也可在「设置」页手动指定）。

## 文档

| 文档 | 内容 |
| --- | --- |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | 模块划分与解耦规则 |
| [docs/COMMAND_API.md](docs/COMMAND_API.md) | 前后端命令 API 全集 |
| [docs/DECISIONS.md](docs/DECISIONS.md) | 全部设计决策（含默认值取舍） |
| [docs/FEATURES.md](docs/FEATURES.md) | 与原版 r236 的功能对照与差异表 |
| [docs/DEVLOG.md](docs/DEVLOG.md) | 开发日志 |

## 免责声明

本项目为个人学习用途的界面复刻与功能重实现，未复制原版任何二进制与素材；运行依赖用户自行安装的原版小丸工具箱工具链。x264/x265/ffmpeg/MKVToolNix 等工具版权归各自项目所有。
