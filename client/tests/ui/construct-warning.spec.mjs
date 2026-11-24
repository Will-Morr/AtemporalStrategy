import {test, expect} from './fixtures.mjs';
import {isolatedServer, players, revision, tile, seek, area} from './game-helpers.mjs';

test('opening construction warning stays visible across 39 blueprints and a mining constructor', async ({review}, testInfo) => {
  const server = await isolatedServer(testInfo, config => { config.match_defaults.max_tick = 300; });
  review.afterClose(server.stop);
  const [page] = await players(review, server.url);
  await page.getByRole('button', {name:'Start match', exact:true}).click();
  await revision(page, 0);
  const warnings = page.locator('#order-warnings');
  await expect(warnings).toBeHidden();
  const {constructor, miner, sites} = await page.evaluate(() => {
    const g = window.atemporal;
    const owned = g.entities().filter(e => e.owner === 0);
    const constructor = owned.find(e => e.type_key === 'constructor');
    const miner = owned.find(e => e.type_key === 'miner');
    const sites = [];
    for (let y=1; y<16; y++) for (let x=1; x<16; x++) {
      if (!g.placementIssue('turret', [{x,y}])) sites.push({x,y});
    }
    return {constructor, miner, sites:sites.slice(0,39)};
  });
  expect(sites).toHaveLength(39);
  await page.keyboard.press('b');
  await page.locator('#mode-options button').filter({hasText:'turret'}).click();
  await page.keyboard.down('Shift');
  for (const site of sites) await tile(page,site);
  await page.keyboard.up('Shift');
  await page.keyboard.press('Escape');
  expect(await page.evaluate(() => window.atemporal.draft.commands.filter(d => d.command.kind === 'place_blueprints').length)).toBe(39);
  await expect(warnings).toContainText('No active Construct order covers 39 planned buildings at this tick');
  await review.capture('opening-construct-warning-idle-constructor',page);

  await tile(page,constructor);
  await page.keyboard.press('m');
  await area(page,sites[0]);
  expect(await page.evaluate(() => { const g=window.atemporal; return g.effectiveOrder(g.selectedViews()[0]).kind; })).toBe('mine');
  await tile(page,miner);
  await expect(warnings).toContainText('No active Construct order covers 39 planned buildings at this tick');
  await review.capture('opening-construct-warning-mining-constructor',page);
  await seek(page,1);
  await expect(warnings).toBeHidden();
  await seek(page,0);
  await expect(warnings).toContainText('No active Construct order covers 39 planned buildings');

  await tile(page,constructor);
  await page.keyboard.press('c');
  const [first, last] = await page.evaluate(sites => {
    const r = window.atemporal.renderer;
    const x = sites.map(t => t.x), y = sites.map(t => t.y);
    const min = {x:Math.min(...x)+.5,y:Math.min(...y)+.5};
    const max = {x:Math.max(...x)+.5,y:Math.max(...y)+.5};
    r.centerOn((min.x+max.x)/2,(min.y+max.y)/2);
    return [r.screen(min.x,min.y),r.screen(max.x,max.y)];
  }, sites);
  await page.mouse.move(...first);
  await page.mouse.down();
  await page.mouse.move(...last);
  await page.mouse.up();
  await expect(warnings).not.toContainText('No active Construct order covers');
  await expect(warnings).not.toHaveAttribute('title', /no constructor assigned/);
  await expect(warnings).toContainText('miner has no orders');
  await review.capture('opening-construct-warning-clears-with-covering-area',page);
});
