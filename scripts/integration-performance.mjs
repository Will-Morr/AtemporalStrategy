// Export-enabled real-server stress: two 1,000-unit queues, cap-length playback, repeated near-zero
// rewrites, process peak memory, cold exact seeks, and a rendered spectator trace/screenshot.
import { mkdir, readFile, writeFile, rm, readdir } from 'node:fs/promises';
import { cpus, totalmem, platform } from 'node:os';
import { root, startServer, Client, lobby, assert, now } from './match-harness.mjs';
import { chromium } from '../client/node_modules/@playwright/test/index.mjs';
const reviewOnly=process.argv.includes('--review-only');
const dir=`${root}target/integration-performance`;await mkdir(dir,{recursive:true});
const config=JSON.parse(await readFile(`${root}config/game.yaml`,'utf8'));
config.match_defaults.map_size=96;config.match_defaults.starting_matter=1020;config.match_defaults.stall_ticks=20000;
const content=JSON.parse(await readFile(`${root}config/content.yaml`,'utf8'));
for(const t of content.types){t.weapon=null;if(t.key==='grunt'||t.key==='factory')t.matter_cost=1;}
await writeFile(`${dir}/config.json`,JSON.stringify(config));await writeFile(`${dir}/content.json`,JSON.stringify(content));
if(!reviewOnly)await rm(`${dir}/dense`,{recursive:true,force:true});
const resumeArgs=reviewOnly?['--resume',(await readdir(`${dir}/dense/replays`))[0]]:[];
const server=await startServer(dir,'dense',['--config',`${dir}/config.json`,'--content',`${dir}/content.json`,'--memory-budget-mb','256','--results-budget-mb','512',...resumeArgs]);
const summary=reviewOnly?JSON.parse(await readFile(`${dir}/simulation-summary.json`,'utf8')):{machine:{cpu:cpus()[0].model,logical_cpus:cpus().length,ram_bytes:totalmem(),platform:platform()},config:config.match_defaults,content_changes:'weapons disabled; factory and grunt cost 1; two queues of 1000 grunts',rounds:[],seeks:[],peak_rss_kib:0};
if(reviewOnly){summary.simulation_peak_rss_kib=summary.peak_rss_kib;summary.peak_rss_kib=0;}
const memory=setInterval(async()=>{try{const text=await readFile(`/proc/${server.child.pid}/status`,'utf8');const peak=Number(text.match(/VmHWM:\s+(\d+)/)?.[1]??0);summary.peak_rss_kib=Math.max(summary.peak_rss_kib,peak);}catch{}},100);
let clients=[],browser;
try{
  if(!reviewOnly){
  const {a,b,s}=await lobby(server.url);clients=[a,b,s];
  const initial=(await s.request({kind:'get_exact_state',revision:0,tick:0},'exact_state',m=>m.tick===0)).snapshot;
  const constructors=[0,1].map(owner=>initial.entities.find(e=>e.owner===owner&&e.type_key==='constructor'));
  const factories=constructors.map(e=>({x:e.tile.x+ (e.owner===0?2:-2),y:e.tile.y+(e.owner===0?-1:1)}));
  const draft=(revision,tick,commands)=>({based_on_revision:revision,tick,commands:commands.map((command,index)=>({local_id:`c${index}`,command,future_orders:'keep'}))});
  const publish=async(revision,tick,commands)=>{
    const started=now();for(const [i,c]of[a,b].entries())assert((await c.sendAndWaitCommit(`r${revision+1}p${i}`,draft(revision,tick,commands[i]))).kind==='commit_accepted','stress commit accepted');
    const result=await s.wait('revision_published',m=>m.revision===revision+1,300000);summary.rounds.push({revision:revision+1,commit_to_publish_ms:Math.round(now()-started),terminal:result.outcome.terminal_state_tick});return result;
  };
  await publish(0,0,constructors.map((c,p)=>[
    {kind:'place_blueprints',type_key:'factory',tiles:[factories[p]],priority:'high',output_directions:[p===0?'e':'w']},
    {kind:'assign_order',entities:[c.id],order:{kind:'construct',area:{min:factories[p],max:factories[p]}}},
  ]));
  const state=(await s.request({kind:'get_exact_state',revision:1,tick:153},'exact_state',m=>m.tick===153)).snapshot;
  const ids=[0,1].map(p=>state.entities.find(e=>e.owner===p&&e.type_key==='factory').id);
  await publish(1,153,ids.map((id,p)=>[
    {kind:'assign_order',entities:[id],order:{kind:'attack_move',destination:p===0?{x:85,y:85}:{x:10,y:10}}},
    {kind:'edit_production',factories:[id],edit:{kind:'append',items:Array(1000).fill('grunt')}},
  ]));
  const final=(await s.request({kind:'get_exact_state',revision:2,tick:20000},'exact_state',m=>m.tick===20000)).snapshot;
  summary.entities=final.entities.length;summary.queued_remaining=final.entities.filter(e=>e.production).map(e=>({owner:e.owner,pending:e.production.pending_items.length,active:!!e.production.active_item}));assert(summary.entities>=1000,`dense population ${summary.entities}`);
  for(const tick of[19003,19997,12347]){const started=now();const result=await s.request({kind:'get_exact_state',revision:2,tick},'exact_state',m=>m.tick===tick);summary.seeks.push({tick,ms:Math.round(now()-started),entities:result.snapshot.entities.length});}
  for(let revision=2;revision<4;revision++)await publish(revision,0,constructors.map((c,p)=>[{kind:'assign_order',entities:[c.id],order:{kind:'construct',area:{min:factories[p],max:factories[p]}}}]));
  const archive=(await import('node:fs')).readdirSync(`${dir}/dense/replays`)[0];
  summary.measurements=(await readFile(`${dir}/dense/replays/${archive}/measurements.jsonl`,'utf8')).trim().split('\n').map(JSON.parse);
  await writeFile(`${dir}/simulation-summary.json`,JSON.stringify(summary,null,2));
  }
  browser=await chromium.launch({headless:true,args:['--no-sandbox']});const context=await browser.newContext({viewport:{width:1440,height:1000}});await context.tracing.start({screenshots:true,snapshots:true,sources:true});
  const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
  await page.goto(server.url.replace('ws:','http:').replace('/ws','/'));await page.waitForFunction(()=>window.atemporal?.current===4&&window.atemporal.exact?.revision===4,null,{timeout:60000});
  const seekStart=now();await page.fill('#tick-input','19003');await page.locator('#tick-input').press('Enter');await page.waitForFunction(()=>window.atemporal.exact?.tick===19003,null,{timeout:60000});summary.browser_seek_ms=Math.round(now()-seekStart);
  summary.frames=await page.evaluate(async()=>{const deltas=[];let previous=performance.now();for(let i=0;i<120;i++)await new Promise(resolve=>requestAnimationFrame(t=>{deltas.push(t-previous);previous=t;resolve();}));deltas.sort((a,b)=>a-b);return{median_ms:deltas[60],p95_ms:deltas[114],heap_bytes:performance.memory?.usedJSHeapSize};});
  await page.screenshot({path:`${dir}/dense-battlefield.png`});
  await page.locator('#tick-input').press('Escape');await page.locator('#play').click();
  summary.playback_frames=await page.evaluate(async()=>{const deltas=[];let previous=performance.now();const start=window.atemporal.playhead;for(let i=0;i<180;i++)await new Promise(resolve=>requestAnimationFrame(t=>{deltas.push(t-previous);previous=t;resolve();}));deltas.sort((a,b)=>a-b);return{median_ms:deltas[90],p95_ms:deltas[171],advanced_ticks:window.atemporal.playhead-start,heap_bytes:performance.memory?.usedJSHeapSize};});
  assert(summary.playback_frames.advanced_ticks>0,'dense playback advances');await page.locator('#play').click();await page.screenshot({path:`${dir}/dense-playback.png`});await page.close();
  const archiveId=(await readdir(`${dir}/dense/replays`))[0];const token=JSON.parse(await readFile(`${dir}/dense/replays/${archiveId}/lobby.json`,'utf8')).tokens[0];
  await context.addInitScript(token=>localStorage.setItem('atemporal-slot-token',token),token);
  const player=await context.newPage();player.on('pageerror',e=>errors.push(e.message));await player.goto(server.url.replace('ws:','http:').replace('/ws','/'));await player.waitForFunction(()=>window.atemporal?.exact?.revision===4,null,{timeout:60000});
  await player.fill('#tick-input','19003');await player.locator('#tick-input').press('Enter');await player.waitForFunction(()=>window.atemporal.exact?.tick===19003,null,{timeout:60000});await player.locator('#tick-input').press('Escape');await player.locator('#play').click();
  summary.player_playback_frames=await player.evaluate(async()=>{const deltas=[];let previous=performance.now();const start=window.atemporal.playhead;for(let i=0;i<180;i++)await new Promise(resolve=>requestAnimationFrame(t=>{deltas.push(t-previous);previous=t;resolve();}));deltas.sort((a,b)=>a-b);return{median_ms:deltas[90],p95_ms:deltas[171],advanced_ticks:window.atemporal.playhead-start,heap_bytes:performance.memory?.usedJSHeapSize};});
  assert(summary.player_playback_frames.advanced_ticks>0,'player playback advances');await player.locator('#play').click();await player.screenshot({path:`${dir}/dense-player.png`});
  await context.tracing.stop({path:`${dir}/trace.zip`});assert(errors.length===0,`browser errors: ${errors}`);
}finally{clearInterval(memory);if(browser)await browser.close();for(const c of clients)c.close();server.child.kill('SIGTERM');await server.exited;await writeFile(`${dir}/summary.json`,JSON.stringify(summary,null,2));await writeFile(`${dir}/server.log`,server.log.join(''));}
console.log(JSON.stringify(summary,null,2));
