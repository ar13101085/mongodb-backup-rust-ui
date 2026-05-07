import {
  $, $$, escapeHtml, fmtDate, fmtRelative, ensureIcons, icon,
  api, Button, Card, Field, Input, Toggle, Badge, EmptyState, Skeleton,
  toast, confirmDialog, openDialog, withSpinner, copyText,
} from '/ui.js';

const local = { connections: [], schedules: [], expanded: new Set() };

export const pageTitle = 'Connections';

export function pageActions() {
  return Button({
    label: 'Add connection',
    iconName: 'plus',
    variant: 'primary',
    attrs: { id: 'add-conn-btn' },
  });
}

export function bindPageActions(actionsRoot) {
  $('#add-conn-btn', actionsRoot).onclick = () => connDialog();
}

export async function render(view) {
  view.innerHTML = `
    <div id="conn-list" class="space-y-3">
      ${[0, 1].map(() =>
        `<div class="card card-pad"><div class="flex items-center justify-between gap-4">
          ${Skeleton({ width: '12rem', height: '1rem' })}
          <div class="flex gap-2">${Skeleton({ width: '5rem', height: '2rem' })}${Skeleton({ width: '5rem', height: '2rem' })}</div>
        </div></div>`
      ).join('')}
    </div>
  `;

  await refresh();
  paint(view);
}

async function refresh() {
  [local.connections, local.schedules] = await Promise.all([
    api.get('/api/connections'),
    api.get('/api/schedules'),
  ]);
}

function paint(view) {
  const list = $('#conn-list', view);
  if (local.connections.length === 0) {
    list.innerHTML = EmptyState({
      iconName: 'database',
      title: 'No connections yet',
      hint: 'Add a MongoDB connection to start scheduling backups.',
      action: Button({
        label: 'Add connection',
        iconName: 'plus',
        variant: 'primary',
        attrs: { 'data-act': 'add' },
      }),
    });
    ensureIcons(list);
    list.querySelector('[data-act="add"]')?.addEventListener('click', () => connDialog());
    return;
  }

  list.innerHTML = local.connections.map(connCard).join('');
  ensureIcons(list);

  list.querySelectorAll('[data-conn]').forEach(card => {
    const id = card.dataset.conn;
    const conn = local.connections.find(c => c.id === id);
    card.querySelector('[data-act="dbs"]').onclick = () => toggleDbs(card, conn);
    card.querySelector('[data-act="edit"]').onclick = () => connDialog(conn);
    card.querySelector('[data-act="del"]').onclick = () => deleteConn(conn);
    card.querySelector('[data-act="copy"]').onclick = () => copyText(conn.uri_masked, { successMsg: 'URI copied' });
    if (local.expanded.has(id)) toggleDbs(card, conn, true);
  });
}

function connCard(c) {
  const sched = local.schedules.filter(s => s.connection_id === c.id);
  const enabledCount = sched.filter(s => s.enabled).length;
  const summary = sched.length === 0
    ? Badge({ tone: 'muted', label: 'no schedules' })
    : Badge({ tone: enabledCount > 0 ? 'ok' : 'muted', label: `${enabledCount}/${sched.length} active` });

  return `
    <article class="card" data-conn="${c.id}">
      <div class="card-pad">
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <div style="width:32px; height:32px; border-radius:8px; background:var(--c-accent-soft); color:var(--c-accent); display:grid; place-items:center; flex-shrink:0;">
                ${icon('database', { size: 16 })}
              </div>
              <div class="min-w-0">
                <div class="font-medium truncate">${escapeHtml(c.label)}</div>
                <div class="flex items-center gap-1.5 mt-0.5">
                  <code class="font-mono text-xs text-text-muted truncate" style="max-width: 26rem;" title="${escapeHtml(c.uri_masked)}">${escapeHtml(c.uri_masked)}</code>
                  <button class="btn btn-ghost btn-icon btn-sm" data-act="copy" title="Copy URI">
                    ${icon('copy', { size: 12 })}
                  </button>
                </div>
              </div>
              <div class="ml-3">${summary}</div>
            </div>
          </div>
          <div class="flex items-center gap-1">
            ${Button({ label: 'Databases', iconName: 'list', variant: 'secondary', size: 'sm', attrs: { 'data-act': 'dbs' } })}
            ${Button({ iconName: 'pencil', variant: 'ghost', size: 'sm', attrs: { 'data-act': 'edit', 'aria-label': 'Edit connection' } })}
            ${Button({ iconName: 'trash-2', variant: 'danger', size: 'sm', attrs: { 'data-act': 'del', 'aria-label': 'Delete connection' } })}
          </div>
        </div>
      </div>
      <div class="dbs-panel hidden" style="border-top:1px solid var(--c-border);"></div>
    </article>
  `;
}

