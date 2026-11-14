import {test,expect} from './fixtures.mjs';
import {isolatedServer,revision} from './game-helpers.mjs';

const small=c=>{c.match_defaults.max_tick=700;c.match_defaults.stall_ticks=180;c.match_defaults.simulation_threads=1;};

test('DEBUG_TEST is unavailable without the test-server flag',async({page,review},testInfo)=>{
  const server=await isolatedServer(testInfo,small,()=>{},['--coordinator']);review.afterClose(server.stop);
  await page.goto(server.url);const directory=await(await page.request.get(server.url+'api/directory')).json();
  expect(directory.debug_test).toBe(false);await expect(page.getByRole('button',{name:'DEBUG_TEST',exact:true})).toBeHidden();
  const response=await page.request.post(server.url+'api/games',{data:{name:'DEBUG_TEST',setup:directory.defaults,debug_test:true}});
  expect(response.status()).toBe(403);expect((await response.json()).error).toContain('--test-server');
  expect((await(await page.request.get(server.url+'api/directory')).json()).games).toEqual([]);
});

test('DEBUG_TEST runs both opening bots then retains human control at turn 2 across refresh and restart',async({page,review},testInfo)=>{
  test.setTimeout(120000);
  const server=await isolatedServer(testInfo,small,()=>{},['--coordinator','--test-server']);review.afterClose(server.stop);
  await page.goto(server.url);const button=page.getByRole('button',{name:'DEBUG_TEST',exact:true});await expect(button).toBeVisible();
  await review.capture('debug-test-home',page);await button.click();
  await expect(page.getByRole('status')).toContainText('bots are playing');await expect(button).toBeDisabled();
  await expect(page).toHaveURL(/\/games\/game-[^/]+\/$/,{timeout:60000});await revision(page,1);
  await expect.poll(()=>page.evaluate(()=>window.atemporal.round)).toBe(2);
  expect(await page.evaluate(()=>window.atemporal.player)).toBe(0);
  await expect.poll(()=>page.evaluate(()=>window.atemporal.exact.tick)).toBe(150);
  await expect.poll(()=>page.evaluate(()=>window.atemporal.experience.turns.get(1)?.filter(t=>t.round===1).map(t=>({player:t.player,commands:t.commands.length})))).toEqual([{player:0,commands:expect.any(Number)},{player:1,commands:expect.any(Number)}]);
  expect(await page.evaluate(()=>window.atemporal.experience.turns.get(1).filter(t=>t.round===1).every(t=>t.commands.some(c=>c.command.kind==='place_blueprints')))).toBe(true);
  expect(await page.evaluate(()=>window.atemporal.entities().some(v=>v.owner===0&&v.type_key==='factory'))).toBe(true);
  expect(await page.evaluate(()=>window.atemporal.committed.includes(0))).toBe(false);await expect(page.locator('#commit')).toBeEnabled();
  await review.capture('debug-test-turn-two-handoff',page);
  const gameUrl=page.url();await page.reload();await revision(page,1);expect(await page.evaluate(()=>window.atemporal.player)).toBe(0);
  // The opponent may commit turn 2, but the opening bot must not consume our turn.
  await expect.poll(()=>page.evaluate(()=>window.atemporal.committed.includes(1))).toBe(true);
  expect(await page.evaluate(()=>({round:window.atemporal.round,committed:window.atemporal.committed.includes(0)}))).toEqual({round:2,committed:false});
  await page.goto(server.url);await server.stop();await server.start();await page.goto(gameUrl);await revision(page,1);
  expect(await page.evaluate(()=>window.atemporal.player)).toBe(0);expect(await page.evaluate(()=>window.atemporal.committed.includes(0))).toBe(false);
  await page.locator('#commit').click();await revision(page,2);await expect.poll(()=>page.evaluate(()=>window.atemporal.round)).toBe(3);
  expect(await page.evaluate(()=>window.atemporal.committed.includes(0))).toBe(false);await review.capture('debug-test-human-next-turn',page);
});
