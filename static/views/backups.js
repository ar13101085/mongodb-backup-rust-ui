import {
  $, $$, escapeHtml, fmtBytes, fmtDate, fmtRelative, ensureIcons, icon,
  api, Button, Card, Select, EmptyState, Skeleton, Badge,
  toast, confirmDialog,
} from '/ui.js';

const local = { connections: [], files: [], filter: { conn: '', q: '' } };

export const pageTitle = 'Backups';

export function pageActions() {
  return Button({ iconName: 'refresh-cw', variant: 'secondary', size: 'sm', attrs: { id: 'refresh-backups', 'aria-label': 'Refresh' } });
}

export function bindPageActions(actionsRoot) {
  $('#refresh-backups', actionsRoot).onclick = () => render($('#view-root'));
}

export async function render(view) {
  view.innerHTML = `
    <div class="card card-pad">
      ${[0, 1, 2].map(() => Skeleton({ width: '100%', height: '2rem', className: 'mb-2' })).join('')}
    </div>
  `;

  [local.connections, local.files] = await Promise.all([
    api.get('/api/connections'),
    api.get('/api/backups'),
  ]);
  paint(view);
}

export function refreshIfActive() {
  const view = $('#view-root');
  if (!view) return;
  // Only refresh if the backups view is currently rendered.
  if ($('#backups-view', view)) render(view);
}

function paint(view) {
  if (local.files.length === 0) {
    view.innerHTML = `<div id="backups-view">${
      EmptyState({
        iconName: 'archive',
        title: 'No backups yet',
        hint: 'Run "Backup now" on a database from the Connections page, or wait for a scheduled run to fire.',
      })
    }</div>`;
    ensureIcons(view);
    return;
  }

  const connOpts = [{ value: '', label: 'All connections' }, ...local.connections.map(c => ({ value: c.id, label: c.label }))];

  view.innerHTML = `
    <div id="backups-view" class="space-y-4">
      <div class="flex flex-col sm:flex-row gap-2 sm:items-center">
        <div class="flex-1 relative">
          <span class="absolute left-3 top-1/2 -translate-y-1/2 text-text-subtle">${icon('search', { size: 14 })}</span>
          <input id="bk-search" class="input" style="padding-left:2rem;" placeholder="Search filename, database…" value="${escapeHtml(local.filter.q)}" />
        </div>
        <div style="min-width:14rem;">
          ${Select({ name: 'conn', value: local.filter.conn, options: connOpts, attrs: { id: 'bk-conn' } })}
        </div>
      </div>

      <div class="card overflow-hidden">
        <div class="overflow-x-auto">
          <table class="tbl">
            <thead>
              <tr>
                <th>Connection</th>
                <th>Database</th>
                <th>Created</th>
                <th class="num">Size</th>
                <th class="actions"></th>
              </tr>
            </thead>
            <tbody id="bk-body"></tbody>
          </table>
        </div>
      </div>

      <p id="bk-empty" class="text-sm text-text-muted text-center py-6 hidden">
        No backups match your filters.
      </p>
    </div>
  `;
  ensureIcons(view);
  $('#bk-search', view).oninput = (e) => { local.filter.q = e.target.value; renderRows(view); };
  $('#bk-conn', view).onchange = (e) => { local.filter.conn = e.target.value; renderRows(view); };
  renderRows(view);
}

function renderRows(view) {
  const body = $('#bk-body', view);
  const empty = $('#bk-empty', view);
  const q = local.filter.q.trim().toLowerCase();
  const filtered = local.files.filter(f => {
    if (local.filter.conn && f.connection_id !== local.filter.conn) return false;
    if (q) {
      const hay = `${f.filename} ${f.database || ''}`.toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  });

  if (filtered.length === 0) {
    body.innerHTML = '';
    empty.classList.remove('hidden');
    return;
  }
  empty.classList.add('hidden');

  const connLabel = (id) => local.connections.find(c => c.id === id)?.label || '(deleted)';

  body.innerHTML = filtered.map(f => `
    <tr data-file="${escapeHtml(f.filename)}">
      <td>${escapeHtml(f.connection_id ? connLabel(f.connection_id) : '—')}</td>
      <td><code class="font-mono text-xs">${escapeHtml(f.database || '—')}</code></td>
      <td>
        <div title="${escapeHtml(fmtDate(f.created_at))}">${escapeHtml(fmtRelative(f.created_at))}</div>
        <div class="text-xs text-text-muted font-mono truncate" style="max-width:24rem;" title="${escapeHtml(f.filename)}">${escapeHtml(f.filename)}</div>
      </td>
      <td class="num font-mono">${fmtBytes(f.size_bytes)}</td>
      <td class="actions">
        <a class="btn btn-ghost btn-icon btn-sm" href="/api/backups/${encodeURIComponent(f.filename)}/download" title="Download">${icon('download', { size: 14 })}</a>
        <button class="btn btn-danger btn-icon btn-sm" data-del="${escapeHtml(f.filename)}" title="Delete">${icon('trash-2', { size: 14 })}</button>
      </td>
    </tr>
  `).join('');
  ensureIcons(body);

  body.querySelectorAll('[data-del]').forEach(btn => {
    btn.onclick = async () => {
      const file = btn.dataset.del;
      const ok = await confirmDialog({
        title: 'Delete backup file?',
        message: `${file}\n\nThis cannot be undone.`,
        confirmLabel: 'Delete',
        danger: true,
      });
      if (!ok) return;
      try {
        await api.del(`/api/backups/${encodeURIComponent(file)}`);
        toast('deleted');
        await render($('#view-root'));
      } catch (e) { toast(e.message, 'err'); }
    };
  });
}
