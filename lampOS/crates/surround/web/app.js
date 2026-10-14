'use strict';
const el = id => document.getElementById(id);
let status = null;
let submitting = false;
let timer = null;
let expiryTimer = null;
let live = false;
let sessionEpoch = 0;
let refreshSequence = 0;
let appliedRefresh = 0;
let connectionWarning = false;
const LIVE_STATUS_MS = 2000;
const say = text => { el('message').textContent = text; };
const words = value => String(value || 'unknown').replaceAll('_', ' ');
function updateText(outcome, stagedRelease) {
  // update::Outcome is a tagged object, including its fieldless Idle variant.
  // Keep failure details and release identifiers out of the customer summary.
  switch (outcome?.status) {
    case 'idle':
      return stagedRelease ? 'A verified release is downloaded. It has not been installed.' : 'No update in progress.';
    case 'updating': return 'Installing the software update…';
    case 'rolling_back': return 'Restoring the previous software…';
    case 'succeeded': return 'Software update installed.';
    case 'rolled_back': return 'Previous software restored.';
    case 'first_install_failed': return 'The first software installation failed.';
    case 'recovery_failed': return 'Software recovery failed. Check status before trying again.';
    default: return 'Update status unavailable.';
  }
}
async function request(path, body) {
  const response = await fetch(path, {method: body === undefined ? 'GET' : 'POST', credentials: 'same-origin', cache: 'no-store', headers: body === undefined ? {} : {'Content-Type': 'application/json'}, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(5000)});
  const result = await response.json();
  if (!response.ok) {
    const error = new Error(response.status === 401 ? 'Sign in to continue.' : result.message || 'The request could not be completed.');
    error.status = response.status;
    throw error;
  }
  return result.data;
}
function clearSession(message) {
  sessionEpoch += 1; refreshSequence += 1;
  clearTimeout(timer); clearTimeout(expiryTimer);
  timer = null; expiryTimer = null; status = null; live = false; submitting = false; connectionWarning = false;
  el('login').hidden = false; el('workspace').hidden = true;
  document.querySelectorAll('.change').forEach(button => { button.disabled = true; });
  if (message) say(message);
}
function withdrawReadiness() {
  live = false; clearTimeout(expiryTimer); expiryTimer = null;
  if (status) {
    status.runtime = 'unknown'; status.voice_capability = null; status.runtime_observed_us = null;
    if (status.setup) { status.setup.network_fresh = false; status.setup.runtime_fresh = false; }
  }
  paint();
}
function paint() {
  if (!status) return;
  el('login').hidden = true; el('workspace').hidden = false;
  el('disabled').hidden = status.enabled;
  const busy = submitting || status.operation?.phase === 'working';
  document.querySelectorAll('.change').forEach(button => { button.disabled = !live || !status.enabled || busy; });
  el('apply-configuration').disabled ||= !status.candidate_configuration;
  el('apply-release').disabled ||= !status.staged_release;
  const network = status.setup?.network;
  el('network-state').textContent = network ? `${words(status.setup.phase)}${network.address ? ` · ${network.address}` : ''}${!status.setup.network_fresh ? ' (last known)' : ''}` : 'Not checked';
  el('runtime-state').textContent = status.voice_capability?.state === 'inhibited' ? 'Privacy inhibited · not listening' : words(status.runtime).replace(/^./, letter => letter.toUpperCase());
  el('operation-state').textContent = status.operation ? `${words(status.operation.kind)}: ${words(status.operation.phase)}${status.operation.error ? ` · ${words(status.operation.error)}` : ''}` : 'None';
  el('update-state').textContent = updateText(status.update_outcome, status.staged_release);
  el('reset-state').textContent = status.reset_phase ? words(status.reset_phase) : '';
}
async function refresh() {
  const epoch = sessionEpoch;
  const sequence = ++refreshSequence;
  const started = performance.now();
  const current = () => epoch === sessionEpoch && sequence === refreshSequence;
  clearTimeout(timer); timer = null;
  try {
    const next = await request('/api/status');
    if (!current()) return;
    status = next; appliedRefresh = sequence; live = true;
    clearTimeout(expiryTimer);
    // Count network delay against the browser lease. An in-flight or lost
    // poll must not keep the last successful capability looking current.
    const elapsed = performance.now() - started;
    const remaining = elapsed >= 0 ? LIVE_STATUS_MS - elapsed : 0;
    if (remaining <= 0) withdrawReadiness();
    else {
      if (connectionWarning) say('Connected to the lamp.');
      connectionWarning = false;
      expiryTimer = setTimeout(() => {
        if (epoch === sessionEpoch && appliedRefresh === sequence) withdrawReadiness();
      }, remaining);
      paint();
    }
  } catch (error) {
    if (!current()) return;
    if (error.status === 401) clearSession(error.message);
    else {
      withdrawReadiness();
      connectionWarning = true;
      say('The lamp’s page is unreachable. Reconnect to its network or last shown address, then reload to check progress.');
    }
  } finally {
    if (current() && status) timer = setTimeout(refresh, LIVE_STATUS_MS);
  }
}
async function submit(command) {
  if (submitting || !live || !status?.enabled) return;
  const epoch = sessionEpoch;
  submitting = true; paint(); say('Saving your request…');
  try {
    const accepted = await request('/api/operation', command);
    if (epoch === sessionEpoch) say(`Request saved. Follow its progress above. Reference: ${accepted.id}`);
  } catch (error) {
    if (epoch === sessionEpoch) {
      if (error.status === 401) clearSession(error.message);
      else say(error.message);
    }
  } finally {
    if (epoch === sessionEpoch) { submitting = false; await refresh(); }
  }
}
el('login-form').addEventListener('submit', async event => {
  event.preventDefault(); const secret = el('login-secret').value; el('login-secret').value = '';
  clearSession('Connecting to the lamp…');
  const epoch = sessionEpoch;
  try {
    await request('/api/login', {secret});
    if (epoch === sessionEpoch) { say('Connected to the lamp.'); await refresh(); }
  } catch (error) { if (epoch === sessionEpoch) say(error.message); }
});
el('logout').addEventListener('click', async () => {
  clearSession('Signed out.');
  try { await request('/api/logout', {}); } catch { /* Session may already have expired. */ }
});
el('wifi-form').addEventListener('submit', event => { event.preventDefault(); const password = el('wifi-password').value; el('wifi-password').value = ''; if (!el('wifi-open').checked && !password) { say('Enter the Wi-Fi password or select an open network.'); return; } submit({operation:'join_wifi', ssid:el('ssid').value, password:el('wifi-open').checked ? null : password}); });
el('probe').addEventListener('click', () => submit({operation:'probe_network'}));
el('provider-form').addEventListener('submit', event => {
  event.preventDefault(); const credential = el('provider-key').value; el('provider-key').value = '';
  submit({operation:'save_provider', document:{schema_version:1, provider:{endpoint:el('endpoint').value, model:el('model').value, voice:el('voice').value, credential_file:'/replaced-by-private-generation/provider.key'}}, credential, instructions:el('instructions').value.trim() ? el('instructions').value : null});
});
el('apply-configuration').addEventListener('click', () => { if (status?.candidate_configuration) submit({operation:'apply_configuration', target:status.candidate_configuration}); });
el('release-form').addEventListener('submit', event => { event.preventDefault(); submit({operation:'stage_release', manifest_target:el('manifest-target').value}); });
el('apply-release').addEventListener('click', () => { if (status?.staged_release) submit({operation:'apply_release', set_sha256:status.staged_release, rollback:false}); });
el('retry-update').addEventListener('click', () => submit({operation:'retry_update'}));
el('reset-form').addEventListener('submit', event => { event.preventDefault(); const confirmation = el('reset-confirmation').value; el('reset-confirmation').value = ''; if (confirmation !== 'erase-personal-data') { say('Type erase-personal-data to confirm.'); return; } submit({operation:'reset', confirmation}); });
el('resume-reset').addEventListener('click', () => submit({operation:'resume_reset'}));
refresh();
