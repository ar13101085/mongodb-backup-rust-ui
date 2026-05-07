import {
  $, $$, escapeHtml, fmtBytes, fmtRelative, ensureIcons, icon,
  api, Button, Card, Field, Input, Select, Toggle, Segmented, bindSegmented,
  EmptyState, Skeleton, toast, withSpinner,
} from '/ui.js';

const local = { connections: [], files: [] };

export const pageTitle = 'Restore';

export function pageActions() { return ''; }
export function bindPageActions() {}

export async function render(view) {
  view.innerHTML = `
    <div class="grid md:grid-cols-2 gap-4">
      ${[0, 1].map(() => `<div class="card card-pad">${Skeleton({ height: '12rem' })}</div>`).join('')}
    </div>
  `;

  [local.connections, local.files] = await Promise.all([
    api.get('/api/connections'),
    api.get('/api/backups'),
  ]);
  paint(view);
}

function paint(view) {
  const connOpts = local.connections.map(c => ({ value: c.id, label: c.label }));
  const firstConnId = connOpts[0]?.value || '';

  const fileOpts = local.files.map(f => ({ value: f.filename, label: f.filename }));
  const firstFile = local.files[0]?.filename || '';

  view.innerHTML = `
    <div class="grid md:grid-cols-2 gap-4">

      <section class="card">
        <div class="card-header">
          <div class="flex items-center gap-2">
            ${icon('hard-drive', { size: 16, className: 'text-text-muted' })}
            <span class="card-title">From server file</span>
          </div>
        </div>
        <div class="card-pad">
          ${
            local.files.length === 0
              ? `<p class="text-sm text-text-muted">No backups exist on this server yet.</p>`
              : `<form id="form-server" class="space-y-3" novalidate>
                  ${Field({
                    label: 'Archive',
                    control: Select({ name: 'filename', value: firstFile, options: fileOpts, attrs: { class: 'select input-mono' } }),
                  })}
                  ${targetFields(connOpts, firstConnId)}
                  <div class="pt-1">
                    ${Button({ label: 'Restore', iconName: 'play', variant: 'primary', type: 'submit', attrs: { id: 'btn-server-submit' } })}
                  </div>
                </form>`
          }
        </div>
      </section>

      <section class="card">
        <div class="card-header">
          <div class="flex items-center gap-2">
            ${icon('upload-cloud', { size: 16, className: 'text-text-muted' })}
            <span class="card-title">From upload</span>
          </div>
        </div>
        <div class="card-pad">
          <form id="form-upload" class="space-y-3" novalidate enctype="multipart/form-data">
            <div id="dropzone" class="rounded-lg border-2 border-dashed border-border bg-surface-muted p-6 text-center cursor-pointer transition" tabindex="0">
              <div class="flex flex-col items-center gap-2 text-text-muted">
                ${icon('upload-cloud', { size: 24 })}
                <div class="text-sm"><span class="font-medium text-text">Click to choose</span> or drop a <code class="font-mono text-xs">*.archive.gz</code> here</div>
                <div id="dz-info" class="text-xs"></div>
              </div>
              <input id="file-input" type="file" name="file" accept=".gz,.archive,.archive.gz" class="hidden" />
            </div>
            ${targetFields(connOpts, firstConnId)}
            <div class="pt-1">
              ${Button({ label: 'Upload & restore', iconName: 'upload', variant: 'primary', type: 'submit', disabled: true, attrs: { id: 'btn-upload-submit' } })}
            </div>
          </form>
        </div>
      </section>

    </div>
  `;
  ensureIcons(view);
  bindSegmented(view);
  bindTargetVisibility(view);
  bindServer(view);
  bindUpload(view);
}

