// tabs/mux.js —— 封装页（MP4 / MKV / 封装转换）

import { invoke, pickFile } from '../bridge.js';
import { $, bindFileField, makeListBox, toast, setFieldValue } from '../components.js';
import { state } from '../state.js';
import { trackJob } from '../jobs.js';

let cvBatch;

export function initMux() {
  // MP4
  bindFileField($('#m4Video'), { kind: 'video', title: '选择视频' });
  bindFileField($('#m4Audio'), { kind: 'audio', title: '选择音频' });
  bindFileField($('#m4Output'), { kind: 'video', title: '选择输出文件', save: true, onSet: () => { $('#m4Output').dataset.auto = ''; } });
  $('#m4Video').addEventListener('change', () => {
    const v = $('#m4Video').value;
    if (v && $('#m4Output').dataset.auto !== '0') {
      $('#m4Output').value = v.replace(/\.[^.]+$/, '') + '封装.mp4';
      $('#m4Output').dataset.auto = '1';
    }
  });
  for (const id of ['m4Video', 'm4Audio', 'm4Output']) {
    $('#id_' + id) // no-op 占位，按钮点击触发字段点击
  }
  $('#m4PickVideo').addEventListener('click', () => $('#m4Video').click());
  $('#m4PickAudio').addEventListener('click', () => $('#m4Audio').click());
  $('#m4PickOutput').addEventListener('click', () => $('#m4Output').click());
  $('#m4Go').addEventListener('click', async () => {
    if (!$('#m4Video').value) return toast('请先选择视频', 'err');
    try {
      const id = await invoke('mux_mp4', {
        video: $('#m4Video').value,
        audio: $('#m4Audio').value || null,
        output: $('#m4Output').value,
        fps: $('#m4Fps').value,
        par: $('#m4Par').value,
        replaceAudio: false,
      });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  });
  $('#m4Replace').addEventListener('click', async () => {
    if (!$('#m4Video').value) return toast('请先选择视频', 'err');
    if (!$('#m4Audio').value) return toast('替换音频需要选择音频文件', 'err');
    try {
      const id = await invoke('mux_mp4', {
        video: $('#m4Video').value,
        audio: $('#m4Audio').value,
        output: $('#m4Output').value,
        fps: 'auto',
        par: 'auto',
        replaceAudio: true,
      });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  });

  // MKV
  bindFileField($('#mkVideo'), { kind: 'video', title: '选择视频' });
  bindFileField($('#mkAudio'), { kind: 'audio', title: '选择音频' });
  bindFileField($('#mkSub'), { kind: 'subtitle', title: '选择字幕' });
  bindFileField($('#mkOutput'), { kind: 'video', title: '选择输出文件', save: true, onSet: () => { $('#mkOutput').dataset.auto = ''; } });
  $('#mkVideo').addEventListener('change', () => {
    const v = $('#mkVideo').value;
    if (v && $('#mkOutput').dataset.auto !== '0') {
      $('#mkOutput').value = v.replace(/\.[^.]+$/, '') + '封装.mkv';
      $('#mkOutput').dataset.auto = '1';
    }
  });
  $('#mkPickVideo').addEventListener('click', () => $('#mkVideo').click());
  $('#mkPickAudio').addEventListener('click', () => $('#mkAudio').click());
  $('#mkPickSub').addEventListener('click', () => $('#mkSub').click());
  $('#mkPickOutput').addEventListener('click', () => $('#mkOutput').click());
  $('#mkGo').addEventListener('click', async () => {
    if (!$('#mkVideo').value) return toast('请先选择视频', 'err');
    try {
      const id = await invoke('mux_mkv', {
        video: $('#mkVideo').value,
        audio: $('#mkAudio').value || null,
        subtitle: $('#mkSub').value || null,
        output: $('#mkOutput').value,
      });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  });

  // 封装转换
  cvBatch = makeListBox($('#cvBatch'));
  $('#cvAdd').addEventListener('click', async () => {
    const p = await pickFile('选择文件', [{ name: '媒体', exts: ['mp4','mkv','flv','avi','mov','ts','webm','wmv','mpg','m4v'] }]);
    if (p) cvBatch.add(p);
  });
  $('#cvDel').addEventListener('click', () => cvBatch.removeSel());
  $('#cvClear').addEventListener('click', () => cvBatch.clear());
  $('#cvGo').addEventListener('click', async () => {
    const files = cvBatch.all();
    if (!files.length) return toast('列表为空', 'err');
    const fmt = $('#cvFormat').value;
    const enc = $('#cvEncoder').value;
    const jobs = files.map((f) => {
      const dir = $('#cvSrcDir').checked ? f.replace(/[^\\/]+$/, '') : (state.batchOutputDir || '');
      const stem = f.replace(/^.*[\\/]/, '').replace(/\.[^.]+$/, '');
      return { input: f, output: `${dir}${dir.endsWith('\\') || dir.endsWith('/') ? '' : dir ? '\\' : ''}${stem}.${fmt}`, audioEncoder: enc };
    });
    try {
      const ids = await invoke('convert_mux', { jobs });
      trackJob(ids[ids.length - 1]);
    } catch (e) {
      toast(String(e), 'err');
    }
  });
}

export function handleDrop(paths, target) {
  const first = paths[0];
  if (!first) return;
  if (target === 'm4Video' || target === 'mkVideo' || target === 'cvBatch') {
    if (target === 'cvBatch') { for (const p of paths) cvBatch.add(p); }
    else setFieldValue($(`#${target}`), first);
  } else if (/\.mp4$|\.mkv$|\.mov$|\.avi$/i.test(first)) {
    setFieldValue($('#m4Video'), first);
  }
}
