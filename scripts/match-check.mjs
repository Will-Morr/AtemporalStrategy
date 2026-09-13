// Match-controller check: timed mode, time penalty, manual stop and archive resume over the
// real server and protocol; no browser. Usage: node scripts/match-check.mjs [--only NAME]
import { mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { existsSync, readdirSync } from 'node:fs';
import { root, now, sleep, assert, buildServer, startServer as spawnServer, writeConfig as makeConfig, Client, lobby } from './match-harness.mjs';

const args = process.argv.slice(2);
const opt = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const only = opt('--only', null);
const work = `${root}target/match-check`;
const startServer = (name, extra, env) => spawnServer(work, name, extra, env);
const writeConfig = (name, edit) => makeConfig(work, name, edit);

const results = {};
const scenarios = {
  // Locked history advances lock_ticks_per_round per resolved round, survives a resume, and
  // reaching max_tick undecided archives the match as history_exhausted without a winner.
  async timed_exhausted() {
    const name = 'timed_exhausted';
    await rm(`${work}/${name}`, { recursive: true, force: true });
    const config = await writeConfig(name, c => { c.match_defaults.objective = { kind: 'timed', lock_ticks_per_round: 100 }; c.match_defaults.max_tick = 400; });
    let server = await startServer(name, ['--config', config]);
    let { a, b, s } = await lobby(server.url);
    const boundaries = [];
    const pass = async (round, revision, tick) => {
      const started = now();
      const ra = await a.sendAndWaitCommit(`a${round}`, { based_on_revision: revision, tick, commands: [] });
      const rb = await b.sendAndWaitCommit(`b${round}`, { based_on_revision: revision, tick, commands: [] });
      assert(ra.kind === 'commit_accepted' && rb.kind === 'commit_accepted', `round ${round} commits: ${ra.message ?? ''} ${rb.message ?? ''}`);
      const published = await a.wait('revision_published', m => m.revision === revision + 1);
      boundaries.push({ round, boundary: published.timed.boundary, status: published.timed.status, lost: published.timed.timed_lost_players, terminal: published.outcome.terminal_state_tick, ms: Math.round(now() - started) });
      return published;
    };
    await pass(1, 0, 0);
    let planning = await a.wait('planning_opened', m => m.round === 2);
    assert(planning.editable_from === 100, `editable_from advanced to ${planning.editable_from}`);
    const stale = await a.sendAndWaitCommit('a-early', { based_on_revision: 1, tick: 50, commands: [] });
    assert(stale.kind === 'commit_rejected' && /within 100/.test(stale.message), `locked tick rejected: ${stale.message}`);
    await pass(2, 1, 100);
    planning = await a.wait('planning_opened', m => m.round === 3);
    assert(planning.editable_from === 200, 'second boundary');
    // Resume mid-match: same match id, same tokens, same lock boundary.
    const matchId = (await readFile(`${work}/${name}/replays/${readdirSync(`${work}/${name}/replays`)[0]}/manifest.json`, 'utf8').then(JSON.parse)).match_id;
    const tokens = [a.token, b.token];
    for (const c of [a, b, s]) c.close();
    server.child.kill('SIGTERM'); await server.exited;
    server = await startServer(name, ['--resume', matchId]);
    a = new Client('A', server.url); b = new Client('B', server.url); s = new Client('S', server.url);
    await Promise.all([a.open(), b.open(), s.open()]);
    const welcome = await a.hello(tokens[0]); await b.hello(tokens[1]); await s.hello();
    a.token = tokens[0]; b.token = tokens[1];
    assert(welcome.phase === 'planning' && welcome.timed.boundary === 200, `resumed timed state ${JSON.stringify(welcome.timed)}`);
    planning = await a.wait('planning_opened', m => m.round === 3);
    assert(planning.editable_from === 200 && planning.revision === 2, 'resumed planning round 3 at lock 200');
    await pass(3, 2, 200);
    await a.wait('planning_opened', m => m.round === 4);
    const last = await pass(4, 3, 300);
    assert(last.timed.status === 'history_exhausted' && last.timed.boundary === 400, `final adjudication ${JSON.stringify(last.timed)}`);
    const finished = await s.wait('match_finished');
    const archived = await b.wait('match_archived');
    assert(finished.reason === 'history_exhausted' && finished.match_winners.length === 0, 'no invented winner');
    assert(archived.archive.reason === 'history_exhausted' && archived.archive.status === 'unfinished', 'archive record');
    const late = await a.sendAndWaitCommit('a-late', { based_on_revision: 4, tick: 399, commands: [] });
    assert(late.kind === 'commit_rejected', 'no planning after exhaustion');
    assert(existsSync(`${work}/${name}/replays/${matchId}/archive.json`), 'archive.json written');
    for (const c of [a, b, s]) c.close();
    server.child.kill('SIGTERM'); await server.exited;
    results[name] = { boundaries, finished: finished.reason, archive: archived.archive.reason, resumed_lines: server.log.join('').split('\n').filter(l => l.startsWith('resumed')) };
  },

  // A player whose constructor dies inside the locked window with no factory is timed-lost;
  // the remaining side wins at S[new_L], not at the mutable simulation end.
  async timed_loss() {
    const name = 'timed_loss';
    await rm(`${work}/${name}`, { recursive: true, force: true });
    const config = await writeConfig(name, c => { c.match_defaults.objective = { kind: 'timed', lock_ticks_per_round: 500 }; c.match_defaults.max_tick = 3000; });
    const server = await startServer(name, ['--config', config]);
    const { a, b, s } = await lobby(server.url);
    const state0 = (await a.request({ kind: 'get_exact_state', revision: 0, tick: 0 }, 'exact_state', m => m.tick === 0)).snapshot;
    const find = (owner, key) => state0.entities.find(e => e.owner === owner && e.type_key === key);
    const turretA = find(0, 'turret'), constructorB = find(1, 'constructor');
    const started = now();
    const ra = await a.sendAndWaitCommit('a1', { based_on_revision: 0, tick: 0, commands: [] });
    const rb = await b.sendAndWaitCommit('b1', { based_on_revision: 0, tick: 0, commands: [{ local_id: 'c0', command: { kind: 'assign_order', entities: [constructorB.id], order: { kind: 'attack_move', destination: turretA.tile } }, future_orders: 'keep' }] });
    assert(ra.kind === 'commit_accepted' && rb.kind === 'commit_accepted', `commits ${ra.message ?? ''} ${rb.message ?? ''}`);
    const published = await s.wait('revision_published', m => m.revision === 1);
    const finished = await a.wait('match_finished');
    const locked = (await a.request({ kind: 'get_exact_state', revision: 1, tick: 500 }, 'exact_state', m => m.tick === 500)).snapshot;
    const bBuilders = locked.entities.filter(e => e.owner === 1 && ['constructor', 'factory'].includes(e.type_key) && e.lifecycle === 'complete');
    assert(bBuilders.length === 0, `B still has builders at S[500]: ${bBuilders.map(e => e.type_key)}`);
    assert(published.timed.timed_lost_players.length === 1 && published.timed.timed_lost_players[0] === 1, `timed lost ${JSON.stringify(published.timed)}`);
    assert(published.timed.status === 'finished' && finished.reason === 'victory', 'finished by locked-state adjudication');
    assert(finished.match_winners.length === 1 && finished.match_winners[0].player_id === 0, 'A wins');
    for (const c of [a, b, s]) c.close();
    server.child.kill('SIGTERM'); await server.exited;
    results[name] = { commit_to_publish_ms: Math.round(now() - started), terminal: published.outcome.terminal_state_tick, sim_outcome: published.outcome.kind, timed: published.timed, winners: finished.match_winners };
  },

  // Scoreboard with the fastest-opponent time penalty: the slower winner's adjusted delta is
  // below the raw point. Then the first slot stops the match; the archive is unfinished with no
  // extra score, later commits are rejected, and a resume reopens it read-only.
  async penalty_and_stop() {
    const name = 'penalty_and_stop';
    await rm(`${work}/${name}`, { recursive: true, force: true });
    const config = await writeConfig(name, c => { c.match_defaults.objective.rules.time_penalty = 'fastest_opponent_ratio'; });
    let server = await startServer(name, ['--config', config]);
    let { a, b, s, config: cfg } = await lobby(server.url);
    a.send({ kind: 'planning_ready', round: 1, revision: 0 }); b.send({ kind: 'planning_ready', round: 1, revision: 0 });
    const state0 = (await a.request({ kind: 'get_exact_state', revision: 0, tick: 0 }, 'exact_state', m => m.tick === 0)).snapshot;
    const find = (owner, key) => state0.entities.find(e => e.owner === owner && e.type_key === key);
    const miner = find(0, 'miner'), constructor = find(0, 'constructor'), enemyMiner = find(1, 'miner');
    const w = state0.terrain.width;
    const ore = state0.ore.map((v, i) => [v, i]).filter(([v]) => v > 0).map(([, i]) => ({ x: i % w, y: Math.floor(i / w) })).filter(t => Math.hypot(t.x - miner.tile.x, t.y - miner.tile.y) < 12);
    const area = { min: { x: Math.min(...ore.map(t => t.x)), y: Math.min(...ore.map(t => t.y)) }, max: { x: Math.max(...ore.map(t => t.x)), y: Math.max(...ore.map(t => t.y)) } };
    const factoryTile = { x: constructor.tile.x + 2, y: constructor.tile.y - 1 };
    const draft = (revision, tick, commands) => ({ based_on_revision: revision, tick, commands: commands.map((command, i) => ({ local_id: `c${i}`, command, future_orders: 'keep' })) });
    // B commits immediately; A thinks for a while so A's time penalty ratio is < 1.
    let rb = await b.sendAndWaitCommit('b1', draft(0, 0, []));
    await sleep(1500);
    let ra = await a.sendAndWaitCommit('a1', draft(0, 0, [
      { kind: 'assign_order', entities: [miner.id], order: { kind: 'mine', area } },
      { kind: 'place_blueprints', type_key: 'factory', tiles: [factoryTile], priority: 'high', output_directions: ['e'] },
      { kind: 'assign_order', entities: [constructor.id], order: { kind: 'construct', area: { min: factoryTile, max: factoryTile } } },
    ]));
    assert(ra.kind === 'commit_accepted' && rb.kind === 'commit_accepted', `round 1 ${ra.message ?? ''} ${rb.message ?? ''}`);
    const rev1 = await a.wait('revision_published', m => m.revision === 1);
    await a.wait('planning_opened', m => m.round === 2);
    assert(rev1.score.entries.every(e => e.raw_delta === 0), 'stalemate scores nothing');
    const factory = (await a.request({ kind: 'get_exact_state', revision: 1, tick: 153 }, 'exact_state', m => m.tick === 153)).snapshot.entities.find(e => e.owner === 0 && e.type_key === 'factory');
    assert(factory?.lifecycle === 'complete', 'factory complete at 153');
    rb = await b.sendAndWaitCommit('b2', draft(1, 153, []));
    await sleep(1500);
    const t2 = now();
    ra = await a.sendAndWaitCommit('a2', draft(1, 153, [
      { kind: 'set_stored_order', factories: [factory.id], order: { kind: 'attack_move', destination: enemyMiner.tile } },
      { kind: 'edit_production', factories: [factory.id], edit: { kind: 'append', items: ['grunt', 'grunt'] } },
      { kind: 'set_queue_loop', factories: [factory.id], enabled: true },
    ]));
    assert(ra.kind === 'commit_accepted' && rb.kind === 'commit_accepted', `round 2 ${ra.message ?? ''} ${rb.message ?? ''}`);
    // A retry with the same request id is answered from the recorded turn, not appended.
    const retry = await a.sendAndWaitCommit('a2', draft(1, 153, []));
    assert(retry.kind === 'commit_accepted', 'idempotent retry');
    const rev2 = await a.wait('revision_published', m => m.revision === 2);
    const publishMs = Math.round(now() - t2);
    assert(rev2.outcome.kind === 'win', `expected a win, got ${rev2.outcome.kind}`);
    const winner = rev2.score.entries.find(e => e.side_id.player_id === 0);
    assert(winner.raw_delta === 1 && winner.adjusted_delta > 0 && winner.adjusted_delta < 1, `penalty applied: ${JSON.stringify(winner)}`);
    assert(rev2.score.match_winners.length === 0, 'match continues toward 5 points');
    const ratios = rev2.time_ratios;
    await a.wait('planning_opened', m => m.round === 3);
    // Manual stop from the first occupied slot.
    const stopFromB = await b.request({ kind: 'stop_and_archive', request_id: 'stop-b', based_on_revision: 2, slot_token: b.token }, 'commit_rejected', m => m.request_id === 'stop-b').catch(() => null);
    a.send({ kind: 'stop_and_archive', request_id: 'stop-a', based_on_revision: 2, slot_token: a.token });
    const [archivedA, archivedB, archivedS] = await Promise.all([a.wait('match_archived'), b.wait('match_archived'), s.wait('match_archived')]);
    assert(archivedA.archive.status === 'unfinished' && archivedA.archive.reason === 'manual_stop' && archivedA.archive.actor === 0, 'archive record');
    const lateCommit = await b.sendAndWaitCommit('b3', draft(2, 200, []));
    assert(lateCommit.kind === 'commit_rejected', 'no commits after stop');
    const commands = await a.request({ kind: 'get_commands', revision: 2, from_tick: 0, to_tick: 20000 }, 'commands', m => m.revision === 2);
    const replays = `${work}/${name}/replays`;
    const matchId = readdirSync(replays)[0];
    const rounds = readdirSync(`${replays}/${matchId}/rounds`).sort();
    assert(rounds.length === 3 && existsSync(`${replays}/${matchId}/archive.json`), `rounds ${rounds} and archive.json`);
    for (const c of [a, b, s]) c.close();
    server.child.kill('SIGTERM'); await server.exited;
    server = await startServer(name, ['--resume', matchId]);
    const again = new Client('A2', server.url); await again.open();
    const welcome = await again.hello(a.token);
    const rev = await again.wait('revision_published', m => m.revision === 2);
    const commandsAgain = await again.request({ kind: 'get_commands', revision: 2, from_tick: 0, to_tick: 20000 }, 'commands', m => m.revision === 2);
    assert(welcome.phase === 'archived', `resumed phase ${welcome.phase}`);
    assert(JSON.stringify(commandsAgain.turns) === JSON.stringify(commands.turns), 'same accepted moves after resume');
    again.close(); server.child.kill('SIGTERM'); await server.exited;
    results[name] = { round2_commit_to_publish_ms: publishMs, terminal: rev2.outcome.terminal_state_tick, score: rev2.score.entries.map(e => ({ side: e.side_id, raw: e.raw_total, adjusted: e.adjusted_total })), time_ratios: ratios, stop_from_second_slot: stopFromB ? stopFromB.message : 'no rejection message', archive: archivedS.archive, resumed_phase: welcome.phase, resumed_revision: rev.revision, turns: commands.turns.length, snapshot_interval: cfg.snapshot_interval };
  },

  // CLI port and routes: an occupied port fails instead of moving, assets/guide/WebSocket share
  // one origin, a restart at the same address is a fresh server instance, and a resumed match
  // serves the guide generated from its archived content even when the CLI names other content.
  async port_routes_and_guide() {
    const name = 'port_routes_and_guide';
    await rm(`${work}/${name}`, { recursive: true, force: true });
    const server = await startServer(name, []);
    const port = server.port;
    const occupied = await startServer(name, ['--port', port]).then(() => 'started', e => String(e));
    assert(/cannot bind port/.test(occupied), `occupied port must fail: ${occupied}`);
    const http = async path => { const r = await fetch(`http://127.0.0.1:${port}${path}`); return { status: r.status, type: r.headers.get('content-type'), body: await r.text() }; };
    const index = await http('/'), guide = await http('/guide/'), content = await http('/guide/content.json'), manifest = await http('/guide/manifest.json'), missing = await http('/guide/../manifest.json');
    assert(index.status === 200 && /text\/html/.test(index.type) && /<script/.test(index.body), 'client index served');
    assert(guide.status === 200 && /text\/html/.test(guide.type), 'guide served');
    assert(content.status === 200 && manifest.status === 200 && missing.status === 404, 'guide files served, traversal rejected');
    const { a, b, s, welcome } = await lobby(server.url);
    assert(welcome.guide_url === '/guide/' && JSON.parse(manifest.body).content_hash === welcome.fingerprint.content_hash, 'guide manifest matches the loaded content');
    const firstInstance = a.instance;
    const matchId = readdirSync(`${work}/${name}/replays`)[0];
    const tokens = [a.token, b.token];
    for (const c of [a, b, s]) c.close();
    server.child.kill('SIGTERM'); await server.exited;
    // Different content on the CLI must not leak into a resumed match or its guide.
    const other = JSON.parse(await readFile(`${root}config/content.yaml`, 'utf8'));
    other.types.find(t => t.key === 'grunt').max_hp += 1;
    await writeFile(`${work}/${name}/other-content.yaml`, JSON.stringify(other));
    const resumed = await startServer(name, ['--port', port, '--resume', matchId, '--content', `${work}/${name}/other-content.yaml`, '--guide-dir', `${work}/${name}/resumed-guide`]);
    const again = new Client('A2', `ws://127.0.0.1:${port}/ws`); await again.open();
    const w2 = await again.hello(tokens[0]);
    assert(again.instance !== firstInstance, 'restart is a fresh server instance');
    assert(w2.phase !== 'lobby' && w2.fingerprint.content_hash === welcome.fingerprint.content_hash, 'resumed under the archived content');
    const manifest2 = JSON.parse((await http('/guide/manifest.json')).body);
    const content2 = JSON.parse((await http('/guide/content.json')).body);
    assert(manifest2.content_hash === welcome.fingerprint.content_hash, 'resumed guide generated from archived content');
    const gruntHp = doc => doc.types.find(t => t.key === 'grunt').max_hp;
    assert(gruntHp(content2) === gruntHp(JSON.parse(content.body)) && gruntHp(content2) !== gruntHp(other), 'guide stats match the archived content, not the CLI file');
    again.close(); resumed.child.kill('SIGTERM'); await resumed.exited;
    results[name] = { port, occupied_port_error: occupied.split('\n').find(l => /cannot bind/.test(l)), routes: { index: index.status, guide: guide.status, content: content.status, traversal: missing.status }, instance_changed: again.instance !== firstInstance, resumed_phase: w2.phase, guide_content_hash: manifest2.content_hash.slice(0, 12) };
  },
};

await buildServer();
await mkdir(work, { recursive: true });
let failed = false;
for (const [name, run] of Object.entries(scenarios)) {
  if (only && name !== only) continue;
  const started = now();
  try { await run(); console.log(`ok   ${name} (${Math.round(now() - started)} ms)`); }
  catch (e) { failed = true; console.log(`FAIL ${name}: ${e.stack ?? e}`); results[name] = { error: String(e) }; }
}
await writeFile(`${root}target/match-check-summary.json`, JSON.stringify(results, null, 2));
console.log(JSON.stringify(results, null, 2));
if (failed) process.exit(1);
