// tabs/help.js —— 帮助页（关于 / 更新）

import { invoke } from '../bridge.js';
import { $, toast } from '../components.js';

export function initHelp() {
  $('#hCheckUpdate').addEventListener('click', async () => {
    const r = await invoke('check_update');
    $('#hUpdateResult').textContent = r.msg;
    toast(r.msg, r.ok ? 'ok' : '');
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
