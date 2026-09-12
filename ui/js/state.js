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
  return opts;
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
