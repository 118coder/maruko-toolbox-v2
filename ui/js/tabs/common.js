// tabs/common.js —— 常用页（一图流 / 截取 / 旋转）

import { invoke } from '../bridge.js';
import { $, bindFileField, toast, setFieldValue, flashField, isImageFile, isAudioFile, baseName, withDebounce } from '../components.js';
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
  $('#oGo').addEventListener('click', withDebounce(async () => {
    if (!$('#oImage').value) { toast('请先选择图片（可拖入）', 'err'); $('#oImage').focus(); return; }
    if (!$('#oAudio').value) { toast('请先选择音频（可拖入）', 'err'); $('#oAudio').focus(); return; }
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
  }));

  bindFileField($('#cVideo'), { kind: 'video', title: '选择视频' });
  bindFileField($('#cOutput'), { kind: 'video', title: '选择输出文件', save: true, onSet: () => { $('#cOutput').dataset.auto = ''; } });
  // 输出自动命名（标记在点截取/旋转时按动作刷新，见 setCAutoName）
  $('#cVideo').addEventListener('change', () => {
    const v = $('#cVideo').value;
    if (v && $('#cOutput').dataset.auto !== '0') setCAutoName();
  });
  $('#cPickVideo').addEventListener('click', () => $('#cVideo').click());
  $('#cPickOutput').addEventListener('click', () => $('#cOutput').click());
  $('#cCut').addEventListener('click', withDebounce(async () => {
    if (!$('#cVideo').value) { toast('请先选择视频', 'err'); $('#cVideo').focus(); return; }
    if (isCAuto()) setCAutoName('截取');
    try {
      const id = await invoke('cut_video', {
        job: { input: $('#cVideo').value, output: $('#cOutput').value, start: $('#cStart').value, end: $('#cEnd').value },
      });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  }));
  $('#cRotate').addEventListener('click', withDebounce(async () => {
    if (!$('#cVideo').value) { toast('请先选择视频', 'err'); $('#cVideo').focus(); return; }
    if (isCAuto()) setCAutoName('旋转');
    try {
      const id = await invoke('rotate_video', {
        job: { input: $('#cVideo').value, output: $('#cOutput').value, transpose: $('#cTranspose').value },
      });
      trackJob(id);
    } catch (e) {
      toast(String(e), 'err');
    }
  }));
}

function isCAuto() {
  return $('#cOutput').dataset.auto === '1';
}

/** 自动命名：源目录/名+动作标记+原扩展（动作默认「截取」，点旋转时自动改为「旋转」） */
function setCAutoName(action = '截取') {
  const v = $('#cVideo').value;
  if (!v) return;
  const ext = v.match(/\.[^.]+$/)?.[0] || '.mp4';
  $('#cOutput').value = v.replace(/\.[^.]+$/, '') + action + ext;
  $('#cOutput').dataset.auto = '1';
}

/** 拖放路由：一图流按文件类型智能归位（拖错栏自动纠正），视频进「其他」组 */
export function handleDrop(paths, target) {
  const first = paths[0];
  if (!first) return;
  const img = paths.find(isImageFile);
  const aud = paths.find(isAudioFile);

  if (target === 'oImage' || target === 'oAudio' || !target) {
    // 一图流区域/页面默认：图、音、视频按类型各归各位
    let dropped = false;
    if (img) { setFieldValue($('#oImage'), img); flashField($('#oImage')); toast(`已添加图片：${baseName(img)}`, 'ok'); dropped = true; }
    if (aud) { setFieldValue($('#oAudio'), aud); flashField($('#oAudio')); toast(`已添加音频：${baseName(aud)}`, 'ok'); dropped = true; }
    const rest = paths.filter((p) => !isImageFile(p) && !isAudioFile(p));
    if (rest.length && (target === 'oImage' || target === 'oAudio') && !dropped) {
      toast('一图流需要图片和音频：已把文件放到「其他」组视频栏', 'ok');
    }
    if (rest.length) { setFieldValue($('#cVideo'), rest[0]); flashField($('#cVideo')); toast(`已添加视频：${baseName(rest[0])}`, 'ok'); }
    if (!dropped && !rest.length) toast('未识别到可用的图片/音频文件', 'err');
    return;
  }

  // 「其他」组（cVideo 或其他栏）：视频归位；音频提示去音频页
  if (isAudioFile(first)) {
    toast('这是音频文件：压制请到「音频」页', 'err');
    return;
  }
  setFieldValue($('#cVideo'), first);
  flashField($('#cVideo'));
  toast(`已添加视频：${baseName(first)}`, 'ok');
}
