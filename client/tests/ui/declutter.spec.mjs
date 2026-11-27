import {test,expect} from './fixtures.mjs';
import {isolatedServer,players,revision,tile,seek,area} from './game-helpers.mjs';

test('shared factory edits preserve unrelated planned and live settings', async ({review}, testInfo) => {
  test.setTimeout(120000);
  const server = await isolatedServer(testInfo, c => {
    c.match_defaults.max_tick = 400;
    c.match_defaults.starting_matter = 5000;
  });
  review.afterClose(server.stop);
  const [a, b] = await players(review, server.url);
  await a.getByRole('button', {name: 'Start match', exact: true}).click();
  await revision(a, 0);
  await revision(b, 0);
  const constructor = await a.evaluate(() => window.atemporal.entities().find(e => e.owner === 0 && e.type_key === 'constructor'));
  const initialTarget = {x: constructor.x + 2, y: constructor.y};
  const sharedTarget = {x: constructor.x + 4, y: constructor.y + 2};
  const sites = [];
  const priority = a.getByRole('combobox', {name: 'Selection priority'});
  const queue = type => a.getByRole('button', {name: `Queue ${type}`, exact: true});
  const editor = a.locator('.shared-factories');
  const setTarget = async target => {
    await a.keyboard.press('f');
    await tile(a, target);
  };
  const selectBoth = async () => {
    await tile(a, sites[0]);
    const point = await a.evaluate(t => window.atemporal.renderer.screen(t.x + .5, t.y + .5), sites[1]);
    await a.keyboard.down('Shift');
    await a.mouse.click(...point);
    await a.keyboard.up('Shift');
  };
  const planned = () => a.evaluate(() => window.atemporal.selectedViews().map(v => v.settings));
  const published = () => a.evaluate(() => window.atemporal.selectedViews().map(v => ({
    lifecycle: v.lifecycle,
    priority: v.exact.priority,
    production: v.exact.production,
  })));
  const publish = async (nextRevision, tick) => {
    await a.locator('#commit').click();
    await b.locator('#commit').click();
    await revision(a, nextRevision);
    await seek(a, tick);
    await selectBoth();
  };

  for (let i = 0; i < 2; i++) {
    await tile(a, constructor);
    const site = await a.evaluate(() => {
      const g = window.atemporal, c = g.selectedViews()[0];
      for (let y = c.y - 3; y <= c.y + 3; y++) {
        for (let x = c.x - 3; x <= c.x + 3; x++) {
          if (g.validPlacement({x, y}, true)) return {x, y};
        }
      }
      throw new Error('No valid factory site near the opening constructor');
    });
    sites.push(site);
    await a.keyboard.press('b');
    await a.locator('#mode-options button').filter({hasText: 'factory'}).click();
    await tile(a, site);
    await tile(a, site);
    if (i === 0) {
      await a.keyboard.press('l');
      await priority.selectOption('high');
      await setTarget(initialTarget);
    }
    await queue(i === 0 ? 'miner' : 'grunt').click();
  }
  await selectBoth();
  const blueprints = await planned();
  expect(blueprints).toMatchObject([
    {queue: ['miner'], queue_loop_flags: [true], loop_enabled: true, priority: 'high', order: {kind: 'attack_move', destination: initialTarget}},
    {queue: ['grunt'], queue_loop_flags: [false], loop_enabled: false, priority: 'medium', order: {kind: 'idle'}},
  ]);
  await expect(editor).toContainText('Queues differ across factories');
  await expect(editor).not.toContainText('clears all selected factories');
  await review.capture('mixed-planned-factories', a);

  await priority.selectOption('low');
  const lowPriority = blueprints.map(settings => ({...settings, priority: 'low'}));
  expect(await planned()).toEqual(lowPriority);
  await setTarget(sharedTarget);
  const newTarget = lowPriority.map(settings => ({...settings, order: {kind: 'attack_move', destination: sharedTarget}}));
  expect(await planned()).toEqual(newTarget);

  await a.keyboard.press('q');
  await a.locator('#mode-options button').filter({hasText: 'scout'}).click();
  expect(await planned()).toEqual(newTarget.map(settings => ({
    ...settings,
    queue: [...settings.queue, 'scout'],
    queue_loop_flags: [...settings.queue_loop_flags, settings.loop_enabled],
  })));
  await a.keyboard.press('Control+z');
  expect(await planned()).toEqual(newTarget);
  await a.getByRole('button', {name: '↻ Loop new items: Mixed', exact: true}).click();
  expect(await planned()).toEqual(newTarget.map(settings => ({...settings, loop_enabled: true})));
  await a.keyboard.press('Control+z');
  expect(await planned()).toEqual(newTarget);

  await a.getByRole('button', {name: 'Clear all waiting queues', exact: true}).click();
  expect(await planned()).toEqual(newTarget.map(settings => ({...settings, queue: [], queue_loop_flags: []})));
  await expect(editor).toContainText('Aligned queues');
  await review.capture('shared-empty-queues-with-different-loop-defaults', a);
  await a.keyboard.press('Control+z');
  expect(await planned()).toEqual(newTarget);
  // Start construction with empty queues, then create controlled active and waiting items.
  await a.getByRole('button', {name: 'Clear all waiting queues', exact: true}).click();
  await tile(a, constructor);
  await a.keyboard.press('c');
  const [from, to] = await a.evaluate(([first, second]) => {
    const renderer = window.atemporal.renderer;
    return [renderer.screen(first.x - 1, first.y - 1), renderer.screen(second.x + 2, second.y + 2)];
  }, sites);
  await a.mouse.move(...from);
  await a.mouse.down();
  await a.mouse.move(...to);
  await a.mouse.up();
  await publish(1, 150);
  expect((await published()).map(v => v.lifecycle)).toEqual(['complete', 'complete']);

  await queue('tank').click();
  for (let i = 0; i < sites.length; i++) {
    await tile(a, sites[i]);
    await a.keyboard.press('j');
    await a.keyboard.press(String(i + 1));
    await queue(i === 0 ? 'miner' : 'grunt').click();
  }
  await publish(2, 152);
  const live = await published();
  expect(live).toMatchObject([
    {production: {active_item: {type_key: 'tank', loop_enabled: true}, pending_items: [{type_key: 'miner'}], spawn_group: {slot: 1}}},
    {production: {active_item: {type_key: 'tank', loop_enabled: false}, pending_items: [{type_key: 'grunt'}], spawn_group: {slot: 2}}},
  ]);
  for (const {production} of live) expect(production.active_item.paid_matter).toBeGreaterThan(0);
  await priority.selectOption('off');
  await setTarget(initialTarget);
  expect(await a.evaluate(() => window.atemporal.draft.commands.map(d => d.command.kind)))
    .toEqual(['set_priority', 'assign_order']);
  await noPanelScroll(a);
  await review.capture('mixed-active-factories-with-shared-priority-and-target', a);
  await publish(3, 153);
  expect(await published()).toEqual(live.map(v => ({
    ...v,
    priority: 'off',
    production: {...v.production, stored_order: {kind: 'attack_move', destination: initialTarget}},
  })));
});

