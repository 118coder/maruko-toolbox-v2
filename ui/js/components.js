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
    const min = box.dataset.min !== undefined ? parseFloat(box.dataset.min) : -Infinity;
    const max = box.dataset.max !== undefined ? parseFloat(box.dataset.max) : Infinity;
    const fmt = (v) => (box.dataset.dec ? Number(v).toFixed(Number(box.dataset.dec)) : String(v));
    // 统一调整入口：步进（每次实时读，模式切换可动态改 step）+ 最小/最大值保护
    const apply = (delta) => {
      const step = parseFloat(box.dataset.step || '1');
      let v = (parseFloat(input.value) || 0) + delta;
      v = Math.min(max, Math.max(min, Math.round(v * 1000) / 1000));
      input.value = fmt(v);
      input.dispatchEvent(new Event('change'));
    };
    up.addEventListener('click', () => apply(step));
    down.addEventListener('click', () => apply(-step));
    // 滚轮调值：悬停在数字框上滚动即可加减
    box.addEventListener('wheel', (e) => {
      e.preventDefault();
      apply(e.deltaY < 0 ? step : -step);
    }, { passive: false });
    // 键盘上下方向键调值
    input.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowUp') { e.preventDefault(); apply(step); }
      else if (e.key === 'ArrowDown') { e.preventDefault(); apply(-step); }
    });
  });
}

// ---------- 文件类型判断（拖放智能路由用） ----------
export const EXT_VIDEO = ['mp4', 'mkv', 'flv', 'avi', 'mov', 'ts', 'm2ts', 'webm', 'wmv', 'rmvb', 'mpg', 'mpeg', 'vob', 'm4v', '3gp', 'ogv'];
export const EXT_AUDIO = ['m4a', 'aac', 'mp3', 'flac', 'wav', 'ape', 'ogg', 'opus', 'wma', 'mka', 'm4b', 'aiff', 'alac', 'ac3', 'eac3', 'dts'];
export const EXT_SUB = ['ass', 'ssa', 'srt'];
export const EXT_IMAGE = ['png', 'jpg', 'jpeg', 'bmp', 'webp', 'gif'];

export function extOf(p) {
  const m = String(p).toLowerCase().match(/\.([a-z0-9]+)$/);
  return m ? m[1] : '';
}
export const isVideoFile = (p) => EXT_VIDEO.includes(extOf(p));
export const isAudioFile = (p) => EXT_AUDIO.includes(extOf(p));
export const isSubFile = (p) => EXT_SUB.includes(extOf(p));
export const isImageFile = (p) => EXT_IMAGE.includes(extOf(p));
export const isAvsFile = (p) => extOf(p) === 'avs';
export function baseName(p) { return String(p).replace(/^.*[\\/]/, ''); }

// ---------- 文件字段（点击选择 + 双击清空 + 拖放） ----------
export const FILTERS = {
  video: [{ name: '视频', exts: ['mp4', 'mkv', 'flv', 'avi', 'mov', 'ts', 'm2ts', 'webm', 'wmv', 'rmvb', 'mpg', 'mpeg', 'vob', 'm4v', '3gp'] }],
  audio: [{ name: '音频', exts: ['m4a', 'aac', 'mp3', 'flac', 'wav', 'ape', 'ogg', 'opus', 'wma', 'mka'] }],
  subtitle: [{ name: '字幕', exts: ['ass', 'ssa', 'srt'] }],
  image: [{ name: '图片', exts: ['png', 'jpg', 'jpeg', 'bmp', 'webp', 'gif'] }],
  avs: [{ name: 'AVS 脚本', exts: ['avs'] }],
  all: [{ name: '全部文件', exts: ['*'] }],
};

/** 字段赋值后同步 title：悬停显示完整路径（文本框会截断长路径） */
export function syncFieldTitle(field) {
  field.title = field.value ? `${field.value}\n（点击选择文件 · 双击清空）` : '';
}

export function bindFileField(field, { kind = 'all', title = '选择文件', save = false, onSet } = {}) {
  field.addEventListener('dblclick', () => {
    if (!field.value) return;
    field.value = '';
    syncFieldTitle(field);
    onSet?.('');
    toast('已清空', 'ok');
  });
  field.addEventListener('click', async () => {
    const p = kind === 'folder' ? await pickFolder(title) : await pickFile(title, FILTERS[kind] || FILTERS.all, save);
    if (p) {
      onSet?.(p);
      field.value = p;
      syncFieldTitle(field);
      // 统一触发 change：输出自动命名联动依赖它
      field.dispatchEvent(new Event('change'));
    }
  });
}

/** 拖放成功后的字段视觉反馈：闪光一下 */
export function flashField(field) {
  field.classList.remove('drop-flash');
  void field.offsetWidth; // 强制重排以重启动画
  field.classList.add('drop-flash');
  setTimeout(() => field.classList.remove('drop-flash'), 750);
}

// 把系统级拖放事件路由到指定字段（由 main.js 的全局 drop 驱动）
export function setFieldFromDrop(field, paths) {
  if (!paths?.length) return;
  setFieldValue(field, paths[0]);
  syncFieldTitle(field);
  flashField(field);
}

/** 统一赋值入口：总是触发 change（自动命名联动依赖它） */
export function setFieldValue(field, v) {
  field.value = v;
  syncFieldTitle(field);
  field.dispatchEvent(new Event('change'));
}

/** 压制按钮防抖：提交后短时间内阻止重复点击（防手滑双击提交两个相同任务） */
export function withDebounce(fn, ms = 1500) {
  let locked = false;
  return async (...args) => {
    if (locked) return;
    locked = true;
    setTimeout(() => { locked = false; }, ms);
    return fn(...args);
  };
}

// ---------- 列表（批量区） ----------
export function makeListBox(el) {
  const items = [];
  el.tabIndex = 0; // 可聚焦：支持键盘删除/移动选中
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
      d.title = it.text;
      d.innerHTML = `<span class="st ${it.cls || ''}">${it.icon || ''}</span><span style="overflow:hidden;text-overflow:ellipsis">${escapeHtml(it.text)}</span>`;
      d.addEventListener('click', () => { el._sel = i; render(); });
      el.appendChild(d);
    });
  };
  render();
  const api = {
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
  // 键盘操作：Delete 删除选中项，↑↓ 移动选中
  el.addEventListener('keydown', (e) => {
    if (e.key === 'Delete' && items.length) {
      e.preventDefault();
      api.removeSel();
    } else if (e.key === 'ArrowDown' && items.length) {
      e.preventDefault();
      el._sel = el._sel == null ? 0 : Math.min(items.length - 1, el._sel + 1);
      render();
    } else if (e.key === 'ArrowUp' && items.length) {
      e.preventDefault();
      el._sel = el._sel == null ? items.length - 1 : Math.max(0, el._sel - 1);
      render();
    }
  });
  return api;
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
