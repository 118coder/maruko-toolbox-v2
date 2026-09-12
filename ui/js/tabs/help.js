// tabs/help.js —— 帮助页（关于 / 更新）

import { invoke } from '../bridge.js';
import { $, toast } from '../components.js';

export function initHelp() {
  $('#hCheckUpdate').addEventListener('click', async () => {
    const r = await invoke('check_update');
    $('#hUpdateResult').textContent = r.msg;
    toast(r.msg, r.ok ? 'ok' : '');
  });
}
