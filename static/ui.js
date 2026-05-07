// Shared UI helpers and components. No framework — these are pure functions
// that return HTML strings. Bind handlers after the host element does
// `innerHTML = ...` and call `ensureIcons(root)` so Lucide replaces the
// `<i data-lucide="...">` placeholders with real SVGs.

export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

export function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c => (
    { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]
  ));
}

export function fmtBytes(n) {
  if (n == null) return '';
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(n < 10 && i > 0 ? 1 : 0)} ${u[i]}`;
}

export function fmtDate(s) {
  if (!s) return '—';
  return new Date(s).toLocaleString();
}

export function fmtRelative(s) {
  if (!s) return '—';
  const ms = Date.now() - new Date(s).getTime();
  const sec = Math.round(ms / 1000);
  if (sec < 60) return `${sec}s ago`;
  const min = Math.round(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.round(hr / 24);
  if (day < 30) return `${day}d ago`;
  const mo = Math.round(day / 30);
  if (mo < 12) return `${mo}mo ago`;
  return `${Math.round(mo / 12)}y ago`;
}

// ---------- Icons ----------

export function icon(name, { size = 16, className = '' } = {}) {
  const cls = className ? ` class="${escapeHtml(className)}"` : '';
  return `<i data-lucide="${escapeHtml(name)}" data-size="${size}"${cls}></i>`;
}

export function ensureIcons(root = document) {
  if (window.lucide && typeof window.lucide.createIcons === 'function') {
    window.lucide.createIcons({ nameAttr: 'data-lucide', root });
  }
}

// ---------- API client ----------

export const api = {
  async get(path) { return handleResp(await fetch(path)); },
  async post(path, body) {
    return handleResp(await fetch(path, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body ?? {}),
    }));
  },
  async put(path, body) {
    return handleResp(await fetch(path, {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body ?? {}),
    }));
  },
  async del(path) { return handleResp(await fetch(path, { method: 'DELETE' })); },
  async upload(path, formData) {
    return handleResp(await fetch(path, { method: 'POST', body: formData }));
  },
};

async function handleResp(r) {
  if (r.status === 401) { location.href = '/login.html'; throw new Error('unauthorized'); }
  if (!r.ok) {
    const data = await r.json().catch(() => ({}));
    throw new Error(data.error || `${r.status} ${r.statusText}`);
  }
  if (r.status === 204) return null;
  const ct = r.headers.get('content-type') || '';
  if (!ct.includes('application/json')) return null;
  return r.json();
}

// ---------- Components ----------

function attrsToHtml(attrs = {}) {
  return Object.entries(attrs)
    .filter(([, v]) => v !== undefined && v !== null && v !== false)
    .map(([k, v]) => v === true ? ` ${k}` : ` ${k}="${escapeHtml(v)}"`)
    .join('');
}

export function Button({
  label = '',
  variant = 'secondary',
  size,
  iconName,
  iconRight = false,
  type = 'button',
  attrs = {},
  className = '',
  disabled = false,
} = {}) {
  const classes = ['btn', `btn-${variant}`, size === 'sm' ? 'btn-sm' : '', !label ? 'btn-icon' : '', className]
    .filter(Boolean).join(' ');
  const ic = iconName ? icon(iconName) : '';
  const inner = !label
    ? ic
    : iconRight ? `<span>${escapeHtml(label)}</span>${ic}` : `${ic}<span>${escapeHtml(label)}</span>`;
  const dis = disabled ? ' disabled' : '';
  return `<button type="${type}" class="${classes}"${attrsToHtml(attrs)}${dis}>${inner}</button>`;
}

export function Card({ title, action = '', body = '', padding = 'pad', className = '' } = {}) {
  const headerHtml = title
    ? `<div class="card-header"><div class="card-title">${escapeHtml(title)}</div>${action}</div>`
    : '';
  const padCls = padding === 'sm' ? 'card-pad-sm' : padding === 'pad' ? 'card-pad' : '';
  const bodyHtml = headerHtml
    ? `<div class="${padCls}">${body}</div>`
    : `<div class="${padCls}">${body}</div>`;
  return `<section class="card ${className}">${headerHtml}${bodyHtml}</section>`;
}

export function Field({ label, hint, error, control, className = '' } = {}) {
  return `<div class="${className}">
    ${label ? `<label class="field-label">${escapeHtml(label)}</label>` : ''}
    ${control}
    ${error ? `<div class="field-error">${escapeHtml(error)}</div>` : ''}
    ${hint && !error ? `<div class="field-hint">${escapeHtml(hint)}</div>` : ''}
  </div>`;
}

export function Input({ name, type = 'text', value = '', placeholder = '', mono = false, attrs = {} } = {}) {
  const cls = `input${mono ? ' input-mono' : ''}`;
  return `<input class="${cls}" name="${escapeHtml(name)}" type="${escapeHtml(type)}" value="${escapeHtml(value)}" placeholder="${escapeHtml(placeholder)}"${attrsToHtml(attrs)} />`;
}

export function Select({ name, value, options = [], attrs = {} } = {}) {
  const opts = options.map(o => {
    const v = typeof o === 'string' ? o : o.value;
    const l = typeof o === 'string' ? o : (o.label ?? o.value);
    const sel = v === value ? ' selected' : '';
    return `<option value="${escapeHtml(v)}"${sel}>${escapeHtml(l)}</option>`;
  }).join('');
  return `<select class="select" name="${escapeHtml(name)}"${attrsToHtml(attrs)}>${opts}</select>`;
}

export function Toggle({ name, checked = false, label = '', attrs = {} } = {}) {
  return `<label class="toggle">
    <input type="checkbox" name="${escapeHtml(name)}"${checked ? ' checked' : ''}${attrsToHtml(attrs)} />
    <span class="toggle-track"></span>
    ${label ? `<span>${escapeHtml(label)}</span>` : ''}
  </label>`;
}

export function Segmented({ name, value, options = [] } = {}) {
  return `<div class="segmented" role="tablist" data-segmented="${escapeHtml(name)}">
    ${options.map(o => {
      const v = typeof o === 'string' ? o : o.value;
      const l = typeof o === 'string' ? o : (o.label ?? o.value);
      return `<button type="button" role="tab" data-value="${escapeHtml(v)}" aria-selected="${v === value}">${escapeHtml(l)}</button>`;
    }).join('')}
    <input type="hidden" name="${escapeHtml(name)}" value="${escapeHtml(value)}" />
  </div>`;
}

export function bindSegmented(root) {
  $$('.segmented', root).forEach(seg => {
    const hidden = seg.querySelector('input[type=hidden]');
    seg.addEventListener('click', (e) => {
      const btn = e.target.closest('button[data-value]');
      if (!btn) return;
      $$('button[data-value]', seg).forEach(b => b.setAttribute('aria-selected', b === btn));
      hidden.value = btn.dataset.value;
      hidden.dispatchEvent(new Event('change', { bubbles: true }));
    });
  });
}

export function Badge({ tone = 'muted', label = '', withDot = true } = {}) {
  return `<span class="badge badge-${tone}">${withDot ? '<span class="badge-dot"></span>' : ''}${escapeHtml(label)}</span>`;
}

export function Spinner({ size = 'sm', className = '' } = {}) {
  return `<span class="spinner${size === 'lg' ? ' spinner-lg' : ''} ${className}"></span>`;
}

export function Skeleton({ width = '100%', height = '0.875rem', className = '' } = {}) {
  return `<div class="skeleton ${className}" style="width:${width};height:${height}"></div>`;
}

export function EmptyState({ iconName = 'inbox', title = 'Nothing here yet', hint = '', action = '' } = {}) {
  return `<div class="empty">
    <div class="empty-icon">${icon(iconName, { size: 20 })}</div>
    <div class="empty-title">${escapeHtml(title)}</div>
    ${hint ? `<div class="empty-hint">${escapeHtml(hint)}</div>` : ''}
    ${action ? `<div class="mt-2">${action}</div>` : ''}
  </div>`;
}

// ---------- Toast ----------

export function toast(msg, kind = 'ok') {
  let root = $('#toast-root');
  if (!root) {
    root = document.createElement('div');
    root.id = 'toast-root';
    root.className = 'toast-root';
    document.body.appendChild(root);
  }
  const el = document.createElement('div');
  el.className = `toast${kind === 'err' ? ' toast-err' : ''}`;
  el.innerHTML = `${icon(kind === 'err' ? 'alert-circle' : 'check-circle', { size: 14 })}<span>${escapeHtml(msg)}</span>`;
  root.appendChild(el);
  ensureIcons(el);
  setTimeout(() => {
    el.classList.add('toast-out');
    setTimeout(() => el.remove(), 220);
  }, 2400);
}

// ---------- Confirm dialog ----------

/**
 * Replaces window.confirm with a styled <dialog>. Returns Promise<boolean>.
 */
export function confirmDialog({ title = 'Are you sure?', message = '', confirmLabel = 'Confirm', cancelLabel = 'Cancel', danger = false } = {}) {
  return new Promise(resolve => {
    const dlg = document.createElement('dialog');
    dlg.innerHTML = `
      <div style="padding:1.25rem 1.25rem 1rem; min-width:22rem; max-width:28rem;">
        <div style="display:flex; gap:0.75rem; align-items:flex-start;">
          <div class="empty-icon" style="background:${danger ? 'var(--c-danger-soft)' : 'var(--c-accent-soft)'}; color:${danger ? 'var(--c-danger)' : 'var(--c-accent)'}; width:36px; height:36px; margin:0;">
            ${icon(danger ? 'alert-triangle' : 'help-circle', { size: 18 })}
          </div>
          <div style="flex:1;">
            <div style="font-size:0.9375rem; font-weight:600;">${escapeHtml(title)}</div>
            ${message ? `<div style="font-size:0.8125rem; color:var(--c-text-muted); margin-top:0.25rem;">${escapeHtml(message)}</div>` : ''}
          </div>
        </div>
        <div style="display:flex; justify-content:flex-end; gap:0.5rem; padding:1rem 0 0;">
          ${Button({ label: cancelLabel, variant: 'secondary', attrs: { 'data-act': 'cancel' } })}
          ${Button({ label: confirmLabel, variant: danger ? 'danger-solid' : 'primary', attrs: { 'data-act': 'confirm' } })}
        </div>
      </div>
    `;
    document.body.appendChild(dlg);
    ensureIcons(dlg);
    const close = (result) => { dlg.close(); dlg.remove(); resolve(result); };
    dlg.querySelector('[data-act="cancel"]').onclick = () => close(false);
    dlg.querySelector('[data-act="confirm"]').onclick = () => close(true);
    dlg.addEventListener('cancel', (e) => { e.preventDefault(); close(false); });
    dlg.showModal();
  });
}

// ---------- Generic dialog opener ----------

/**
 * Creates a <dialog>, populates inner HTML, calls onMount(dlg) for binding.
 * Returns { close } to close programmatically.
 */
export function openDialog({ html, onMount }) {
  const dlg = document.createElement('dialog');
  dlg.innerHTML = html;
  document.body.appendChild(dlg);
  ensureIcons(dlg);
  const close = () => { try { dlg.close(); } catch {} dlg.remove(); };
  if (onMount) onMount(dlg, close);
  dlg.addEventListener('cancel', (e) => { e.preventDefault(); close(); });
  dlg.showModal();
  return { dialog: dlg, close };
}

// ---------- Submit-with-spinner helper ----------

export async function withSpinner(btn, fn, { busyLabel } = {}) {
  if (!btn) return fn();
  const origLabel = btn.innerHTML;
  btn.disabled = true;
  btn.innerHTML = `${Spinner()}<span>${escapeHtml(busyLabel || 'Working…')}</span>`;
  try { return await fn(); }
  finally { btn.disabled = false; btn.innerHTML = origLabel; }
}

// ---------- Copy-to-clipboard helper ----------

export async function copyText(text, { successMsg = 'copied' } = {}) {
  try {
    await navigator.clipboard.writeText(text);
    toast(successMsg);
  } catch {
    toast('copy failed', 'err');
  }
}
