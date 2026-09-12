// tabs/video.js —— 视频压制页

import { invoke } from '../bridge.js';
import { $, bindFileField, makeListBox, initNumBoxes, toast } from '../components.js';
import { state, videoEncoderOptions, isGpu, saveSettings } from '../state.js';
import { trackJob } from '../jobs.js';

let batch;

export function initVideo() {
  // 文件字段
  bindFileField($('#vVideo'), { kind: 'video', title: '选择视频' });
  bindFileField($('#vOutput'), { kind: 'video', title: '选择输出文件', save: true });
  bindFileField($('#vSubtitle'), { kind: 'subtitle', title: '选择字幕文件' });
  $('#vPickVideo').addEventListener('click', () => $('#vVideo').click());
  $('#vPickOutput').addEventListener('click', () => $('#vOutput').click());
  $('#vPickSubtitle').addEventListener('click', () => $('#vSubtitle').click());

  initNumBoxes();

  // 编码器下拉（工具信息加载后由 refreshEncoders 填充）
  // 模式切换：CRF ⇄ 码率 共用同一个数字框
  for (const r of document.querySelectorAll('input[name=vmode]')) {
    r.addEventListener('change', () => {
      const m = document.querySelector('input[name=vmode]:checked').value;
      const lbl = $('#vCrf')?.closest('.row')?.querySelector('.lbl');
      if (m === 'bitrate') {
        lbl.textContent = '码率';
        $('#vCrf').value = '800';
        for (const c of ['#vCrf', ...[]]) {}
        $('#vCrf').dataset.step = '100';
      } else {
        lbl.textContent = 'CRF';
        $('#vCrf').value = '23.5';
        $('#vCrf').dataset.step = '0.1';
      }
    });
  }

  // 音频参数弹窗
  $('#vAudioParams').addEventListener('click', () => {
    $('#apMode').value = $('#vAudioMode').value;
    fillAudioEncoders($('#apEncoder'));
    const a = state.audio;
    $('#apEncoder').value = a.encoder;
    $('#apBitrate').value = String(a.bitrate);
    $('#audioParamsOverlay').classList.add('show');
  });
  $('#apOk').addEventListener('click', () => {
    $('#vAudioMode').value = $('#apMode').value;
    state.audio.encoder = $('#apEncoder').value;
    state.audio.bitrate = parseInt($('#apBitrate').value) || 128;
    saveSettings();
    $('#audioParamsOverlay').classList.remove('show');
  });
  $('#vAudioMode').addEventListener('change', () => { state.audio.mode = $('#vAudioMode').value; });

  // 单个压制
  $('#vEncode').addEventListener('click', async () => {
    const input = $('#vVideo').value;
    if (!input) return toast('请先选择视频文件', 'err');
    const job = buildJob(input, $('#vOutput').value || '');
    try {
      const id = await invoke('encode_video', { job });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  });

  // 批量
  batch = makeListBox($('#vBatch'));
  $('#vBatchAdd').addEventListener('click', async () => {
    const p = await import('../bridge.js').then((b) => b.pickFile('选择视频', [{ name: '视频', exts: ['mp4','mkv','flv','avi','mov','ts','m2ts','webm','wmv','rmvb','mpg','mpeg','vob','m4v','3gp'] }]));
    if (p) batch.add(p);
  });
  $('#vBatchDel').addEventListener('click', () => batch.removeSel());
  $('#vBatchClear').addEventListener('click', () => batch.clear());
  $('#vBatchOut').addEventListener('click', async () => {
    const { pickFolder } = await import('../bridge.js');
    const d = await pickFolder('选择批量输出路径');
    if (d) {
      state.batchOutputDir = d;
      toast(`批量输出路径：${d}`, 'ok');
    }
  });
  $('#vBatchGo').addEventListener('click', async () => {
    const files = batch.all();
    if (!files.length) return toast('批量列表为空', 'err');
    const container = $('#vContainer').value;
    const suffix = $('#vBurnSub').checked ? $('#vSubSuffix').value : 'none';
    const jobs = files.map((f) => {
      const out = state.batchOutputDir
        ? state.batchOutputDir.replace(/[\\/]+$/, '') + '\\' + f.replace(/^.*[\\/]/, '').replace(/\.[^.]+$/, '') + '.' + container
        : '';
      return buildJob(f, out, container);
    });
    try {
      const ids = await invoke('encode_video_batch', { jobs, subSuffix: suffix });
      trackJob(ids[ids.length - 1]);
    } catch (e) {
      toast(String(e), 'err');
    }
  });
}

function buildJob(input, output, containerOverride) {
  const mode = document.querySelector('input[name=vmode]:checked').value;
  const gpu = isGpu($('#vEncoder').value);
  return {
    input,
    output,
    encoder: $('#vEncoder').value,
    demuxer: $('#vDemuxer').value,
    mode: mode === 'bitrate' ? 'bitrate' : mode,
    crf: parseFloat($('#vCrf').value) || 23.5,
    bitrate: parseInt($('#vCrf').value) || 800,
    startFrame: parseInt($('#vStart').value) || 0,
    frames: parseInt($('#vFrames').value) || 0,
    width: parseInt($('#vWidth').value) || 0,
    height: parseInt($('#vHeight').value) || 0,
    keepRes: $('#vKeepRes').checked,
    audioMode: $('#vAudioMode').value,
    audioEncoder: state.audio.encoder,
    audioBitrate: state.audio.bitrate,
    subtitle: $('#vSubtitle').value || '',
    container: containerOverride || $('#vContainer').value,
    avsScript: '',
    shutdownAfter: $('#vShutdown').checked,
  };
}

function fillAudioEncoders(sel) {
  sel.innerHTML = '';
  for (const e of state.tools?.audioEncoders || []) {
    if (!e.available) continue;
    const o = document.createElement('option');
    o.value = e.id;
    o.textContent = e.label;
    sel.appendChild(o);
  }
  sel.value = state.audio.encoder;
}

/** 工具信息加载后刷新编码器下拉（由 main.js 调用） */
export function refreshEncoders() {
  const sel = $('#vEncoder');
  const cur = sel.value;
  sel.innerHTML = '';
  const opts = videoEncoderOptions();
  if (!opts.length) {
    const o = document.createElement('option');
    o.value = '';
    o.textContent = '（未发现可用编码器）';
    sel.appendChild(o);
    return;
  }
  for (const e of opts) {
    const o = document.createElement('option');
    o.value = e.id;
    o.textContent = e.label;
    sel.appendChild(o);
  }
  if (cur && opts.some((e) => e.id === cur)) sel.value = cur;
  // GPU 2Pass 禁用提示
  sel.title = isGpu(sel.value) ? 'GPU 编码器：不支持 2Pass 与 AVS 滤镜管线' : '';
}

/** 全局拖放进入本页时：把文件路径分配到对应字段 */
export function handleDrop(paths, target) {
  const map = { vVideo: 'video', vBatch: 'batch', vSubtitle: 'subtitle' };
  if (target === 'vSubtitle') {
    $('#vSubtitle').value = paths[0];
  } else if (target === 'vBatch') {
    for (const p of paths) batch.add(p);
  } else if (target === 'vVideo' || target === 'vVideoAny') {
    $('#vVideo').value = paths[0];
    if (paths.length > 1) {
      for (const p of paths) batch.add(p);
    }
  } else {
    // 默认：第一个进视频，其余进批量
    $('#vVideo').value = paths[0];
    for (const p of paths.slice(1)) batch.add(p);
  }
}
