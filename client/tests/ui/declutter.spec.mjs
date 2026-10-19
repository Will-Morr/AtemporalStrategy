import {test,expect} from './fixtures.mjs';
import {isolatedServer,players,revision,tile,seek,area} from './game-helpers.mjs';

test('shared factory editing uses inline warning and preserves aligned additions',async({review},testInfo)=>{
  test.setTimeout(120000);
  const server=await isolatedServer(testInfo,c=>{c.match_defaults.max_tick=400;c.match_defaults.starting_matter=5000;});review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
  const constructor=await a.evaluate(()=>window.atemporal.entities().find(e=>e.owner===0&&e.type_key==='constructor'));
  const sites=[];
  for(let i=0;i<2;i++){
    await tile(a,constructor);
    const site=await a.evaluate(()=>{const g=window.atemporal,c=g.selectedViews()[0];for(let y=c.y-3;y<=c.y+3;y++)for(let x=c.x-3;x<=c.x+3;x++)if(g.validPlacement({x,y},true))return{x,y};});sites.push(site);
    await a.keyboard.press('b');await a.locator('#mode-options button').filter({hasText:'factory'}).click();await tile(a,site);await tile(a,site);
    if(i===0)await a.getByRole('button',{name:'Queue miner',exact:true}).click();
  }
  const selectBoth=async()=>{await tile(a,sites[0]);const point=await a.evaluate(t=>window.atemporal.renderer.screen(t.x+.5,t.y+.5),sites[1]);await a.keyboard.down('Shift');await a.mouse.click(...point);await a.keyboard.up('Shift');};
  await selectBoth();await expect(a.locator('.shared-factories')).toContainText('Different configurations');await expect(a.getByRole('button',{name:'Queue miner',exact:true})).toHaveCount(1);
  await review.capture('mixed-factory-override-warning',a);
  const before=await a.evaluate(()=>window.atemporal.draft.commands);
  a.on('dialog',async d=>{await d.dismiss();throw new Error('Factory edits must not open browser dialogs');});await a.getByRole('button',{name:'Queue scout',exact:true}).click();
  expect(await a.evaluate(()=>window.atemporal.selectedViews().map(v=>v.settings.queue))).toEqual([['scout'],['scout']]);
  await a.getByRole('button',{name:'Queue miner',exact:true}).click({modifiers:['Shift']});
  expect(await a.evaluate(()=>window.atemporal.selectedViews().map(v=>v.settings.queue))).toEqual(Array(2).fill(['scout',...Array(5).fill('miner')]));
  await a.keyboard.press('Control+z');expect(await a.evaluate(()=>window.atemporal.selectedViews().map(v=>v.settings.queue))).toEqual([['scout'],['scout']]);
  await a.keyboard.press('Control+z');expect(await a.evaluate(()=>window.atemporal.draft.commands)).toEqual(before);
  await a.keyboard.press('q');await a.locator('#mode-options button').filter({hasText:'scout'}).click();
  await a.keyboard.press('m');await area(a,{x:constructor.x,y:constructor.y});
  await review.capture('shared-aligned-factory-editor',a);
  await tile(a,constructor);await a.keyboard.press('c');const from=await a.evaluate(t=>window.atemporal.renderer.screen(t.x-1,t.y-1),sites[0]);const to=await a.evaluate(t=>window.atemporal.renderer.screen(t.x+2,t.y+2),sites[1]);await a.mouse.move(...from);await a.mouse.down();await a.mouse.move(...to);await a.mouse.up();
  await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,1);await seek(a,1);await selectBoth();
  expect(await a.evaluate(()=>window.atemporal.selectedViews().map(v=>v.settings?.queue??v.exact.production.pending_items.map(i=>i.type_key)))).toEqual([['scout'],['scout']]);
  await a.getByRole('button',{name:'Queue miner',exact:true}).click();await expect(a.locator('.shared-factories')).toContainText('Aligned queues');
  await a.getByRole('button',{name:'Remove shared item 2',exact:true}).click();
  await review.capture('shared-live-factory-editor',a);
  await a.keyboard.press('Control+z');await a.keyboard.press('Control+z');await seek(a,150);await selectBoth();
  expect(await a.evaluate(()=>window.atemporal.selectedViews().map(v=>v.lifecycle))).toEqual(['complete','complete']);
  await a.getByRole('button',{name:'Queue tank',exact:true}).click();await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,2);await seek(a,152);await tile(a,sites[0]);
  await a.getByRole('button',{name:'Queue miner',exact:true}).click();await selectBoth();
  await a.getByRole('combobox',{name:'Selection priority'}).selectOption('off');
  expect(await a.evaluate(()=>window.atemporal.draft.commands.filter(d=>d.command.kind==='edit_production'&&d.command.edit.kind==='cancel_active').length)).toBe(2);
  await noPanelScroll(a);await review.capture('active-factories-overridden-together',a);
  await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,3);await seek(a,153);await selectBoth();
  expect(await a.evaluate(()=>window.atemporal.selectedViews().map(v=>({priority:v.exact.priority,pending:v.exact.production.pending_items.length,active:v.exact.production.active_item??null})))).toEqual(Array(2).fill({priority:'off',pending:0,active:null}));
});

