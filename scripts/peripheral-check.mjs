// Native peripheral check: an inputs-only server never serves world state directly; two players
// play three rounds through a runner that reproduces every revision locally and matches the
// controller's hashes. Then: runner restart (catch-up from the ledger), controller restart with
// --resume (link reconnect, same match kept), a corrupted-hash runner (mismatch closes order
// entry) and a tiny memory budget (eviction and hash-checked regeneration).
// Usage: node scripts/peripheral-check.mjs  → target/peripheral-summary.json
import { spawn } from 'node:child_process';
import { mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { root, now, sleep, assert, freePort, buildServer, Client } from './match-harness.mjs';

const work = `${root}target/peripheral-check`;
await rm(work, { recursive: true, force: true });
await mkdir(work, { recursive: true });
await buildServer();
await new Promise((ok, err) => { const b = spawn('cargo', ['build', '--release', '--quiet', '-p', 'atemporal-runner'], { cwd: root, stdio: 'inherit' }); b.on('exit', c => (c === 0 ? ok() : err(new Error(`runner build exited ${c}`)))); });

const children = new Set();
process.on('exit', () => { for (const child of children) child.kill('SIGKILL'); });
function launch(binary, args, env = {}) {
  const log = [];
  const child = spawn(`${root}target/release/${binary}`, args, { cwd: root, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, ...env } });
  children.add(child); child.on('close', () => children.delete(child));
  child.stdout.on('data', c => log.push(c.toString()));
  child.stderr.on('data', c => log.push(c.toString()));
  const ready = new Promise((ok, err) => {
    const check = () => { if (log.join('').includes('listening')) ok(); };
    child.stdout.on('data', check);
    child.on('exit', code => err(new Error(`${binary} exited ${code}: ${log.join('')}`)));
  });
  const stop = () => new Promise(ok => { child.once('close', ok); child.kill('SIGTERM'); });
  return { child, log, ready, stop };
}
const serverPort = await freePort();
const startServer = (extra) => launch('atemporal-server', ['--port', serverPort, '--seed', '42', '--replays', `${work}/replays`, '--inputs-only', ...extra]);
const startRunner = (port, extra = [], env = {}) => launch('atemporal-runner', ['--controller', `ws://127.0.0.1:${serverPort}`, '--port', port, '--guide-dir', `${work}/guide-${port}`, ...extra], env);
const status = async port => (await fetch(`http://127.0.0.1:${port}/peripheral/status`)).json();
const waitStatus = async (port, test, timeout = 60000) => {
  const started = now();
  for (;;) {
    const s = await status(port).catch(() => null);
    if (s && test(s)) return s;
    assert(now() - started < timeout, `timeout waiting for peripheral status on ${port}: ${JSON.stringify(s)}`);
    await sleep(200);
  }
};
const verified = (s, revision) => s.store?.revisions.find(r => r.revision === revision)?.status === 'verified';

const summary = { server_port: serverPort, rounds: [], checks: {} };
let server = startServer([]);
await server.ready;
const runnerPort = await freePort();
let runner = startRunner(runnerPort);
await runner.ready;
const local = `ws://127.0.0.1:${runnerPort}/ws`;

// Lobby and start go through the peripheral with the players' own slot tokens.
const a = new Client('A', local), b = new Client('B', local), s = new Client('S', local);
await Promise.all([a.open(), b.open(), s.open()]);
const welcome = await a.hello(); await b.hello(); await s.hello();
const matchId = welcome.match_id;
await a.claim(0); await b.claim(1);
const lobby = (await a.wait('lobby_updated', m => m.lobby.can_start)).lobby;
let t0 = now();
a.send({ kind: 'start_match', slot_token: a.token, based_on_lobby_revision: lobby.revision });
const rev0 = await a.wait('revision_published', m => m.revision === 0);
await a.wait('planning_opened', m => m.round === 1);
summary.rounds.push({ round: 0, revision: 0, start_to_local_publish_ms: Math.round(now() - t0), terminal: rev0.outcome.terminal_state_tick });
const s0 = await waitStatus(runnerPort, st => verified(st, 0));
assert(s0.store.fingerprint.content_hash === welcome.fingerprint.content_hash, 'fingerprint differs');
summary.checks.guide_content_json = (await fetch(`http://127.0.0.1:${runnerPort}/guide/content.json`)).status;
assert(summary.checks.guide_content_json === 200, 'peripheral guide missing');

