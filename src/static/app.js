'use strict';
(() => {
  const $ = (selector) => document.querySelector(selector);
  const list = $('#service-list');
  const detail = $('#detail');
  const placeholder = detail.innerHTML;
  const form = $('#service-form');
  const dialog = $('#service-dialog');
  const deleteDialog = $('#delete-dialog');
  let services = [];
  let selected = document.body.dataset.selectedId;
  let filter = 'all';
  let activeTab = 'overview';
  let editing = null;
  let deleting = null;
  let source = null;
  let streamId = null;
  let latestLogs = { stdout: '', stderr: '' };
  let streamState = 'Connecting…';
  let refreshPending = false;
  let refreshAgain = false;
  let toastTimer;
  const pending = new Set();
  const serverIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="6" rx="2"/><rect x="3" y="14" width="18" height="6" rx="2"/><path d="M7 7h.01M7 17h.01M12 7h5M12 17h5"/></svg>';
  const escape = (value) => String(value ?? '').replace(/[&<>"']/g, (char) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[char]);
  const state = (service) => ['running', 'stopped', 'starting', 'error'].includes(service.status.state) ? service.status.state : 'error';
  const endpoint = (id) => `/api/services/${encodeURIComponent(id)}`;
  const duration = (seconds) => seconds == null ? '—' : seconds < 60 ? `${seconds}s` : seconds < 3600 ? `${Math.floor(seconds / 60)}m` : `${Math.floor(seconds / 3600)}h ${Math.floor(seconds % 3600 / 60)}m`;
  const badge = (service) => `<span class="badge ${state(service)}"><i class="dot ${state(service)}"></i>${state(service)[0].toUpperCase() + state(service).slice(1)}</span>`;
  const actionButtons = (service) => {
    const id = escape(service.config.id);
    const disabled = pending.has(service.config.id) ? ' disabled' : '';
    const running = ['running', 'starting'].includes(state(service));
    return `<button data-action="${running ? 'stop' : 'start'}" data-id="${id}"${disabled}>${running ? 'Stop' : 'Start'}</button><button data-action="restart" data-id="${id}"${disabled}>Restart</button>`;
  };
  function showError(selector, message) {
    const node = $(selector);
    node.textContent = message || '';
    node.hidden = !message;
  }
  function toast(message) {
    clearTimeout(toastTimer);
    $('#toast').textContent = message;
    $('#toast').hidden = false;
    toastTimer = setTimeout(() => { $('#toast').hidden = true; }, 4500);
  }
  async function request(url, options = {}) {
    const response = await fetch(url, { ...options, headers: { ...(options.method ? { 'X-L2D-Request': '1' } : {}), ...(options.body ? { 'Content-Type': 'application/json' } : {}), ...options.headers } });
    if (!response.ok) {
      let message = `Request failed (${response.status}). Please try again.`;
      try { const body = await response.json(); message = body.error || message; } catch { /* Keep the HTTP error if the response is not JSON. */ }
      const error = new Error(message);
      error.status = response.status;
      throw error;
    }
    if (response.status === 204) return null;
    const text = await response.text();
    return text ? JSON.parse(text) : null;
  }
  function renderList() {
    const query = $('#search').value.toLowerCase();
    for (const key of ['all', 'running', 'stopped', 'error']) $('#count-' + key).textContent = key === 'all' ? services.length : services.filter((service) => state(service) === key).length;
    const visible = services.filter((service) => (filter === 'all' || state(service) === filter) && `${service.config.id} ${service.config.executable}`.toLowerCase().includes(query));
    list.innerHTML = visible.map((service) => `<article class="service-card ${selected === service.config.id ? 'selected' : ''}"><div class="service-icon">${serverIcon}</div><div><div class="card-top"><h2><a data-select="${escape(service.config.id)}" href="/services/${encodeURIComponent(service.config.id)}">${escape(service.config.id)}</a></h2>${badge(service)}</div><p class="command" title="${escape(service.config.executable)}">${escape(service.config.executable)}</p><div class="card-bottom"><div class="metrics-inline"><span>PID ${escape(service.status.pid ?? '—')}</span><span>Uptime ${duration(service.status.uptime_seconds)}</span></div><div class="card-actions">${actionButtons(service)}</div></div></div></article>`).join('') || `<div class="empty-state"><div class="service-icon">${serverIcon}</div><h2>${services.length ? 'No matching services' : 'Make room for your next service'}</h2><p>${services.length ? 'Try another search or choose a different status.' : 'Add an executable and let launchd handle the rest. Your local services will appear here.'}</p><button ${services.length ? 'data-action="clear-filters"' : 'class="primary" data-action="create"'}>${services.length ? 'Clear filters' : 'Add your first service'}</button></div>`;
    $('#service-nav').innerHTML = services.map((service) => `<a class="service-nav-item ${selected === service.config.id ? 'selected' : ''}" data-select="${escape(service.config.id)}" href="/services/${encodeURIComponent(service.config.id)}"${selected === service.config.id ? ' aria-current="page"' : ''}><i class="dot ${state(service)}"></i><span>${escape(service.config.id)}</span></a>`).join('') || '<p class="empty-nav">No services yet</p>';
    list.setAttribute('aria-busy', 'false');
  }
  function facts(entries) {
    return `<dl class="facts">${entries.map(([label, value, mono]) => `<div class="fact"><dt>${escape(label)}</dt><dd${mono ? ' class="mono"' : ''}>${escape(value ?? '—')}</dd></div>`).join('')}</dl>`;
  }
  function renderDetail() {
    const service = services.find((entry) => entry.config.id === selected);
    if (!service) { detail.innerHTML = placeholder; disconnect(); return; }
    const logPositions = ['stdout', 'stderr'].map((channel) => {
      const node = $('#' + channel);
      return node ? { channel, top: node.scrollTop, follow: node.scrollHeight - node.scrollTop - node.clientHeight < 30 } : null;
    });
    const config = service.config;
    const overview = facts([['Command', config.executable, true], ['Directory', config.working_directory || 'Default'], ['Host', 'This Mac · localhost'], ['Autostart', config.autostart ? 'Yes' : 'No'], ['Restart policy', config.restart_on_failure ? 'On failure' : 'Never']]);
    const configuration = `<section class="config-block"><h3>LaunchAgent configuration</h3>${facts([['Label', `launch2dashboard.${config.id}`, true], ['Executable', config.executable, true], ['Arguments', config.arguments.join('\n') || 'None', true], ['Directory', config.working_directory || 'Default'], ['Environment', Object.entries(config.environment).map(([key, value]) => `${key}=${value}`).join('\n') || 'None', true], ['Autostart', config.autostart ? 'Yes' : 'No'], ['Restart policy', config.restart_on_failure ? 'On failure' : 'Never']])}<p><small>Environment values are visible only in this local dashboard.</small></p></section>`;
    detail.innerHTML = `<div class="detail-header"><div class="detail-heading"><div class="service-icon">${serverIcon}</div><div><h2>${escape(config.id)}</h2>${badge(service)}</div></div><button class="icon-button" data-action="close-detail" aria-label="Close service details"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18"/></svg></button></div><div class="tabs" role="tablist" aria-label="Service details">${['overview', 'logs', 'config'].map((tab) => `<button class="tab" id="tab-${tab}" role="tab" aria-controls="detail-panel" aria-selected="${activeTab === tab}" tabindex="${activeTab === tab ? 0 : -1}" data-tab="${tab}">${tab[0].toUpperCase() + tab.slice(1)}</button>`).join('')}</div><div class="detail-content" id="detail-panel" role="tabpanel" aria-labelledby="tab-${activeTab}" tabindex="0">${service.status.error ? `<p class="error-banner">${escape(service.status.error)}</p>` : ''}${activeTab === 'config' ? configuration : activeTab === 'overview' ? overview : ''}<div class="detail-actions">${actionButtons(service)}<button data-action="edit" data-id="${escape(config.id)}"${pending.has(config.id) ? ' disabled' : ''}>Edit</button><button data-action="delete" data-id="${escape(config.id)}"${pending.has(config.id) ? ' disabled' : ''}>Delete</button></div>${activeTab === 'overview' ? `<div class="metrics"><div class="metric"><small>Process ID</small><strong>${escape(service.status.pid ?? '—')}</strong></div><div class="metric"><small>Uptime</small><strong>${duration(service.status.uptime_seconds)}</strong></div><div class="metric"><small>Restarts</small><strong>${escape(service.status.restart_count ?? '—')}</strong></div></div>` : ''}${activeTab !== 'config' ? '<section aria-label="Service logs"><div class="logs-heading"><h3>Live logs</h3><span class="stream-state" id="stream-state"></span></div><p class="log-label">STDOUT</p><pre class="terminal" id="stdout" tabindex="0" aria-label="Standard output"></pre><p class="log-label">STDERR</p><pre class="terminal" id="stderr" tabindex="0" aria-label="Standard error"></pre></section>' : ''}</div>`;
    updateLogs();
    for (const position of logPositions) {
      const node = position && $('#' + position.channel);
      if (node) node.scrollTop = position.follow ? node.scrollHeight : position.top;
    }
    connect(config.id);
  }
  function render() {
    // Polling must not steal focus from the control currently in use.
    const focused = document.activeElement;
    const focusRoot = detail.contains(focused) ? detail : list.contains(focused) ? list : $('#service-nav').contains(focused) ? $('#service-nav') : document;
    const focusedId = focused?.id;
    const marker = focused?.dataset.action ? ['action', focused.dataset.action, focused.dataset.id] : focused?.dataset.tab ? ['tab', focused.dataset.tab] : focused?.dataset.select ? ['select', focused.dataset.select] : null;
    renderList(); renderDetail();
    if (marker) {
      const candidates = focusRoot.querySelectorAll(`[data-${marker[0]}]`);
      [...candidates].find((node) => node.dataset[marker[0]] === marker[1] && (!marker[2] || node.dataset.id === marker[2]))?.focus({ preventScroll: true });
    } else if (focusedId) {
      document.getElementById(focusedId)?.focus({ preventScroll: true });
    }
  }
  async function refresh() {
    if (refreshPending) { refreshAgain = true; return; }
    refreshPending = true;
    try {
      services = await request('/api/services');
      if (!Array.isArray(services)) throw new Error('Unexpected service response. Reload the dashboard.');
      showError('#page-error', '');
      $('#refresh-status').textContent = 'Live · refreshes every 3s';
      render();
    } catch (error) { showError('#page-error', error.message); $('#refresh-status').textContent = 'Connection interrupted'; list.setAttribute('aria-busy', 'false'); }
    finally { refreshPending = false; if (refreshAgain) { refreshAgain = false; void refresh(); } }
  }
  function disconnect() {
    source?.close(); source = null; streamId = null;
    latestLogs = { stdout: '', stderr: '' };
  }
  function updateLogs() {
    for (const channel of ['stdout', 'stderr']) {
      const node = $('#' + channel);
      if (!node) continue;
      const follow = node.scrollHeight - node.scrollTop - node.clientHeight < 30;
      const value = String(latestLogs[channel] || '').slice(-65536);
      if (node.textContent !== value) { node.textContent = value || 'No output yet.'; if (follow) node.scrollTop = node.scrollHeight; }
    }
    if ($('#stream-state')) $('#stream-state').textContent = streamState;
  }
  function connect(id) {
    if (streamId === id) return;
    disconnect(); streamId = id; streamState = 'Connecting…'; updateLogs();
    const current = new EventSource(`${endpoint(id)}/logs/stream`);
    source = current;
    current.onmessage = ({ data }) => {
      if (source !== current) return;
      try { latestLogs = JSON.parse(data); streamState = 'Live · latest output'; updateLogs(); }
      catch { streamState = 'Could not read log stream'; updateLogs(); }
    };
    current.addEventListener('service-error', ({ data }) => {
      if (source !== current) return;
      streamState = data || 'Log stream unavailable';
      current.close();
      updateLogs();
    });
    current.onopen = () => { if (source === current) { streamState = 'Live · latest output'; updateLogs(); } };
    current.onerror = async () => {
      if (source !== current) return;
      streamState = 'Disconnected · reconnecting…'; updateLogs();
      try { await request(`${endpoint(id)}/logs`); }
      catch (error) {
        if (source === current && error.status >= 400 && error.status < 500) {
          streamState = error.message;
          current.close();
          updateLogs();
        }
      }
    };
  }
  function select(id, push = true) {
    selected = id; activeTab = 'overview';
    if (push) history.pushState({}, '', id ? `/services/${encodeURIComponent(id)}` : '/');
    render();
    if (id && matchMedia('(max-width: 950px)').matches) detail.scrollIntoView({ block: 'start' });
  }
  function openForm(id = null) {
    editing = id;
    form.reset(); showError('#form-error', '');
    const config = services.find((service) => service.config.id === id)?.config;
    for (const name of ['id', 'executable', 'working_directory']) form.elements[name].value = config?.[name] || '';
    form.elements.id.disabled = Boolean(id);
    form.elements.arguments.value = config?.arguments.join('\n') || '';
    form.elements.environment.value = config ? Object.entries(config.environment).map(([key, value]) => `${key}=${value}`).join('\n') : '';
    form.elements.autostart.checked = config?.autostart ?? true;
    form.elements.restart_on_failure.checked = config?.restart_on_failure ?? false;
    $('#form-title').textContent = id ? 'Edit service' : 'Create service';
    $('#save-service').textContent = id ? 'Save changes' : 'Create service';
    dialog.showModal();
    form.elements[id ? 'executable' : 'id'].focus();
  }
  async function serviceAction(id, action) {
    if (pending.has(id)) return;
    pending.add(id); render();
    try { await request(`${endpoint(id)}/${action}`, { method: 'POST' }); toast(`${id}: ${action} requested.`); await refresh(); }
    catch (error) { showError('#page-error', error.message); }
    finally { pending.delete(id); render(); }
  }
  document.addEventListener('click', (event) => {
    const link = event.target.closest('[data-select]');
    if (link && !event.metaKey && !event.ctrlKey && !event.shiftKey && !event.altKey) { event.preventDefault(); select(link.dataset.select); return; }
    const button = event.target.closest('button');
    if (!button || button.disabled) return;
    if (button.dataset.filter) {
      filter = button.dataset.filter;
      document.querySelectorAll('[data-filter]').forEach((node) => { node.classList.toggle('selected', node.dataset.filter === filter); node.setAttribute('aria-pressed', String(node.dataset.filter === filter)); });
      renderList(); return;
    }
    if (button.dataset.tab) { activeTab = button.dataset.tab; renderDetail(); $(`#tab-${activeTab}`).focus(); return; }
    const { action, id } = button.dataset;
    if (['start', 'stop', 'restart'].includes(action)) void serviceAction(id, action);
    else if (action === 'create') openForm();
    else if (action === 'edit') openForm(id);
    else if (action === 'close-form' && !$('#save-service').disabled) dialog.close();
    else if (action === 'close-detail') select('');
    else if (action === 'clear-filters') { $('#search').value = ''; $('[data-filter="all"]').click(); }
    else if (action === 'delete') { deleting = id; $('#delete-description').textContent = `This will stop ${id} and remove its LaunchAgent configuration. Its log files will be kept.`; showError('#delete-error', ''); deleteDialog.showModal(); $('[data-action="cancel-delete"]').focus(); }
    else if (action === 'cancel-delete' && !$('#confirm-delete').disabled) deleteDialog.close();
  });
  detail.addEventListener('keydown', (event) => {
    if (!event.target.matches('[role=tab]')) return;
    const tabs = ['overview', 'logs', 'config'];
    const index = tabs.indexOf(activeTab);
    const next = event.key === 'ArrowRight' ? (index + 1) % 3 : event.key === 'ArrowLeft' ? (index + 2) % 3 : event.key === 'Home' ? 0 : event.key === 'End' ? 2 : null;
    if (next !== null) { event.preventDefault(); activeTab = tabs[next]; renderDetail(); $(`#tab-${activeTab}`).focus(); }
  });
  dialog.addEventListener('cancel', (event) => { if ($('#save-service').disabled) event.preventDefault(); });
  deleteDialog.addEventListener('cancel', (event) => { if ($('#confirm-delete').disabled) event.preventDefault(); });
  $('#search').addEventListener('input', renderList);
  form.elements.restart_on_failure.addEventListener('change', () => { if (form.elements.restart_on_failure.checked) form.elements.autostart.checked = true; });
  form.elements.autostart.addEventListener('change', () => { if (!form.elements.autostart.checked) form.elements.restart_on_failure.checked = false; });
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    if ($('#save-service').disabled) return;
    showError('#form-error', '');
    const environment = Object.create(null);
    try {
      for (const line of form.elements.environment.value.split('\n').filter((line) => line.trim())) {
        const position = line.indexOf('=');
        const key = line.slice(0, position).trim();
        if (position < 1 || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) throw new Error('Environment entries must use KEY=value with a valid variable name.');
        if (Object.hasOwn(environment, key)) throw new Error(`Environment key ${key} is repeated.`);
        environment[key] = line.slice(position + 1);
      }
      const config = { id: form.elements.id.value.trim(), executable: form.elements.executable.value.trim(), arguments: form.elements.arguments.value.split('\n').filter((line) => line !== ''), working_directory: form.elements.working_directory.value.trim() || null, environment, autostart: form.elements.autostart.checked, restart_on_failure: form.elements.restart_on_failure.checked };
      if (!config.executable.startsWith('/') || (config.working_directory && !config.working_directory.startsWith('/'))) throw new Error('Executable and working directory must be absolute paths beginning with /.');
      $('#save-service').disabled = true;
      await request(editing ? endpoint(editing) : '/api/services', { method: editing ? 'PUT' : 'POST', body: JSON.stringify(config) });
      dialog.close(); toast(editing ? 'Service updated.' : 'Service created.');
      selected = config.id; history.pushState({}, '', `/services/${encodeURIComponent(selected)}`);
      await refresh();
    } catch (error) { showError('#form-error', error.message); }
    finally { $('#save-service').disabled = false; }
  });
  $('#delete-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    if ($('#confirm-delete').disabled || !deleting) return;
    $('#confirm-delete').disabled = true;
    try { await request(endpoint(deleting), { method: 'DELETE' }); deleteDialog.close(); if (selected === deleting) select(''); toast('Service deleted.'); deleting = null; await refresh(); }
    catch (error) { showError('#delete-error', error.message); }
    finally { $('#confirm-delete').disabled = false; }
  });
  window.addEventListener('popstate', () => { const match = location.pathname.match(/^\/services\/([^/]+)$/); select(match ? decodeURIComponent(match[1]) : '', false); });
  window.addEventListener('pagehide', disconnect);
  window.addEventListener('pageshow', () => { if (selected && !source) renderDetail(); });
  void refresh();
  setInterval(() => { if (!document.hidden) void refresh(); }, 3000);
  document.addEventListener('visibilitychange', () => { if (!document.hidden) void refresh(); });
})();
