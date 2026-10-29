// tabs/help.js —— 帮助页（关于 / 更新：检查 → 一键下载 → 自动重启替换）

import { invoke, listen } from '../bridge.js';
import { $, toast } from '../components.js';

let lastResult = null;
let busy = false;

/** 检查更新。silent=true 用于启动自动检查（无 toast，静默失败） */
export async function doCheckUpdate(silent = false) {
  if (!silent) $('#hUpdateResult').textContent = '检查中…';
  try {
    const r = await invoke('check_update');
    lastResult = r;
    $('#hUpdateResult').textContent = r.msg;
    const hasNew = r.ok && r.msg.includes('新版本') && r.updateAsset;
    $('#hDownloadUpdate').style.display = hasNew ? '' : 'none';
    if (!silent) toast(r.msg, r.ok ? 'ok' : 'err');
    return r;
  } catch (e) {
    lastResult = null;
    $('#hUpdateResult').textContent = `检查失败：${e}`;
    $('#hDownloadUpdate').style.display = 'none';
    if (!silent) toast(`检查更新失败：${e}`, 'err');
    return null;
  }
}

function setUpdProgress(pct) {
  $('#hUpdateProgressRow').style.display = '';
  $('#hUpdateBar').style.width = pct.toFixed(1) + '%';
  $('#hUpdatePct').textContent = pct.toFixed(0) + '%';
}

export function initHelp() {
  $('#hCheckUpdate').addEventListener('click', () => doCheckUpdate(false));

  $('#hDownloadUpdate').addEventListener('click', async () => {
    if (busy || !lastResult?.updateAsset) return;
    const a = lastResult.updateAsset;
    busy = true;
    $('#hDownloadUpdate').disabled = true;
    setUpdProgress(0);
    try {
      listen('update://progress', (e) => {
        const p = e.payload || {};
        setUpdProgress(p.pct || 0);
      });
      $('#hUpdateResult').textContent = '下载中…';
      const zip = await invoke('download_update', { url: a.url, name: a.name });
      $('#hUpdateResult').textContent = '下载完成，正在应用更新（程序将自动重启）…';
      setUpdProgress(100);
      await invoke('apply_update', { zipPath: zip });
      // 应用成功后后端直接退出重启，前端不再收到返回
    } catch (e) {
      toast(`更新失败：${e}`, 'err');
      $('#hUpdateResult').textContent = `更新失败：${e}`;
      $('#hUpdateProgressRow').style.display = 'none';
      busy = false;
      $('#hDownloadUpdate').disabled = false;
    }
  });

  // 开源地址：Tauri 内导航会被拦截，走系统默认浏览器
  $('#hRepoLink').addEventListener('click', async (e) => {
    e.preventDefault();
    try {
      await invoke('open_url', { url: 'https://github.com/118coder/maruko-toolbox-v2' });
    } catch (err) {
      toast('打开浏览器失败：' + err);
    }
  });
}
