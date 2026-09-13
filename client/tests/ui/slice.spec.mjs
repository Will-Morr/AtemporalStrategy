// Gate 2 browser walkthrough against the real server: two players and a spectator play the
// opening with actual mouse/keyboard input, commit, seek non-sample ticks and play back.
// Run: ATEMPORAL_UI_SERVER_COMMAND='cargo run --release -q -p atemporal-server -- --replays target/ui-replays' npm run ui:review --prefix client -- --grep slice --project=desktop-chromium
import { test, expect } from './fixtures.mjs';

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
  const [x0, y0] = r.screen(t.x + 0.5, t.y + 0.5);
  if (y0 > window.innerHeight - 260 || y0 < 50 || x0 < 0 || x0 > window.innerWidth) r.centerOn(t.x, t.y);
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
  const p = await screenOf(page, a), q = await screenOf(page, b);
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
  await expect.poll(async () => (await state(page)).current, { timeout: 120000 }).toBe(revision);
  await expect.poll(async () => (await state(page)).exactRevision, { timeout: 30000 }).toBe(revision);
}

test('two players and a spectator play the opening, rewrite and replay', async ({ review }) => {
  test.setTimeout(400000);
  const contexts = {};
  const pages = {};
  for (const name of ['player-a', 'player-b', 'spectator']) {
    contexts[name] = await review.newContext(name);
    pages[name] = await contexts[name].newPage();
    await pages[name].goto('/');
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

  // Round 2 at tick 153: queue looping grunts with a stored attack-move toward the enemy miner.
  await clickTile(a, factoryTile);
  await expect(a.locator('#selection-body')).toContainText('factory');
  await a.keyboard.press('q');
  await a.keyboard.press('4');
  await a.keyboard.press('l');
  await a.keyboard.press('r');
  await a.keyboard.press('f');
  const enemyMiner = await findEntity(a, 1, 'miner');
  await clickTile(a, enemyMiner);
  await expect.poll(async () => (await state(a)).draft).toBe(3);
  expect((await state(a)).draftTick).toBe(153);
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
  await a.keyboard.press(' ');
  await expect.poll(async () => (await state(a)).exactTick, { timeout: 15000 }).toBe((await state(a)).playhead);
  await s.fill('#tick-input', String(rev2.terminal));
  await s.locator('#tick-input').press('Enter');
  await expect.poll(async () => (await state(s)).exactTick, { timeout: 15000 }).toBe(rev2.terminal);
  await review.capture('spectator-final-tick', s);

  // Round 3 rewrites an earlier tick: the constructor idles at tick 40 with drop-all, so the
  // factory is never completed and the later round-2 commands become no-ops.
  await a.fill('#tick-input', '40');
  await a.locator('#tick-input').press('Enter');
  await expect.poll(async () => (await state(a)).exactTick, { timeout: 15000 }).toBe(40);
  await clickTile(a, await findEntity(a, 0, 'constructor'));
  await a.keyboard.press('o');
  await expect(a.locator('#draft-title')).toContainText('drop_all');
  await a.keyboard.press('x');
  await expect.poll(async () => (await state(a)).draft).toBe(1);
  await a.keyboard.press('Enter');
  await b.keyboard.press('Enter');
  await waitRevision(a, 3);
  await a.fill('#tick-input', '153');
  await a.locator('#tick-input').press('Enter');
  await expect.poll(async () => (await state(a)).exactTick, { timeout: 15000 }).toBe(153);
  const rewritten = await state(a);
  expect(rewritten.entities.find(e => e.owner === 0 && e.type === 'factory')?.lifecycle ?? 'absent').not.toBe('complete');
  await review.capture('round4-after-rewrite-a', a);
  await a.keyboard.press('?');
  await review.capture('hotkey-help', a);
});
