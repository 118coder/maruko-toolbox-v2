// components.js —— 通用控件行为与小组件。不 import 任何 tab 模块。

import { pickFile, pickFolder } from './bridge.js';

// ---------- NumBox（数字微调框） ----------
export function initNumBoxes(root = document) {
  root.querySelectorAll('.num').forEach((box) => {
    if (box.dataset.init) return;
    box.dataset.init = '1';
    const input = box.querySelector('input');
    const [up, down] = box.querySelectorAll('.btns button');
    const step = parseFloat(box.dataset.step || '1');
    const fmt = (v) => (box.dataset.dec ? Number(v).toFixed(Number(box.dataset.dec)) : String(v));
    up.addEventListener('click', () => {
      input.value = fmt((parseFloat(input.value) || 0) + step);
      input.dispatchEvent(new Event('change'));
    });
    down.addEventListener('click', () => {
      input.value = fmt((parseFloat(input.value) || 0) - step);
      input.dispatchEvent(new Event('change'));
    });
  });
}

// ---------- 文件字段（点击选择 + 双击清空 + 拖放） ----------
export const FILTERS = {
  video: [{ name: '视频', exts: ['mp4', 'mkv', 'flv', 'avi', 'mov', 'ts', 'm2ts', 'webm', 'wmv', 'rmvb', 'mpg', 'mpeg', 'vob', 'm4v', '3gp'] }],
  audio: [{ name: '音频', exts: ['m4a', 'aac', 'mp3', 'flac', 'wav', 'ape', 'ogg', 'opus', 'wma', 'mka'] }],
  subtitle: [{ name: '字幕', exts: ['ass', 'ssa', 'srt'] }],
  image: [{ name: '图片', exts: ['png', 'jpg', 'jpeg', 'bmp', 'webp', 'gif'] }],
  avs: [{ name: 'AVS 脚本', exts: ['avs'] }],
  all: [{ name: '全部文件', exts: ['*'] }],
};

export function bindFileField(field, { kind = 'all', title = '选择文件', save = false, onSet } = {}) {
  field.addEventListener('dblclick', () => {
    field.value = '';
    onSet?.('');
  });
  field.addEventListener('click', async () => {
    const p = kind === 'folder' ? await pickFolder(title) : await pickFile(title, FILTERS[kind] || FILTERS.all, save);
    if (p) {
      onSet?.(p);
      field.value = p;
      // 统一触发 change：输出自动命名联动依赖它
      field.dispatchEvent(new Event('change'));
    }
  });
}

// 把系统级拖放事件路由到指定字段（由 main.js 的全局 drop 驱动）
export function setFieldFromDrop(field, paths) {
  if (!paths?.length) return;
  setFieldValue(field, paths[0]);
}

/** 统一赋值入口：总是触发 change（自动命名联动依赖它） */
export function setFieldValue(field, v) {
  field.value = v;
  field.dispatchEvent(new Event('change'));
}

// ---------- 列表（批量区） ----------
export function makeListBox(el) {
  const items = [];
  const render = () => {
    el.innerHTML = '';
    if (!items.length) {
      const d = document.createElement('div');
      d.className = 'empty';
      d.textContent = el.dataset.empty || '（空）';
      el.appendChild(d);
      return;
    }
    items.forEach((it, i) => {
      const d = document.createElement('div');
      d.className = 'item' + (i === el._sel ? ' sel' : '');
      d.innerHTML = `<span class="st ${it.cls || ''}">${it.icon || ''}</span><span style="overflow:hidden;text-overflow:ellipsis">${escapeHtml(it.text)}</span>`;
      d.addEventListener('click', () => { el._sel = i; render(); });
      el.appendChild(d);
    });
  };
  render();
  return {
    add(text) { items.push({ text }); el._sel = items.length - 1; render(); },
    removeSel() {
      if (el._sel == null || !items.length) return;
      items.splice(el._sel, 1);
      el._sel = Math.min(el._sel, items.length - 1);
      if (!items.length) el._sel = null;
      render();
    },
    clear() { items.length = 0; el._sel = null; render(); },
    all() { return items.map((i) => i.text); },
    setStat(i, icon, cls) { if (items[i]) { items[i].icon = icon; items[i].cls = cls; render(); } },
    clearStats() { items.forEach((i) => { i.icon = ''; i.cls = ''; }); render(); },
    get sel() { return el._sel; },
  };
}

export function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// ---------- Toast ----------
export function toast(msg, type = '') {
  const box = document.getElementById('toasts');
  const t = document.createElement('div');
  t.className = `toast ${type}`;
  t.textContent = msg;
  box.appendChild(t);
  setTimeout(() => { t.style.opacity = '0'; t.style.transition = 'opacity .3s'; }, 3200);
  setTimeout(() => t.remove(), 3600);
}

// ---------- 简易确认弹窗 ----------
export function confirmModal(title, text) {
  return new Promise((resolve) => {
    const ov = document.createElement('div');
    ov.className = 'overlay show';
    ov.innerHTML = `
      <div class="modal">
        <h3>${escapeHtml(title)}</h3>
        <p style="margin:0 0 14px">${escapeHtml(text)}</p>
        <div class="mrow">
          <button class="btn" data-r="0">取消</button>
          <button class="btn primary" data-r="1">确定</button>
        </div>
      </div>`;
    ov.addEventListener('click', (e) => {
      const r = e.target.dataset?.r;
      if (r !== undefined) { ov.remove(); resolve(r === '1'); }
    });
    document.body.appendChild(ov);
  });
}

// ---------- 通用工具 ----------
export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

export function fmtTime(sec) {
  sec = Math.max(0, Math.round(sec));
  const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = sec % 60;
  return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}
