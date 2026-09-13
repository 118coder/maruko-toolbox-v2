// state.js —— 全局状态：设置对象 + 工具链信息 + 防抖保存。

import { invoke } from './bridge.js';
import { applyLanguage } from './i18n.js';

export const state = {
  settings: null,
  tools: null,       // get_tools_info 结果（含 gpu 编码器、avs 插件）
  features: null,    // check_ffmpeg_features
  batchOutputDir: '',// 批量输出路径
};

let saveTimer = null;

export async function loadSettings() {
  state.settings = await invoke('get_settings');
  return state.settings;
}

/** 保存（防抖 400ms）；dark 立即同步窗口主题 */
export function saveSettings(immediate = false) {
  clearTimeout(saveTimer);
  const doSave = async () => {
    state.settings = await invoke('save_settings', { newSettings: state.settings });
    applyLanguage(state.settings.language);
    document.documentElement.classList.toggle('dark', !!state.settings.dark);
  };
  if (immediate) return doSave();
  saveTimer = setTimeout(doSave, 400);
}

export async function loadTools() {
  state.tools = await invoke('get_tools_info');
  state.features = await invoke('check_ffmpeg_features');
  return state.tools;
}

/** 软编码器 + GPU 编码器合成视频编码器下拉选项 */
export function videoEncoderOptions() {
  const opts = (state.tools?.encoders || []).map((e) => ({ id: e.id, label: e.label }));
  for (const g of state.tools?.gpuInfo || []) {
    if (g.ok) opts.push({ id: g.id, label: g.label });
  }
  for (const f of state.tools?.ffmpegEncoders || []) {
    opts.push({ id: f.id, label: f.label });
  }
  return opts;
}

/** 编码器命名标签（与后端 cli::video::encoder_tag 对齐）：测试.mp4 → 测试x264.mp4 */
export function encoderTag(id) {
  if (!id) return '';
  if (id.startsWith('x264')) return 'x264';
  if (id.startsWith('x265')) return 'x265';
  if (id.includes('nvenc')) return 'nvenc';
  if (id.includes('qsv')) return 'qsv';
  if (id.includes('amf')) return 'amf';
  const pro = { prores_ks: 'prores', cfhd: 'cineform', ffv1: 'ffv1', utvideo: 'utvideo', 'libvpx-vp9': 'vp9', libx265: 'x265m', libx264: 'x264m' };
  return pro[id] || '';
}

/** 是否 Voukoder 风格专业编码器（ffmpeg 后端） */
export function isProEncoder(id) {
  return ['prores_ks', 'cfhd', 'ffv1', 'utvideo', 'libvpx-vp9', 'libx265', 'libx264'].includes(id);
}

/** 音频编码器命名标签（与后端对齐）：测试.wav + NeroAAC → 测试nero.m4a */
export function audioEncoderTag(id) {
  return ({ neroaac: 'nero', qaac: 'qaac', fdkaac: 'fdk', lame: 'lame', flac: 'flac', ffmpeg_aac: 'aac' })[id] || 'audio';
}

/** 源目录 + 名 + 标签 + 扩展名 */
export function taggedName(input, tag, ext) {
  if (!input) return '';
  const dir = input.replace(/[^\\/]+$/, '');
  const stem = input.replace(/^.*[\\/]/, '').replace(/\.[^.]+$/, '');
  return dir + stem + tag + '.' + ext;
}

export function isGpu(id) {
  return /^(h264|hevc|av1)_(nvenc|qsv|amf)/.test(id);
}

export function hasTools() {
  return !!state.tools?.toolsDir;
}

export function hasFfmpeg() {
  return !!state.tools?.ffmpeg;
}