// The inputs-only server refuses world state on its ordinary route.
const direct = new Client('D', `ws://127.0.0.1:${serverPort}/ws`); await direct.open(); await direct.hello();
const refused = await direct.request({ kind: 'get_exact_state', revision: 0, tick: 0 }, 'commit_rejected');
summary.checks.server_refuses_world_state = refused.message;
assert(refused.code === 'query_failed' && /inputs only/.test(refused.message), 'server served world state');
const commandsOk = await direct.request({ kind: 'get_commands', revision: 0, from_tick: 0, to_tick: 100 }, 'commands');
assert(commandsOk.revision === 0, 'server must still serve inputs');
direct.close();

// The peripheral serves exact state, snapshots and events from its own simulation.
const exact0 = await a.request({ kind: 'get_exact_state', revision: 0, tick: 0 }, 'exact_state', m => m.tick === 0);
const state0 = exact0.snapshot;
const mine = (owner, key) => state0.entities.find(e => e.owner === owner && e.type_key === key);
const miner = mine(0, 'miner'), constructor = mine(0, 'constructor');
const w = state0.terrain.width;
const oreTiles = state0.ore.map((v, i) => [v, i]).filter(([v]) => v > 0).map(([, i]) => ({ x: i % w, y: Math.floor(i / w) }));
const ownOre = oreTiles.filter(t => Math.hypot(t.x - miner.tile.x, t.y - miner.tile.y) < 12);
const area = { min: { x: Math.min(...ownOre.map(t => t.x)), y: Math.min(...ownOre.map(t => t.y)) }, max: { x: Math.max(...ownOre.map(t => t.x)), y: Math.max(...ownOre.map(t => t.y)) } };
const factoryTile = { x: constructor.tile.x + 2, y: constructor.tile.y - 1 };

async function round(roundNumber, revision, tick, commandsA, commandsB = []) {
  const draft = (commands) => ({ based_on_revision: revision, tick, commands: commands.map((command, i) => { const { future_orders, ...rest } = command; return { local_id: `c${i}`, command: rest, future_orders: future_orders ?? 'keep' }; }) });
  const ra = await a.sendAndWaitCommit(`A-r${roundNumber}`, draft(commandsA));
  assert(ra.kind === 'commit_accepted', `A commit rejected: ${ra.message}`);
  const started = now();
  const rb = await b.sendAndWaitCommit(`B-r${roundNumber}`, draft(commandsB));
  assert(rb.kind === 'commit_accepted', `B commit rejected: ${rb.message}`);
  const [published] = await Promise.all([a.wait('revision_published', m => m.revision === revision + 1), s.wait('revision_published', m => m.revision === revision + 1)]);
  await a.wait('planning_opened', m => m.round === roundNumber + 1);
  const st = await waitStatus(runnerPort, x => verified(x, revision + 1));
  const rec = st.store.revisions.find(r => r.revision === revision + 1);
  summary.rounds.push({ round: roundNumber, revision: revision + 1, tick, last_commit_to_local_publish_ms: Math.round(now() - started), terminal: published.outcome.terminal_state_tick, base_tick: rec.base_tick, local_sim_ms: rec.sim_ms, local_hash: rec.local_hash.slice(0, 12) });
  return published;
}
await round(1, 0, 0, [
  { kind: 'assign_order', entities: [miner.id], order: { kind: 'mine', area } },
  { kind: 'place_blueprints', type_key: 'factory', tiles: [factoryTile], priority: 'high', output_directions: ['e'] },
  { kind: 'assign_order', entities: [constructor.id], order: { kind: 'construct', area: { min: factoryTile, max: factoryTile } } },
]);
const tick2 = 153;
const exact153 = await a.request({ kind: 'get_exact_state', revision: 1, tick: tick2 }, 'exact_state', m => m.tick === tick2);
const factory = exact153.snapshot.entities.find(e => e.owner === 0 && e.type_key === 'factory');
assert(factory?.lifecycle === 'complete', 'factory not complete at tick 153 in the local reproduction');
const enemyMiner = state0.entities.find(e => e.owner === 1 && e.type_key === 'miner');
await round(2, 1, tick2, [
  { kind: 'set_stored_order', factories: [factory.id], order: { kind: 'attack_move', destination: enemyMiner.tile } },
  { kind: 'edit_production', factories: [factory.id], edit: { kind: 'append', items: ['grunt', 'grunt'] } },
  { kind: 'set_queue_loop', factories: [factory.id], enabled: true },
]);
const events2 = await a.request({ kind: 'get_events', revision: 2, from_tick: 0, to_tick: 20000 }, 'events', m => m.revision === 2);
summary.checks.attack_events_rev2 = events2.events.filter(e => e.event.kind === 'attack').length;
assert(summary.checks.attack_events_rev2 > 0, 'no attacks in the local reproduction');
const effects2 = await a.request({ kind: 'get_events', revision: 2, from_tick: 0, to_tick: 20000, effects_only: true }, 'events', m => m.revision === 2);
assert(JSON.stringify(effects2.events) === JSON.stringify(events2.events.filter(e => ['attack', 'impact', 'destroyed'].includes(e.event.kind))), 'peripheral effects filter changed replay effects');
summary.checks.filtered_effects = effects2.events.length;

