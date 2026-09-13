// Gate 2 browser walkthrough against the real server: two players and a spectator play the
// opening with actual mouse/keyboard input, commit, seek non-sample ticks and play back.
// Run: ATEMPORAL_UI_SERVER_COMMAND='cargo run --release -q -p atemporal-server -- --replays target/ui-replays' npm run ui:review --prefix client -- --grep slice --project=desktop-chromium
import { test, expect } from './fixtures.mjs';
import { isolatedServer } from './game-helpers.mjs';

const state = page => page.evaluate(() => {
  const g = window.atemporal;
  return {
    current: g.current,
    playhead: Math.floor(g.playhead),
    round: g.round,
    committed: g.committed,
    exactTick: g.exact?.tick ?? null,
    exactRevision: g.exact?.revision ?? null,
    draft: g.draft.commands.length,
    draftTick: g.draft.tick,
    selection: g.selection.size,
    mode: g.mode.kind,
    toast: document.getElementById('toast').textContent,
    title: document.getElementById('draft-title').textContent,
    terminal: g.rev()?.outcome.terminal_state_tick ?? null,
    outcome: g.rev()?.outcome.kind ?? null,
    score: g.rev()?.score?.entities ?? null,
    entities: g.entities().map(e => ({ owner: e.owner, type: e.type_key, x: Math.round(e.x), y: Math.round(e.y), lifecycle: e.lifecycle })),
  };
});
// Pan to the tile first (as a player would with WASD/minimap) so it is not under a panel.
const screenOf = (page, tile) => page.evaluate(t => {
  const r = window.atemporal.renderer;
  r.camera.scale = 26;
  r.centerOn(t.x, t.y);
  const [x, y] = r.screen(t.x + 0.5, t.y + 0.5);
  return { x, y };
}, tile);
const findEntity = (page, owner, type) => page.evaluate(([o, t]) => {
  const e = window.atemporal.entities().find(e => e.owner === o && e.type_key === t);
  return e ? { x: Math.round(e.x), y: Math.round(e.y) } : null;
}, [owner, type]);
const oreArea = page => page.evaluate(() => {
  const g = window.atemporal;
  const w = g.terrain.width;
  const miner = g.entities().find(e => e.owner === g.player && e.type_key === 'miner');
  const tiles = g.initialOre.map((v, i) => [v, i]).filter(([v]) => v > 0).map(([, i]) => ({ x: i % w, y: Math.floor(i / w) })).filter(t => Math.hypot(t.x - miner.x, t.y - miner.y) < 12);
  return { min: { x: Math.min(...tiles.map(t => t.x)), y: Math.min(...tiles.map(t => t.y)) }, max: { x: Math.max(...tiles.map(t => t.x)), y: Math.max(...tiles.map(t => t.y)) } };
});
async function dragTiles(page, a, b) {
  const [p, q] = await page.evaluate(([a, b]) => {
    const r = window.atemporal.renderer;
    // Fit both endpoints between the floating replay/minimap controls before dragging.
    r.camera.scale = Math.min(r.camera.scale, (r.map.width-140)/(Math.abs(a.x-b.x)+4), (r.map.height-240)/(Math.abs(a.y-b.y)+4));
    r.centerOn((a.x+b.x)/2, (a.y+b.y)/2);
    return [a,b].map(t => {const [x,y] = r.screen(t.x+.5,t.y+.5); return {x,y};});
  }, [a,b]);
  await page.mouse.move(p.x, p.y);
  await page.mouse.down();
  await page.mouse.move(q.x, q.y, { steps: 5 });
  await page.mouse.up();
}
async function clickTile(page, tile) {
  const p = await screenOf(page, tile);
  await page.mouse.click(p.x, p.y);
}
async function waitRevision(page, revision) {
  await expect.poll(()=>page.evaluate(()=>!window.atemporal?.viewingPreview && window.atemporal?.current),{timeout:120000}).toBe(revision);
  await expect(page.locator('#game')).toHaveClass(/active/,{timeout:60000});
  await expect.poll(async () => (await state(page)).current, { timeout: 120000 }).toBe(revision);
  await expect.poll(async () => (await state(page)).exactRevision, { timeout: 30000 }).toBe(revision);
}

