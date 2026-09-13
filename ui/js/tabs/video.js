// tabs/video.js —— 视频压制页

import { invoke } from '../bridge.js';
import { $, bindFileField, makeListBox, initNumBoxes, toast, setFieldValue } from '../components.js';
import { state, videoEncoderOptions, isGpu, isProEncoder, saveSettings, encoderTag, taggedName } from '../state.js';
import { trackJob } from '../jobs.js';

let batch;

export function initVideo() {
  // 文件字段
  bindFileField($('#vVideo'), { kind: 'video', title: '选择视频' });
  bindFileField($('#vOutput'), { kind: 'video', title: '选择输出文件', save: true, onSet: () => { $('#vOutput').dataset.auto = ''; } });
  bindFileField($('#vSubtitle'), { kind: 'subtitle', title: '选择字幕文件' });
  $('#vPickVideo').addEventListener('click', () => $('#vVideo').click());
  $('#vPickOutput').addEventListener('click', () => $('#vOutput').click());
  $('#vPickSubtitle').addEventListener('click', () => $('#vSubtitle').click());

  // 输出名自动生成：源目录/名+编码器标签+容器（测试.mp4 → 测试x264.mp4）
  // 用户手动选择过输出后不再覆盖（dataset.auto 清空），直到下次输入变化
  $('#vVideo').addEventListener('change', autoFillOutput);
  $('#vEncoder').addEventListener('change', () => { syncProPanel(); if (isAutoOutput()) autoFillOutput(); refreshGpuHint(); });
  $('#vContainer').addEventListener('change', () => { if (isAutoOutput()) autoFillOutput(); });

  // 保持原分辨率 → 宽/高锁定
  const setResLocked = () => {
    const dis = $('#vKeepRes').checked;
    $('#vWidth').disabled = dis;
    $('#vHeight').disabled = dis;
  };
  $('#vKeepRes').addEventListener('change', setResLocked);
  setResLocked();

  initNumBoxes();
  syncProPanel();

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
    const tag = encoderTag($('#vEncoder').value);
    const suffix = $('#vBurnSub').checked ? $('#vSubSuffix').value : 'none';
    const jobs = files.map((f) => {
      const stem = f.replace(/^.*[\\/]/, '').replace(/\.[^.]+$/, '');
      const out = state.batchOutputDir
        ? state.batchOutputDir.replace(/[\\/]+$/, '') + '\\' + stem + tag + '.' + container
        : taggedName(f, tag, container);
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

function isAutoOutput() {
  return $('#vOutput').dataset.auto === '1';
}

/* ---- Voukoder 风格专业编码器面板 ---- */
const PRO_CONTAINERS = { prores_ks: 'mov', cfhd: 'mov', ffv1: 'mkv', utvideo: 'mkv', 'libvpx-vp9': 'webm' };
const NO_RATE_CONTROLS = ['prores_ks', 'cfhd', 'ffv1', 'utvideo'];
const PRO_ROWS = { prores_ks: 'row-prores', cfhd: 'row-cfhd', ffv1: 'row-ffv1', utvideo: 'row-utvideo', 'libvpx-vp9': 'row-vp9', libx265: 'row-x265', libx264: 'row-x264' };

// iAvoe x265 调参预设（参数集来源：Voukoder 内置预设 / 压制社区 iAvoe 方案，致敬原作者）
const X265_PRESETS = {
  general8: { label: '通用 8bit', pix_fmt: 'yuv420p', crf: '19.0', preset: 'slow', params: { ctu: '64', 'min-cu-size': '16', ref: '3', hash: '2', 'limit-tu': '1', 'tu-intra-depth': '2', 'tu-inter-depth': '2', me: 'umh', subme: '5', merange: '48', rskip: '1', weightb: '1', mcstf: '0', 'early-skip': '1', 'max-merge': '2', 'min-keyint': '5', fades: '1', bframes: '11', 'b-adapt': '2', radl: '2', 'fast-intra': '1', 'hist-scenecut': '0', crqpoffs: '-2', 'aq-mode': '4', 'aq-motion': '1', 'qg-size': '16', rd: '3', rdpenalty: '1', 'splitrd-skip': '1', 'rdoq-level': '1', 'limit-modes': '1', rect: '1', 'tskip-fast': '1', 'limit-sao': '1', 'sao-non-deblock': '1', deblock: '0:-1', 'open-gop': '0', 'allow-non-conformance': '1' } },
  general10: { label: '通用 10bit', pix_fmt: 'yuv420p10le', crf: '19.0', preset: 'slow', params: { ctu: '64', 'min-cu-size': '16', ref: '3', hash: '2', 'limit-tu': '1', 'tu-intra-depth': '2', 'tu-inter-depth': '2', me: 'umh', subme: '5', merange: '48', rskip: '1', weightb: '1', mcstf: '0', 'early-skip': '1', 'max-merge': '2', 'min-keyint': '5', fades: '1', bframes: '11', 'b-adapt': '2', radl: '2', 'fast-intra': '1', 'hist-scenecut': '0', crqpoffs: '-2', 'aq-mode': '4', 'aq-motion': '1', 'qg-size': '16', rd: '3', rdpenalty: '1', 'splitrd-skip': '1', 'rdoq-level': '1', 'limit-modes': '1', rect: '1', 'tskip-fast': '1', 'limit-sao': '1', 'sao-non-deblock': '1', deblock: '0:-1', 'open-gop': '0', 'allow-non-conformance': '1' } },
  anime10: { label: '动漫 / 低帧率 10bit', pix_fmt: 'yuv420p10le', crf: '22', preset: 'slow', params: { ctu: '64', ref: '3', hash: '2', 'min-cu-size': '16', 'limit-tu': '1', 'tu-intra-depth': '4', 'tu-inter-depth': '4', me: 'umh', subme: '3', merange: '48', weightb: '1', mcstf: '0', 'max-merge': '4', keyint: '480', 'min-keyint': '3', fades: '1', bframes: '16', 'b-adapt': '2', radl: '3', 'bframe-bias': '20', 'hist-scenecut': '0', 'no-fast-intra': '1', 'constrained-intra': '1', 'b-intra': '1', qpmin: '8', crqpoffs: '-4', cbqpoffs: '-2', ipratio: '1.6', pbratio: '1.3', 'cu-lossless': '1', tskip: '1', 'aq-mode': '1', 'aq-strength': '0.9', 'qg-size': '8', rd: '3', 'splitrd-skip': '1', 'rdoq-level': '2', rskip: '1', 'limit-modes': '1', 'limit-refs': '1', rect: '1', amp: '1', 'tskip-fast': '1', 'psy-rd': '1.5', 'rd-refine': '0', rdpenalty: '2', 'qp-adaptation-range': '3', deblock: '0:-1', 'limit-sao': '1', 'sao-non-deblock': '1', 'open-gop': '0', 'allow-non-conformance': '1' } },
  high8: { label: '电影级高压缩 8bit', pix_fmt: 'yuv420p', crf: '21.8', preset: 'slow', params: { ctu: '64', ref: '3', hash: '2', 'min-cu-size': '16', 'limit-tu': '1', 'tu-intra-depth': '4', 'tu-inter-depth': '4', me: 'star', subme: '5', merange: '48', 'analyze-src-pics': '1', weightb: '1', mcstf: '0', 'max-merge': '4', keyint: '480', 'min-keyint': '3', fades: '1', bframes: '14', 'b-adapt': '2', radl: '3', 'hist-scenecut': '0', 'no-fast-intra': '1', 'constrained-intra': '1', 'b-intra': '1', qpmin: '8', crqpoffs: '-3', ipratio: '1.2', pbratio: '1.5', 'aq-mode': '4', 'aq-strength': '0.9', 'qg-size': '8', rd: '3', 'splitrd-skip': '1', 'rdoq-level': '2', 'limit-modes': '1', 'limit-refs': '0', 'no-rskip': '1', rect: '1', amp: '1', 'tskip-fast': '1', 'psy-rd': '1.6', 'rd-refine': '0', rdpenalty: '1', 'qp-adaptation-range': '3', deblock: '0:0', 'limit-sao': '1', 'sao-non-deblock': '1', 'open-gop': '0', 'allow-non-conformance': '1' } },
  high10: { label: '电影级高压缩 10bit', pix_fmt: 'yuv420p10le', crf: '21.8', preset: 'slow', params: { ctu: '64', ref: '3', hash: '2', 'min-cu-size': '16', 'limit-tu': '1', 'tu-intra-depth': '4', 'tu-inter-depth': '4', me: 'star', subme: '5', merange: '48', 'analyze-src-pics': '1', weightb: '1', mcstf: '0', 'early-skip': '1', 'max-merge': '4', keyint: '480', 'min-keyint': '3', fades: '1', bframes: '14', 'b-adapt': '2', radl: '3', 'no-fast-intra': '1', 'hist-scenecut': '0', 'constrained-intra': '1', 'b-intra': '1', qpmin: '8', crqpoffs: '-3', ipratio: '1.2', pbratio: '1.5', 'aq-mode': '4', 'aq-strength': '0.9', 'qg-size': '8', rd: '3', 'splitrd-skip': '1', 'rdoq-level': '2', 'limit-modes': '1', 'limit-refs': '0', 'no-rskip': '1', rect: '1', amp: '1', 'tskip-fast': '1', 'psy-rd': '1.6', 'rd-refine': '0', rdpenalty: '1', 'qp-adaptation-range': '3', deblock: '0:0', 'limit-sao': '1', 'sao-non-deblock': '1', 'open-gop': '0', 'allow-non-conformance': '1' } },
  fast: { label: '快速', pix_fmt: 'yuv420p', crf: '21', preset: 'faster', params: {} }
};
const X264_PRESETS = { general: { label: '通用', preset: 'slow', crf: '19' }, fast: { label: '快速', preset: 'faster', crf: '20' }, high: { label: '高压缩', preset: 'veryslow', crf: '19' } };

/** 根据所选编码器：显示对应参数行、自动切换容器、禁用不适用的码控选项 */
function syncProPanel() {
  const id = $('#vEncoder').value;
  const isPro = isProEncoder(id);
  $('#proParams').style.display = isPro ? 'block' : 'none';
  if (!isPro) {
    setRateControlsEnabled(true);
    return;
  }
  for (const row of document.querySelectorAll('#proParams .row')) {
    row.style.display = 'none';
  }
  const rowId = PRO_ROWS[id];
  const row = document.getElementById(rowId);
  if (row) row.style.display = 'flex';
  const wantContainer = PRO_CONTAINERS[id];
  if (wantContainer && $('#vContainer').value !== wantContainer) {
    $('#vContainer').value = wantContainer;
    if (isAutoOutput()) autoFillOutput();
  }
  const allowRate = !NO_RATE_CONTROLS.includes(id);
  setRateControlsEnabled(allowRate);
  // ffmpeg 直编管线暂未实现 2Pass：仅软编（AVS 管线）开放
  const isSoft = !isProEncoder(id) && !isGpu(id);
  const r2 = document.querySelector('input[name=vmode][value="2pass"]');
  if (r2) {
    r2.disabled = !isSoft;
    r2.closest('label.ck').style.opacity = isSoft ? '1' : '0.45';
  }
  if (id === 'libx265') {
    // 切预设时自动填 CRF
    const t = X265_PRESETS[$('#pX265Preset').value];
    if (t && allowRate) $('#vCrf').value = t.crf;
  } else if (id === 'libx264') {
    const t = X264_PRESETS[$('#pX264Preset').value];
    if (t && allowRate) $('#vCrf').value = t.crf;
  }
}

function setRateControlsEnabled(enabled) {
  for (const r of document.querySelectorAll('input[name=vmode]')) {
    r.disabled = !enabled;
    r.closest('label.ck').style.opacity = enabled ? '1' : '0.45';
  }
  const crf = $('#vCrf');
  crf.disabled = !enabled;
  crf.closest('.num').style.opacity = enabled ? '1' : '0.45';
}

function autoFillOutput() {
  const input = $('#vVideo').value;
  if (!input) return;
  $('#vOutput').value = taggedName(input, encoderTag($('#vEncoder').value), $('#vContainer').value);
  $('#vOutput').dataset.auto = '1';
}

/** 收集当前所选专业编码器的参数 */
function collectProOptions() {
  const id = $('#vEncoder').value;
  if (!isProEncoder(id)) return {};
  const val = (sel) => $(sel)?.value || '';
  switch (id) {
    case 'prores_ks':
      return { profile: val('#pProResProfile'), qscale: $('#pProResQ')?.value || '9' };
    case 'cfhd':
      return { quality: val('#pCfhdQuality') };
    case 'ffv1':
      return { coder: val('#pFfv1Coder') };
    case 'utvideo':
      return { pred: val('#pUtPred') };
    case 'libvpx-vp9':
      return { deadline: val('#pVp9Deadline'), cpu_used: val('#pVp9Cpu') };
    case 'libx265': {
      const t = X265_PRESETS[$('#pX265Preset')?.value || 'general8'];
      if (!t) return {};
      // 预设参数 → x265-params（值内的冒号需转义），preset 单独走 -preset
      const extra = Object.entries(t.params)
        .map(([k, v]) => `${k}=${v.replace(/:/g, '\\:')}`)
        .join(':');
      const o = { preset: t.preset, pix_fmt: t.pix_fmt };
      if (extra) o['x265-params'] = extra;
      return o;
    }
    case 'libx264':
      return { preset: (X264_PRESETS[$('#pX264Preset')?.value] || {}).preset || 'slow' };
    case 'h264_nvenc':
    case 'hevc_nvenc':
    case 'av1_nvenc':
      return {
        preset: val('#pNvPreset'),
        multipass: val('#pNvMultipass'),
        'spatial-aq': val('#pNvSpatialAQ'),
        lookahead: val('#pNvLookahead'),
      };
    default:
      return {};
  }
}

function refreshGpuHint() {
  const id = $('#vEncoder').value;
  $('#vEncoder').title = isGpu(id)
    ? 'GPU 编码器：不支持 2Pass 与 AVS 滤镜管线'
    : isProEncoder(id)
      ? '专业编码器（ffmpeg 后端）：不支持 2Pass 与 AVS 滤镜管线'
      : '';
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
    encOptions: collectProOptions(),
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
  refreshGpuHint();
  syncProPanel();
  if (isAutoOutput()) autoFillOutput();
}

/** 全局拖放进入本页时：把文件路径分配到对应字段 */
export function handleDrop(paths, target) {
  const map = { vVideo: 'video', vBatch: 'batch', vSubtitle: 'subtitle' };
  if (target === 'vSubtitle') {
    setFieldValue($('#vSubtitle'), paths[0]);
  } else if (target === 'vBatch') {
    for (const p of paths) batch.add(p);
  } else if (target === 'vVideo' || target === 'vVideoAny') {
    setFieldValue($('#vVideo'), paths[0]);
    if (paths.length > 1) {
      for (const p of paths) batch.add(p);
    }
  } else {
    // 默认：第一个进视频，其余进批量
    setFieldValue($('#vVideo'), paths[0]);
    for (const p of paths.slice(1)) batch.add(p);
  }
}
