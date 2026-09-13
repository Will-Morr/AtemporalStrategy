// Shared harness for protocol-level checks: spawn the release server, talk to it over the real
// WebSocket protocol, and set up a two-player lobby. Imported by match-check and gate5-check.
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { fileURLToPath } from 'node:url';
import { mkdir, writeFile, readFile } from 'node:fs/promises';

export const root = fileURLToPath(new URL('../', import.meta.url));
export const now = () => performance.now();
export const sleep = ms => new Promise(ok => setTimeout(ok, ms));
export const assert = (cond, message) => { if (!cond) throw new Error(message); };

export async function freePort() {
  const probe = createServer(); await new Promise((ok, err) => { probe.once('error', err); probe.listen(0, '127.0.0.1', ok); });
  const port = probe.address().port; await new Promise(ok => probe.close(ok)); return String(port);
}

export async function buildServer() {
  await new Promise((ok, err) => { const b = spawn('cargo', ['build', '--release', '--quiet', '-p', 'atemporal-server'], { cwd: root, stdio: 'inherit' }); b.on('exit', c => (c === 0 ? ok() : err(new Error(`build exited ${c}`)))); });
}

/** Spawn the release server; `extra` may include --resume/--config. Resolves when listening. */
export async function startServer(work, name, extra, env = {}) {
  const port = await freePort();
  const log = [];
  const child = spawn(`${root}target/release/atemporal-server`, ['--port', port, '--replays', `${work}/${name}/replays`, ...extra], { cwd: root, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, ...env } });
  child.stdout.on('data', c => log.push(c.toString()));
  child.stderr.on('data', c => log.push(c.toString()));
  await new Promise((ok, err) => {
    const check = () => { if (log.join('').includes('listening')) ok(); };
    child.stdout.on('data', check);
    child.on('exit', code => err(new Error(`server exited ${code}: ${log.join('')}`)));
  });
  return { child, port, log, url: `ws://127.0.0.1:${port}/ws`, exited: new Promise(ok => child.on('close', ok)) };
}

export async function writeConfig(work, name, edit) {
  const config = JSON.parse(await readFile(`${root}config/game.yaml`, 'utf8'));
  edit(config);
  await mkdir(`${work}/${name}`, { recursive: true });
  const path = `${work}/${name}/game.yaml`;
  await writeFile(path, JSON.stringify(config, null, 2));
  return path;
}

export class Client {
  constructor(name, url) { this.name = name; this.url = url; this.waiters = []; this.unconsumed = []; this.token = null; }
  async open() {
    this.ws = new WebSocket(this.url);
    await new Promise((ok, err) => { this.ws.onopen = ok; this.ws.onerror = err; });
    this.ws.onmessage = event => {
      const envelope = JSON.parse(event.data);
      this.instance = envelope.server_instance_id;
      const message = envelope.message;
      const waiter = this.waiters.find(w => w.test(message));
      if (waiter) { this.waiters.splice(this.waiters.indexOf(waiter), 1); waiter.resolve(message); }
      else { this.unconsumed.push(message); if (this.unconsumed.length > 500) this.unconsumed.shift(); }
    };
  }
  send(message) { this.ws.send(JSON.stringify({ schema_version: 2, message })); }
  wait(kind, test = () => true, timeout = 120000) {
    const early = this.unconsumed.findIndex(m => m.kind === kind && test(m));
    if (early >= 0) return Promise.resolve(this.unconsumed.splice(early, 1)[0]);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`${this.name}: timeout waiting for ${kind}`)), timeout);
      this.waiters.push({ test: m => m.kind === kind && test(m), resolve: v => { clearTimeout(timer); resolve(v); } });
    });
  }
  waitAny(kinds, test = () => true, timeout = 120000) {
    const early = this.unconsumed.findIndex(m => kinds.includes(m.kind) && test(m));
    if (early >= 0) return Promise.resolve(this.unconsumed.splice(early, 1)[0]);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`${this.name}: timeout waiting for ${kinds}`)), timeout);
      this.waiters.push({ test: m => kinds.includes(m.kind) && test(m), resolve: v => { clearTimeout(timer); resolve(v); } });
    });
  }
  async request(message, kind, test) { const p = this.wait(kind, test); this.send(message); return p; }
  hello(token = null) { this.send({ kind: 'hello', protocol_version: 2, last_revision: null, slot_token: token }); return this.wait('welcome'); }
  async claim(slot, team = null) {
    const m = await this.request({ kind: 'claim_slot', slot, username: this.name, color: '#4fc3f7', team_id: team }, 'slot_claimed');
    this.token = m.private_token; return m;
  }
  sendAndWaitCommit(requestId, draft) {
    const p = this.waitAny(['commit_accepted', 'commit_rejected'], m => m.request_id === requestId);
    this.send({ kind: 'commit', request: { request_id: requestId, slot_token: this.token, draft } });
    return p;
  }
  close() { this.ws.close(); }
}

/** Two players and a spectator, claimed and started; returns revision 0 publication. */
export async function lobby(url, teams = [null, null]) {
  const a = new Client('A', url), b = new Client('B', url), s = new Client('S', url);
  await Promise.all([a.open(), b.open(), s.open()]);
  const welcome = await a.hello(); await b.hello(); await s.hello();
  await a.claim(0, teams[0]); await b.claim(1, teams[1]);
  const state = (await a.wait('lobby_updated', m => m.lobby.can_start)).lobby;
  a.send({ kind: 'start_match', slot_token: a.token, based_on_lobby_revision: state.revision });
  const rev0 = await a.wait('revision_published', m => m.revision === 0);
  await a.wait('planning_opened', m => m.round === 1);
  return { a, b, s, welcome, rev0, config: welcome.config };
}

