// tabs/mediainfo.js —— MediaInfo 页

import { invoke, openPath, pickFile } from '../bridge.js';
import { $, toast } from '../components.js';

let currentFile = '';

export function initMediaInfo() {
  $('#miCopy').addEventListener('click', async () => {
    const text = $('#miText').value;
    if (!text) return toast('没有可复制的信息', 'err');
    try {
      await navigator.clipboard.writeText(text);
      toast('信息已复制到剪贴板', 'ok');
    } catch {
      const ta = document.createElement('textarea');
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      ta.remove();
      toast('信息已复制到剪贴板', 'ok');
    }
  });
  $('#miPlay').addEventListener('click', async () => {
    if (!currentFile) return toast('请先加载媒体文件', 'err');
    await openPath(currentFile);
  });
  $('#miPick').addEventListener('click', async () => {
    const p = await pickFile('选择媒体文件', [{ name: '媒体', exts: ['mp4','mkv','flv','avi','mov','ts','m2ts','webm','wmv','mpg','m4a','aac','mp3','flac','wav','ogg'] }]);
    if (p) analyze(p);
  });
}

export async function analyze(path) {
  currentFile = path;
  $('#miText').value = '正在分析…';
  const r = await invoke('mediainfo_analyze', { path });
  if (r.error) {
    $('#miText').value = `错误：${r.error}`;
    toast(r.error, 'err');
    return;
  }
  $('#miText').value = r.text;
  toast(`分析完成（来源：${r.source === 'mediainfo' ? 'MediaInfo.dll' : 'ffprobe'}）`, 'ok');
}

export function handleDrop(paths) {
  if (paths[0]) analyze(paths[0]);
}
