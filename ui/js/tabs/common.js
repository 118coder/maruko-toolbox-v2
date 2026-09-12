// tabs/common.js —— 常用页（一图流 / 截取 / 旋转）

import { invoke } from '../bridge.js';
import { $, bindFileField, toast, setFieldValue } from '../components.js';
import { trackJob } from '../jobs.js';

export function initCommon() {
  bindFileField($('#oImage'), { kind: 'image', title: '选择图片' });
  bindFileField($('#oAudio'), { kind: 'audio', title: '选择音频' });
  bindFileField($('#oOutput'), { kind: 'video', title: '选择输出文件', save: true, onSet: () => { $('#oOutput').dataset.auto = ''; } });
  // 一图流输出自动命名：图片目录/名一图流.mp4
  $('#oImage').addEventListener('change', () => {
    const img = $('#oImage').value;
    if (img && $('#oOutput').dataset.auto !== '0') {
      $('#oOutput').value = img.replace(/\.[^.]+$/, '') + '一图流.mp4';
      $('#oOutput').dataset.auto = '1';
    }
  });
  $('#oPickImage').addEventListener('click', () => $('#oImage').click());
  $('#oPickAudio').addEventListener('click', () => $('#oAudio').click());
  $('#oPickOutput').addEventListener('click', () => $('#oOutput').click());
  $('#oGo').addEventListener('click', async () => {
    if (!$('#oImage').value || !$('#oAudio').value) return toast('一图流需要图片和音频', 'err');
    try {
      const id = await invoke('one_pic', {
        job: {
          image: $('#oImage').value,
          audio: $('#oAudio').value,
          output: $('#oOutput').value,
          audioBitrate: parseInt($('#oBitrate').value) || 128,
          fps: parseFloat($('#oFps').value) || 1,
          crf: parseFloat($('#oCrf').value) || 24,
          duration: parseInt($('#oDuration').value) || 120,
          copyAudio: $('#oCopyAudio').checked,
        },
      });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  });

  bindFileField($('#cVideo'), { kind: 'video', title: '选择视频' });
  bindFileField($('#cOutput'), { kind: 'video', title: '选择输出文件', save: true, onSet: () => { $('#cOutput').dataset.auto = ''; } });
  // 其他组输出自动命名（标签在点截取/旋转时按动作刷新）
  $('#cVideo').addEventListener('change', () => {
    const v = $('#cVideo').value;
    if (v && $('#cOutput').dataset.auto !== '0') {
      $('#cOutput').value = v.replace(/\.[^.]+$/, '') + '截取' + (v.match(/\.[^.]+$/)?.[0] || '.mp4');
      $('#cOutput').dataset.auto = '1';
    }
  });
  $('#cPickVideo').addEventListener('click', () => $('#cVideo').click());
  $('#cPickOutput').addEventListener('click', () => $('#cOutput').click());
  $('#cCut').addEventListener('click', async () => {
    if (!$('#cVideo').value) return toast('请先选择视频', 'err');
    if (isCAuto()) $('#cOutput').value = $('#cVideo').value.replace(/\.[^.]+$/, '') + '截取' + ($('#cVideo').value.match(/\.[^.]+$/)?.[0] || '.mp4');
    try {
      const id = await invoke('cut_video', {
        job: { input: $('#cVideo').value, output: $('#cOutput').value, start: $('#cStart').value, end: $('#cEnd').value },
      });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  });
  $('#cRotate').addEventListener('click', async () => {
    if (!$('#cVideo').value) return toast('请先选择视频', 'err');
    if (isCAuto()) $('#cOutput').value = $('#cVideo').value.replace(/\.[^.]+$/, '') + '旋转.mp4';
    try {
      const id = await invoke('rotate_video', {
        job: { input: $('#cVideo').value, output: $('#cOutput').value, transpose: $('#cTranspose').value },
      });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  });
}

function isCAuto() {
  return $('#cOutput').dataset.auto === '1';
}

export function handleDrop(paths, target) {
  if (target === 'oImage' && paths[0]) setFieldValue($('#oImage'), paths[0]);
  else if (target === 'oAudio' && paths[0]) setFieldValue($('#oAudio'), paths[0]);
  else if (paths[0]) setFieldValue($('#cVideo'), paths[0]);
}
