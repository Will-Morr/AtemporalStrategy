import { test, expect } from './fixtures.mjs';
import { isolatedServer, isolatedPeripheral, revision, tile } from './game-helpers.mjs';

test('same-browser players refresh independently and uncommit restores an editable plan', async ({review}, testInfo) => {
  test.setTimeout(120000);
  const server = await (process.env.ATEMPORAL_UI_PERIPHERAL ? isolatedPeripheral : isolatedServer)(testInfo);
  review.afterClose(server.stop);
  const context = await review.newContext('shared-browser');
  const pages = [];
  for (let slot = 0; slot < 2; slot++) {
    const p = await context.newPage(); await p.goto(server.url);
    await p.fill('#username', `Window ${slot}`);
    await p.locator('#roster li', {hasText:`Slot ${slot}`}).getByRole('button', {name:'Claim',exact:true}).click();
    await expect(p.locator('#status')).toContainText(`You hold slot ${slot}`);
    pages.push(p);
  }
  const [a,b] = pages;
  await a.getByRole('button',{name:'Start match',exact:true}).click();
  await revision(a,0); await revision(b,0);
  for (let slot = 0; slot < 2; slot++) {
    await pages[slot].reload(); await revision(pages[slot],0);
    expect(await pages[slot].evaluate(()=>window.atemporal.player)).toBe(slot);
  }
  const start = await a.evaluate(()=>{const g=window.atemporal;const e=g.entities().find(e=>e.type_key==='constructor'&&e.owner===g.player);return {x:e.x,y:e.y};});
  await tile(a,start);
  await expect(a.locator('#selection-icons svg')).toBeVisible();
  await expect(a.locator('#selection-body')).toContainText('Vision');
  await a.keyboard.press('m'); await tile(a,{x:start.x+2,y:start.y});
  const original = await a.evaluate(()=>window.atemporal.draft.commands);
  expect(original).toHaveLength(1);
  await a.locator('#commit').click();
  await expect(a.locator('#commit')).toHaveText('Uncommit · edit my moves');
  await a.reload(); await revision(a,0);
  await a.locator('#commit').click();
  await expect.poll(()=>a.evaluate(()=>window.atemporal.draft.commands)).toEqual(original);
  await expect(b.locator('#top-phase')).toContainText('planning');
  await review.capture('uncommitted-plan-restored',a);
  await a.keyboard.press('Control+z');
  expect(await a.evaluate(()=>window.atemporal.draft.commands)).toEqual([]);
  await a.keyboard.press('Control+u');
  expect(await a.evaluate(()=>window.atemporal.draft.commands)).toEqual(original);
  await a.locator('#commit').click(); await b.locator('#commit').click();
  await revision(a,1); await revision(b,1);
  await expect(a.locator('#commit')).not.toHaveText('Uncommit · edit my moves');
  const chooser = await context.newPage(); await chooser.goto(server.url);
  await expect(chooser.getByRole('button',{name:'Rejoin Window 0',exact:true})).toBeVisible();
  await expect(chooser.getByRole('button',{name:'Rejoin Window 1',exact:true})).toBeVisible();
  await review.capture('explicit-rejoin-player-choice',chooser);
  await chooser.getByRole('button',{name:'Rejoin Window 1',exact:true}).click();
  await revision(chooser,1);
  expect(await chooser.evaluate(()=>window.atemporal.player)).toBe(1);
});