function targetFields(connOpts, firstConnId) {
  return `
    <div class="space-y-3">
      ${Field({
        label: 'Target',
        control: Segmented({
          name: 'target_mode',
          value: connOpts.length ? 'connection' : 'uri',
          options: [
            { value: 'connection', label: 'Existing connection' },
            { value: 'uri', label: 'Connection URI' },
          ],
        }),
      })}

      <div class="target-conn ${connOpts.length ? '' : 'hidden'}">
        ${Field({
          label: 'Connection',
          control: connOpts.length
            ? Select({ name: 'target_connection_id', value: firstConnId, options: connOpts })
            : `<div class="text-sm text-text-muted">No saved connections.</div>`,
        })}
      </div>

      <div class="target-uri ${connOpts.length ? 'hidden' : ''}">
        ${Field({
          label: 'Connection URI',
          control: Input({ name: 'target_uri', placeholder: 'mongodb://user:pass@host:27017', mono: true }),
        })}
      </div>

      ${Field({
        label: 'Restore as database',
        hint: 'Optional. Renames the database during restore (single-database archives only).',
        control: Input({ name: 'target_database', mono: true, placeholder: 'leave blank to keep original name' }),
      })}

      <label class="toggle">
        <input type="checkbox" name="drop_existing" />
        <span class="toggle-track"></span>
        <span>Drop existing collections first</span>
      </label>
    </div>
  `;
}

function bindTargetVisibility(view) {
  view.querySelectorAll('.segmented input[type=hidden]').forEach(hidden => {
    const form = hidden.closest('form');
    if (!form) return;
    const update = () => {
      const mode = hidden.value;
      form.querySelector('.target-conn')?.classList.toggle('hidden', mode !== 'connection');
      form.querySelector('.target-uri')?.classList.toggle('hidden', mode !== 'uri');
    };
    hidden.addEventListener('change', update);
    update();
  });
}

function collectTarget(form) {
  const fd = new FormData(form);
  const mode = fd.get('target_mode');
  return {
    target_connection_id: mode === 'connection' ? (fd.get('target_connection_id') || null) : null,
    target_uri: mode === 'uri' ? (fd.get('target_uri') || null) : null,
    target_database: ((fd.get('target_database') || '') + '').trim() || null,
    drop_existing: fd.get('drop_existing') === 'on',
  };
}

function bindServer(view) {
  const form = $('#form-server', view);
  if (!form) return;
  const submit = $('#btn-server-submit', form);
  form.onsubmit = async (e) => {
    e.preventDefault();
    const body = collectTarget(form);
    body.filename = (new FormData(form)).get('filename');
    await withSpinner(submit, async () => {
      try {
        await api.post('/api/restore/server', body);
        toast('restore complete');
      } catch (err) { toast(err.message, 'err'); }
    }, { busyLabel: 'Restoring' });
  };
}

function bindUpload(view) {
  const form = $('#form-upload', view);
  const dz = $('#dropzone', form);
  const input = $('#file-input', form);
  const info = $('#dz-info', form);
  const submit = $('#btn-upload-submit', form);

  function onFile(file) {
    if (!file) { info.textContent = ''; submit.disabled = true; return; }
    info.innerHTML = `<span class="font-mono">${escapeHtml(file.name)}</span> · ${fmtBytes(file.size)}`;
    submit.disabled = false;
  }

  dz.onclick = () => input.click();
  dz.onkeydown = (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); input.click(); } };

  input.onchange = () => onFile(input.files[0]);

  ['dragenter', 'dragover'].forEach(ev => dz.addEventListener(ev, (e) => {
    e.preventDefault(); e.stopPropagation();
    dz.style.borderColor = 'var(--c-accent)';
    dz.style.background = 'var(--c-accent-soft)';
  }));
  ['dragleave', 'drop'].forEach(ev => dz.addEventListener(ev, (e) => {
    e.preventDefault(); e.stopPropagation();
    dz.style.borderColor = '';
    dz.style.background = '';
  }));
  dz.addEventListener('drop', (e) => {
    const file = e.dataTransfer?.files?.[0];
    if (!file) return;
    const dt = new DataTransfer();
    dt.items.add(file);
    input.files = dt.files;
    onFile(file);
  });

  form.onsubmit = async (e) => {
    e.preventDefault();
    if (!input.files[0]) return;
    const target = collectTarget(form);
    const out = new FormData();
    out.append('file', input.files[0]);
    if (target.target_connection_id) out.append('target_connection_id', target.target_connection_id);
    if (target.target_uri) out.append('target_uri', target.target_uri);
    if (target.target_database) out.append('target_database', target.target_database);
    out.append('drop_existing', target.drop_existing ? 'true' : 'false');

    await withSpinner(submit, async () => {
      try {
        await api.upload('/api/restore/upload', out);
        toast('restore complete');
        input.value = '';
        onFile(null);
      } catch (err) { toast(err.message, 'err'); }
    }, { busyLabel: 'Uploading' });
  };
}