async function noPanelScroll(page){
  expect(await page.evaluate(()=>Array.from(document.querySelectorAll('#panels>div')).filter(e=>e.scrollHeight>e.clientHeight+2||e.scrollWidth>e.clientWidth+2).map(e=>({id:e.id,height:[e.scrollHeight,e.clientHeight],width:[e.scrollWidth,e.clientWidth]})))).toEqual([]);
}

test('order warnings, committed banner, selection markers and unit reference fit desktop windows',async({review},testInfo)=>{
  test.setTimeout(120000);
  const server=await isolatedServer(testInfo,c=>{c.match_defaults.max_tick=300;});review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
  const units=await a.evaluate(()=>window.atemporal.entities().filter(e=>e.owner===0));const constructor=units.find(e=>e.type_key==='constructor');
  await expect(a.locator('#order-warnings')).toBeHidden();
  for(const viewport of [{width:1920,height:1080},{width:1366,height:768}]){
    await a.setViewportSize(viewport);await tile(a,constructor);await noPanelScroll(a);
    expect(await a.locator('#panels').evaluate(e=>e.getBoundingClientRect().height)).toBeLessThanOrEqual(225);
    await review.capture(`constructor-${viewport.width}`,a);
    await a.locator('.selection-info>summary').click();await expect(a.locator('#unit-stats')).toBeVisible();await review.capture(`unit-info-${viewport.width}`,a);await a.locator('.selection-info>summary').click();
  }
  await a.keyboard.press('b');await a.locator('#mode-options button').filter({hasText:'factory'}).click();
  const site=await a.evaluate(()=>{const g=window.atemporal,c=g.selectedViews()[0];for(let y=c.y-2;y<=c.y+2;y++)for(let x=c.x-2;x<=c.x+2;x++)if(g.validPlacement({x,y},true))return{x,y};});
  await tile(a,site);await tile(a,site);await a.getByRole('button',{name:'Queue miner',exact:true}).click();await a.getByRole('button',{name:'Queue grunt',exact:true}).click();
  await expect(a.locator('#order-warnings')).toHaveAttribute('title',/will start idle/);await expect(a.locator('#order-warnings')).toHaveAttribute('title',/no constructor assigned/);
  await expect(a.locator('#order-warnings')).toBeVisible();
  await seek(a,1);await expect(a.locator('#order-warnings')).toBeHidden();
  await seek(a,0);await expect(a.locator('#order-warnings')).toBeVisible();
  await a.keyboard.press('m');await area(a,site);await expect(a.locator('#order-warnings')).not.toHaveAttribute('title',/will start idle|cannot follow/);
  await expect(a.locator('#timeline')).toHaveAttribute('data-order-ticks','0');await noPanelScroll(a);await review.capture('factory-no-scroll-1366',a);
  await tile(a,constructor);await a.keyboard.press('c');await area(a,site);await expect(a.locator('#order-warnings')).not.toHaveAttribute('title',/no constructor assigned/);
  await tile(a,units.find(e=>e.type_key==='miner'));await a.keyboard.press('m');await area(a,site);
  await expect(a.locator('#order-warnings')).toBeHidden();await tile(a,site);await review.capture('mixed-production-mining-without-warning',a);await tile(a,constructor);
  await a.locator('#actions .planning>summary').click();await review.capture('planning-menu-1366',a);
  expect(await a.locator('#actions .planning').evaluate(e=>e.getBoundingClientRect().top>=0)).toBe(true);
  await a.locator('#actions .planning>summary').click();
  await a.keyboard.press('?');expect(await a.locator('#help').evaluate(e=>e.getBoundingClientRect().bottom<=innerHeight)).toBe(true);await review.capture('hotkeys-no-scroll-1366',a);await a.keyboard.press('Escape');
  const popup=a.waitForEvent('popup');await a.locator('#unit-reference').click();const reference=await popup;await reference.waitForLoadState();await expect(reference.locator('#units')).toContainText('Unit stats');await expect(reference.locator('body')).toContainText('miner');
  await a.locator('#commit').click();await expect(a.locator('#turn-banner')).toBeVisible();await review.capture('committed-waiting-1366',a);
  await a.locator('#commit').click();await expect(a.locator('#turn-banner')).toBeHidden();
  await a.locator('#commit').click();await expect(a.locator('#turn-banner')).toBeVisible();await b.locator('#commit').click();await revision(a,1);await expect(a.locator('#turn-banner')).toBeHidden();await expect(a.locator('#order-warnings')).toBeHidden();
  await seek(a,0);await tile(a,constructor);await expect(a.locator('#timeline')).toHaveAttribute('data-order-ticks','0');await review.capture('published-selection-markers-1366',a);
  await a.keyboard.press('Escape');await expect(a.locator('#timeline')).toHaveAttribute('data-order-ticks','');
});

