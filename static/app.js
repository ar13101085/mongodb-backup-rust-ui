// SPA entrypoint: auth check, shell wiring, view orchestration, jobs SSE.

import { $, $$, ensureIcons, icon, openDialog, Button } from '/ui.js';
import * as connections from '/views/connections.js';
import * as backups from '/views/backups.js';
import * as restore from '/views/restore.js';
import { startJobsStream, setOnFinished } from '/views/jobs.js';

const VIEWS = {
  connections,
  backups,
  restore,
};

const state = { activeView: null };

function setNavActive(name) {
  $$('.sidebar-link').forEach(b => {
    if (b.dataset.nav === name) b.setAttribute('aria-current', 'page');
    else b.removeAttribute('aria-current');
  });
}

async function showView(name) {
  if (!VIEWS[name]) name = 'connections';
  state.activeView = name;
  history.replaceState(null, '', `#${name}`);
  setNavActive(name);

  const view = VIEWS[name];
  $('#page-title').textContent = view.pageTitle || '';

  const actions = $('#page-actions');
  actions.innerHTML = view.pageActions ? view.pageActions() : '';
  ensureIcons(actions);
  if (view.bindPageActions) view.bindPageActions(actions);

  const root = $('#view-root');
  await view.render(root);

  // Close mobile sidebar after navigation
  $('#sidebar')?.classList.remove('open');
}

function userMenu(user) {
  const html = `
    <div style="padding:0.625rem; min-width:14rem;">
      <div style="padding:0.5rem 0.5rem 0.625rem; border-bottom:1px solid var(--c-border); margin-bottom:0.375rem;">
        <div style="font-size:0.8125rem; font-weight:500;">${escapeUser(user.username)}</div>
        <div style="font-size:0.75rem; color:var(--c-text-muted);">${user.is_admin ? 'Administrator' : 'User'}</div>
      </div>
      ${Button({ label: 'Sign out', iconName: 'log-out', variant: 'ghost', attrs: { 'data-act': 'logout', style: 'width:100%; justify-content:flex-start;' } })}
    </div>
  `;
  openDialog({
    html,
    onMount(dlg, close) {
      dlg.style.position = 'fixed';
      dlg.style.left = '12px';
      dlg.style.bottom = '64px';
      dlg.style.margin = '0';
      dlg.querySelector('[data-act="logout"]').onclick = async () => {
        await fetch('/api/auth/logout', { method: 'POST' });
        location.href = '/login.html';
      };
      // Click outside to close
      dlg.addEventListener('click', (e) => { if (e.target === dlg) close(); });
    },
  });
}

function escapeUser(s) {
  return String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

(async function boot() {
  ensureIcons();

  let status;
  try { status = await fetch('/api/auth/status').then(r => r.json()); }
  catch { return; }

  if (status.needs_setup) { location.href = '/setup.html'; return; }
  if (!status.user) { location.href = '/login.html'; return; }

  $('#loading').classList.add('hidden');
  $('#app').classList.remove('hidden');

  // Sidebar user
  $('#user-name').textContent = status.user.username;
  $('#user-avatar').textContent = (status.user.username[0] || '?').toUpperCase();
  $('#user-menu').onclick = () => userMenu(status.user);

  // Sidebar nav
  $$('.sidebar-link').forEach(b => {
    b.onclick = () => showView(b.dataset.nav);
  });

  // Mobile toggle
  $('#sidebar-toggle')?.addEventListener('click', () => {
    $('#sidebar').classList.toggle('open');
  });

  // Jobs stream — wire callback so the Backups table refreshes when a job ends
  setOnFinished(() => {
    if (state.activeView === 'backups') backups.refreshIfActive();
  });
  startJobsStream();

  // Initial view from URL hash
  const initial = (location.hash || '#connections').replace('#', '');
  await showView(initial);

  // Keep nav in sync with hash changes
  window.addEventListener('hashchange', () => {
    const n = (location.hash || '#connections').replace('#', '');
    if (n !== state.activeView) showView(n);
  });

  // Re-render Lucide whenever big chunks of HTML are written. Views call
  // ensureIcons themselves; this is a defensive pass for the shell.
  ensureIcons();
})();
