// Gate 2 engineering check: real server, real sim thread, real protocol; no browser.
// Usage: node scripts/gate2-check.mjs [--port N] [--url ws://host:port/ws] [--keep]
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { fileURLToPath } from 'node:url';
import { mkdir, writeFile } from 'node:fs/promises';

const root = fileURLToPath(new URL('../', import.meta.url));
const args = process.argv.slice(2);
const opt = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
let port = opt('--port', null);
let url = opt('--url', null);
let server = null;
const serverLog = [];
if (!url) {
  if (!port) {
    const probe = createServer(); await new Promise((ok, err) => { probe.once('error', err); probe.listen(0, '127.0.0.1', ok); });
    port = String(probe.address().port); await new Promise(ok => probe.close(ok));
  }
  server = spawn('cargo', ['run', '--release', '--quiet', '-p', 'atemporal-server', '--', '--port', port, '--seed', '42', '--replays', 'target/gate2-replays'], { cwd: root, stdio: ['ignore', 'pipe', 'inherit'] });
  await new Promise((ok, err) => {
    server.stdout.on('data', chunk => { const text = chunk.toString(); serverLog.push(text); if (text.includes('listening')) ok(); });
    server.on('exit', code => err(new Error(`server exited ${code}`)));
  });
  url = `ws://127.0.0.1:${port}/ws`;
}
const summary = { url, rounds: [], seeks: [], payloads: {} };
const now = () => performance.now();

class Client {
  constructor(name) { this.name = name; this.waiters = []; this.log = []; this.unconsumed = []; this.token = null; this.instance = null; }
  async open() {
    this.ws = new WebSocket(url);
    await new Promise((ok, err) => { this.ws.onopen = ok; this.ws.onerror = err; });
    this.ws.onmessage = event => {
      const bytes = typeof event.data === 'string' ? event.data.length : event.data.byteLength;
      const envelope = JSON.parse(event.data);
      this.instance = envelope.server_instance_id;
      const message = envelope.message;
      this.log.push({ kind: message.kind, bytes });
      const waiter = this.waiters.find(w => w.test(message));
      if (waiter) { this.waiters.splice(this.waiters.indexOf(waiter), 1); waiter.resolve({ message, bytes }); }
      else { this.unconsumed.push({ message, bytes }); if (this.unconsumed.length > 500) this.unconsumed.shift(); }
    };
  }
  send(message) { this.ws.send(JSON.stringify({ schema_version: 2, message })); }
  wait(kind, test = () => true, timeout = 120000) {
    // Broadcasts may arrive before a waiter exists; consume the earliest matching one first.
    const early = this.unconsumed.findIndex(({ message }) => message.kind === kind && test(message));
    if (early >= 0) return Promise.resolve(this.unconsumed.splice(early, 1)[0]);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`${this.name}: timeout waiting for ${kind}`)), timeout);
      this.waiters.push({ test: m => m.kind === kind && test(m), resolve: v => { clearTimeout(timer); resolve(v); } });
    });
  }
  async request(message, kind, test) { const p = this.wait(kind, test); this.send(message); return p; }
  close() { this.ws.close(); }
}

const a = new Client('A'), b = new Client('B'), spectator = new Client('S');
await Promise.all([a.open(), b.open(), spectator.open()]);
for (const c of [a, b, spectator]) c.send({ kind: 'hello', protocol_version: 2, last_revision: null, slot_token: null });
const welcome = (await a.wait('welcome')).message;
summary.payloads.welcome_bytes = a.log.at(-1).bytes;
const config = welcome.config;
a.token = (await a.request({ kind: 'claim_slot', slot: 0, username: 'Ada', color: '#4fc3f7', team_id: null }, 'slot_claimed')).message.private_token;
b.token = (await b.request({ kind: 'claim_slot', slot: 1, username: 'Bo', color: '#ff8a65', team_id: null }, 'slot_claimed')).message.private_token;
const lobby = (await a.wait('lobby_updated', m => m.lobby.can_start)).message.lobby;
if (!lobby.slots.every(s => s.claimed)) throw new Error('lobby not fully claimed');

// Start: revision 0 is the initial full simulation with no commands.
let t0 = now();
a.send({ kind: 'start_match', slot_token: a.token, based_on_lobby_revision: lobby.revision });
const [rev0, spectatorRev0] = await Promise.all([a.wait('revision_published', m => m.revision === 0), spectator.wait('revision_published', m => m.revision === 0)]);
summary.rounds.push({ round: 0, revision: 0, start_to_publish_ms: Math.round(now() - t0), outcome: rev0.message.outcome.kind, terminal: rev0.message.outcome.terminal_state_tick, revision_published_bytes: rev0.bytes, spectator_saw: !!spectatorRev0 });
let planning = (await a.wait('planning_opened', m => m.round === 1)).message;
a.send({ kind: 'planning_ready', round: 1, revision: 0 }); b.send({ kind: 'planning_ready', round: 1, revision: 0 });

