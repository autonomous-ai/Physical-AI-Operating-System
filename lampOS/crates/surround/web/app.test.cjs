'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const script = fs.readFileSync(path.join(__dirname, 'app.js'), 'utf8');
const html = fs.readFileSync(path.join(__dirname, 'index.html'), 'utf8');
const settle = () => new Promise(resolve => setImmediate(resolve));

// Execute the shipped script with only the DOM operations it uses. HTTP and
// monotonic timers are controlled fixtures: no browser, network or lamp runs.
function page() {
  const element = () => ({
    value: '', hidden: false, disabled: false, checked: false, textContent: '',
    listeners: new Map(),
    addEventListener(name, callback) { this.listeners.set(name, callback); },
  });
  const elements = new Map([...html.matchAll(/id="([^"]+)"/g)].map(match => [match[1], element()]));
  elements.get('workspace').hidden = true;
  const changes = ['probe', 'apply-configuration', 'apply-release', 'retry-update', 'resume-reset'].map(id => elements.get(id));
  changes.push(element(), element(), element(), element());
  const calls = [];
  const timers = new Map();
  let nextTimer = 0;
  let now = 0;
  const context = vm.createContext({
    document: {
      getElementById(id) { assert.ok(elements.has(id), id); return elements.get(id); },
      querySelectorAll(selector) { assert.equal(selector, '.change'); return changes; },
    },
    fetch(url, options) {
      return new Promise((resolve, reject) => calls.push({url, options, resolve, reject}));
    },
    AbortSignal: {timeout(ms) { assert.equal(ms, 5000); return {}; }},
    performance: {now: () => now},
    setTimeout(callback, delay) {
      const id = ++nextTimer;
      timers.set(id, {at: now + delay, callback});
      return id;
    },
    clearTimeout(id) { timers.delete(id); },
  });
  vm.runInContext(script, context, {filename: 'app.js'});
  return {
    calls, changes, timers,
    el: id => elements.get(id),
    poll() { void vm.runInContext('refresh()', context); },
    event(id, name) {
      const callback = elements.get(id).listeners.get(name);
      assert.ok(callback, `${id}:${name}`);
      void callback({preventDefault() {}});
    },
    async reply(index, data, status = 200) {
      assert.ok(calls[index], `request ${index}`);
      calls[index].resolve({ok: status >= 200 && status < 300, status, json: async () => ({data: structuredClone(data), message: status === 401 ? 'login required' : null})});
      await settle();
    },
    async fail(index) { calls[index].reject(new Error('scripted disconnect')); await settle(); },
    async advance(ms) {
      const target = now + ms;
      for (;;) {
        const due = [...timers].filter(([, timer]) => timer.at <= target).sort((a, b) => a[1].at - b[1].at || a[0] - b[0])[0];
        if (!due) break;
        now = due[1].at; timers.delete(due[0]); due[1].callback();
        await settle();
      }
      now = target;
      await settle();
    },
  };
}

function ready(capability = 'listening') {
  return {
    enabled: true,
    runtime: 'ready',
    runtime_observed_us: 1,
    voice_capability: {state: capability},
    operation: null,
    setup: {phase: 'ready', network: {address: '192.0.2.10'}, network_fresh: true, runtime_fresh: true},
    candidate_configuration: {id: 'fixture-generation'},
    staged_release: 'fixture-release',
    update_outcome: {status: 'idle'},
  };
}
function withdrawn(view) {
  assert.equal(view.el('runtime-state').textContent, 'Unknown');
  assert.match(view.el('network-state').textContent, /192\.0\.2\.10 \(last known\)/);
  assert.ok(view.changes.every(button => button.disabled));
}

test('a failed status poll withdraws live readiness and changes; a fresh reply restores them', async () => {
  const view = page();
  await view.reply(0, ready());
  assert.equal(view.el('runtime-state').textContent, 'Ready');
  assert.ok(view.changes.every(button => !button.disabled));
  view.poll();
  await view.fail(1);
  withdrawn(view);
  assert.equal(view.el('workspace').hidden, false);
  assert.match(view.el('message').textContent, /unreachable/);
  view.event('probe', 'click');
  assert.equal(view.calls.length, 2, 'stale forms cannot submit an operation');
  await view.advance(2000);
  assert.equal(view.calls[2].url, '/api/status');
  await view.reply(2, ready('inhibited'));
  assert.equal(view.el('runtime-state').textContent, 'Privacy inhibited · not listening');
  assert.equal(view.el('message').textContent, 'Connected to the lamp.');
  assert.doesNotMatch(view.el('network-state').textContent, /last known/);
  assert.ok(view.changes.every(button => !button.disabled));
});

test('live readiness expires while polling is pending and a slow reply cannot renew an old lease', async () => {
  const view = page();
  await view.reply(0, ready());
  await view.advance(500);
  view.poll();
  await view.advance(1499);
  assert.equal(view.el('runtime-state').textContent, 'Ready');
  await view.advance(1);
  withdrawn(view);
  await view.advance(500);
  await view.reply(1, ready());
  withdrawn(view);
  await view.advance(2000);
  await view.reply(2, ready());
  assert.equal(view.el('runtime-state').textContent, 'Ready');
});

test('401 hides private status, disables changes, requires sign-in and stops polling', async () => {
  const view = page();
  await view.reply(0, ready());
  view.poll();
  await view.reply(1, null, 401);
  assert.equal(view.el('login').hidden, false);
  assert.equal(view.el('workspace').hidden, true);
  assert.equal(view.el('message').textContent, 'Sign in to continue.');
  assert.ok(view.changes.every(button => button.disabled));
  await view.advance(10_000);
  assert.equal(view.calls.length, 2);
});

test('a status response arriving after logout cannot reveal or repaint the signed-out page', async () => {
  const view = page();
  await view.reply(0, ready());
  view.poll();
  view.event('logout', 'click');
  assert.equal(view.el('workspace').hidden, true, 'logout hides status before the network answers');
  await view.reply(1, ready('inhibited'));
  await view.reply(2, null);
  assert.equal(view.el('login').hidden, false);
  assert.equal(view.el('workspace').hidden, true);
  assert.equal(view.el('message').textContent, 'Signed out.');
  assert.ok(view.changes.every(button => button.disabled));
  await view.advance(10_000);
  assert.equal(view.calls.length, 3);
});

test('an admitted operation may finish after logout without repainting or restarting status polling', async () => {
  const view = page();
  await view.reply(0, ready());
  view.event('probe', 'click');
  assert.equal(view.calls[1].url, '/api/operation');
  view.event('logout', 'click');
  await view.reply(1, {id: 'late-accepted-operation'}, 202);
  await view.reply(2, null);
  assert.equal(view.el('workspace').hidden, true);
  assert.equal(view.el('message').textContent, 'Signed out.');
  await view.advance(10_000);
  assert.equal(view.calls.length, 3);
});

test('an old unauthorized reply and old logout completion cannot erase a newer signed-in page', async () => {
  const view = page();
  await view.reply(0, ready());
  view.poll();
  view.event('logout', 'click');
  view.el('login-secret').value = 'fixture-code';
  view.event('login-form', 'submit');
  assert.equal(view.el('login-secret').value, '');
  await view.reply(3, {expires_in_seconds: 900});
  assert.equal(view.calls[4].url, '/api/status');
  await view.reply(4, ready('inhibited'));
  await view.reply(1, null, 401);
  await view.reply(2, null);
  assert.equal(view.el('login').hidden, true);
  assert.equal(view.el('workspace').hidden, false);
  assert.equal(view.el('runtime-state').textContent, 'Privacy inhibited · not listening');
  assert.ok(view.changes.every(button => !button.disabled));
});

test('an older overlapping status response cannot replace a newer capability', async () => {
  const view = page();
  await view.reply(0, ready());
  view.poll(); view.poll();
  await view.reply(2, ready('inhibited'));
  await view.reply(1, ready());
  assert.equal(view.el('runtime-state').textContent, 'Privacy inhibited · not listening');
  await view.advance(2000);
  withdrawn(view);
  assert.equal(view.calls.length, 4, 'only the newest poll schedules one successor');
});

test('a login response arriving after logout cannot start an authenticated refresh', async () => {
  const view = page();
  await view.reply(0, null, 401);
  view.el('login-secret').value = 'fixture-code';
  view.event('login-form', 'submit');
  view.event('logout', 'click');
  await view.reply(1, {expires_in_seconds: 900});
  await view.reply(2, null);
  assert.equal(view.el('workspace').hidden, true);
  assert.equal(view.el('message').textContent, 'Signed out.');
  assert.equal(view.calls.length, 3);
});

test('all serialized Rust update outcomes display customer text, never object coercion or raw details', async () => {
  // src/update.rs uses #[serde(tag = "status", rename_all = "snake_case")]
  // and ReleaseId is a transparent SHA-256 string.
  const release = 'a'.repeat(64);
  const cases = [
    [{status: 'idle'}, null, 'No update in progress.'],
    [{status: 'idle'}, release, 'A verified release is downloaded. It has not been installed.'],
    [{status: 'updating'}, release, 'Installing the software update…'],
    [{status: 'rolling_back'}, release, 'Restoring the previous software…'],
    [{status: 'succeeded', release}, release, 'Software update installed.'],
    [{status: 'rolled_back', release}, release, 'Previous software restored.'],
    [{status: 'first_install_failed', reason: 'fixture internal reason'}, release, 'The first software installation failed.'],
    [{status: 'recovery_failed', reason: 'fixture internal reason'}, release, 'Software recovery failed. Check status before trying again.'],
    [{status: 'future_variant'}, release, 'Update status unavailable.'],
    [null, release, 'Update status unavailable.'],
  ];
  for (const [update_outcome, staged_release, expected] of cases) {
    const view = page();
    await view.reply(0, {...ready(), update_outcome, staged_release});
    assert.equal(view.el('update-state').textContent, expected);
    assert.doesNotMatch(view.el('update-state').textContent, /\[object Object\]|fixture internal reason|a{64}/);
  }
});
