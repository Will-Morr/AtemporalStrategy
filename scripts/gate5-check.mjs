// Gate 5 failure injection over the real server: kill after durable input, during a job, after
// result writes and before publication; duplicate commits; a full disk. Every case must keep the
// last published revision intact and recover the same accepted moves and round hash as a clean run.
// Usage: node scripts/gate5-check.mjs [--only NAME]
import { readFile, rm, writeFile, mkdir } from 'node:fs/promises';
import { existsSync, readdirSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { root, now, assert, buildServer, startServer as spawnServer, Client, lobby } from './match-harness.mjs';

const args = process.argv.slice(2);
const opt = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const only = opt('--only', null);
const work = `${root}target/gate5-check`;
const startServer = (name, extra, env) => spawnServer(work, name, extra, env);
const replaysOf = name => `${work}/${name}/replays`;
const matchIdOf = name => readdirSync(replaysOf(name))[0];
const readJson = async path => JSON.parse(await readFile(path, 'utf8'));
const stripDurations = turns => turns.map(({ duration_ms, ...t }) => t);

/** Round 1 (opening) and the round-2 draft used by every scenario. */
async function opening(server) {
  const { a, b, s } = await lobby(server.url);
  const state0 = (await a.request({ kind: 'get_exact_state', revision: 0, tick: 0 }, 'exact_state', m => m.tick === 0)).snapshot;
  const find = (owner, key) => state0.entities.find(e => e.owner === owner && e.type_key === key);
  const miner = find(0, 'miner'), constructor = find(0, 'constructor'), enemyMiner = find(1, 'miner');
  const w = state0.terrain.width;
  const ore = state0.ore.map((v, i) => [v, i]).filter(([v]) => v > 0).map(([, i]) => ({ x: i % w, y: Math.floor(i / w) })).filter(t => Math.hypot(t.x - miner.tile.x, t.y - miner.tile.y) < 12);
  const area = { min: { x: Math.min(...ore.map(t => t.x)), y: Math.min(...ore.map(t => t.y)) }, max: { x: Math.max(...ore.map(t => t.x)), y: Math.max(...ore.map(t => t.y)) } };
  const factoryTile = { x: constructor.tile.x + 2, y: constructor.tile.y - 1 };
  const draft = (revision, tick, commands) => ({ based_on_revision: revision, tick, commands: commands.map((command, i) => ({ local_id: `c${i}`, command, future_orders: 'keep' })) });
  const ra = await a.sendAndWaitCommit('a1', draft(0, 0, [
    { kind: 'assign_order', entities: [miner.id], order: { kind: 'mine', area } },
    { kind: 'place_blueprints', type_key: 'factory', tiles: [factoryTile], priority: 'high', output_directions: ['e'] },
    { kind: 'assign_order', entities: [constructor.id], order: { kind: 'construct', area: { min: factoryTile, max: factoryTile } } },
  ]));
  const rb = await b.sendAndWaitCommit('b1', draft(0, 0, []));
  assert(ra.kind === 'commit_accepted' && rb.kind === 'commit_accepted', `round 1 ${ra.message ?? ''} ${rb.message ?? ''}`);
  await a.wait('revision_published', m => m.revision === 1);
  await a.wait('planning_opened', m => m.round === 2);
  const factory = (await a.request({ kind: 'get_exact_state', revision: 1, tick: 153 }, 'exact_state', m => m.tick === 153)).snapshot.entities.find(e => e.owner === 0 && e.type_key === 'factory');
  assert(factory?.lifecycle === 'complete', 'factory complete at 153');
  const round2 = {
    a: draft(1, 153, [
      { kind: 'set_stored_order', factories: [factory.id], order: { kind: 'attack_move', destination: enemyMiner.tile } },
      { kind: 'edit_production', factories: [factory.id], edit: { kind: 'append', items: ['grunt', 'grunt'] } },
      { kind: 'set_queue_loop', factories: [factory.id], enabled: true },
    ]),
    b: draft(1, 153, []),
  };
  return { a, b, s, round2 };
}

/** Connect with a slot token to a (resumed) server and collect its current state. */
async function observe(url, token) {
  const c = new Client('R', url); await c.open();
  const welcome = await c.hello(token);
  const published = welcome.phase === 'lobby' ? null : await c.wait('revision_published');
  const planning = welcome.phase === 'planning' ? await c.wait('planning_opened') : null;
  return { c, welcome, published, planning };
}

async function finishRound2(client, published, planning, round2, tokens, expectRevision) {
  // Whatever is still missing for round 2 is committed now; then the revision must publish.
  if (planning && planning.round === 2) {
    for (const [player, draft, id] of [[0, round2.a, 'a2'], [1, round2.b, 'b2']]) {
      client.token = tokens[player];
      const r = await client.sendAndWaitCommit(id, draft);
      assert(r.kind === 'commit_accepted', `commit ${id}: ${r.message ?? ''}`);
    }
  }
  return published?.revision === expectRevision ? published : client.wait('revision_published', m => m.revision === expectRevision);
}

async function verifyArchive(name) {
  const out = [];
  const code = await new Promise(ok => { const p = spawn(`${root}target/release/atemporal-server`, ['--verify', matchIdOf(name), '--replays', replaysOf(name)], { cwd: root }); p.stdout.on('data', c => out.push(c.toString())); p.stderr.on('data', c => out.push(c.toString())); p.on('exit', ok); });
  assert(code === 0, `--verify failed for ${name}: ${out.join('')}`);
  return out.join('').trim().split('\n');
}

const baseline = {};
const results = {};

async function checkRecovered(name, published, turns) {
  const matchId = matchIdOf(name);
  const round1 = await readJson(`${replaysOf(name)}/${matchId}/rounds/1.json`);
  const round2 = await readJson(`${replaysOf(name)}/${matchId}/rounds/2.json`);
  assert(round1.final_hash === baseline.round1Hash, 'revision 1 record intact');
  assert(round2.final_hash === baseline.round2Hash && published.outcome.terminal_state_tick === baseline.terminal, `round 2 hash ${round2.final_hash} vs baseline ${baseline.round2Hash}`);
  assert(JSON.stringify(stripDurations(turns)) === JSON.stringify(baseline.turns), 'same accepted moves');
  assert(readdirSync(`${replaysOf(name)}/${matchId}/turns`).filter(f => f.endsWith('.json')).length === 4, 'exactly four turn files');
  assert(readdirSync(`${replaysOf(name)}/${matchId}/rounds`).length === 3, 'rounds 0..2 only');
  return { round2_hash: round2.final_hash, verify: await verifyArchive(name) };
}

const scenarios = {
  async baseline() {
    const name = 'baseline';
    await rm(`${work}/${name}`, { recursive: true, force: true });
    const server = await startServer(name, []);
    const { a, b, s, round2 } = await opening(server);
    const t = now();
    // Duplicate commits: the same request twice appends one turn; a second draft is rejected.
    const first = await a.sendAndWaitCommit('a2', round2.a);
    const again = await a.sendAndWaitCommit('a2', round2.a);
    const other = await a.sendAndWaitCommit('a2-other', round2.a);
    assert(first.kind === 'commit_accepted' && again.kind === 'commit_accepted' && other.kind === 'commit_rejected', 'duplicate handling');
    const rb = await b.sendAndWaitCommit('b2', round2.b);
    assert(rb.kind === 'commit_accepted', 'b2');
    const published = await s.wait('revision_published', m => m.revision === 2);
    const ms = Math.round(now() - t);
    const commands = await a.request({ kind: 'get_commands', revision: 2, from_tick: 0, to_tick: 20000 }, 'commands', m => m.revision === 2);
    const matchId = matchIdOf(name);
    baseline.round1Hash = (await readJson(`${replaysOf(name)}/${matchId}/rounds/1.json`)).final_hash;
    baseline.round2Hash = (await readJson(`${replaysOf(name)}/${matchId}/rounds/2.json`)).final_hash;
    baseline.terminal = published.outcome.terminal_state_tick;
    baseline.turns = stripDurations(commands.turns);
    assert(commands.turns.length === 4 && readdirSync(`${replaysOf(name)}/${matchId}/turns`).filter(f => f.endsWith('.json')).length === 4, 'one turn per player per round');
    for (const c of [a, b, s]) c.close();
    server.child.kill('SIGTERM'); await server.exited;
    results[name] = { round2_commit_to_publish_ms: ms, round2_hash: baseline.round2Hash, terminal: baseline.terminal, duplicate: { again: again.kind, other: other.message }, verify: await verifyArchive(name) };
  },

  async kill_after_turn_written() {
    const name = 'kill_after_turn_written';
    await rm(`${work}/${name}`, { recursive: true, force: true });
    let server = await startServer(name, [], { ATEMPORAL_FAIL_AT: 'after_turn_written:2' });
    const { a, b, s, round2 } = await opening(server);
    const tokens = [a.token, b.token];
    a.send({ kind: 'commit', request: { request_id: 'a2', slot_token: a.token, draft: round2.a } });
    const code = await server.exited;
    assert(code !== 0, 'server aborted at the fail point');
    for (const c of [a, b, s]) c.close();
    const matchId = matchIdOf(name);
    assert(existsSync(`${replaysOf(name)}/${matchId}/turns/2-0.json`) && !existsSync(`${replaysOf(name)}/${matchId}/rounds/2.json`), 'turn durable, round not');
    server = await startServer(name, ['--resume', matchId]);
    const { c, welcome, published, planning } = await observe(server.url, tokens[0]);
    assert(welcome.phase === 'planning' && published.revision === 1 && planning.round === 2, `resumed into planning round 2 (${welcome.phase})`);
    assert(planning.committed_players.length === 1 && planning.committed_players[0] === 0, 'player 0 already committed');
    // The retry of the unacknowledged commit is answered idempotently, not appended.
    c.token = tokens[0];
    const retry = await c.sendAndWaitCommit('a2', round2.a);
    assert(retry.kind === 'commit_accepted', 'retry accepted');
    c.token = tokens[1];
    const rb = await c.sendAndWaitCommit('b2', round2.b);
    assert(rb.kind === 'commit_accepted', 'b2');
    const rev2 = await c.wait('revision_published', m => m.revision === 2);
    const commands = await c.request({ kind: 'get_commands', revision: 2, from_tick: 0, to_tick: 20000 }, 'commands', m => m.revision === 2);
    results[name] = await checkRecovered(name, rev2, commands.turns);
    c.close(); server.child.kill('SIGTERM'); await server.exited;
  },

  async kill_generic(name, point, expectRecordAfterKill) {
    await rm(`${work}/${name}`, { recursive: true, force: true });
    let server = await startServer(name, [], { ATEMPORAL_FAIL_AT: `${point}:2` });
    const { a, b, s, round2 } = await opening(server);
    const tokens = [a.token, b.token];
    a.send({ kind: 'commit', request: { request_id: 'a2', slot_token: a.token, draft: round2.a } });
    b.send({ kind: 'commit', request: { request_id: 'b2', slot_token: b.token, draft: round2.b } });
    const code = await server.exited;
    assert(code !== 0, 'server aborted at the fail point');
    for (const c of [a, b, s]) c.close();
    const matchId = matchIdOf(name);
    const dir = `${replaysOf(name)}/${matchId}`;
    assert(existsSync(`${dir}/turns/2-0.json`) && existsSync(`${dir}/turns/2-1.json`), 'both turns durable');
    assert(existsSync(`${dir}/rounds/2.json`) === expectRecordAfterKill, `round record presence after ${point}`);
    const resultsBefore = existsSync(`${dir}/results/2/complete.json`);
    const t = now();
    server = await startServer(name, ['--resume', matchId]);
    const { c, welcome, published, planning } = await observe(server.url, tokens[0]);
    const rev2 = await finishRound2(c, published, planning, round2, tokens, 2);
    const recoverMs = Math.round(now() - t);
    const commands = await c.request({ kind: 'get_commands', revision: 2, from_tick: 0, to_tick: 20000 }, 'commands', m => m.revision === 2);
    results[name] = { phase_on_resume: welcome.phase, results_dir_before_resume: resultsBefore, resume_to_published_ms: recoverMs, ...(await checkRecovered(name, rev2, commands.turns)) };
    c.close(); server.child.kill('SIGTERM'); await server.exited;
  },
  kill_during_job: () => scenarios.kill_generic('kill_during_job', 'during_job', false),
  kill_after_results: () => scenarios.kill_generic('kill_after_results', 'after_results', false),
  kill_before_publish: () => scenarios.kill_generic('kill_before_publish', 'before_publish', true),

  // Writes fail like ENOSPC while round 2's results are being written: the round reopens with the
  // last revision intact, both durable turns survive, and a resume replays them.
  async worker_panic() {
    const name='worker_panic';await rm(`${work}/${name}`,{recursive:true,force:true});
    const server=await startServer(name,[],{ATEMPORAL_WORKER_PANIC_ONCE:'1'});
    const {a,b,s,round2}=await opening(server);
    try {
      for(const [c,id,draft] of [[a,'a2',round2.a],[b,'b2',round2.b]]) assert((await c.sendAndWaitCommit(id,draft)).kind==='commit_accepted','durable panic-test commit');
      const published=await s.wait('revision_published',m=>m.revision===2);
      const commands=await s.request({kind:'get_commands',revision:2,from_tick:0,to_tick:20000},'commands',m=>m.revision===2);
      assert(server.log.join('').includes('retrying panicked worker'),'worker panic retried automatically');
      results[name]={retry_logged:true,...(await checkRecovered(name,published,commands.turns))};
      const exact=await s.request({kind:'get_exact_state',revision:2,tick:154},'exact_state',m=>m.tick===154);
      assert(exact.snapshot.tick===154,'worker services subsequent exact-state jobs');
    } finally {for(const c of [a,b,s])c.close();server.child.kill('SIGTERM');await server.exited;}
  },
  async disk_full() {
    const name = 'disk_full';
    await rm(`${work}/${name}`, { recursive: true, force: true });
    // Durable writes before round 2's results: 5 (start) + 8 (round 0) + 4 + 8 (round 1) + 4 turn files.
    let server = await startServer(name, [], { ATEMPORAL_DISK_FULL_AFTER: '29' });
    const { a, b, s, round2 } = await opening(server);
    const tokens = [a.token, b.token];
    // Discard the earlier planning announcement before sending commits; a fast failed
    // publication may reopen the round before the final commit acknowledgement arrives.
    s.unconsumed = s.unconsumed.filter(m => m.kind !== 'planning_opened');
    const ra = await a.sendAndWaitCommit('a2', round2.a);
    const rb = await b.sendAndWaitCommit('b2', round2.b);
    assert(ra.kind === 'commit_accepted' && rb.kind === 'commit_accepted', 'both turns durable before the disk fills');
    const reopened = await s.wait('planning_opened', m => m.round === 2 && m.committed_players.length === 0);
    assert(reopened.revision === 1, 'last published revision retained');
    const matchId = matchIdOf(name);
    const dir = `${replaysOf(name)}/${matchId}`;
    assert(!existsSync(`${dir}/rounds/2.json`) && existsSync(`${dir}/turns/2-1.json`), 'no round record, turns kept');
    assert((await readJson(`${dir}/rounds/1.json`)).final_hash === baseline.round1Hash, 'round 1 intact');
    for (const c of [a, b, s]) c.close();
    server.child.kill('SIGTERM'); await server.exited;
    const failureLines = server.log.join('').split('\n').filter(l => /failed|space left/i.test(l));
    const logged = failureLines.some(l => l.includes('No space left on device'));
    server = await startServer(name, ['--resume', matchId]);
    const { c, welcome, published, planning } = await observe(server.url, tokens[0]);
    const rev2 = await finishRound2(c, published, planning, round2, tokens, 2);
    const commands = await c.request({ kind: 'get_commands', revision: 2, from_tick: 0, to_tick: 20000 }, 'commands', m => m.revision === 2);
    results[name] = { disk_full_logged: logged, failure_lines: failureLines, phase_on_resume: welcome.phase, ...(await checkRecovered(name, rev2, commands.turns)) };
    c.close(); server.child.kill('SIGTERM'); await server.exited;
  },
};

await buildServer();
await mkdir(work, { recursive: true });
let failed = false;
for (const name of ['baseline', 'kill_after_turn_written', 'kill_during_job', 'kill_after_results', 'kill_before_publish', 'disk_full', 'worker_panic']) {
  if (only && name !== only && name !== 'baseline') continue;
  const started = now();
  try { await scenarios[name](); console.log(`ok   ${name} (${Math.round(now() - started)} ms)`); }
  catch (e) { failed = true; console.log(`FAIL ${name}: ${e.stack ?? e}`); results[name] = { error: String(e) }; }
}
await writeFile(`${root}target/gate5-summary.json`, JSON.stringify(results, null, 2));
console.log(JSON.stringify(results, null, 2));
if (failed) process.exit(1);
