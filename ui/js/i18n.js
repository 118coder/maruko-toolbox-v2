// i18n.js —— 界面语言。zh-CN 为基准（硬编码在 HTML 里），en 走字典替换。
// 原理：启动时按 data-i18n-scan 扫描文本节点做 zh→en 替换（见 DECISIONS D17）。

const EN = {
  '视频': 'Video', '音频': 'Audio', '常用': 'Common', '封装': 'Muxing', '抽取': 'Extract',
  '媒体信息': 'MediaInfo', '设置': 'Settings', '帮助': 'Help',
  '批量压制': 'Batch', '压制': 'Encode', '封装': 'Mux', '合并': 'Merge',
  '添加': 'Add', '删除': 'Remove', '清空': 'Clear',
  '输出路径': 'Output dir', '起始帧': 'Start frame', '编码帧数': 'Frames',
  '保持原分辨率': 'Keep resolution', '自动关机': 'Auto shutdown',
  '宽度': 'Width', '高度': 'Height', '编码器': 'Encoder', '分离器': 'Demuxer',
  '音频模式': 'Audio mode', '复制音频流': 'Copy audio stream',
  '字幕文件和视频文件在同一目录下且同名，不同名仅有语言后缀时请在左侧选择后缀': 'Subtitle with language suffix in same directory',
  '内嵌字幕': 'Burn subtitle', '格式': 'Format',
  '音频压制': 'Audio encoding', '输入': 'Input', '输出': 'Output',
  '码率': 'Bitrate', '自定义': 'Custom', '新文件生成在源文件目录': 'New files are written to the source directory',
  '一图流': 'One-picture stream', '图片': 'Image', '时间': 'Duration', '秒': 's',
  '复制音频': 'Copy audio', '其他': 'Others', '起始时刻': 'Start time', '结束时刻': 'End time',
  '截取': 'Cut', '旋转': 'Rotate',
  '合并为MP4': 'Merge to MP4', '合并为MKV': 'Merge to MKV', '封装转换': 'Convert',
  '替换音频': 'Replace audio', '字幕': 'Subtitle',
  '所有格式': 'All formats', '抽取音频': 'Extract audio', '抽取视频': 'Extract video',
  '引用第三方工具抽取': 'Extract via third-party tools',
  '外置滤镜': 'External filter', '插入': 'Insert',
  '可直接把AVS脚本粘贴到这里': 'Paste AVS script here',
  '保存AVS': 'Save AVS', '预览': 'Preview', '压制音频': 'Encode audio',
  '复制信息': 'Copy info', '播放视频': 'Play', '选择视频': 'Choose video',
  '界面设置': 'Interface', '界面语言': 'Language',
  '显示启动画面': 'Splash screen', '托盘模式': 'Tray mode', '深色模式': 'Dark mode',
  '优先级': 'Priority', '线程': 'Threads',
  '自定义命令行(下面参数将覆盖小丸内置参数 界面设置的参数依然有)': 'Custom CLI args (override built-ins; UI params still apply)',
  '功能设置': 'Features', '预览播放器': 'Preview player',
  '退出程序时删除所有临时文件': 'Delete temp files on exit',
  '检查更新': 'Check updates', '启用x265': 'Enable x265',
  '还原默认设置': 'Reset defaults', '查看日志': 'View logs', '删除日志': 'Delete logs',
};

export let currentLang = 'zh-CN';

export function applyLanguage(lang) {
  currentLang = lang;
  if (lang !== 'en') return;
  // 文本节点替换（浅遍历）
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  let n;
  while ((n = walker.nextNode())) {
    const t = n.textContent.trim();
    if (EN[t]) n.textContent = n.textContent.replace(t, EN[t]);
  }
  // option 元素
  document.querySelectorAll('option').forEach((o) => {
    const t = o.textContent.trim();
    if (EN[t]) o.textContent = EN[t];
  });
}
