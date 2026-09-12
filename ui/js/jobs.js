// jobs.js —— 任务进度浮层：订阅 job:// 事件，展示进度/日志/队列，取消与关机倒计时。

import { invoke, onJobProgress, onJobState, onJobLog, onQueueIdle, onShutdownArmed } from './bridge.js';
import { fmtTime, toast } from './components.js';

const jobs = new Map(); // id -> {id,title,state,output,progress}
let visible = false;

function el(id) { return document.getElementById(id); }

function renderQueue() {
  const q = el('pqueue');
  if (!q) return;
  q.innerHTML = '';
  const list = [...jobs.values()].sort((a, b) => a.id - b.id);
  for (const j of list) {
    const icon = { queued: '…', running: '▶', done: '✓', failed: '✗', cancelled: '⏹' }[j.state] || '…';
    const cls = { done: 'st-done', running: 'st-run', failed: 'st-fail', queued: 'st-queue' }[j.state] || '';
    const d = document.createElement('div');
    d.className = 'item';
    d.innerHTML = `<span class="st ${cls}">${icon}</span><span style="flex:1;overflow:hidden;text-overflow:ellipsis">${j.title}</span>`;
    d.addEventListener('click', () => selectJob(j.id));
    q.appendChild(d);
  }
}

let currentId = null;
function selectJob(id) {
  currentId = id;
  const j = jobs.get(id);
  if (!j) return;
  el('ptitle').textContent = j.title;
  el('pbarfill').style.width = (j.progress?.pct || 0) + '%';
  el('ppct').textContent = (j.progress?.pct || 0).toFixed(1) + '%';
  renderStats(j);
  el('plog').textContent = j.logs?.join('\n') || '';
  el('plog').scrollTop = el('plog').scrollHeight;
}

function renderStats(j) {
  const p = j.progress || {};
  const set = (id, v) => { el(id).textContent = v; };
  set('pframe', p.total ? `${p.frame}/${p.total}` : (p.frame || '-'));
  set('pfps', p.fps ? p.fps.toFixed(2) : '-');
  set('pkbps', p.kbps ? p.kbps.toFixed(0) + ' kbps' : '-');
  set('peta', p.eta_s ? fmtTime(p.eta_s) : '-');
  set('pelapsed', fmtTime(p.elapsed_s || 0));
}

function ensureVisible() {
  if (visible) return;
  visible = true;
  el('progressOverlay').classList.add('show');
}

export function hideProgress() {
  visible = false;
  el('progressOverlay').classList.remove('show');
}

export function initJobsUI() {
  onJobState((s) => {
    if (s.id === 0 && s.state === 'shutdown') {
      showShutdown(s.msg);
      return;
    }
    let j = jobs.get(s.id);
    if (!j) {
      j = { id: s.id, title: s.title, logs: [] };
      jobs.set(s.id, j);
    }
    j.state = s.state;
    j.title = j.title || s.title;
    j.output = s.output;
    renderQueue();
    if (s.state === 'running') {
      ensureVisible();
      if (currentId == null || j.state === 'running') selectJob(s.id);
    }
    if (s.state === 'done') toast(`完成：${j.title}`, 'ok');
    if (s.state === 'failed') toast(`失败：${j.title}（详见进度窗日志）`, 'err');
    // 全部结束后 2s 自动收起（保持可手动打开）
    if (['done', 'failed', 'cancelled'].includes(s.state) && [...jobs.values()].every((x) => x.state !== 'running' && x.state !== 'queued')) {
      setTimeout(() => { if (![...jobs.values()].some((x) => x.state === 'running' || x.state === 'queued')) hideProgress(); }, 2500);
    }
  });

  onJobProgress((p) => {
    const j = jobs.get(p.id);
    if (!j) return;
    j.progress = p;
    if (currentId === p.id) {
      el('pbarfill').style.width = p.pct.toFixed(1) + '%';
      el('ppct').textContent = p.pct.toFixed(1) + '%';
      renderStats(j);
    }
  });

  onJobLog((l) => {
    const j = jobs.get(l.id);
    if (!j) return;
    j.logs.push(l.line);
    if (j.logs.length > 400) j.logs.shift();
    if (currentId === l.id && visible) {
      const box = el('plog');
      box.textContent += l.line + '\n';
      box.scrollTop = box.scrollHeight;
    }
  });

  onQueueIdle(({ lastFailed }) => {
    if (!lastFailed) toast('批量队列已全部完成', 'ok');
  });

  onShutdownArmed((s) => showShutdown(s.msg));

  // 取消按钮
  el('pcancel').addEventListener('click', async () => {
    const j = jobs.get(currentId);
    if (j) {
      await invoke('job_cancel', { id: j.id });
      toast(`已请求取消：${j.title}`);
    }
  });
  el('pcanceldismiss').addEventListener('click', hideProgress);
  el('shutdowncancel').addEventListener('click', async () => {
    await invoke('shutdown_cancel_cmd');
    el('shutdownbar').classList.remove('show');
    toast('已取消自动关机', 'ok');
  });
}

function showShutdown(msg) {
  el('shutdownbar').classList.add('show');
  toast(msg || '自动关机已布置', 'err');
}

/** 供 tab 模块调用：提交任务后打开进度窗 */
export function trackJob(id) {
  if (id != null) {
    ensureVisible();
  }
}
