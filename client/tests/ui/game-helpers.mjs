import { expect } from './fixtures.mjs';
import { readFile, writeFile, mkdir, appendFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { spawn } from 'node:child_process';
import { once } from 'node:events';

export async function isolatedServer(testInfo, edit = () => {}, editContent = () => {}, serverArgs = [], env = {}) {
  const root = new URL('../../../', import.meta.url).pathname;
  const config = JSON.parse(await readFile(`${root}config/game.yaml`, 'utf8'));
  config.match_defaults.seed = 42;
  edit(config);
  const dir = testInfo.outputPath('server'); await mkdir(dir, { recursive:true });
  const content=JSON.parse(await readFile(`${root}config/content.yaml`,'utf8'));editContent(content);const contentPath=`${dir}/content.json`;await writeFile(contentPath,JSON.stringify(content));
  const configPath = `${dir}/game.json`; await writeFile(configPath,JSON.stringify(config));
  const probe = createServer(); await new Promise(r=>probe.listen(0,'127.0.0.1',r));
  const port = probe.address().port; await new Promise(r=>probe.close(r));
  let child;
  const start = async (extra=[]) => {
    child=spawn(`${root}target/release/atemporal-server`,['--port',String(port),'--config',configPath,'--content',contentPath,'--replays',`${dir}/replays`,...serverArgs,...extra],{cwd:root,stdio:['ignore','pipe','pipe'],env:{...process.env,...env}});
    await new Promise((resolve,reject)=>{
      const timeout=setTimeout(()=>reject(new Error('Server startup timeout')),15000);
      child.stdout.on('data',d=>{void appendFile(`${dir}/server.log`,d);if(String(d).includes('listening on')){clearTimeout(timeout);resolve();}});
      child.stderr.on('data',d=>void appendFile(`${dir}/server.log`,d));
      child.once('exit',c=>{clearTimeout(timeout);reject(new Error(`Server exited ${c}`));});
    });
  };
  const stop=async()=>{if(child.exitCode!==null)return;const exited=once(child,'exit');child.kill('SIGTERM');await exited;};
  await start();return {url:`http://127.0.0.1:${port}/`,dir,start,stop};
}
export async function players(review,url,count=2,teams=false) {
  const pages=[];
  for(let slot=0;slot<count;slot++){
    const p=await(await review.newContext(`player-${slot}`)).newPage();await p.goto(url);
    await expect(p.locator('#status')).toContainText('Claim a slot');await p.fill('#username',`Player ${slot}`);await p.fill('#color',['#4fc3f7','#aed581','#ff8a65','#ce93d8'][slot]);
    if(teams)await p.selectOption('#team',slot<2?'cyan':'orange');
    await p.locator('#roster li',{hasText:`Slot ${slot}`}).getByRole('button',{name:'Claim',exact:true}).click();
    await expect(p.locator('#status')).toContainText(`You hold slot ${slot}`);pages.push(p);
  }return pages;
}
export async function revision(p,n){await expect.poll(()=>p.evaluate(()=>!window.atemporal?.preview && window.atemporal?.latest),{timeout:60000}).toBe(n);await expect.poll(()=>p.evaluate(()=>window.atemporal?.current),{timeout:60000}).toBe(n);await expect.poll(()=>p.evaluate(()=>window.atemporal.exact?.revision),{timeout:30000}).toBe(n);}
export async function seek(p,t){await p.fill('#tick-input',String(t));await p.locator('#tick-input').press('Enter');await expect.poll(()=>p.evaluate(()=>window.atemporal.exact?.tick)).toBe(t);await p.keyboard.press('Escape');}
export async function tile(p,t){const point=await p.evaluate(t=>{const r=window.atemporal.renderer;r.centerOn(t.x,t.y);return r.screen(t.x+.5,t.y+.5);},t);await p.mouse.click(...point);}
export async function area(p,t){const point=await p.evaluate(t=>{const r=window.atemporal.renderer;r.centerOn(t.x,t.y);return r.screen(t.x+.5,t.y+.5);},t);await p.mouse.move(...point);await p.mouse.down();await p.mouse.up();}


// Exercise the same real input scenario through inputs-only replication when requested.
export async function isolatedPeripheral(testInfo, edit = () => {}, env = {}) {
  const controller = await isolatedServer(testInfo, edit, () => {}, ['--inputs-only'], env);
  const root = new URL('../../../', import.meta.url).pathname;
  const probe = createServer(); await new Promise(r => probe.listen(0, '127.0.0.1', r));
  const port = probe.address().port; await new Promise(r => probe.close(r));
  const child = spawn(`${root}target/release/atemporal-runner`, ['--controller', controller.url.replace('http:', 'ws:').replace(/\/$/, ''), '--port', String(port), '--guide-dir', `${controller.dir}/runner-guide`], {cwd: root, stdio: ['ignore', 'pipe', 'pipe'], env: {...process.env, ...env}});
  const stop = async () => {
    if (child.exitCode === null && child.signalCode === null) { const exited = once(child, 'exit'); child.kill('SIGTERM'); await exited; }
    await controller.stop();
  };
  try {
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error('Peripheral startup timeout')), 15000);
      child.stdout.on('data', d => { void appendFile(`${controller.dir}/runner.log`, d); if (String(d).includes('listening')) { clearTimeout(timeout); resolve(); } });
      child.stderr.on('data', d => void appendFile(`${controller.dir}/runner.log`, d));
      child.once('exit', c => { clearTimeout(timeout); reject(new Error(`Peripheral exited ${c}`)); });
    });
    return {url: `http://127.0.0.1:${port}/`, dir: controller.dir, stop};
  } catch (error) { await stop(); throw error; }
}
