// main.js —— 启动序列与全局事件路由。

import { invoke, listen } from './bridge.js';
import { $, $$, initNumBoxes, toast } from './components.js';
import { state, loadSettings, loadTools, saveSettings, mergeToolsReady } from './state.js';
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

// ---------- 全局拖放（Tauri 事件坐标为物理像素，需换算成 CSS 像素） ----------
const toCss = (pos) => {
  const dpr = window.devicePixelRatio || 1;
  return { x: (pos?.x || 0) / dpr, y: (pos?.y || 0) / dpr };
};

/** 拖放命中测试：先 elementFromPoint，再对所有拖放区做矩形兜底（换算误差/边缘时也能命中） */
function hitDropZone(x, y) {
  const el = document.elementFromPoint(x, y);
  const zone = el?.closest('[data-drop]');
  if (zone) return zone;
  let best = null, bestArea = Infinity;
  for (const z of document.querySelectorAll('.tab-page.active [data-drop]')) {
    const r = z.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue;
    if (x >= r.left - 2 && x <= r.right + 2 && y >= r.top - 2 && y <= r.bottom + 2) {
      const area = r.width * r.height;
      if (area < bestArea) { best = z; bestArea = area; } // 取最内层（面积最小）命中
    }
  }
  return best;
}

/** 由物理像素坐标找拖放目标 id；找不到则用当前页默认目标 */
function dropTarget(px, py) {
  const { x, y } = toCss({ x: px, y: py });
  const zone = hitDropZone(x, y);
  if (zone) return zone.id;
  const page = document.querySelector('.tab-page.active');
  return page?.dataset.dropDefault || null;
}

// 拖动过程中的实时反馈：提示层 + 悬停字段高亮（rAF 节流）
let hoverZone = null;
let overRaf = 0;
let overPos = null;
function setHoverZone(zone) {
  if (hoverZone === zone) return;
  hoverZone?.classList.remove('drop-hot');
  hoverZone = zone;
  hoverZone?.classList.add('drop-hot');
}
function paintOver() {
  overRaf = 0;
  if (overPos) setHoverZone(hitDropZone(overPos.x, overPos.y));
}
function dragFeedbackStart(pos) {
  $('#dropOverlay')?.classList.add('show');
  overPos = toCss(pos);
  if (!overRaf) overRaf = requestAnimationFrame(paintOver);
}
function dragFeedbackEnd() {
  $('#dropOverlay')?.classList.remove('show');
  overPos = null;
  setHoverZone(null);
}

// 禁用 WebView2 默认右键菜单（避免出现网页相关菜单项）
window.addEventListener('contextmenu', (e) => e.preventDefault());

// Esc 关闭最上层弹窗；Ctrl+1~9 快速切换标签页
window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    const open = [...document.querySelectorAll('.overlay.show')];
    const top = open[open.length - 1];
    if (!top) return;
    e.preventDefault();
    if (top.id === 'previewOverlay') document.getElementById('previewVideo')?.pause();
    if (top.id === 'audioParamsOverlay' || top.id === 'previewOverlay') { top.classList.remove('show'); return; }
    // 进度详情：点「收起」；确认框：点「取消」（保证 Promise 正常 resolve）
    (top.querySelector('#pcanceldismiss') || top.querySelector('[data-r="0"]'))?.click();
    return;
  }
  if (e.ctrlKey && !e.altKey && !e.shiftKey && /^Digit[1-9]$/.test(e.code)) {
    const idx = parseInt(e.code.slice(5), 10) - 1;
    const tab = [...document.querySelectorAll('.tab')][idx];
    if (tab) { tab.click(); e.preventDefault(); }
  }
});

async function boot() {
  // 启动错误上报到后端日志
  window.addEventListener('error', (ev) => {
    invoke('log_frontend', { message: `前端错误: ${ev.message} @ ${ev.filename}:${ev.lineno}` }).catch(() => {});
  });
  window.addEventListener('unhandledrejection', (ev) => {
    invoke('log_frontend', { message: `未处理的Promise: ${ev.reason}` }).catch(() => {});
  });
  // 工具链深度探测（GPU 试编码等）在后台完成：先注册监听再触发探测，防事件丢失
  listen('tools://ready', (e) => {
    mergeToolsReady(e.payload);
    if ($('#vEncoder')) video.refreshEncoders();
    console.log('[maruko-v2] 工具链深度探测就绪');
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
  $('#tab-common').dataset.dropDefault = 'oImage';
  $('#tab-mux').dataset.dropDefault = 'm4Video';
  $('#tab-extract').dataset.dropDefault = 'eVideo';
  $('#tab-avs').dataset.dropDefault = 'sVideo';
  $('#tab-mediainfo').dataset.dropDefault = 'miText';

  // 全局拖放：进入/悬停给反馈，落下按命中字段精确路由
  listen('tauri://drag-enter', (e) => dragFeedbackStart(e.payload?.position));
  listen('tauri://drag-over', (e) => dragFeedbackStart(e.payload?.position));
  listen('tauri://drag-leave', () => dragFeedbackEnd());
  listen('tauri://drag-drop', (e) => {
    dragFeedbackEnd();
    const { paths } = e.payload || {};
    if (!paths?.length) return;
    const name = document.querySelector('.tab-page.active')?.id.replace('tab-', '');
    const target = dropTarget(e.payload.position?.x || 0, e.payload.position?.y || 0);
    // MediaInfo 页特殊：整页吃掉拖放
    if (name === 'mediainfo') return tabs.mediainfo.handleDrop(paths);
    const handler = tabs[name];
    if (handler?.handleDrop) handler.handleDrop(paths, target);
  });

  // 告诉后端 UI 就绪（关闭启动画面、显示主窗）
  await invoke('ui_ready');

  // 自动检查更新（设置页可关）：发现新版本时轻提示，不阻塞任何操作
  if (state.settings?.checkUpdate) {
    invoke('check_update').then((r) => {
      if (r?.ok && r.msg.includes('新版本')) {
        toast(`${r.msg}，可到「帮助」页一键更新`, 'ok');
      }
    }).catch(() => {});
  }

  console.log('[maruko-v2] 就绪');
}

boot().catch((e) => {
  invoke('log_frontend', { message: `boot失败: ${e}
${e?.stack || ''}` }).catch(() => {});
  document.body.innerHTML = `<pre style="color:#e54d42;padding:20px;font-family:monospace">启动失败：${e}\n${e?.stack || ''}</pre>`;
});