async function toggleDbs(card, conn, forceOpen = false) {
  const panel = card.querySelector('.dbs-panel');
  const isOpen = !panel.classList.contains('hidden');
  if (isOpen && !forceOpen) {
    panel.classList.add('hidden');
    panel.innerHTML = '';
    local.expanded.delete(conn.id);
    return;
  }
  panel.classList.remove('hidden');
  local.expanded.add(conn.id);
  panel.innerHTML = `
    <div class="card-pad">
      <div class="space-y-2">
        ${[0, 1, 2].map(() => `${Skeleton({ width: '100%', height: '3rem' })}`).join('')}
      </div>
    </div>
  `;

  let dbs;
  try { dbs = await api.get(`/api/connections/${conn.id}/databases`); }
  catch (e) {
    panel.innerHTML = `<div class="card-pad"><div class="badge badge-err">${icon('alert-circle', { size: 12 })}<span>${escapeHtml(e.message)}</span></div></div>`;
    ensureIcons(panel);
    return;
  }

  if (dbs.length === 0) {
    panel.innerHTML = `<div class="card-pad"><div class="text-sm text-text-muted">No user databases on this connection.</div></div>`;
    return;
  }

  panel.innerHTML = `<div class="card-pad space-y-2">${dbs.map(db => dbRow(conn, db)).join('')}</div>`;
  ensureIcons(panel);
  bindDbRows(panel, conn);
}

function dbRow(conn, db) {
  const sched = local.schedules.find(s => s.connection_id === conn.id && s.database === db);
  const enabled = sched ? sched.enabled : false;
  const interval = sched ? sched.interval_minutes : 60;
  const retention = sched ? sched.retention : 7;

  let lastBadge;
  if (!sched) lastBadge = Badge({ tone: 'muted', label: 'never run' });
  else if (sched.last_status === 'ok') lastBadge = Badge({ tone: 'ok', label: `ran ${fmtRelative(sched.last_run_at)}` });
  else if (!sched.last_run_at) lastBadge = Badge({ tone: 'muted', label: enabled ? 'pending' : 'paused' });
  else lastBadge = Badge({ tone: 'err', label: 'last run failed' });

  return `
    <div class="rounded-lg border border-border bg-surface" data-db="${escapeHtml(db)}">
      <div class="flex items-center justify-between gap-3 p-3">
        <div class="flex items-center gap-2 min-w-0">
          ${icon('hard-drive', { size: 14, className: 'text-text-subtle' })}
          <code class="font-mono text-sm truncate">${escapeHtml(db)}</code>
          ${lastBadge}
        </div>
        <div class="flex items-center gap-1">
          ${Button({ label: 'Backup now', iconName: 'play', variant: 'secondary', size: 'sm', attrs: { 'data-run': db } })}
          ${Button({ iconName: 'sliders-horizontal', variant: 'ghost', size: 'sm', attrs: { 'data-act': 'sched-toggle', 'aria-label': 'Edit schedule' } })}
        </div>
      </div>
      <div class="schedule hidden" style="border-top:1px solid var(--c-border); background:var(--c-surface-muted);">
        <form class="schedule-form p-3 grid grid-cols-2 sm:grid-cols-4 gap-3 items-end">
          ${Field({
            label: 'Interval (minutes)',
            control: Input({ name: 'interval_minutes', type: 'number', value: interval, attrs: { min: 1 } }),
          })}
          ${Field({
            label: 'Keep (most recent)',
            control: Input({ name: 'retention', type: 'number', value: retention, attrs: { min: 1 } }),
          })}
          <div class="self-end pb-1.5">
            ${Toggle({ name: 'enabled', checked: enabled, label: 'Enabled' })}
          </div>
          <div class="flex justify-end gap-2">
            ${sched ? Button({ label: 'Remove', variant: 'danger', size: 'sm', attrs: { type: 'button', 'data-del-sched': sched.id } }) : ''}
            ${Button({ label: 'Save', variant: 'primary', size: 'sm', type: 'submit' })}
          </div>
        </form>
      </div>
    </div>
  `;
}

function bindDbRows(panel, conn) {
  panel.querySelectorAll(':scope > div > [data-db]').forEach(rowEl => {
    const db = rowEl.dataset.db;
    const schedSec = rowEl.querySelector('.schedule');
    const sched = local.schedules.find(s => s.connection_id === conn.id && s.database === db);

    rowEl.querySelector('[data-act="sched-toggle"]').onclick = () => {
      schedSec.classList.toggle('hidden');
    };

    rowEl.querySelector('[data-run]').onclick = async (e) => {
      const btn = e.currentTarget;
      await withSpinner(btn, async () => {
        try {
          const r = await api.post('/api/backups/run', {
            connection_id: conn.id,
            database: db,
            schedule_id: sched ? sched.id : null,
          });
          toast(`backup ok: ${r.filename}`);
        } catch (err) { toast(err.message, 'err'); }
      }, { busyLabel: 'Running' });
    };

    const form = rowEl.querySelector('form.schedule-form');
    if (form) {
      form.onsubmit = async (e) => {
        e.preventDefault();
        const fd = new FormData(form);
        try {
          await api.post('/api/schedules', {
            connection_id: conn.id,
            database: db,
            interval_minutes: Number(fd.get('interval_minutes')),
            retention: Number(fd.get('retention')),
            enabled: fd.get('enabled') === 'on',
          });
          toast('schedule saved');
          await refresh();
          repaintRow(panel, conn, db);
        } catch (err) { toast(err.message, 'err'); }
      };
      const delBtn = form.querySelector('[data-del-sched]');
      if (delBtn) {
        delBtn.onclick = async () => {
          if (!await confirmDialog({
            title: 'Remove schedule?',
            message: 'This stops automatic backups for this database. Existing backup files are kept.',
            confirmLabel: 'Remove',
            danger: true,
          })) return;
          try {
            await api.del(`/api/schedules/${delBtn.dataset.delSched}`);
            toast('schedule removed');
            await refresh();
            repaintRow(panel, conn, db);
          } catch (err) { toast(err.message, 'err'); }
        };
      }
    }
  });
}

