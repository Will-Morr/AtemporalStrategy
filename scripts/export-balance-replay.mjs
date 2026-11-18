// Export only the public inputs/outcomes needed for offline balance trials.
import {mkdir, writeFile} from 'node:fs/promises';
import {join} from 'node:path';

const [address, destination] = process.argv.slice(2);
if (!address || !destination) throw new Error('Usage: node scripts/export-balance-replay.mjs REPLAY_PAGE_URL NEW_DIRECTORY');
const url = new URL(address);
if (!['https:', 'http:'].includes(url.protocol) || !/^\/games\/[^/]+\/replays\/[^/]+\/?$/.test(url.pathname)) {
  throw new Error('Use the spectator link for a completed or archived match.');
}
url.pathname = url.pathname.replace(/\/?$/, '/ws');
url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
url.search = '';
url.hash = '';
const ws = new WebSocket(url);
const save = (name, data) => writeFile(join(destination, name), JSON.stringify(data), {mode: 0o600});
const send = message => ws.send(JSON.stringify({schema_version: 2, message}));
const records = new Map();
let config;
let latest;
let pending;
const timer = setTimeout(() => {console.error('Timed out reading archived replay'); process.exit(1);}, 120_000);
ws.addEventListener('error', () => {console.error('Spectator connection failed'); process.exitCode = 1;});
ws.addEventListener('open', () => send({kind: 'hello', protocol_version: 2, last_revision: null, slot_token: null}));
ws.addEventListener('message', async ({data}) => {
  try {
    const {message: m} = JSON.parse(data);
    if (m.kind === 'commit_rejected') throw new Error(m.message);
    if (m.kind === 'welcome') config = m.config;
    if (m.kind === 'revision_published' && latest === undefined) {
      latest = m.revision;
      pending = latest + 2;
      send({kind: 'get_exact_state', revision: 0, tick: 0});
      send({kind: 'get_commands', revision: latest, from_tick: 0, to_tick: config.max_tick});
      for (let revision = 1; revision <= latest; revision++) send({kind: 'get_round', revision});
    }
    const file = m.kind === 'exact_state' ? 'initial.json' : m.kind === 'commands' ? 'commands.json' :
      m.kind === 'round_result' ? `round_result${m.revision}.json` : null;
    if (!file || records.has(file)) return;
    records.set(file, m);
    if (records.size !== pending) return;
    await mkdir(destination, {mode: 0o700});
    await save('game.json', {config});
    for (const [name, value] of records) await save(name, value);
    console.log(`Saved ${latest} rounds to ${destination}`);
    ws.close();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
    ws.close();
  }
});
ws.addEventListener('close', () => {
  clearTimeout(timer);
  if (pending === undefined || records.size !== pending) process.exitCode = 1;
});