// Retroactive rewrite at tick 40: the local job restarts from checkpoint 0 like the controller's.
await round(3, 2, 40, [{ kind: 'assign_order', entities: [constructor.id], order: { kind: 'idle' }, future_orders: 'drop_all' }]);
const exact3 = await a.request({ kind: 'get_exact_state', revision: 3, tick: tick2 }, 'exact_state', m => m.tick === tick2 && m.revision === 3);
summary.checks.factory_after_rewrite = exact3.snapshot.entities.find(e => JSON.stringify(e.id) === JSON.stringify(factory.id))?.lifecycle ?? 'absent';
assert(summary.checks.factory_after_rewrite !== 'complete', 'rewrite did not change local history');
const roundResult = await a.request({ kind: 'get_round', revision: 3 }, 'round_result', m => m.revision === 3);
summary.checks.round_result_outcomes = roundResult.command_outcomes.length;

// Local hashes equal the controller's archived round records.
const archived = {};
for (const r of [0, 1, 2, 3]) archived[r] = JSON.parse(await readFile(`${work}/replays/${matchId}/rounds/${r}.json`, 'utf8')).final_hash;
const st3 = await status(runnerPort);
summary.checks.hashes = st3.store.revisions.map(r => ({ revision: r.revision, status: r.status, matches_archive: r.local_hash === archived[r.revision] }));
assert(summary.checks.hashes.every(h => h.status === 'verified' && h.matches_archive), 'local hashes differ from the archive');

// Runner restart: bootstrap again and catch up all revisions from inputs alone.
await runner.stop();
t0 = now();
runner = startRunner(runnerPort);
await runner.ready;
const caught = await waitStatus(runnerPort, x => [0, 1, 2, 3].every(r => verified(x, r)));
summary.checks.runner_restart = { catch_up_ms: Math.round(now() - t0), revisions: caught.store.revisions.map(r => r.status) };
const a2 = new Client('A2', local); await a2.open();
a2.send({ kind: 'hello', protocol_version: 2, last_revision: 3, slot_token: a.token });
await a2.wait('welcome'); await a2.wait('revision_published', m => m.revision === 3); await a2.wait('planning_opened', m => m.round === 4);
const again = await a2.request({ kind: 'get_exact_state', revision: 3, tick: tick2 }, 'exact_state', m => m.tick === tick2 && m.revision === 3);
assert(JSON.stringify(again.snapshot) === JSON.stringify(exact3.snapshot), 'exact state differs after runner restart');
summary.checks.runner_restart.exact_state_identical = true;