function repaintRow(panel, conn, db) {
  const old = panel.querySelector(`[data-db="${cssEscape(db)}"]`);
  if (!old) return;
  const wrap = document.createElement('div');
  wrap.innerHTML = dbRow(conn, db);
  const fresh = wrap.firstElementChild;
  old.replaceWith(fresh);
  ensureIcons(fresh);
  bindDbRows(panel, conn);
}

function cssEscape(s) { return s.replace(/[\\"]/g, '\\$&'); }

// ---------- Connection add/edit dialog ----------

function connDialog(existing) {
  const isEdit = !!existing;
  const html = `
    <form class="card-pad" style="width:28rem; max-width:90vw;" novalidate>
      <h3 class="auth-title" style="font-size:1rem;">${isEdit ? 'Edit' : 'Add'} connection</h3>
      <div class="space-y-3 mt-4">
        ${Field({ label: 'Label', control: Input({ name: 'label', value: existing?.label || '', placeholder: 'Production' }) })}
        ${Field({
          label: 'Connection URI',
          hint: isEdit ? 'Leave blank to keep the current URI.' : '',
          control: Input({ name: 'uri', placeholder: 'mongodb://user:pass@host:27017', mono: true }),
        })}
        <p class="err hidden field-error"></p>
      </div>
      <div class="flex justify-end gap-2 mt-5">
        ${Button({ label: 'Cancel', variant: 'ghost', attrs: { type: 'button', 'data-act': 'cancel' } })}
        ${Button({ label: 'Test', iconName: 'check-circle', variant: 'secondary', attrs: { type: 'button', 'data-act': 'test' } })}
        ${Button({ label: isEdit ? 'Save' : 'Create', iconName: 'check', variant: 'primary', attrs: { type: 'submit', 'data-act': 'save' } })}
      </div>
    </form>
  `;

  openDialog({
    html,
    onMount(dlg, close) {
      const form = dlg.querySelector('form');
      const errEl = dlg.querySelector('.err');
      const showErr = (m) => { errEl.textContent = m; errEl.classList.remove('hidden'); };
      const hideErr = () => errEl.classList.add('hidden');

      dlg.querySelector('[data-act="cancel"]').onclick = () => close();

      dlg.querySelector('[data-act="test"]').onclick = async (e) => {
        hideErr();
        const fd = new FormData(form);
        const uri = (fd.get('uri') || '').toString().trim();
        if (!uri) return showErr('URI required to test');
        await withSpinner(e.currentTarget, async () => {
          try {
            const r = await api.post('/api/connections/test', { uri });
            toast(`connection ok — ${r.databases.length} db(s)`);
          } catch (err) { showErr(err.message); }
        }, { busyLabel: 'Testing' });
      };

      form.addEventListener('submit', async (e) => {
        e.preventDefault();
        hideErr();
        const fd = new FormData(form);
        const label = (fd.get('label') || '').toString().trim();
        const uri = (fd.get('uri') || '').toString().trim();
        const submitBtn = form.querySelector('[data-act="save"]');
        await withSpinner(submitBtn, async () => {
          try {
            if (isEdit) {
              await api.put(`/api/connections/${existing.id}`, { label, uri: uri || null });
            } else {
              await api.post('/api/connections', { label, uri });
            }
            close();
            toast(isEdit ? 'saved' : 'connection added');
            const view = $('#view-root');
            if (view) await render(view);
          } catch (err) { showErr(err.message); }
        }, { busyLabel: isEdit ? 'Saving' : 'Creating' });
      });
    },
  });
}

async function deleteConn(conn) {
  const ok = await confirmDialog({
    title: `Delete "${conn.label}"?`,
    message: 'This removes the connection and its schedules. Backup files on disk are kept.',
    confirmLabel: 'Delete',
    danger: true,
  });
  if (!ok) return;
  try {
    await api.del(`/api/connections/${conn.id}`);
    toast('connection deleted');
    const view = $('#view-root');
    if (view) await render(view);
  } catch (e) { toast(e.message, 'err'); }
}
