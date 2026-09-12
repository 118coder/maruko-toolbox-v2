// tabs/avs.js —— AVS 页（滤镜生成 / 预览 / 压制）
// 滤镜以结构化对象传给后端（后端按 D6 顺序拼脚本）；脚本框非空时原样使用。

import { invoke, pickFile, convertFileSrc } from '../bridge.js';
import { $, bindFileField, toast } from '../components.js';
import { state, encoderTag, taggedName } from '../state.js';
import { trackJob } from '../jobs.js';

export function initAvs() {
  bindFileField($('#sVideo'), { kind: 'video', title: '选择视频' });
  bindFileField($('#sSub'), { kind: 'subtitle', title: '选择字幕' });
  bindFileField($('#sOutput'), { kind: 'video', title: '选择输出文件', save: true, onSet: () => { $('#sOutput').dataset.auto = ''; } });
  // 输出自动命名：与视频页编码器一致（测试.mp4 → 测试x264.mp4）
  $('#sVideo').addEventListener('change', autoFillOutput);
  $('#vEncoder').addEventListener('change', () => { if (isAutoOutput()) autoFillOutput(); });
  $('#sPickVideo').addEventListener('click', () => $('#sVideo').click());
  $('#sPickSub').addEventListener('click', () => $('#sSub').click());
  $('#sPickOutput').addEventListener('click', () => $('#sOutput').click());

  // 外置滤镜下拉（仅向脚本框插入 LoadPlugin + 调用模板行）
  const sel = $('#fExternal');
  for (const p of state.tools?.avsPlugins || []) {
    const o = document.createElement('option');
    o.value = p;
    o.textContent = p;
    sel.appendChild(o);
  }
  $('#fInsert').addEventListener('click', () => {
    const name = sel.value;
    if (!name) return toast('请先选择一个外置滤镜', 'err');
    const tools = (state.tools?.toolsDir || '').replace(/\\/g, '/');
    const base = name.replace(/\.(dll|avsi)$/i, '');
    const line = `LoadPlugin("${tools}/avs/plugins/${name}")\n${base}()\n`;
    $('#sScript').value = $('#sScript').value.replace(/\s*$/, '\n') + line;
    toast(`已插入 ${name}（模板行，请自行补参数）`, 'ok');
  });

  $('#sClear').addEventListener('click', () => { $('#sScript').value = ''; });

  $('#sSave').addEventListener('click', async () => {
    const p = await pickFile('保存 AVS', [{ name: 'AVS 脚本', exts: ['avs'] }], true);
    if (!p) return;
    try {
      await invoke('save_avs', { path: p.endsWith('.avs') ? p : p + '.avs', content: scriptText() });
      toast('AVS 已保存', 'ok');
    } catch (e) {
      toast(String(e), 'err');
    }
  });

  $('#sPreview').addEventListener('click', async () => {
    if (!$('#sVideo').value) return toast('请先选择视频', 'err');
    toast('正在渲染预览（≤96 帧）…');
    try {
      const r = await invoke('avs_preview', { job: buildJob() });
      if (r.ok) {
        $('#previewVideo').src = convertFileSrc(r.path);
        $('#previewOverlay').classList.add('show');
      } else {
        toast(r.error || '预览失败', 'err');
      }
    } catch (e) {
      toast(String(e), 'err');
    }
  });
  $('#previewClose').addEventListener('click', () => {
    $('#previewVideo').pause();
    $('#previewOverlay').classList.remove('show');
  });

  $('#sGo').addEventListener('click', async () => {
    if (!$('#sVideo').value) return toast('请先选择视频', 'err');
    try {
      const id = await invoke('encode_video', { job: buildJob() });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  });
}

/** 脚本文本：手动脚本原样；否则返回由勾选滤镜生成的"滤镜部分"（不含 Source 行） */
function isAutoOutput() {
  return $('#sOutput').dataset.auto === '1';
}

function autoFillOutput() {
  const v = $('#sVideo').value;
  if (!v) return;
  $('#sOutput').value = taggedName(v, encoderTag($('#vEncoder').value), 'mp4');
  $('#sOutput').dataset.auto = '1';
}

function scriptText() {
  const manual = $('#sScript').value.trim();
  if (manual) return manual;
  const lines = [];
  const num = (id) => parseFloat($(id).value) || 0;
  if ($('#fTweak').checked) lines.push(`Tweak(hue=${num('#fTweakHue')}, sat=${num('#fTweakSat')}, bright=${num('#fTweakBright')}, cont=${num('#fTweakCont')})`);
  if ($('#fResize').checked) lines.push(`LanczosResize(${parseInt(num('#fW')) || 1280}, ${parseInt(num('#fH')) || 720})`);
  if ($('#fBorders').checked) lines.push(`AddBorders(${num('#fBL')}, ${num('#fBT')}, ${num('#fBR')}, ${num('#fBB')})`);
  if ($('#fCrop').checked) lines.push(`# Crop(${ $('#fCropText').value })`);
  if ($('#fLevels').checked) lines.push(`Levels(0, ${num('#fLevelsV')}, 255, 0, 255)`);
  if ($('#fSharpen').checked) lines.push(`Sharpen(${num('#fSharpenV')})`);
  if ($('#fUndot').checked) lines.push('Undot()');
  if ($('#fTrim').checked) lines.push(`Trim(${parseInt(num('#fTrimS'))}, ${parseInt(num('#fTrimE'))})`);
  return lines.join('\n');
}

/** 结构化滤镜（后端 avs.rs AvsFilters 对应） */
function structuredFilters() {
  const num = (id) => parseFloat($(id).value) || 0;
  const opt = (on, v) => (on ? v : null);
  return {
    tweak: opt($('#fTweak').checked, [num('#fTweakHue'), num('#fTweakSat'), num('#fTweakBright'), num('#fTweakCont')]),
    resize: opt($('#fResize').checked, [parseInt(num('#fW')) || 1280, parseInt(num('#fH')) || 720]),
    borders: opt($('#fBorders').checked, [num('#fBL'), num('#fBT'), num('#fBR'), num('#fBB')]),
    crop: opt($('#fCrop').checked, $('#fCropText').value),
    trim: opt($('#fTrim').checked, [parseInt(num('#fTrimS')) || 0, parseInt(num('#fTrimE')) || 0]),
    levels: opt($('#fLevels').checked, num('#fLevelsV')),
    sharpen: opt($('#fSharpen').checked, num('#fSharpenV')),
    undot: $('#fUndot').checked,
  };
}

function buildJob() {
  const manual = !!$('#sScript').value.trim();
  // 编码器/码控设置直接复用视频页的 DOM（同一窗口内的共享设置，对齐原版行为）
  const mode = document.querySelector('input[name=vmode]:checked')?.value || 'crf';
  return {
    input: $('#sVideo').value,
    output: $('#sOutput').value || '',
    encoder: $('#vEncoder').value || '',
    demuxer: $('#vDemuxer').value || 'auto',
    mode,
    crf: parseFloat($('#vCrf').value) || 23.5,
    bitrate: parseInt($('#vCrf').value) || 800,
    startFrame: 0,
    frames: 0,
    width: 0,
    height: 0,
    keepRes: true,
    audioMode: $('#sAudio').checked ? 'encode' : 'copy',
    audioEncoder: state.audio.encoder,
    audioBitrate: state.audio.bitrate,
    subtitle: $('#sSub').value || '',
    container: $('#vContainer').value || 'mp4',
    avsScript: manual ? scriptText() : '',
    avsFilters: manual ? null : structuredFilters(),
    shutdownAfter: false,
  };
}

export function handleDrop(paths) {
  if (paths[0]) {
    if (/\.(ass|ssa|srt)$/i.test(paths[0])) $('#sSub').value = paths[0];
    else {
      $('#sVideo').value = paths[0];
      $('#sVideo').dispatchEvent(new Event('change'));
    }
  }
}