async function noPanelScroll(page){
  expect(await page.evaluate(()=>Array.from(document.querySelectorAll('#panels>div')).filter(e=>e.scrollHeight>e.clientHeight+2||e.scrollWidth>e.clientWidth+2).map(e=>({id:e.id,height:[e.scrollHeight,e.clientHeight],width:[e.scrollWidth,e.clientWidth]})))).toEqual([]);
}

test('order warnings, committed banner, selection markers and unit reference fit desktop windows',async({review},testInfo)=>{
  test.setTimeout(120000);
  const server=await isolatedServer(testInfo,c=>{c.match_defaults.max_tick=300;});review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
  const units=await a.evaluate(()=>window.atemporal.entities().filter(e=>e.owner===0));const constructor=units.find(e=>e.type_key==='constructor');
  await expect(a.locator('#order-warnings')).toBeVisible();await expect(a.locator('#order-warnings')).toHaveAttribute('title',/miner has no orders/);
  for(const viewport of [{width:1920,height:1080},{width:1366,height:768}]){
    await a.setViewportSize(viewport);await tile(a,constructor);await noPanelScroll(a);
    expect(await a.locator('#panels').evaluate(e=>e.getBoundingClientRect().height)).toBeLessThanOrEqual(225);
    await review.capture(`constructor-${viewport.width}`,a);
    await a.locator('.selection-info>summary').click();await expect(a.locator('#unit-stats')).toBeVisible();await review.capture(`unit-info-${viewport.width}`,a);await a.locator('.selection-info>summary').click();
  }
  await a.keyboard.press('b');await a.locator('#mode-options button').filter({hasText:'factory'}).click();
  const site=await a.evaluate(()=>{const g=window.atemporal,c=g.selectedViews()[0];for(let y=c.y-2;y<=c.y+2;y++)for(let x=c.x-2;x<=c.x+2;x++)if(g.validPlacement({x,y},true))return{x,y};});
  await tile(a,site);await tile(a,site);await a.getByRole('button',{name:'Queue miner',exact:true}).click();
  await expect(a.locator('#order-warnings')).toHaveAttribute('title',/will start idle/);await expect(a.locator('#order-warnings')).toHaveAttribute('title',/no constructor assigned/);
  await a.keyboard.press('m');await area(a,site);await expect(a.locator('#order-warnings')).not.toHaveAttribute('title',/will start idle/);
  await expect(a.locator('#timeline')).toHaveAttribute('data-order-ticks','0');await noPanelScroll(a);await review.capture('factory-no-scroll-1366',a);
  await tile(a,constructor);await a.keyboard.press('c');await area(a,site);await expect(a.locator('#order-warnings')).not.toHaveAttribute('title',/no constructor assigned/);
  await a.locator('#actions .planning>summary').click();await review.capture('planning-menu-1366',a);
  expect(await a.locator('#actions .planning').evaluate(e=>e.getBoundingClientRect().top>=0)).toBe(true);
  await a.locator('#actions .planning>summary').click();
  await a.keyboard.press('?');expect(await a.locator('#help').evaluate(e=>e.getBoundingClientRect().bottom<=innerHeight)).toBe(true);await review.capture('hotkeys-no-scroll-1366',a);await a.keyboard.press('Escape');
  const popup=a.waitForEvent('popup');await a.locator('#unit-reference').click();const reference=await popup;await reference.waitForLoadState();await expect(reference.locator('#units')).toContainText('Unit reference');await expect(reference.locator('body')).toContainText('miner');
  await a.locator('#commit').click();await expect(a.locator('#turn-banner')).toBeVisible();await review.capture('committed-waiting-1366',a);
  await a.locator('#commit').click();await expect(a.locator('#turn-banner')).toBeHidden();
  await a.locator('#commit').click();await expect(a.locator('#turn-banner')).toBeVisible();await b.locator('#commit').click();await revision(a,1);await expect(a.locator('#turn-banner')).toBeHidden();
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
