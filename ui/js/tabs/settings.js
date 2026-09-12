// tabs/settings.js —— 设置页

import { invoke, pickFile, pickFolder, openPath } from '../bridge.js';
import { $, toast, confirmModal } from '../components.js';
import { state, saveSettings } from '../state.js';
import { applyLanguage } from '../i18n.js';

export function initSettings() {
  const s = state.settings;
  $('#stLang').value = s.language;
  $('#stSplash').checked = s.splash;
  $('#stTray').checked = s.trayMode;
  $('#stDark').checked = s.dark;
  $('#stPriority').value = s.x264Priority;
  $('#stThreads').value = String(s.x264Threads);
  $('#stExtra').value = s.x264Extra;
  $('#stPlayer').value = s.previewPlayer;
  $('#stToolsDir').value = s.toolsDir;
  $('#stFfmpeg').value = s.ffmpegPath;
  $('#stDelTemp').checked = s.deleteTemp;
  $('#stUpdate').checked = s.checkUpdate;
  $('#stX265').checked = s.enableX265;

  const bind = (id, key, transform = (v) => v, after) => {
    $(id).addEventListener('change', async () => {
      state.settings[key] = transform($(id));
      await saveSettings(true);
      after?.();
      toast('设置已保存', 'ok');
    });
  };
  bind('#stLang', 'language', (e) => e.value, () => applyLanguage(state.settings.language));
  bind('#stSplash', 'splash', (e) => e.checked);
  bind('#stTray', 'trayMode', (e) => e.checked);
  bind('#stDark', 'dark', (e) => e.checked, () =>
    document.documentElement.classList.toggle('dark', state.settings.dark));
  bind('#stPriority', 'x264Priority', (e) => e.value);
  bind('#stThreads', 'x264Threads', (e) => parseInt(e.value) || 0);
  bind('#stExtra', 'x264Extra', (e) => e.value);
  bind('#stPlayer', 'previewPlayer', (e) => e.value);
  bind('#stDelTemp', 'deleteTemp', (e) => e.checked);
  bind('#stUpdate', 'checkUpdate', (e) => e.checked);
  bind('#stX265', 'enableX265', (e) => e.checked, () => location.reload());

  $('#stPickPlayer').addEventListener('click', async () => {
    const p = await pickFile('选择预览播放器 exe', [{ name: '程序', exts: ['exe'] }], true);
    if (p) { $('#stPlayer').value = p; state.settings.previewPlayer = p; await saveSettings(true); }
  });
  $('#stPickTools').addEventListener('click', async () => {
    const d = await pickFolder('选择原版小丸 tools 目录');
    if (d) {
      $('#stToolsDir').value = d;
      state.settings.toolsDir = d;
      await saveSettings(true);
      await reloadTools();
    }
  });
  $('#stPickFfmpeg').addEventListener('click', async () => {
    const p = await pickFile('选择 ffmpeg.exe', [{ name: '程序', exts: ['exe'] }], true);
    if (p) {
      $('#stFfmpeg').value = p;
      state.settings.ffmpegPath = p;
      await saveSettings(true);
      await reloadTools();
    }
  });

  $('#stReset').addEventListener('click', async () => {
    if (!(await confirmModal('还原默认设置', '所有设置将恢复默认值，确定继续？'))) return;
    state.settings = await invoke('reset_settings');
    applyLanguage(state.settings.language);
    document.documentElement.classList.toggle('dark', !!state.settings.dark);
    location.reload();
  });
  $('#stViewLog').addEventListener('click', () => invoke('open_logs_dir'));
  $('#stDelLog').addEventListener('click', async () => {
    if (!(await confirmModal('删除日志', '删除全部日志文件？'))) return;
    await invoke('delete_logs');
    toast('日志已删除', 'ok');
  });

  // 环境说明（工具链定位结果）
  renderEnvNotes();
}

async function reloadTools() {
  toast('正在重新定位工具链…');
  const { loadTools } = await import('../state.js');
  await loadTools();
  const { refreshEncoders } = await import('./video.js');
  refreshEncoders();
  renderEnvNotes();
  toast('工具链已更新', 'ok');
}

function renderEnvNotes() {
  const el = document.getElementById('stEnvNote');
  el.innerHTML = (state.tools?.notes || []).map((n) => `· ${n}`).join('<br>');
}
