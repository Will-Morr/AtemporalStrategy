import {test,expect} from './fixtures.mjs';
import {isolatedServer,revision} from './game-helpers.mjs';

function small(setup){setup.match_defaults.max_tick=700;setup.match_defaults.stall_ticks=180;setup.match_defaults.simulation_threads=1;}

test('landing page starts a default bot match with one click and retains the player on refresh',async({page,review},testInfo)=>{
  const server=await isolatedServer(testInfo,small,()=>{},['--coordinator']);review.afterClose(server.stop);
  await page.goto(server.url);
  await expect(page.getByRole('heading',{name:'Atemporal',exact:true})).toBeVisible();
  await expect(page.getByText('An RTS from the perspective of God.',{exact:true})).toBeVisible();
  await expect(page.locator('.description p')).toHaveCount(3);
  await expect(page.locator('dialog[open]')).toHaveCount(0);
  await review.capture('landing-desktop',page);
  await page.setViewportSize({width:1366,height:768});await review.capture('landing-1366',page);
  await page.setViewportSize({width:390,height:844});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  await review.capture('landing-narrow',page);
  await page.setViewportSize({width:1920,height:1080});
  await page.getByRole('button',{name:'Play Against Bot',exact:true}).click();
  await expect(page).toHaveURL(/\/games\/game-[^/]+\/$/);
  await revision(page,0);
  expect(await page.evaluate(()=>window.atemporal.player)).toBe(0);
  await expect(page.locator('#commit')).toBeEnabled();
  const directory=await(await page.request.get(server.url+'api/directory')).json();
  expect(directory.games).toHaveLength(1);
  expect(directory.games[0].bots).toEqual([null,directory.default_bot]);
  expect(directory.user).toBe('Player');
  await page.getByRole('button',{name:'All units',exact:true}).click();
  await expect(page.locator('#selection .unit-icon')).toHaveCount(3);
  await page.locator('#commit').click();await revision(page,1);
  await review.capture('quick-play-first-round',page);
  await page.reload();await revision(page,1);
  expect(await page.evaluate(()=>window.atemporal.player)).toBe(0);
  await page.getByRole('link',{name:'All games',exact:true}).click();
  await expect(page).toHaveURL(server.url+'lobbies/');
  await expect(page.getByRole('heading',{name:'Game lobbies',exact:true})).toBeVisible();
});

test('landing navigation, content-backed explainer and retry after creation failure',async({page,review},testInfo)=>{
  const server=await isolatedServer(testInfo,small,content=>{content.types.find(t=>t.key==='miner').max_hp=137;},['--coordinator']);review.afterClose(server.stop);
  await page.goto(server.url);
  await page.getByRole('link',{name:'Game Explained',exact:true}).click();
  await expect(page.getByRole('heading',{name:'Game Explained',exact:true})).toBeVisible();
  await expect(page.locator('.doc-warning')).toHaveText('Chatbot Documentation Warning: No human has taken the time to document this part yet');
  expect(await page.locator('.doc-warning').evaluate(el=>el.getBoundingClientRect().top)).toBeLessThan(80);
  await expect(page.locator('.unit-roles .unit-icon')).toHaveCount(14);
  await expect(page.locator('.table-scroll .unit-icon')).toHaveCount(14);
  await expect(page.locator('[data-unit-icon]')).toHaveCount(0);
  await expect(page.getByRole('heading',{name:'Your first turn',exact:true})).toBeVisible();
  await expect(page.getByRole('heading',{name:'What commands do',exact:true})).toBeVisible();
  await expect(page.getByRole('heading',{name:'Meet your units',exact:true})).toBeVisible();
  await expect(page.locator('tbody tr').filter({has:page.locator('td:first-child',{hasText:/^miner$/})}).locator('td').nth(3)).toHaveText('137');
  await review.capture('game-explained',page);
  await page.setViewportSize({width:390,height:844});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  await review.capture('game-explained-narrow',page);
  await page.setViewportSize({width:1920,height:1080});
  await page.locator('#full-rules summary').click();await expect(page.locator('#controls')).toBeVisible();
  await page.getByRole('link',{name:'← Atemporal',exact:true}).click();
  await page.getByRole('link',{name:'View Lobby',exact:true}).click();
  await expect(page).toHaveURL(server.url+'lobbies/');
  await expect(page.locator('#identity')).toBeVisible();
  await page.getByLabel('Username',{exact:true}).fill('Returning player');
  await page.getByRole('button',{name:'Continue',exact:true}).click();
  await page.getByRole('link',{name:'← Atemporal',exact:true}).click();
  // Inject an application-level failure, then retry against the real server.
  await page.route('**/api/games',route=>route.fulfill({json:{error:'Please try again.'}}));
  await page.getByRole('button',{name:'Play Against Bot',exact:true}).click();
  await expect(page.getByRole('status')).toHaveText('Please try again.');
  await expect(page.getByRole('button',{name:'Play Against Bot',exact:true})).toBeEnabled();
  await page.unroute('**/api/games');
  await page.getByRole('button',{name:'Play Against Bot',exact:true}).click();await revision(page,0);
  const directory=await(await page.request.get(server.url+'api/directory')).json();
  expect(directory.user).toBe('Returning player');expect(directory.games).toHaveLength(1);
});
