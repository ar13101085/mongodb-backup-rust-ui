// Vanilla SPA — checks auth, then renders Connections / Backups / Restore views.

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

const api = {
  async get(path) { return handle(await fetch(path)); },
  async post(path, body) {
    return handle(await fetch(path, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body ?? {}),
    }));
  },
  async put(path, body) {
    return handle(await fetch(path, {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body ?? {}),
    }));
  },
  async del(path) { return handle(await fetch(path, { method: 'DELETE' })); },
  async upload(path, formData) { return handle(await fetch(path, { method: 'POST', body: formData })); },
};

async function handle(r) {
  if (r.status === 401) { location.href = '/login.html'; throw new Error('unauthorized'); }
  if (!r.ok) {
    const data = await r.json().catch(() => ({}));
    throw new Error(data.error || `${r.status} ${r.statusText}`);
  }
  if (r.status === 204) return null;
  return r.json();
}

function toast(msg, ok = true) {
  const t = $('#toast');
  t.textContent = msg;
  t.className = `fixed bottom-4 right-4 text-sm px-3 py-2 rounded-md shadow-lg ${ok ? 'bg-slate-900' : 'bg-red-600'} text-white`;
  setTimeout(() => t.classList.add('hidden'), 2500);
  t.classList.remove('hidden');
}

