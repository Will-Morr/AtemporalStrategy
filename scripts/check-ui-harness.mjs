// Prove failures are red and useful evidence survives fixture teardown.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFile, readdir, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const artifacts = `${root}artifacts/harness/${Date.now()}-${process.pid}`;
const child = spawn(process.execPath, [`${root}scripts/review-ui.mjs`, '--project=desktop-chromium'], {
  cwd: root, stdio: 'inherit', env: { ...process.env, ATEMPORAL_UI_ARTIFACTS: artifacts, ATEMPORAL_UI_HARNESS_CHECK: '1' }
});
const code = await new Promise((done, reject) => { child.once('error', reject); child.once('close', done); });
assert.equal(code, 1, 'Injected failures must return exit code 1');
const report = JSON.parse(await readFile(`${artifacts}/results.json`, 'utf8'));
assert.equal(report.stats.unexpected, 4);
assert.equal(report.stats.expected, 0);
const specs = report.suites.flatMap(suite => suite.specs);
assert.equal(specs.length, 4);
for (const spec of specs) {
  const result = spec.tests[0].results[0];
  assert.equal(result.status, 'failed', spec.title);
  for (const name of ['browser-log', 'trace', 'failure-page-0', 'failure-page-0-aria']) {
    const attachment = result.attachments.find(item => item.name === name);
    assert(attachment, `${spec.title}: missing ${name}`);
    const file = await stat(attachment.path);
    assert(file.isFile());
    if (!name.endsWith('-aria')) assert(file.size > 0);
  }
  const logs = JSON.parse(await readFile(result.attachments.find(item => item.name === 'browser-log').path, 'utf8'));
  if (spec.title.includes('HTTP')) assert(logs.some(item => item.kind === 'http-error' && item.status === 503));
  if (spec.title.includes('page errors')) assert(logs.some(item => item.kind === 'pageerror' && item.text.includes('intentional pageerror probe')));
  if (spec.title.includes('failed requests')) assert(logs.some(item => item.kind === 'requestfailed'));
  if (spec.title.includes('each identity')) {
    for (const name of ['player-a', 'player-b', 'spectator']) assert(result.attachments.some(item => item.name === `${name}-video-0`));
    for (const index of [1, 2, 3]) assert(result.attachments.some(item => item.name === `failure-page-${index}`));
    assert(result.errors.some(item => item.message.includes('intentional assertion probe')));
  }
}
const files = await readdir(artifacts);
for (const name of ['run.json', 'runner.log', 'junit.xml', 'report']) assert(files.includes(name));
console.log(`Harness failure checks passed: four intentional failures, diagnostics, screenshots, traces and identity videos verified.\nArtifacts: ${artifacts}`);