test('structure planning is global and unsupported selection actions stay unavailable',async({review},testInfo)=>{
  const server=await isolatedServer(testInfo,c=>{c.match_defaults.max_tick=300;});review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
  const constructor=await a.evaluate(()=>window.atemporal.entities().find(e=>e.owner===0&&e.type_key==='constructor'));
  await expect(a.locator('#actions [data-key="b"]')).toBeVisible();
  await expect(a.locator('#actions [data-key="m"]')).toBeHidden();
  await a.getByRole('button',{name:'B Plan structures',exact:true}).click();
  await a.locator('#mode-options button').filter({hasText:'turret'}).click();
  const site=await a.evaluate(c=>{const g=window.atemporal;for(let y=c.y-3;y<=c.y+3;y++)for(let x=c.x-3;x<=c.x+3;x++)if(g.validPlacement({x,y},false))return{x,y};},constructor);
  await tile(a,site);await tile(a,site);
  for(const key of ['f','g','m','c','x']){
    await expect(a.locator(`#actions [data-key="${key}"]`)).toBeHidden();
    await a.keyboard.press(key);expect(await a.evaluate(()=>window.atemporal.mode.kind)).toBe('none');
  }
  expect(await a.evaluate(()=>window.atemporal.draft.commands.map(c=>c.command.kind))).toEqual(['place_blueprints']);
  await review.capture('structure-planned-without-constructor-selection',a);
  const miner=await a.evaluate(()=>window.atemporal.entities().find(e=>e.owner===0&&e.type_key==='miner'));
  await tile(a,miner);await expect(a.locator('#actions [data-key="m"]')).toBeVisible();await expect(a.locator('#actions [data-key="c"]')).toBeHidden();await expect(a.locator('#actions [data-key="b"]')).toBeVisible();
  await a.keyboard.press('c');expect(await a.evaluate(()=>window.atemporal.mode.kind)).toBe('none');
  await tile(a,constructor);await expect(a.locator('#actions [data-key="c"]')).toBeVisible();
  await a.keyboard.press('c');await area(a,site);
  expect(await a.evaluate(()=>window.atemporal.draft.commands.some(c=>c.command.kind==='assign_order'&&c.command.order.kind==='construct'))).toBe(true);
  await noPanelScroll(a);await review.capture('constructor-work-assigned-separately',a);
});
