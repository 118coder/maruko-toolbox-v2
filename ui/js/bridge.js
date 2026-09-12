// bridge.js —— 前端触达后端的唯一通道。
// 规则（docs/ARCHITECTURE.md）：任何 tab 模块不得直接使用 window.__TAURI__。

const T = () => globalThis.__TAURI__;

export const invoke = (cmd, args = {}) => T().core.invoke(cmd, args);

export const listen = (event, handler) => T().event.listen(event, handler);

/** 系统文件对话框（单选）。filters: [{name, exts:["mp4","mkv"]}] */
export const pickFile = (title, filters = [], save = false) =>
  invoke('pick_file', { title, filters, save });

export const pickFolder = (title) => invoke('pick_folder', { title });

export const openPath = (path, reveal = false) => invoke('open_path', { path, reveal });

/** 资源协议：把本地绝对路径转成 webview 可加载的 URL */
export const convertFileSrc = (path) => T().core.convertFileSrc(path);

// ---------- 任务事件 ----------
export const onJobProgress = (fn) => listen('job://progress', (e) => fn(e.payload));
export const onJobState = (fn) => listen('job://state', (e) => fn(e.payload));
export const onJobLog = (fn) => listen('job://log', (e) => fn(e.payload));
export const onQueueIdle = (fn) => listen('job://queue_idle', (e) => fn(e.payload));
export const onShutdownArmed = (fn) => listen('job://state', (e) => {
  if (e.payload?.state === 'shutdown') fn(e.payload);
});