function fmtBytes(n) {
  if (n == null) return '';
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(n < 10 && i > 0 ? 1 : 0)} ${u[i]}`;
}

function fmtDate(s) {
  if (!s) return '—';
  return new Date(s).toLocaleString();
}

const state = { connections: [], schedules: [], activeTab: 'connections', jobs: [] };

async function refresh() {
  [state.connections, state.schedules] = await Promise.all([
    api.get('/api/connections'),
    api.get('/api/schedules'),
  ]);
}

function setTab(name) {
  state.activeTab = name;
  $$('.tab').forEach(b => b.classList.toggle('active', b.dataset.tab === name));
  $$('.view').forEach(v => v.classList.toggle('hidden', v.id !== `view-${name}`));
  if (name === 'connections') renderConnections();
  if (name === 'backups') renderBackups();
  if (name === 'restore') renderRestore();
}

// ---------- Connections ----------

async function renderConnections() {
  const view = $('#view-connections');
  view.innerHTML = `
    <div class="flex items-center justify-between mb-4">
      <h2 class="text-lg font-semibold">Connections</h2>
      <button id="add-conn" class="px-3 py-1.5 rounded-md bg-slate-900 text-white text-sm hover:bg-slate-800">+ Add connection</button>
    </div>
    <div id="conn-list" class="space-y-3"></div>
  `;
  $('#add-conn').onclick = () => connDialog();
  await refresh();
  const list = $('#conn-list');
  if (state.connections.length === 0) {
    list.innerHTML = `<p class="text-sm text-slate-500">No connections yet. Add one to get started.</p>`;
    return;
  }
  list.innerHTML = state.connections.map(c => `
    <div class="bg-white border rounded-lg p-4">
      <div class="flex items-start justify-between gap-4">
        <div>
          <div class="font-medium">${escapeHtml(c.label)}</div>
          <div class="text-xs text-slate-500 font-mono mt-0.5">${escapeHtml(c.uri_masked)}</div>
        </div>
        <div class="flex gap-2 text-sm">
          <button class="px-2.5 py-1 rounded-md border hover:bg-slate-50" data-act="dbs" data-id="${c.id}">Databases</button>
          <button class="px-2.5 py-1 rounded-md border hover:bg-slate-50" data-act="edit" data-id="${c.id}">Edit</button>
          <button class="px-2.5 py-1 rounded-md border text-red-600 hover:bg-red-50" data-act="del" data-id="${c.id}">Delete</button>
        </div>
      </div>
      <div class="dbs hidden mt-4 border-t pt-4" data-dbs="${c.id}"></div>
    </div>
  `).join('');
  list.querySelectorAll('button[data-act]').forEach(btn => {
    btn.onclick = () => {
      const id = btn.dataset.id;
      const conn = state.connections.find(c => c.id === id);
      if (btn.dataset.act === 'dbs') toggleDbs(conn);
      if (btn.dataset.act === 'edit') connDialog(conn);
      if (btn.dataset.act === 'del') deleteConn(conn);
    };
  });
}

async function toggleDbs(conn) {
  const panel = document.querySelector(`.dbs[data-dbs="${conn.id}"]`);
  if (!panel.classList.contains('hidden')) { panel.classList.add('hidden'); return; }
  panel.classList.remove('hidden');
  panel.innerHTML = `<p class="text-sm text-slate-500">Loading databases…</p>`;
  let dbs;
  try { dbs = await api.get(`/api/connections/${conn.id}/databases`); }
  catch (e) { panel.innerHTML = `<p class="text-sm text-red-600">${escapeHtml(e.message)}</p>`; return; }
  if (dbs.length === 0) { panel.innerHTML = `<p class="text-sm text-slate-500">No user databases on this connection.</p>`; return; }
  panel.innerHTML = `
    <div class="space-y-2">
      ${dbs.map(db => {
        const sched = state.schedules.find(s => s.connection_id === conn.id && s.database === db);
        return dbRow(conn.id, db, sched);
      }).join('')}
    </div>
  `;
  panel.querySelectorAll('form[data-db]').forEach(form => {
    form.onsubmit = async (e) => {
      e.preventDefault();
      const fd = new FormData(form);
      try {
        await api.post('/api/schedules', {
          connection_id: conn.id,
          database: form.dataset.db,
          interval_minutes: Number(fd.get('interval_minutes')),
          retention: Number(fd.get('retention')),
          enabled: fd.get('enabled') === 'on',
        });
        toast('schedule saved');
        await refresh();
        toggleDbs(conn); toggleDbs(conn);
      } catch (e) { toast(e.message, false); }
    };
  });
  panel.querySelectorAll('button[data-run]').forEach(btn => {
    btn.onclick = async () => {
      btn.disabled = true; btn.textContent = 'Running…';
      try {
        const sched = state.schedules.find(s => s.connection_id === conn.id && s.database === btn.dataset.run);
        const r = await api.post('/api/backups/run', {
          connection_id: conn.id,
          database: btn.dataset.run,
          schedule_id: sched ? sched.id : null,
        });
        toast(`backup ok: ${r.filename}`);
      } catch (e) { toast(e.message, false); }
      finally { btn.disabled = false; btn.textContent = 'Backup now'; }
    };
  });
  panel.querySelectorAll('button[data-del-sched]').forEach(btn => {
    btn.onclick = async () => {
      if (!confirm('Delete schedule?')) return;
      try {
        await api.del(`/api/schedules/${btn.dataset.delSched}`);
        toast('schedule deleted');
        await refresh();
        toggleDbs(conn); toggleDbs(conn);
      } catch (e) { toast(e.message, false); }
    };
  });
}

function dbRow(connId, db, sched) {
  const enabled = sched ? sched.enabled : false;
  const interval = sched ? sched.interval_minutes : 60;
  const retention = sched ? sched.retention : 7;
  const last = sched ? `last: ${fmtDate(sched.last_run_at)} (${escapeHtml(sched.last_status || '—')})` : 'never run';
  return `
    <div class="rounded-md border p-3 bg-slate-50/50">
      <div class="flex items-center justify-between">
        <div>
          <div class="font-mono text-sm">${escapeHtml(db)}</div>
          <div class="text-xs text-slate-500">${last}</div>
        </div>
        <button data-run="${escapeHtml(db)}" class="px-2.5 py-1 rounded-md border bg-white hover:bg-slate-50 text-sm">Backup now</button>
      </div>
      <form data-db="${escapeHtml(db)}" class="mt-3 grid grid-cols-2 sm:grid-cols-5 gap-2 items-end text-xs">
        <label class="block">
          <span class="text-slate-600">Interval (min)</span>
          <input name="interval_minutes" type="number" min="1" value="${interval}" class="mt-1 w-full rounded border px-2 py-1" />
        </label>
        <label class="block">
          <span class="text-slate-600">Retention</span>
          <input name="retention" type="number" min="1" value="${retention}" class="mt-1 w-full rounded border px-2 py-1" />
        </label>
        <label class="flex items-center gap-2 mt-4">
          <input name="enabled" type="checkbox" ${enabled ? 'checked' : ''} />
          <span>Enabled</span>
        </label>
        <button class="px-2.5 py-1 rounded-md bg-slate-900 text-white">Save</button>
        ${sched ? `<button type="button" data-del-sched="${sched.id}" class="px-2.5 py-1 rounded-md border text-red-600 hover:bg-red-50">Remove</button>` : '<span></span>'}
      </form>
    </div>
  `;
}

function connDialog(existing) {
  const dlg = document.createElement('dialog');
  dlg.className = 'rounded-xl';
  dlg.innerHTML = `
    <form method="dialog" class="bg-white p-6 w-[28rem]">
      <h3 class="text-base font-semibold">${existing ? 'Edit' : 'Add'} connection</h3>
      <div class="mt-4 space-y-3 text-sm">
        <label class="block">
          <span class="text-slate-700">Label</span>
          <input name="label" required class="mt-1 w-full rounded-md border px-3 py-2" value="${existing ? escapeHtml(existing.label) : ''}" />
        </label>
        <label class="block">
          <span class="text-slate-700">Connection URI</span>
          <input name="uri" required placeholder="mongodb://user:pass@host:27017"
            class="mt-1 w-full rounded-md border px-3 py-2 font-mono text-xs" value="${existing ? '' : ''}" />
          ${existing ? '<span class="text-xs text-slate-500">Leave blank to keep existing URI.</span>' : ''}
        </label>
        <p class="err hidden text-sm text-red-600"></p>
      </div>
      <div class="mt-6 flex justify-end gap-2">
        <button type="button" class="cancel px-3 py-1.5 rounded-md border">Cancel</button>
        <button type="button" class="test px-3 py-1.5 rounded-md border">Test</button>
        <button type="button" class="save px-3 py-1.5 rounded-md bg-slate-900 text-white">${existing ? 'Save' : 'Create'}</button>
      </div>
    </form>
  `;
  document.body.appendChild(dlg);
  dlg.showModal();
  const form = dlg.querySelector('form');
  const errEl = dlg.querySelector('.err');
  const showErr = (m) => { errEl.textContent = m; errEl.classList.remove('hidden'); };
  dlg.querySelector('.cancel').onclick = () => { dlg.close(); dlg.remove(); };
  dlg.querySelector('.test').onclick = async () => {
    errEl.classList.add('hidden');
    const fd = new FormData(form);
    const uri = fd.get('uri').trim();
    if (!uri) return showErr('URI required to test');
    try {
      const r = await api.post('/api/connections/test', { uri });
      toast(`ok — ${r.databases.length} db(s)`);
    } catch (e) { showErr(e.message); }
  };
  dlg.querySelector('.save').onclick = async () => {
    errEl.classList.add('hidden');
    const fd = new FormData(form);
    const label = fd.get('label').trim();
    const uri = fd.get('uri').trim();
    try {
      if (existing) {
        await api.put(`/api/connections/${existing.id}`, { label, uri: uri || null });
      } else {
        await api.post('/api/connections', { label, uri });
      }
      dlg.close(); dlg.remove();
      toast('saved');
      await renderConnections();
    } catch (e) { showErr(e.message); }
  };
}

async function deleteConn(conn) {
  if (!confirm(`Delete connection "${conn.label}"? This also removes its schedules. Backup files are kept.`)) return;
  try {
    await api.del(`/api/connections/${conn.id}`);
    toast('connection deleted');
    await renderConnections();
  } catch (e) { toast(e.message, false); }
}

// ---------- Backups ----------

async function renderBackups() {
  const view = $('#view-backups');
  view.innerHTML = `
    <div class="flex items-center justify-between mb-4">
      <h2 class="text-lg font-semibold">Backups</h2>
      <button id="refresh-backups" class="px-3 py-1.5 rounded-md border text-sm hover:bg-slate-50">Refresh</button>
    </div>
    <div id="backup-list"></div>
  `;
  $('#refresh-backups').onclick = renderBackups;
  await refresh();
  const files = await api.get('/api/backups');
  const list = $('#backup-list');
  if (files.length === 0) {
    list.innerHTML = `<p class="text-sm text-slate-500">No backups yet.</p>`;
    return;
  }
  const connLabel = (id) => state.connections.find(c => c.id === id)?.label || '(deleted)';
  list.innerHTML = `
    <div class="bg-white border rounded-lg overflow-hidden">
      <table class="w-full text-sm">
        <thead class="bg-slate-50 text-slate-600 text-left">
          <tr>
            <th class="px-3 py-2 font-medium">File</th>
            <th class="px-3 py-2 font-medium">Connection</th>
            <th class="px-3 py-2 font-medium">Database</th>
            <th class="px-3 py-2 font-medium">Created</th>
            <th class="px-3 py-2 font-medium">Size</th>
            <th class="px-3 py-2"></th>
          </tr>
        </thead>
        <tbody>
          ${files.map(f => `
            <tr class="border-t">
              <td class="px-3 py-2 font-mono text-xs truncate max-w-[18rem]" title="${escapeHtml(f.filename)}">${escapeHtml(f.filename)}</td>
              <td class="px-3 py-2">${escapeHtml(f.connection_id ? connLabel(f.connection_id) : '—')}</td>
              <td class="px-3 py-2 font-mono text-xs">${escapeHtml(f.database || '—')}</td>
              <td class="px-3 py-2">${fmtDate(f.created_at)}</td>
              <td class="px-3 py-2">${fmtBytes(f.size_bytes)}</td>
              <td class="px-3 py-2 text-right whitespace-nowrap">
                <a href="/api/backups/${encodeURIComponent(f.filename)}/download" class="px-2 py-1 rounded-md border hover:bg-slate-50">Download</a>
                <button data-del="${escapeHtml(f.filename)}" class="px-2 py-1 rounded-md border text-red-600 hover:bg-red-50">Delete</button>
              </td>
            </tr>
          `).join('')}
        </tbody>
      </table>
    </div>
  `;
  list.querySelectorAll('button[data-del]').forEach(btn => {
    btn.onclick = async () => {
      if (!confirm(`Delete ${btn.dataset.del}?`)) return;
      try {
        await api.del(`/api/backups/${encodeURIComponent(btn.dataset.del)}`);
        toast('deleted');
        renderBackups();
      } catch (e) { toast(e.message, false); }
    };
  });
}

// ---------- Restore ----------

async function renderRestore() {
  await refresh();
  const files = await api.get('/api/backups');
  const view = $('#view-restore');
  view.innerHTML = `
    <h2 class="text-lg font-semibold mb-4">Restore</h2>
    <div class="grid md:grid-cols-2 gap-4">

      <div class="bg-white border rounded-lg p-4">
        <h3 class="font-medium">From server file</h3>
        <p class="text-xs text-slate-500 mt-1">Pick a backup that already lives on this server.</p>
        <form id="form-server" class="mt-3 space-y-3 text-sm">
          <label class="block">
            <span>Archive</span>
            <select name="filename" required class="mt-1 w-full rounded-md border px-2 py-1.5 font-mono text-xs">
              ${files.map(f => `<option value="${escapeHtml(f.filename)}">${escapeHtml(f.filename)}</option>`).join('')}
            </select>
          </label>
          ${targetFields()}
          <button class="px-3 py-1.5 rounded-md bg-slate-900 text-white">Restore</button>
        </form>
      </div>

      <div class="bg-white border rounded-lg p-4">
        <h3 class="font-medium">From upload</h3>
        <p class="text-xs text-slate-500 mt-1">Upload a *.archive.gz produced by this tool.</p>
        <form id="form-upload" class="mt-3 space-y-3 text-sm" enctype="multipart/form-data">
          <label class="block">
            <span>File</span>
            <input name="file" type="file" accept=".gz" required class="mt-1 w-full text-xs" />
          </label>
          ${targetFields()}
          <button class="px-3 py-1.5 rounded-md bg-slate-900 text-white">Upload &amp; restore</button>
        </form>
      </div>
    </div>
  `;

  bindTargetFields(view);

  $('#form-server').onsubmit = async (e) => {
    e.preventDefault();
    const fd = new FormData(e.target);
    const body = collectTarget(fd);
    body.filename = fd.get('filename');
    try {
      await api.post('/api/restore/server', body);
      toast('restore complete');
    } catch (err) { toast(err.message, false); }
  };

  $('#form-upload').onsubmit = async (e) => {
    e.preventDefault();
    const fd = new FormData(e.target);
    const target = collectTarget(fd);
    const out = new FormData();
    out.append('file', fd.get('file'));
    if (target.target_connection_id) out.append('target_connection_id', target.target_connection_id);
    if (target.target_uri) out.append('target_uri', target.target_uri);
    if (target.target_database) out.append('target_database', target.target_database);
    out.append('drop_existing', target.drop_existing ? 'true' : 'false');
    const btn = e.submitter; btn.disabled = true; btn.textContent = 'Uploading…';
    try {
      await api.upload('/api/restore/upload', out);
      toast('restore complete');
    } catch (err) { toast(err.message, false); }
    finally { btn.disabled = false; btn.textContent = 'Upload & restore'; }
  };
}

function targetFields() {
  const opts = state.connections.map(c => `<option value="${c.id}">${escapeHtml(c.label)}</option>`).join('');
  return `
    <fieldset class="space-y-2">
      <label class="block">
        <span>Target — choose</span>
        <select name="target_mode" class="mt-1 w-full rounded-md border px-2 py-1.5">
          <option value="connection">Existing connection</option>
          <option value="uri">Connection URI</option>
        </select>
      </label>
      <label class="block target-conn">
        <span>Connection</span>
        <select name="target_connection_id" class="mt-1 w-full rounded-md border px-2 py-1.5">${opts}</select>
      </label>
      <label class="block target-uri hidden">
        <span>URI</span>
        <input name="target_uri" placeholder="mongodb://..." class="mt-1 w-full rounded-md border px-2 py-1.5 font-mono text-xs" />
      </label>
      <label class="block">
        <span>Restore as database (optional)</span>
        <input name="target_database" class="mt-1 w-full rounded-md border px-2 py-1.5 font-mono text-xs" />
      </label>
      <label class="flex items-center gap-2">
        <input name="drop_existing" type="checkbox" />
        <span>Drop existing collections before restore</span>
      </label>
    </fieldset>
  `;
}

function bindTargetFields(root) {
  root.querySelectorAll('select[name="target_mode"]').forEach(sel => {
    const form = sel.closest('form');
    const onChange = () => {
      const mode = sel.value;
      form.querySelector('.target-conn').classList.toggle('hidden', mode !== 'connection');
      form.querySelector('.target-uri').classList.toggle('hidden', mode !== 'uri');
    };
    sel.addEventListener('change', onChange);
    onChange();
  });
}

function collectTarget(fd) {
  const mode = fd.get('target_mode');
  return {
    target_connection_id: mode === 'connection' ? fd.get('target_connection_id') : null,
    target_uri: mode === 'uri' ? fd.get('target_uri') : null,
    target_database: (fd.get('target_database') || '').trim() || null,
    drop_existing: fd.get('drop_existing') === 'on',
  };
}

// ---------- Jobs (SSE) ----------

function upsertJob(job) {
  const i = state.jobs.findIndex(j => j.id === job.id);
  if (i === -1) state.jobs.unshift(job);
  else state.jobs[i] = job;
  // Cap visible list
  state.jobs = state.jobs.slice(0, 50);
}

function renderJobs() {
  const panel = $('#jobs-panel');
  const running = state.jobs.filter(j => j.state === 'running');
  const recent = state.jobs.filter(j => j.state !== 'running').slice(0, 5);
  if (running.length === 0 && recent.length === 0) {
    panel.classList.add('hidden');
    panel.innerHTML = '';
    return;
  }
  panel.classList.remove('hidden');
  const dot = (s) => s === 'running'
    ? '<span class="inline-block w-2 h-2 rounded-full bg-amber-500 animate-pulse"></span>'
    : s === 'ok'
      ? '<span class="inline-block w-2 h-2 rounded-full bg-emerald-500"></span>'
      : '<span class="inline-block w-2 h-2 rounded-full bg-red-500"></span>';
  const row = (j) => {
    const dur = j.finished_at
      ? `${Math.max(0, Math.round((new Date(j.finished_at) - new Date(j.started_at)) / 1000))}s`
      : `${Math.max(0, Math.round((Date.now() - new Date(j.started_at)) / 1000))}s`;

    let detail = '';
    if (j.state === 'running') {
      const p = j.progress || {};
      const done = p.collections_done || 0;
      const cur = p.current_collection;
      if (cur) detail = ` — <span class="font-mono">${escapeHtml(cur)}</span> <span class="text-slate-400">(${done} done)</span>`;
      else if (done > 0) detail = ` — <span class="text-slate-500">${done} collections done</span>`;
      else detail = ` — <span class="text-slate-500">starting…</span>`;
    } else if (j.message) {
      detail = ` — ${escapeHtml(j.message)}`;
    }

    return `
      <div class="flex items-center gap-3 text-sm py-1">
        ${dot(j.state)}
        <span class="font-mono text-xs text-slate-500 w-12">${dur}</span>
        <span class="font-medium">${escapeHtml(j.connection_label)}</span>
        <span class="text-slate-400">/</span>
        <span class="font-mono">${escapeHtml(j.database)}</span>
        <span class="text-xs text-slate-500">${escapeHtml(j.source)}</span>
        <span class="text-xs text-slate-700 truncate">${detail}</span>
      </div>
    `;
  };
  panel.innerHTML = `
    <div class="flex items-center justify-between">
      <h3 class="text-sm font-semibold">
        Backups
        ${running.length ? `<span class="ml-2 text-xs font-normal text-amber-700">${running.length} running</span>` : ''}
      </h3>
    </div>
    <div class="mt-2 divide-y">
      ${running.map(row).join('')}
      ${recent.map(row).join('')}
    </div>
  `;
}

let evtSource = null;
let evtRetry = null;
function startJobsStream() {
  if (evtSource) return;
  const es = new EventSource('/api/backups/jobs/stream');
  evtSource = es;
  es.onmessage = (e) => {
    let msg;
    try { msg = JSON.parse(e.data); } catch { return; }
    if (msg.type === 'snapshot') {
      state.jobs = msg.jobs || [];
    } else if (msg.type === 'started' || msg.type === 'progress' || msg.type === 'finished') {
      upsertJob(msg.job);
      if (msg.type === 'finished' && state.activeTab === 'backups') {
        renderBackups().catch(() => {});
      }
    }
    renderJobs();
  };
  es.onerror = () => {
    es.close();
    evtSource = null;
    if (evtRetry) clearTimeout(evtRetry);
    evtRetry = setTimeout(startJobsStream, 3000);
  };
}

setInterval(() => {
  if (state.jobs.some(j => j.state === 'running')) renderJobs();
}, 1000);

// ---------- Boot ----------

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
}

(async function boot() {
  let status;
  try { status = await fetch('/api/auth/status').then(r => r.json()); }
  catch { return; }

  if (status.needs_setup) { location.href = '/setup.html'; return; }
  if (!status.user) { location.href = '/login.html'; return; }

  $('#loading').classList.add('hidden');
  $('#app').classList.remove('hidden');
  $('#who').textContent = status.user.username;
  $('#logout').onclick = async () => {
    await fetch('/api/auth/logout', { method: 'POST' });
    location.href = '/login.html';
  };
  $$('.tab').forEach(b => b.onclick = () => setTab(b.dataset.tab));
  startJobsStream();
  setTab('connections');
})();
