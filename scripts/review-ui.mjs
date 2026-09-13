// Agent-neutral entry point: one isolated run, artifacts, exit code, and optional external app.
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdir, writeFile, readdir } from 'node:fs/promises';
import { createWriteStream } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
const root = fileURLToPath(new URL('../', import.meta.url));
const runId = `${new Date().toISOString().replaceAll(':', '-')}-${process.pid}`;
const artifacts = resolve(process.env.ATEMPORAL_UI_ARTIFACTS ?? `${root}artifacts/ui/${runId}`);
await mkdir(artifacts, { recursive: true });
if ((await readdir(artifacts)).length) throw new Error(`Artifact directory must be empty: ${artifacts}`);
const metadata = { runId, artifacts, command: process.argv.slice(2), externalServer: !!process.env.ATEMPORAL_UI_BASE_URL, startedAt: new Date().toISOString(), node: process.version, status: 'starting' };
const save = () => writeFile(`${artifacts}/run.json`, JSON.stringify(metadata, null, 2));
await save();
console.log(`Review artifacts: ${artifacts}`);
try {
  let port = process.env.ATEMPORAL_UI_PORT;
  if (!process.env.ATEMPORAL_UI_BASE_URL && !port) {
    const probe = createServer();
    await new Promise((done, reject) => { probe.once('error', reject); probe.listen(0, '127.0.0.1', done); });
    port = String(probe.address().port);
    await new Promise(done => probe.close(done));
  }
  if (port && (!Number.isInteger(Number(port)) || Number(port) < 1 || Number(port) > 65535)) throw new Error('ATEMPORAL_UI_PORT must be 1..65535');
  const baseURL = process.env.ATEMPORAL_UI_BASE_URL ?? `http://127.0.0.1:${port}`;
  if (!['http:', 'https:'].includes(new URL(baseURL).protocol)) throw new Error('Base URL must use HTTP(S)');
  Object.assign(metadata, { baseURL, status: 'running', serverCommand: metadata.externalServer ? null : process.env.ATEMPORAL_UI_SERVER_COMMAND ?? 'npm run build --prefix client && node scripts/serve-client.mjs' });
  await save();
  const output = createWriteStream(`${artifacts}/runner.log`);
  const child = spawn(process.execPath, [`${root}client/node_modules/@playwright/test/cli.js`, 'test', '--config', `${root}client/playwright.config.mjs`, ...process.argv.slice(2)], { cwd: root, stdio: ['inherit', 'pipe', 'pipe'], env: { ...process.env, ATEMPORAL_UI_PORT: port ?? '', ATEMPORAL_UI_ARTIFACTS: artifacts, ATEMPORAL_UI_RESOLVED_URL: baseURL } });
  child.stdout.on('data', data => { process.stdout.write(data); output.write(data); });
  child.stderr.on('data', data => { process.stderr.write(data); output.write(data); });
  for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
  const result = await new Promise((done, reject) => { child.once('error', reject); child.once('close', (code, signal) => done({ code: code ?? 1, signal })); });
  await new Promise(done => output.end(done));
  Object.assign(metadata, { status: result.code === 0 ? 'passed' : 'failed', exitCode: result.code, signal: result.signal });
  process.exitCode = result.code;
} catch (error) {
  Object.assign(metadata, { status: 'failed', exitCode: 1, error: error.stack });
  console.error(error);
  process.exitCode = 1;
} finally {
  metadata.finishedAt = new Date().toISOString();
  await save();
}