// Controller restart with --resume: the sync link reconnects, keeps the same match and store.
const wasConnected = (await status(runnerPort)).controller_connected;
await server.stop();
await sleep(1500);
assert((await status(runnerPort)).controller_connected === false, 'runner did not notice the controller drop');
server = startServer(['--resume', matchId]);
await server.ready;
const reconnected = await waitStatus(runnerPort, x => x.controller_connected && x.store?.match_id === matchId, 30000);
summary.checks.controller_restart = { was_connected: wasConnected, reconnected: true, regenerations: reconnected.store.regenerations, revisions: reconnected.store.revisions.map(r => r.status) };
const a3 = new Client('A3', local); await a3.open();
a3.send({ kind: 'hello', protocol_version: 2, last_revision: 3, slot_token: a.token });
const w3 = await a3.wait('welcome');
await a3.wait('revision_published', m => m.revision === 3); await a3.wait('planning_opened', m => m.round === 4);
summary.checks.controller_restart.new_instance = w3.match_id === matchId && a3.instance !== a2.instance;
assert(summary.checks.controller_restart.new_instance, 'resumed controller must present the same match under a new instance');
a2.close(); a3.close();

// Mismatch: a runner whose revision 3 hash is corrupted shows the diagnostic, keeps planning
// closed and refuses world state for that revision; earlier revisions stay usable.
const badPort = await freePort();
const bad = startRunner(badPort, [], { ATEMPORAL_PERIPHERAL_CORRUPT: '3' });
await bad.ready;
const badStatus = await waitStatus(badPort, x => x.store?.revisions.find(r => r.revision === 3)?.status.startsWith('mismatch'));
const m = new Client('M', `ws://127.0.0.1:${badPort}/ws`); await m.open();
m.send({ kind: 'hello', protocol_version: 2, last_revision: null, slot_token: b.token });
await m.wait('revision_published', m => m.revision === 3);
const planningLeaked = await m.wait('planning_opened', () => true, 3000).then(() => true, () => false);
const refusedLocal = await m.request({ kind: 'get_exact_state', revision: 3, tick: 0 }, 'commit_rejected');
const olderOk = await m.request({ kind: 'get_exact_state', revision: 2, tick: 0 }, 'exact_state', x => x.revision === 2);
summary.checks.mismatch = { status: badStatus.store.revisions.find(r => r.revision === 3).status, planning_opened_leaked: planningLeaked, query_message: refusedLocal.message, older_revision_served: olderOk.tick === 0, log_line: bad.log.join('').split('\n').find(l => l.includes('mismatch')) };
assert(!planningLeaked && /mismatch/.test(refusedLocal.message), 'mismatch did not stop order entry');
m.close(); await bad.stop();

// Retention: a 1 MiB budget evicts older bodies; querying one regenerates it and re-checks the hash.
const tinyPort = await freePort();
const tiny = startRunner(tinyPort, ['--memory-budget-mb', '1']);
await tiny.ready;
await waitStatus(tinyPort, x => [0, 1, 2, 3].every(r => verified(x, r)));
const before = await status(tinyPort);
const t = new Client('T', `ws://127.0.0.1:${tinyPort}/ws`); await t.open(); await t.hello();
const regen = await (async () => { const p = t.waitAny(['exact_state', 'commit_rejected'], x => x.kind === 'commit_rejected' || (x.revision === 1 && x.tick === tick2)); t.send({ kind: 'get_exact_state', revision: 1, tick: tick2 }); return p; })();
assert(regen.kind === 'exact_state', `regeneration failed: ${regen.message}`);
const after = await status(tinyPort);
summary.checks.retention = { evictions: before.store.evictions, loaded_before: before.store.revisions.map(r => r.loaded), regenerations: after.store.regenerations, factory_complete_again: regen.snapshot.entities.some(e => e.type_key === 'factory' && e.lifecycle === 'complete') };
assert(before.store.evictions > 0 && after.store.regenerations > 0, 'tiny budget did not evict/regenerate');
t.close(); await tiny.stop();

for (const c of [a, b, s]) c.close();
await runner.stop(); await server.stop();
summary.runner_log = runner.log.join('').split('\n').filter(l => /revision|bootstrapped|controller/.test(l));
await writeFile(`${root}target/peripheral-summary.json`, JSON.stringify(summary, null, 2));
console.log(JSON.stringify(summary, null, 2));