test('two players and a spectator play the opening, rewrite and replay', async ({ review }, testInfo) => {
  test.setTimeout(180000);
  const server = await isolatedServer(testInfo);
  review.afterClose(server.stop);
  const contexts = {};
  const pages = {};
  for (const name of ['player-a', 'player-b', 'spectator']) {
    contexts[name] = await review.newContext(name);
    pages[name] = await contexts[name].newPage();
    await pages[name].goto(server.url);
    await expect(pages[name].locator('#status')).toContainText(/slot/);
  }
  const a = pages['player-a'], b = pages['player-b'], s = pages['spectator'];
  await a.fill('#username', 'Ada');
  await a.locator('#roster li', { hasText: 'Slot 0' }).getByRole('button', { name: 'Claim' }).click();
  await expect(a.locator('#status')).toContainText('You hold slot 0');
  await b.fill('#username', 'Bo');
  await b.fill('#color', '#ff8a65');
  await b.locator('#roster li', { hasText: 'Slot 1' }).getByRole('button', { name: 'Claim' }).click();
  await expect(b.locator('#status')).toContainText('You hold slot 1');
  await expect(s.locator('#roster')).toContainText('Ada');
  await expect(s.locator('#roster')).toContainText('Bo');
  await review.capture('lobby-spectator-sees-both', s);
  await s.getByRole('button', { name: 'Spectate' }).click();
  await expect(a.getByRole('button', { name: 'Start match', exact: true })).toBeEnabled();
  await a.getByRole('button', { name: 'Start match', exact: true }).click();
  for (const page of [a, b, s]) await expect(page.locator('#game')).toHaveClass(/active/, { timeout: 60000 });
  await waitRevision(a, 0);
  await waitRevision(b, 0);
  await expect(a.locator('#top-phase')).toContainText('Round 1: planning');
  await review.capture('round1-planning-a', a);

  // Browser controls: group membership edits are staged, projected and reversible.
  const originalMiner = await findEntity(a, 0, 'miner');
  await clickTile(a, originalMiner);
  await a.keyboard.press('h'); await a.keyboard.press('3');
  await expect(a.locator('#draft-list')).toContainText('edit_group_members');
  await a.locator('#groups summary').click();
  await expect(a.locator('#group-list')).toContainText('3: 1 living');
  await clickTile(a, await findEntity(a,0,'constructor')); await a.keyboard.press('h'); await a.keyboard.press('3');
  await expect(a.locator('#group-list')).toContainText('3: 2 living');
  await a.getByRole('button',{name:'Clear group',exact:true}).click(); await a.keyboard.press('3');
  await expect(a.locator('#group-list button')).toHaveCount(0);
  expect(await a.evaluate(()=>window.atemporal.experience.groups().find(g=>g.id.slot===3)?.members)).toEqual([]);
  await a.keyboard.press('Control+z'); await a.keyboard.press('Control+z');
  await clickTile(a,originalMiner);
  await a.keyboard.press('Control+z');
  await a.keyboard.press('Shift+h'); await a.keyboard.press('3');
  await expect.poll(() => a.evaluate(() => window.atemporal.draft.commands.at(-1).command.edit.kind)).toBe('add');
  await a.keyboard.press('Control+z');
  await a.keyboard.press('p');await a.keyboard.press('1');
  await expect(a.locator('#draft-list')).toContainText('priority high');await a.keyboard.press('Control+z');
  await a.keyboard.press('3');
  await expect(a.locator('#recipient')).toContainText('Group 3');
  await a.keyboard.press('f'); await clickTile(a, originalMiner);
  await expect(a.locator('#draft-list')).toContainText('group 3: attack_move');
  await a.keyboard.press('Control+z');
  await clickTile(a, originalMiner);
  await expect(a.locator('#recipient')).toContainText('Selection');
  await a.locator('#groups summary').click();
  // Every stored-order kind uses the same action/target gestures (factory tests below).
  // Wall line placement uses connected axis steps, with visible validity feedback.
  const wallTiles = await a.evaluate(() => {
    const g = window.atemporal;
    for (let y=3;y<g.terrain.height-3;y++) for(let x=3;x<g.terrain.width-5;x++) {
      const tiles = [0,1,2].map(dx => ({x:x+dx,y}));
      if(tiles.every(t => g.validPlacement(t,false))) return tiles;
    }
  });
  await a.keyboard.press('b'); await a.keyboard.press('3');
  await dragTiles(a,wallTiles[0],wallTiles[2]);
  await expect.poll(() => a.evaluate(() => window.atemporal.draft.commands.at(-1)?.command.tiles?.length)).toBe(3);
  await review.capture('wall-line-draft',a);
  await a.keyboard.press('Control+z');
  await a.keyboard.press('Control+Shift+z');
  await a.locator('#draft-list li').last().click(); await a.keyboard.press('Delete');
  expect((await state(a)).draft).toBe(0);
  await a.keyboard.press('Escape');
  const diagonal = await a.evaluate(() => { const g=window.atemporal;for(let y=3;y<g.terrain.height-3;y++)for(let x=3;x<g.terrain.width-3;x++){const a={x,y},b={x:x+2,y:y+2};if(g.placementTiles(a,b).every(t=>g.validPlacement(t,false)))return [a,b];} });
  await a.keyboard.press('b');await a.keyboard.press('3');await dragTiles(a,diagonal[0],diagonal[1]);
  expect(await a.evaluate(() => window.atemporal.draft.commands.at(-1).command.tiles.length)).toBe(5);
  await review.capture('diagonal-connected-wall',a);await a.keyboard.press('Control+z');

  // Player A: mine with the miner, place a factory, construct it. Player B passes.
  const miner = await findEntity(a, 0, 'miner');
  await clickTile(a, miner);
  expect((await state(a)).selection).toBe(1);
  await a.keyboard.press('m');
  const area = await oreArea(a);
  await dragTiles(a, area.min, area.max);
  const constructor = await findEntity(a, 0, 'constructor');
  const factoryTile = { x: constructor.x + 2, y: constructor.y - 1 };
  await a.keyboard.press('b');
  await a.keyboard.press('1');
  await expect(a.locator('#mode')).toContainText('Place factory');
  const previewPoint = await screenOf(a,factoryTile);
  await a.mouse.move(previewPoint.x,previewPoint.y);
  await a.keyboard.press('r');
  await expect(a.locator('#mode')).toContainText('output');
  await review.capture('factory-output-preview',a);
  await clickTile(a, factoryTile);
  await clickTile(a, constructor);
  await a.keyboard.press('c');
  await dragTiles(a, factoryTile, factoryTile);
  await expect.poll(async () => (await state(a)).draft).toBe(3);
  await expect(a.locator('#draft-list')).toContainText('place factory');
  await review.capture('round1-draft-a', a);
  await a.keyboard.press('Enter');
  await expect(a.locator('#top-phase')).toContainText('committed', { timeout: 15000 });
  await b.keyboard.press('Enter');
  await waitRevision(a, 1);
  await waitRevision(s, 1);
  const rev1 = await state(a);
  expect(rev1.outcome).toBe('stalemate');
  expect(rev1.terminal).toBeGreaterThan(1000);

  // Seek to a non-sample tick; exact state must arrive and show the completed factory.
  await a.fill('#tick-input', '153');
  await a.locator('#tick-input').press('Enter');
  await expect.poll(async () => (await state(a)).exactTick, { timeout: 15000 }).toBe(153);
  const at153 = await state(a);
  expect(at153.entities.find(e => e.owner === 0 && e.type === 'factory')?.lifecycle).toBe('complete');
  await review.capture('round2-tick153-exact-a', a);

  // Timeline zoom around the cursor, ruler drag, keyboard pan and speed presets.
  const timeline = await a.locator('#timeline').boundingBox();
  await a.mouse.move(timeline.x+timeline.width*.6,timeline.y+20);
  await expect(a.locator('#timeline')).toHaveAttribute('title',/Tick.*Drag bottom ruler/);
  await a.mouse.wheel(0,-500);
  const zoomed = await a.evaluate(() => ({...window.atemporal.view}));
  expect(zoomed.t1-zoomed.t0).toBeLessThan(rev1.terminal);
  await a.mouse.move(timeline.x+timeline.width*.6,timeline.y+timeline.height-5);
  await a.mouse.down(); await a.mouse.move(timeline.x+timeline.width*.5,timeline.y+timeline.height-5,{steps:5}); await a.mouse.up();
  expect(await a.evaluate(() => window.atemporal.view.t0)).not.toBe(zoomed.t0);
  await a.keyboard.press('='); await a.keyboard.press('-');
  await a.keyboard.press('Shift+]'); await a.keyboard.press('Shift+[');
  await a.selectOption('#speed','8'); expect(await a.evaluate(() => window.atemporal.rate)).toBe(8);
  await a.selectOption('#speed','1');
  const mini = await a.locator('#minimap').boundingBox();
  await a.mouse.move(mini.x+mini.width*.3,mini.y+mini.height*.3);await a.mouse.down();
  await a.mouse.move(mini.x+mini.width*.7,mini.y+mini.height*.7,{steps:5});await a.mouse.up();
  await a.locator('#tick-input').focus(); await a.keyboard.press('v');
  await expect(a.locator('#statistics')).not.toHaveClass(/active/);
  await a.keyboard.press('Escape');
  await a.fill('#tick-input','153'); await a.locator('#tick-input').press('Enter'); await a.keyboard.press('Escape');
  await a.keyboard.press('v');
  await expect(a.locator('#statistics')).toHaveClass(/active/);
  for (const metric of ['bank','mined','spend','army','infrastructure','attrition','thinking']) {
    await a.selectOption('#metric',metric);
    await expect(a.locator('#graph-caption')).toContainText(metric);
    await review.capture(`graph-${metric}`,a);
  }
  await a.selectOption('#graph-player','1'); await a.selectOption('#graph-opponent','0'); await a.selectOption('#graph-window','visible');
  const graph = await a.locator('#graph').boundingBox(); await a.mouse.move(graph.x+graph.width*.5,graph.y+100);
  await expect(a.locator('#graph-hover')).toContainText('Round');
  await a.getByRole('button',{name:'Close statistics (V)'}).click();
  // Stored orders cover all five kinds. Each edit is undone after checking its payload.
  for (const [key,kind] of [['x','idle'],['g','support'],['m','mine'],['c','construct'],['f','attack_move']]) {
    await clickTile(a,factoryTile); await a.keyboard.press('r'); await a.keyboard.press(key);
    if(key === 'g') await clickTile(a,await findEntity(a,0,'miner'));
    if(key === 'f') await clickTile(a,factoryTile);
    if(key === 'm' || key === 'c') await dragTiles(a,factoryTile,factoryTile);
    await expect.poll(() => a.evaluate(() => window.atemporal.draft.commands.at(-1)?.command.order?.kind)).toBe(kind);
    expect(await a.evaluate(() => window.atemporal.draft.commands.at(-1).command.kind)).toBe('assign_order');
    await a.keyboard.press('Control+z');
  }
  await clickTile(a,factoryTile); await a.keyboard.press('j'); await a.keyboard.press('3');
  await expect(a.locator('#draft-list')).toContainText('bind_factory_group');
  await a.keyboard.press('Control+z');
  await a.keyboard.press('j'); await a.keyboard.press('Backspace');
  expect(await a.evaluate(() => window.atemporal.draft.commands.at(-1).command.group)).toBeNull();
  await a.keyboard.press('Control+z');

  // Round 2 at tick 153: queue looping grunts with a stored attack-move toward the enemy miner.
  await clickTile(a, factoryTile);
  await expect(a.locator('#selection-body')).toContainText('factory');
  await a.keyboard.press('q');
  await a.keyboard.press('4');
  await a.keyboard.press('l');
  await a.keyboard.press('r');
  await a.keyboard.press('f');
  const enemyMiner = await findEntity(s, 1, 'miner');
  await clickTile(a, enemyMiner);
  await expect.poll(async () => (await state(a)).draft).toBe(3);
  expect((await state(a)).draftTick).toBe(153);
  // Persist a factory binding and empty-group saved order through the real simulation.
  await a.keyboard.press('j'); await a.keyboard.press('3');
  await a.keyboard.press('3'); await a.keyboard.press('f'); await clickTile(a,enemyMiner);
  await clickTile(a,factoryTile); await a.keyboard.press('q'); await a.keyboard.press('4');
  await clickTile(a,factoryTile); await a.keyboard.press('h');await a.keyboard.press('4');
  await clickTile(a,await findEntity(a,0,'constructor'));await a.keyboard.press('Shift+h');await a.keyboard.press('4');await a.keyboard.press('x');
  await review.capture('group-binding-and-saved-order',a);
  await a.keyboard.press('Enter');
  await b.keyboard.press('Enter');
  await waitRevision(a, 2);
  const rev2 = await state(a);
  expect(rev2.terminal).toBeGreaterThan(153);
  await expect(a.locator('#result')).toContainText('Revision 2');

  // Playback from the draft tick: the playhead advances and the spectator can follow too.
  await a.keyboard.press('t');
  await a.keyboard.press(']');
  await a.keyboard.press(']');
  await a.keyboard.press(' ');
  await a.waitForTimeout(1500);
  const during = await state(a);
  expect(during.playhead).toBeGreaterThan(153);
  await review.capture('round3-playback-a', a);
  if (await a.evaluate(()=>window.atemporal.playing)) await a.keyboard.press(' ');
  await expect.poll(async () => { const s=await state(a);return s.exactTick === Math.floor(s.playhead); }, { timeout: 15000 }).toBe(true);
  expect(await a.evaluate(() => window.atemporal.exact.state.control_groups.find(g=>g.id.owner===0 && g.id.slot===3)?.members.length)).toBeGreaterThan(0);
  expect(await a.evaluate(() => window.atemporal.exact.state.control_groups.find(g=>g.id.owner===0 && g.id.slot===4)?.members.length)).toBe(2);
  await s.fill('#tick-input', String(rev2.terminal));
  await s.locator('#tick-input').press('Enter');
  await expect.poll(async () => (await state(s)).exactTick, { timeout: 15000 }).toBe(rev2.terminal);
  await review.capture('spectator-final-tick', s);

  // Inspect accepted inputs, authoritative outcomes and historical group state.
  await a.locator('#replay summary').click();
  await expect(a.locator('#accepted-inputs')).toContainText('bind_factory_group');
  await a.selectOption('#round-picker','1');
  await expect.poll(async () => (await state(a)).current).toBe(1);
  await expect(a.locator('#commit')).toBeDisabled();
  await review.capture('historical-round-viewer',a);
  await a.getByRole('button',{name:'Return to live'}).click();
  await expect.poll(async () => (await state(a)).current).toBe(2);
  await a.locator('#replay summary').click();
  // Select queue entries with actual focus and Delete, without committing those edits.
  await a.fill('#tick-input','154'); await a.locator('#tick-input').press('Enter');
  await expect.poll(async () => (await state(a)).exactTick).toBe(154);
  await clickTile(a,factoryTile);
  const active = a.getByRole('button',{name:/^Cancel active /});
  await expect(active).toHaveCount(1); await active.click();
  await expect(a.locator('#draft-list')).toContainText('cancel_active'); await a.keyboard.press('Control+z');
  await a.getByRole('button',{name:'Clear waiting queue',exact:true}).click();
  await expect(a.locator('#draft-list')).toContainText('replace_pending'); await a.keyboard.press('Control+z');
  const pending = a.getByRole('button',{name:/^Remove one queued /}).first();
  await pending.focus(); await pending.press('Enter');
  await expect(a.locator('#draft-list')).toContainText('remove_pending'); await a.keyboard.press('Control+z');
  await a.keyboard.press('Escape');
  // A same-instance reconnect also preserves historical inspection and a live draft.
  await clickTile(a,await findEntity(a,0,'constructor'));await a.keyboard.press('x');
  await a.locator('#replay summary').click();await a.selectOption('#round-picker','1');
  await expect.poll(async () => (await state(a)).current).toBe(1);
  await contexts['player-a'].setOffline(true);
  await expect(a.locator('#connection')).toContainText('Disconnected');
  await contexts['player-a'].setOffline(false);
  await expect(a.locator('#connection')).toBeHidden({timeout:15000});
  expect((await state(a)).current).toBe(1);expect((await state(a)).draft).toBe(1);
  await a.getByRole('button',{name:'Return to live'}).click();
  await expect.poll(async () => (await state(a)).current).toBe(2);
  await a.keyboard.press('Control+z');await a.locator('#replay summary').click();

  // Round 3 rewrites an earlier tick: the constructor idles at tick 40 with drop-all, so the
  // factory is never completed and the later round-2 commands become no-ops.
  await a.fill('#tick-input', '40');
  await a.locator('#tick-input').press('Enter');
  await expect.poll(async () => (await state(a)).exactTick, { timeout: 15000 }).toBe(40);
  await clickTile(a, await findEntity(a, 0, 'constructor'));
  await a.keyboard.press('o');
  expect(await a.evaluate(()=>window.atemporal.draft.policy)).toBe('drop_all');
  await a.keyboard.press('x');
  await expect.poll(async () => (await state(a)).draft).toBe(1);
  await expect(a.locator('#lock-preview')).toContainText('1 member deliveries');
  await clickTile(a,factoryTile);
  await a.getByRole('button',{name:'Cancel selected construction',exact:true}).click();
  await expect(a.locator('#draft-list')).toContainText('cancel_blueprints');
  await a.keyboard.press('Escape');
  await a.keyboard.press('Enter');
  await b.keyboard.press('Enter');
  await waitRevision(a, 3);
  await expect.poll(()=>a.evaluate(()=>JSON.parse(document.getElementById('timeline').dataset.latestTicks)[0])).toBe(40);
  expect(await a.evaluate(()=>JSON.parse(document.getElementById('timeline').dataset.actionTicks)[0])).toEqual([0,40,153]);
  await a.fill('#tick-input', '153');
  await a.locator('#tick-input').press('Enter');
  await expect.poll(async () => (await state(a)).exactTick, { timeout: 15000 }).toBe(153);
  const rewritten = await state(a);
  expect(rewritten.entities.find(e => e.owner === 0 && e.type === 'factory')?.lifecycle ?? 'absent').not.toBe('complete');
  await review.capture('round4-after-rewrite-a', a);
  await a.locator('#replay summary').click();
  await expect(a.locator('#accepted-inputs')).toContainText(/absent|incompatible/);
  await expect(a.locator('#rewrite-summary')).toContainText('Before → after');
  await review.capture('rewrite-command-skipped-reasons',a);
  await a.locator('#replay summary').click();
  await expect(a.locator('#accepted-inputs')).toContainText('locked_by_later_round');
  await a.keyboard.press('?');
  await review.capture('hotkey-help', a);
  await a.keyboard.press('Escape');
  await s.locator('#groups summary').click();
  await s.getByLabel('Inspect player groups').selectOption('1');
  await expect(s.locator('#group-list button')).toHaveCount(0);
  const scoreBeforeArchive = await a.locator('#top-score').textContent();
  await a.getByRole('button',{name:'Stop and archive unfinished',exact:true}).click();
  await expect(a.locator('#result')).toContainText('unfinished');
  await expect(a.locator('#commit')).toBeDisabled();
  await expect(a.locator('#top-sim')).not.toContainText('live planning');
  expect(await a.locator('#top-score').textContent()).toBe(scoreBeforeArchive);
  await review.capture('unfinished-archive',a);
});

