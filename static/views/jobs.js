// SSE-backed jobs drawer pinned to the bottom-right of the viewport.
// Subscribes to /api/backups/jobs/stream and renders running + recent jobs.

import { $, escapeHtml, ensureIcons, icon } from '/ui.js';

const state = {
  jobs: [],
  collapsed: false,
  evtSource: null,
  evtRetry: null,
  onFinished: null, // optional callback (job) => void, used by the Backups view
};

const MAX_VISIBLE = 50;

function upsert(job) {
  const i = state.jobs.findIndex(j => j.id === job.id);
  if (i === -1) state.jobs.unshift(job);
  else state.jobs[i] = job;
  state.jobs = state.jobs.slice(0, MAX_VISIBLE);
}

function root() { return $('#jobs-root'); }

export function setOnFinished(cb) { state.onFinished = cb; }

export function startJobsStream() {
  if (state.evtSource) return;
  const es = new EventSource('/api/backups/jobs/stream');
  state.evtSource = es;
  es.onmessage = (e) => {
    let msg; try { msg = JSON.parse(e.data); } catch { return; }
    if (msg.type === 'snapshot') state.jobs = msg.jobs || [];
    else if (msg.type === 'started' || msg.type === 'progress') upsert(msg.job);
    else if (msg.type === 'finished') {
      upsert(msg.job);
      try { state.onFinished?.(msg.job); } catch {}
    }
    render();
  };
  es.onerror = () => {
    es.close();
    state.evtSource = null;
    if (state.evtRetry) clearTimeout(state.evtRetry);
    state.evtRetry = setTimeout(startJobsStream, 3000);
  };
}

// Re-render once a second while there's a running job, so durations tick.
setInterval(() => {
  if (state.jobs.some(j => j.state === 'running')) render();
}, 1000);

function durationSec(j) {
  const start = new Date(j.started_at).getTime();
  const end = j.finished_at ? new Date(j.finished_at).getTime() : Date.now();
  return Math.max(0, Math.round((end - start) / 1000));
}

function fmtDur(sec) {
  if (sec < 60) return `${sec}s`;
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}m${s ? ` ${s}s` : ''}`;
}

function statusIcon(j) {
  if (j.state === 'running') return `<span class="job-status-dot status-running"></span>`;
  if (j.state === 'ok') return icon('check', { size: 14, className: 'lucide' }).replace('<i ', '<i style="color:var(--c-success); width:0.875rem; height:0.875rem;" ');
  return icon('x', { size: 14, className: 'lucide' }).replace('<i ', '<i style="color:var(--c-danger); width:0.875rem; height:0.875rem;" ');
}

function detailFor(j) {
  if (j.state === 'running') {
    const p = j.progress || {};
    const done = p.collections_done || 0;
    const cur = p.current_collection;
    if (cur) return `<span class="font-mono">${escapeHtml(cur)}</span> <span style="color:var(--c-text-subtle)">· ${done} done</span>`;
    if (done > 0) return `${done} collections done`;
    if (durationSec(j) > 4) return 'working…';
    return 'starting…';
  }
  if (j.state === 'ok') {
    const file = j.message || '';
    if (file && file.endsWith('.archive.gz')) {
      const href = `/api/backups/${encodeURIComponent(file)}/download`;
      return `<a href="${href}" class="copyable" title="Download backup">${escapeHtml(file)}</a>`;
    }
    return escapeHtml(j.message || 'completed');
  }
  return `<span title="${escapeHtml(j.message || 'failed')}">${escapeHtml((j.message || 'failed').slice(0, 90))}</span>`;
}

function jobRow(j) {
  return `
    <div class="job-row" data-job="${j.id}">
      ${statusIcon(j)}
      <div class="job-when">${fmtDur(durationSec(j))}</div>
      <div class="job-target">
        <div class="target-line">
          <span class="target-conn" title="${escapeHtml(j.connection_label)}">${escapeHtml(j.connection_label)}</span>
          <span style="color:var(--c-text-subtle)">/</span>
          <span class="target-db" title="${escapeHtml(j.database)}">${escapeHtml(j.database)}</span>
        </div>
        <div class="job-detail">${detailFor(j)}</div>
      </div>
      <div style="padding-left:0.5rem; color:var(--c-text-subtle); font-size:0.6875rem; text-transform:uppercase; letter-spacing:0.04em;">${j.source}</div>
    </div>
  `;
}

function render() {
  const r = root();
  if (!r) return;

  const running = state.jobs.filter(j => j.state === 'running');
  const recent = state.jobs.filter(j => j.state !== 'running').slice(0, 5);

  if (running.length === 0 && recent.length === 0) {
    r.innerHTML = '';
    return;
  }

  const collapsedCls = state.collapsed ? ' collapsed' : '';
  const headerLabel = running.length > 0
    ? `<span class="badge badge-running"><span class="badge-dot"></span>${running.length} running</span>`
    : `<span class="badge badge-muted"><span class="badge-dot"></span>idle</span>`;

  r.innerHTML = `
    <div class="jobs-drawer${collapsedCls}" id="jobs-drawer">
      <div class="jobs-drawer-head" data-act="toggle">
        <div style="display:flex; align-items:center; gap:0.625rem;">
          ${icon('activity', { size: 16 })}
          <span style="font-size:0.875rem; font-weight:600;">Backups</span>
          ${headerLabel}
        </div>
        <button class="btn btn-ghost btn-icon btn-sm" aria-label="Toggle">
          ${icon(state.collapsed ? 'chevron-up' : 'chevron-down', { size: 14 })}
        </button>
      </div>
      <div class="jobs-drawer-body">
        ${running.map(jobRow).join('')}
        ${recent.length && running.length ? `<div class="divider" style="margin:0.25rem 1rem;"></div>` : ''}
        ${recent.map(jobRow).join('')}
      </div>
    </div>
  `;
  ensureIcons(r);

  const head = r.querySelector('[data-act="toggle"]');
  if (head) head.onclick = () => { state.collapsed = !state.collapsed; render(); };
}
