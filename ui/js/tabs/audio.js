// tabs/audio.js —— 音频压制页

import { invoke, pickFile } from '../bridge.js';
import { $, bindFileField, makeListBox, toast } from '../components.js';
import { state, saveSettings, audioEncoderTag, taggedName } from '../state.js';
import { trackJob } from '../jobs.js';

let batch;

export function initAudio() {
  bindFileField($('#aInput'), { kind: 'audio', title: '选择音频' });
  bindFileField($('#aOutput'), { kind: 'audio', title: '选择输出文件', save: true, onSet: () => { $('#aOutput').dataset.auto = ''; } });
  $('#aPickInput').addEventListener('click', () => $('#aInput').click());
  $('#aPickOutput').addEventListener('click', () => $('#aOutput').click());

  // 编码器下拉
  const sel = $('#aEncoder');
  sel.innerHTML = '';
  for (const e of state.tools?.audioEncoders || []) {
    if (!e.available) continue;
    const o = document.createElement('option');
    o.value = e.id;
    o.textContent = e.label;
    sel.appendChild(o);
  }
  sel.value = state.audio.encoder;

  // 输出名自动生成：源目录/名+编码器标签+扩展名（测试.wav → 测试nero.m4a）
  $('#aInput').addEventListener('change', autoFillOutput);
  $('#aEncoder').addEventListener('change', () => { if (isAutoOutput()) autoFillOutput(); });

  // 模式切换
  for (const r of document.querySelectorAll('input[name=amode]')) {
    r.addEventListener('change', () => {
      const m = document.querySelector('input[name=amode]:checked').value;
      $('#aBitrateRow').style.display = m === 'bitrate' ? '' : 'none';
      $('#aCustomRow').style.display = m === 'custom' ? '' : 'none';
    });
  }
  const sync = () => {
    state.audio.encoder = sel.value;
    state.audio.bitrate = parseInt($('#aBitrate').value) || 128;
    state.audio.custom = $('#aCustom').value;
    state.audio.mode = document.querySelector('input[name=amode]:checked').value;
    saveSettings();
  };
  [sel, $('#aBitrate'), $('#aCustom')].forEach((e) => e.addEventListener('change', sync));

  $('#aEncode').addEventListener('click', async () => {
    const input = $('#aInput').value;
    if (!input) return toast('请先选择音频文件', 'err');
    const job = buildJob(input, $('#aOutput').value || '');
    try {
      const id = await invoke('encode_audio', { job });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  });

  batch = makeListBox($('#aBatch'));
  $('#aBatchAdd').addEventListener('click', async () => {
    const p = await pickFile('选择音频', [{ name: '音频', exts: ['m4a','aac','mp3','flac','wav','ape','ogg','opus','wma','mka'] }]);
    if (p) batch.add(p);
  });
  $('#aBatchDel').addEventListener('click', () => batch.removeSel());
  $('#aBatchClear').addEventListener('click', () => batch.clear());
  $('#aBatchGo').addEventListener('click', async () => {
    const files = batch.all();
    if (!files.length) return toast('批量列表为空', 'err');
    const jobs = files.map((f) => buildJob(f, ''));
    try {
      const ids = await invoke('encode_audio_batch', { jobs });
      trackJob(ids[ids.length - 1]);
    } catch (e) {
      toast(String(e), 'err');
    }
  });
  $('#aMerge').addEventListener('click', async () => {
    const files = batch.all();
    if (files.length < 2) return toast('合并至少需要两个音频文件', 'err');
    try {
      const id = await invoke('merge_audio', {
        inputs: files,
        encoder: sel.value,
        bitrate: parseInt($('#aBitrate').value) || 128,
      });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  });
}

function isAutoOutput() {
  return $('#aOutput').dataset.auto === '1';
}

function autoFillOutput() {
  const input = $('#aInput').value;
  if (!input) return;
  $('#aOutput').value = taggedName(input, audioEncoderTag($('#aEncoder').value), outputExtOf($('#aEncoder').value));
  $('#aOutput').dataset.auto = '1';
}

function outputExtOf(enc) {
  return ({ neroaac: 'm4a', qaac: 'm4a', fdkaac: 'm4a', lame: 'mp3', flac: 'flac', ffmpeg_aac: 'm4a' })[enc] || 'm4a';
}

function buildJob(input, output) {
  const mode = document.querySelector('input[name=amode]:checked').value;
  return {
    input,
    output,
    encoder: $('#aEncoder').value,
    mode,
    bitrate: parseInt($('#aBitrate').value) || 128,
    custom: $('#aCustom').value,
  };
}

export function handleDrop(paths) {
  for (const p of paths) batch.add(p);
  if (paths[0] && !$('#aInput').value) {
    $('#aInput').value = paths[0];
    $('#aInput').dispatchEvent(new Event('change'));
  }
}