const exact0 = (await a.request({ kind: 'get_exact_state', revision: 0, tick: 0 }, 'exact_state', m => m.tick === 0));
summary.payloads.exact_state_t0_bytes = exact0.bytes;
const state0 = exact0.message.snapshot;
const mine = (owner, key) => state0.entities.find(e => e.owner === owner && e.type_key === key);
const miner = mine(0, 'miner'), constructor = mine(0, 'constructor');
const w = state0.terrain.width;
const oreTiles = state0.ore.map((v, i) => [v, i]).filter(([v]) => v > 0).map(([, i]) => ({ x: i % w, y: Math.floor(i / w) }));
const ownOre = oreTiles.filter(t => Math.hypot(t.x - miner.tile.x, t.y - miner.tile.y) < 12);
const area = { min: { x: Math.min(...ownOre.map(t => t.x)), y: Math.min(...ownOre.map(t => t.y)) }, max: { x: Math.max(...ownOre.map(t => t.x)), y: Math.max(...ownOre.map(t => t.y)) } };
const factoryTile = { x: constructor.tile.x + 2, y: constructor.tile.y - 1 };

async function round(roundNumber, revision, tickA, commandsA, commandsB = []) {
  const commit = (client, tick, commands) => client.request({ kind: 'commit', request: { request_id: `${client.name}-r${roundNumber}`, slot_token: client.token, draft: { based_on_revision: revision, tick, commands: commands.map((command, i) => ({ local_id: `c${i}`, command, future_orders: command.future_orders ?? 'keep' })).map(c => { delete c.command.future_orders; return c; }) } } }, 'commit_accepted', m => m.request_id === `${client.name}-r${roundNumber}`);
  const accepted = await commit(a, tickA, commandsA);
  const started = now();
  const acceptedB = await commit(b, tickA, commandsB);
  const [published] = await Promise.all([a.wait('revision_published', m => m.revision === revision + 1), spectator.wait('revision_published', m => m.revision === revision + 1)]);
  const record = { round: roundNumber, revision: revision + 1, tick: tickA, last_commit_to_publish_ms: Math.round(now() - started), outcome: published.message.outcome.kind, terminal: published.message.outcome.terminal_state_tick, revision_published_bytes: published.bytes, timeline_buckets: published.message.timeline_index.length, score: published.message.score?.entries.map(e => e.raw_total) };
  summary.rounds.push(record);
  if (!accepted || !acceptedB) throw new Error('commit not accepted');
  planning = (await a.wait('planning_opened', m => m.round === roundNumber + 1)).message;
  return published.message;
}

// Round 1 at tick 0: mine, place a factory, construct it.
await round(1, 0, 0, [
  { kind: 'assign_order', entities: [miner.id], order: { kind: 'mine', area } },
  { kind: 'place_blueprints', type_key: 'factory', tiles: [factoryTile], priority: 'high', output_directions: ['e'] },
  { kind: 'assign_order', entities: [constructor.id], order: { kind: 'construct', area: { min: factoryTile, max: factoryTile } } },
]);

// Round 2 at a non-sample tick: exact state must show the completed factory before we queue.
const tick2 = 153;
let seek = now();
const exact153 = await a.request({ kind: 'get_exact_state', revision: 1, tick: tick2 }, 'exact_state', m => m.tick === tick2);
summary.seeks.push({ revision: 1, tick: tick2, cold: true, ms: Math.round(now() - seek), bytes: exact153.bytes });
const factory = exact153.message.snapshot.entities.find(e => e.owner === 0 && e.type_key === 'factory');
if (!factory || factory.lifecycle !== 'complete') throw new Error('factory not complete at tick 153');
const enemyMiner = state0.entities.find(e => e.owner === 1 && e.type_key === 'miner');
await round(2, 1, tick2, [
  { kind: 'set_stored_order', factories: [factory.id], order: { kind: 'attack_move', destination: enemyMiner.tile } },
  { kind: 'edit_production', factories: [factory.id], edit: { kind: 'append', items: ['grunt', 'grunt'] } },
  { kind: 'set_queue_loop', factories: [factory.id], enabled: true },
]);
const events2 = await a.request({ kind: 'get_events', revision: 2, from_tick: 0, to_tick: 20000 }, 'events', m => m.revision === 2);
summary.payloads.events_rev2_bytes = events2.bytes;
summary.payloads.events_rev2_count = events2.message.events.length;
const attacks = events2.message.events.filter(e => e.event.kind === 'attack');
summary.payloads.attack_events_rev2 = attacks.length;
if (attacks.length === 0) throw new Error('no attacks after producing grunts');