// Isolated real server: timed boundaries and changed server-instance IDs cannot be tested by
// mutating browser state. This process is independent of the review runner's scoreboard match.
test('slice timed history, guide and server restart', async ({ review }, testInfo) => {
  test.setTimeout(90000);
  const { readFile, writeFile, mkdir } = await import('node:fs/promises');
  const { createServer } = await import('node:net');
  const { spawn } = await import('node:child_process');
  const { once } = await import('node:events');
  const root = new URL('../../../',import.meta.url).pathname;
  const config = JSON.parse(await readFile(`${root}config/game.yaml`,'utf8'));
  config.match_defaults.seed = 42;
  config.match_defaults.objective = {kind:'timed',lock_ticks_per_round:100};
  const dir = testInfo.outputPath('timed-server'); await mkdir(dir,{recursive:true});
  const configPath = `${dir}/game.json`; await writeFile(configPath,JSON.stringify(config));
  const probe = createServer(); await new Promise(resolve => probe.listen(0,'127.0.0.1',resolve));
  const port = probe.address().port; await new Promise(resolve => probe.close(resolve));
  let child;
  const start = async () => {
    child = spawn(`${root}target/release/atemporal-server`,['--port',String(port),'--config',configPath,'--replays',`${dir}/replays`],{cwd:root,stdio:['ignore','pipe','pipe']});
    await new Promise((resolve,reject) => {
      const timeout = setTimeout(() => reject(new Error('Timed server startup timeout')),10000);
      child.stdout.on('data',data => { if(String(data).includes('listening on')) {clearTimeout(timeout);resolve();} });
      child.once('exit',code => {clearTimeout(timeout);reject(new Error(`Timed server exited ${code}`));});
    });
  };
  const stop = async () => { const exited = once(child,'exit');child.kill('SIGTERM');await exited; };
  await start();
  try {
    const a = await (await review.newContext('timed-a')).newPage();
    const b = await (await review.newContext('timed-b')).newPage();
    for (const [page,name,slot] of [[a,'Timed A',0],[b,'Timed B',1]]) {
      await page.goto(`http://127.0.0.1:${port}/`); await expect(page.locator('#status')).toContainText('Claim a slot'); await page.fill('#username',name);
      if (slot === 1) await page.fill('#color','#ff8a65');
      await page.locator('#roster li',{hasText:`Slot ${slot}`}).getByRole('button',{name:'Claim'}).click();
      await expect(page.locator('#status')).toContainText(`You hold slot ${slot}`);
    }
    await a.fill('#username','Timer'); await a.locator('#color').focus();
    await expect(b.locator('#roster')).toContainText('Timer');
    await a.getByRole('button',{name:'Start match',exact:true}).click();
    await waitRevision(a,0); await waitRevision(b,0);
    await a.keyboard.press('Escape'); await a.keyboard.press('Enter'); await b.keyboard.press('Enter');
    await waitRevision(a,1);
    await expect.poll(() => a.evaluate(() => window.atemporal.editableFrom)).toBe(100);
    await a.locator('#tick-input').fill('40');await a.locator('#tick-input').press('Enter');
    await expect.poll(async () => (await state(a)).exactTick).toBe(40);
    await clickTile(a,await findEntity(a,0,'constructor')); await a.keyboard.press('x');
    expect((await state(a)).draft).toBe(0);
    await review.capture('timed-hatched-immutable-history',a);
    await a.keyboard.press('Home');
    await expect.poll(async () => (await state(a)).exactTick).toBe(100);
    await a.keyboard.press('x');
    await a.keyboard.press('Shift+.');
    await expect.poll(async () => (await state(a)).exactTick).toBe(200);
    await review.capture('draft-marker-versus-playhead',a);
    await a.getByText('More planning tools',{exact:true}).click();await a.getByRole('button',{name:'Rebase draft here'}).click(); expect((await state(a)).draftTick).toBe(200);
    await a.keyboard.press('Control+z'); expect((await state(a)).draftTick).toBe(100);
    await a.keyboard.press('t'); expect((await state(a)).playhead).toBe(100);
    // Advance twice from nonzero checkpoints; earlier authoritative outcomes must survive.
    await a.locator('#commit').click();
    await b.fill('#tick-input','100'); await b.locator('#tick-input').press('Enter'); await b.keyboard.press('Escape'); await b.keyboard.press('Enter');
    await waitRevision(a,2);
    await a.fill('#tick-input','200');await a.locator('#tick-input').press('Enter');
    await expect.poll(async () => (await state(a)).exactTick).toBe(200);
    await clickTile(a,await findEntity(a,0,'constructor')); await a.keyboard.press('x'); await a.keyboard.press('Enter');
    await b.fill('#tick-input','200');await b.locator('#tick-input').press('Enter');await b.keyboard.press('Escape');await b.keyboard.press('Enter');
    await waitRevision(a,3);
    await expect.poll(() => a.evaluate(() => window.atemporal.experience.rounds.get(3)?.command_outcomes.some(o => o.command_id.round === 2 && o.applied_entities.length === 1))).toBe(true);
    const popup = a.waitForEvent('popup'); await a.getByRole('link',{name:'Guide',exact:true}).click(); const guide = await popup;
    await expect(guide.getByRole('heading',{name:'Browser controls'})).toBeVisible();
    await review.capture('guide-actual-controls-desktop',guide);
    await guide.setViewportSize({width:390,height:844});
    await review.capture('guide-actual-controls-narrow',guide);
    await guide.getByRole('heading',{name:'Browser controls'}).scrollIntoViewIfNeeded();
    await guide.screenshot({path:testInfo.outputPath('guide-controls-narrow-viewport.png')});
    await guide.getByRole('heading',{name:'Graphs, rounds and reconnecting'}).scrollIntoViewIfNeeded();
    await guide.screenshot({path:testInfo.outputPath('guide-inspection-narrow-viewport.png')});
    // Keep the existing tabs open at the same address while replacing the actual server.
    await stop(); await start();
    await expect(a.locator('#connection')).toContainText('Server restarted',{timeout:15000});
    await review.capture('stale-server-instance',a);
    await a.getByRole('button',{name:'Reload current server'}).click();
    await expect(a.locator('#status')).toContainText('Claim a slot');
    await review.capture('fresh-lobby-after-server-restart',a);
    for (const page of [a,b,guide]) await page.goto('about:blank');
  } finally { if (child.exitCode === null) await stop(); }
});
