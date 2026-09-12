// tabs/extract.js —— 抽取页

import { invoke } from '../bridge.js';
import { $, bindFileField, toast } from '../components.js';
import { trackJob } from '../jobs.js';

export function initExtract() {
  bindFileField($('#eVideo'), { kind: 'video', title: '选择视频' });
  bindFileField($('#fVideo'), { kind: 'video', title: '选择 FLV' });
  bindFileField($('#kVideo'), { kind: 'video', title: '选择 MKV' });
  $('#ePickVideo').addEventListener('click', () => $('#eVideo').click());
  $('#fPickVideo').addEventListener('click', () => $('#fVideo').click());
  $('#kPickVideo').addEventListener('click', () => $('#kVideo').click());

  const runCommon = (kind, index) => async () => {
    const p = $('#eVideo').value;
    if (!p) return toast('请先选择视频', 'err');
    try {
      const id = await invoke('extract_common', { path: p, kind, index });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  };
  $('#eAudio1').addEventListener('click', runCommon('audio', 1));
  $('#eAudio2').addEventListener('click', runCommon('audio', 2));
  $('#eAudio3').addEventListener('click', runCommon('audio', 3));
  $('#eVideoGo').addEventListener('click', runCommon('video', 0));

  const runFlv = (kind) => async () => {
    const p = $('#fVideo').value;
    if (!p) return toast('请先选择 FLV 文件', 'err');
    try {
      const r = await invoke('extract_flv', { path: p, kind });
      if (r === 'ok') toast('抽取完成（输出在源文件目录）', 'ok');
      else trackJob(parseInt(r));
    } catch (e) {
      toast(String(e), 'err');
    }
  };
  $('#fAudio').addEventListener('click', runFlv('audio'));
  $('#fVideoGo').addEventListener('click', runFlv('video'));

  const runTrack = (track) => async () => {
    const p = $('#kVideo').value;
    if (!p) return toast('请先选择 MKV 文件', 'err');
    try {
      const tracks = await invoke('identify_mkv', { path: p });
      const t = tracks.find((x) => x.index === track);
      if (!t) return toast(`轨道 ${track} 不存在（共 ${tracks.length} 条）`, 'err');
      const ext = trackExt(t);
      const id = await invoke('extract_mkv_track', { path: p, track, ext });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  };
  $('#kT0').addEventListener('click', runTrack(0));
  $('#kT1').addEventListener('click', runTrack(1));
  $('#kT2').addEventListener('click', runTrack(2));
  $('#kT3').addEventListener('click', runTrack(3));
  $('#kT4').addEventListener('click', runTrack(4));
}

function trackExt(t) {
  if (t.kind === 'subtitles') {
    return /ass/i.test(t.codec) ? 'ass' : /ssa/i.test(t.codec) ? 'ssa' : 'srt';
  }
  if (t.kind === 'audio') {
    return ({ aac: 'aac', mp3: 'mp3', flac: 'flac', vorbis: 'ogg', opus: 'opus' })[t.codec] || 'mka';
  }
  if (t.kind === 'video') {
    return ({ h264: '264', hevc: 'hevc' })[t.codec] || 'mkv';
  }
  return 'bin';
}

export function handleDrop(paths, target) {
  const p = paths[0];
  if (!p) return;
  if (target === 'fVideo' || /\.flv$/i.test(p)) $('#fVideo').value = p;
  else if (target === 'kVideo' || /\.mkv$/i.test(p)) $('#kVideo').value = p;
  else $('#eVideo').value = p;
}