// Round 3 rewrites an earlier tick (40): idle the constructor and drop its future orders, so the
// factory never completes and the round-2 commands become no-ops in the rerun from checkpoint 0.
const rev3 = await round(3, 2, 40, [
  { kind: 'assign_order', entities: [constructor.id], order: { kind: 'idle' }, future_orders: 'drop_all' },
]);
const exact3 = await a.request({ kind: 'get_exact_state', revision: 3, tick: tick2 }, 'exact_state', m => m.tick === tick2 && m.revision === 3);
const factoryAfter = exact3.message.snapshot.entities.find(e => e.id && JSON.stringify(e.id) === JSON.stringify(factory.id));
summary.rewrite = { factory_complete_before: factory.lifecycle, factory_lifecycle_after: factoryAfter?.lifecycle ?? 'absent', outcome_rev2: summary.rounds[2].outcome, outcome_rev3: rev3.outcome.kind };
if (factoryAfter?.lifecycle === 'complete') throw new Error('rewrite did not change history');

// Seek latency: cold non-sample ticks and a warm repeat.
for (const tick of [7, 999, 1234, 2001]) {
  const t = now();
  const r = await a.request({ kind: 'get_exact_state', revision: 3, tick }, 'exact_state', m => m.tick === tick && m.revision === 3).catch(() => null);
  if (r) summary.seeks.push({ revision: 3, tick, cold: true, ms: Math.round(now() - t), bytes: r.bytes });
}
{
  const t = now();
  const r = await a.request({ kind: 'get_exact_state', revision: 3, tick: 999 }, 'exact_state', m => m.tick === 999 && m.revision === 3);
  summary.seeks.push({ revision: 3, tick: 999, cold: false, ms: Math.round(now() - t), bytes: r.bytes });
}
// Payload sizes for the whole timeline at sample stride and a 1000-tick window.
const terminal = rev3.outcome.terminal_state_tick;
const full = await a.request({ kind: 'get_snapshot_range', revision: 3, from_tick: 0, to_tick: terminal, stride: config.snapshot_interval }, 'snapshot_range', m => m.revision === 3);
summary.payloads.snapshot_range_full = { bytes: full.bytes, samples: full.message.samples.length, dictionary: full.message.entity_dictionary.length, terminal };
const window1k = await a.request({ kind: 'get_snapshot_range', revision: 3, from_tick: 0, to_tick: 1000, stride: config.snapshot_interval }, 'snapshot_range', m => m.revision === 3);
summary.payloads.snapshot_range_1000 = { bytes: window1k.bytes, samples: window1k.message.samples.length };
const stats = await a.request({ kind: 'get_stats', revision: 3, from_tick: 0, to_tick: terminal, bucket_width: config.snapshot_interval }, 'stats_range', m => m.revision === 3);
summary.payloads.stats_full_bytes = stats.bytes;
const commands = await a.request({ kind: 'get_commands', revision: 3, from_tick: 0, to_tick: terminal }, 'commands', m => m.revision === 3);
summary.payloads.commands_bytes = commands.bytes;
summary.payloads.commands_turns = commands.message.turns.length;

// Reconnect: a fresh socket with the slot token gets welcome, the current revision and planning.
const again = new Client('A2'); await again.open();
again.send({ kind: 'hello', protocol_version: 2, last_revision: 3, slot_token: a.token });
const [w2, p2] = await Promise.all([again.wait('revision_published', m => m.revision === 3), again.wait('planning_opened', m => m.round === 4)]);
summary.reconnect = { revision: w2.message.revision, round: p2.message.round, instance_matches: again.instance === a.instance };
again.close();

for (const c of [a, b, spectator]) c.close();
if (server && !args.includes('--keep')) server.kill('SIGTERM');
summary.server_measurements = serverLog.join('').split('\n').filter(l => l.startsWith('round ') || l.startsWith('exact state'));
await mkdir(`${root}target`, { recursive: true });
await writeFile(`${root}target/gate2-summary.json`, JSON.stringify(summary, null, 2));
console.log(JSON.stringify(summary, null, 2));
