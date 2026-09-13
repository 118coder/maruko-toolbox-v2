import shutil, zipfile, io, os, sys

SRC_TOOLS = r"E:\Program Files (x86)\MarukoToolbox\tools"
MODERN_FF = r"C:\Program Files\FFmpeg"
STAGE = r"E:\maruko-build\package_full\小丸工具箱V2现代版"
APP = r"E:\maruko-build\release\maruko-v2.exe"
WV = r"E:\maruko-build\release\WebView2Loader.dll"

print("① 清理旧暂存...")
shutil.rmtree(r"E:\maruko-build\package_full", ignore_errors=True)

print("② 复制应用三件套...")
os.makedirs(STAGE, exist_ok=True)
shutil.copy2(APP, os.path.join(STAGE, "小丸工具箱V2现代版.exe"))
shutil.copy2(WV, os.path.join(STAGE, "WebView2Loader.dll"))

print("③ 完整复制原版工具链 tools\\（277MB，约需 1 分钟）...")
shutil.copytree(SRC_TOOLS, os.path.join(STAGE, "tools"))

print("④ 用现代 ffmpeg 7.1（含 libx265 4.1/GPU/专业编码器）顶替 2016 老版...")
tools = os.path.join(STAGE, "tools")
old_ff = os.path.join(tools, "ffmpeg.exe")
if os.path.isfile(old_ff):
    os.remove(old_ff)
shutil.copy2(os.path.join(MODERN_FF, "ffmpeg.exe"), os.path.join(tools, "ffmpeg.exe"))
shutil.copy2(os.path.join(MODERN_FF, "ffprobe.exe"), os.path.join(tools, "ffprobe.exe"))
for dll in os.listdir(MODERN_FF):
    if dll.startswith(("av", "sw", "post")) and dll.endswith(".dll"):
        shutil.copy2(os.path.join(MODERN_FF, dll), os.path.join(tools, dll))

print("⑤ 清理工具链里的运行时杂物...")
for junk in ("logs", "temp"):
    shutil.rmtree(os.path.join(tools, junk), ignore_errors=True)

print("⑥ 写使用说明...")
txt = """小丸工具箱V2现代版 v2.2 · 完整版
================================

【解压即用 · 零依赖】
把整个文件夹解压到任意位置，双击「小丸工具箱V2现代版.exe」运行。
工具链（x264/x265、AviSynth 滤镜、MKVToolNix、MP4Box、专业编码器等）
已全部内置在 tools\\ 文件夹，无需安装原版小丸或任何其他软件。

【唯一系统要求】
Windows 10 / 11（界面所需的 WebView2 系统自带）。

【功能速览】
· 编码器：原版 x264/x265 全系（AVS 管线）· 现代版 x265/x264
  （x265 4.1 引擎 + iAvoe 调参预设，含 10bit）·
  GPU 硬编（NVENC/QSV/AMF 自动探测）·
  ProRes / CineForm / FFV1 / UtVideo / VP9
· 音频：NeroAAC / qaac / fdkaac / LAME / FLAC / AC3 / E-AC3 / Opus / AAC
· 批量压制（输出自动命名）· 常驻进度条 · 自动关机（60 秒可撤销）
· AVS 滤镜脚本与预览 · MediaInfo · 封装（MP4/MKV/转换）/ 抽取

【常见问题】
Q：体积为什么这么大？
A：完整工具链（含 AviSynth 90 余个滤镜、多版本编码器）全部内置，彻底离线可用。
Q：想重置所有设置？
A：删除本目录的 config.json 后重启；logs\\ 是运行日志，可随时删除。
Q：tools\\ 可以删吗？
A：不可以——所有压制能力都来自它。

免责声明：本项目为个人学习用途的界面复刻与功能重实现；
tools\\ 中的工具来自用户自行安装的原版小丸工具箱，仅限个人使用，
x264/x265/FFmpeg/MKVToolNix 等版权归各自项目所有（x264/x265/FFmpeg 为 GPL）。
"""
io.open(os.path.join(STAGE, "使用说明.txt"), "w", encoding="utf-8-sig").write(txt)

print("⑦ 压缩 zip（约需 1-2 分钟）...")
zf_path = r"E:\网页小工具\小丸新版本\小丸工具箱V2现代版-v2.2完整版.zip"
if os.path.isfile(zf_path):
    os.remove(zf_path)
with zipfile.ZipFile(zf_path, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as zf:
    for root, _dirs, files in os.walk(STAGE):
        for f in files:
            full = os.path.join(root, f)
            rel = os.path.relpath(full, os.path.dirname(STAGE))
            zf.write(full, rel)

size = os.path.getsize(zf_path) / 1024 / 1024
print(f"⑧ 完成：{zf_path}  ({size:.1f} MB)")
