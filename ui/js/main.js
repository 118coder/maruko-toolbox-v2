// main.js —— 启动序列与全局事件路由。

import { invoke, listen } from './bridge.js';
import { $, $$, initNumBoxes, setFieldFromDrop, toast } from './components.js';
import { state, loadSettings, loadTools, saveSettings } from './state.js';
import { applyLanguage } from './i18n.js';
import { initJobsUI } from './jobs.js';
import * as video from './tabs/video.js';
import * as audio from './tabs/audio.js';
import * as common from './tabs/common.js';
import * as mux from './tabs/mux.js';
import * as extract from './tabs/extract.js';
import * as avs from './tabs/avs.js';
import * as mediainfo from './tabs/mediainfo.js';
import * as settings from './tabs/settings.js';
import * as help from './tabs/help.js';


// 全局音频参数共享（视频页 / 音频页 / AVS 页共用）
state.audio = { encoder: 'neroaac', bitrate: 128, custom: '', mode: 'copy' };

const tabs = { video, audio, common, mux, extract, avs, mediainfo, settings, help };

function switchTab(name) {
  $$('.tab').forEach((t) => t.classList.toggle('active', t.dataset.tab === name));
  $$('.tab-page').forEach((p) => p.classList.toggle('active', p.id === `tab-${name}`));
}

/** 找到拖放目标：命中 [data-drop] 元素 → 其 id；否则当前页的默认目标 */
function dropTarget(x, y) {
  const dpr = window.devicePixelRatio || 1;
  const el = document.elementFromPoint(x / dpr, y / dpr);
  const zone = el?.closest('[data-drop]');
  if (zone) return zone.id;
  const page = document.querySelector('.tab-page.active');
  return page?.dataset.dropDefault || null;
}

// 禁用 WebView2 默认右键菜单（避免出现网页相关菜单项）
window.addEventListener('contextmenu', (e) => e.preventDefault());

async function boot() {
  // 启动错误上报到后端日志
  window.addEventListener('error', (ev) => {
    invoke('log_frontend', { message: `前端错误: ${ev.message} @ ${ev.filename}:${ev.lineno}` }).catch(() => {});
  });
  window.addEventListener('unhandledrejection', (ev) => {
    invoke('log_frontend', { message: `未处理的Promise: ${ev.reason}` }).catch(() => {});
  });
  await loadSettings();
  applyLanguage(state.settings.language);
  document.documentElement.classList.toggle('dark', !!state.settings.dark);
  await loadTools();

  initNumBoxes();
  initJobsUI();
  tabs.video.initVideo();
  tabs.audio.initAudio();
  tabs.common.initCommon();
  tabs.mux.initMux();
  tabs.extract.initExtract();
  tabs.avs.initAvs();
  tabs.mediainfo.initMediaInfo();
  tabs.settings.initSettings();
  tabs.help.initHelp();
  video.refreshEncoders();

  // 标签切换
  $$('.tab').forEach((t) => t.addEventListener('click', () => switchTab(t.dataset.tab)));

  // 每页默认拖放目标（data-drop-default 写在 section 上）
  $('#tab-video').dataset.dropDefault = 'vVideo';
  $('#tab-audio').dataset.dropDefault = 'aInput';
  $('#tab-common').dataset.dropDefault = 'cVideo';
  $('#tab-mux').dataset.dropDefault = 'm4Video';
  $('#tab-extract').dataset.dropDefault = 'eVideo';
  $('#tab-avs').dataset.dropDefault = 'sVideo';
  $('#tab-mediainfo').dataset.dropDefault = 'miText';

  // 全局拖放（Tauri 事件，物理像素坐标）
  listen('tauri://drag-drop', (e) => {
    const { paths, position } = e.payload || {};
    if (!paths?.length) return;
    const name = document.querySelector('.tab-page.active')?.id.replace('tab-', '');
    const target = dropTarget(position?.x || 0, position?.y || 0);
    // MediaInfo 页特殊：整页吃掉拖放
    if (name === 'mediainfo') return tabs.mediainfo.handleDrop(paths);
    const handler = tabs[name];
    if (handler?.handleDrop) handler.handleDrop(paths, target);
  });
  listen('tauri://drag-over', () => {});

  // 告诉后端 UI 就绪（关闭启动画面、显示主窗）
  await invoke('ui_ready');

  console.log('[maruko-v2] 就绪');
}

boot().catch((e) => {
  invoke('log_frontend', { message: `boot失败: ${e}
${e?.stack || ''}` }).catch(() => {});
  document.body.innerHTML = `<pre style="color:#e54d42;padding:20px;font-family:monospace">启动失败：${e}\n${e?.stack || ''}</pre>`;
});
