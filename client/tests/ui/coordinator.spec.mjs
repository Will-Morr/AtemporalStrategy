import {test,expect} from './fixtures.mjs';
import {isolatedServer,revision,seek} from './game-helpers.mjs';
import {readFile,readdir} from 'node:fs/promises';

async function identify(page,name){await expect(page.getByRole('dialog')).toBeVisible();await page.getByLabel('Username',{exact:true}).fill(name);await page.getByRole('button',{name:'Continue',exact:true}).click();}
async function create(page,name,{password='',bot='raider',all=false,rounds=4,edit=()=>{}}={}){
  await page.getByRole('button',{name:'Create game',exact:true}).click();
  await page.getByLabel('Game name',{exact:true}).fill(name);await page.getByLabel('Lobby password (optional)',{exact:true}).fill(password);
  if(all){await page.locator('#all-bots').check();await page.locator('#round-budget').fill(String(rounds));}
  await page.locator('#bot-1').selectOption(bot);
  await page.locator('#advanced summary').click();
  const setup=JSON.parse(await page.locator('#setup').inputValue());setup.match_defaults.max_tick=700;setup.match_defaults.stall_ticks=180;setup.match_defaults.simulation_threads=1;edit(setup);
  await page.locator('#setup').fill(JSON.stringify(setup,null,2));
  await page.getByRole('button',{name:'Create lobby',exact:true}).click();await expect(page).toHaveURL(/\/games\/game-[^/]+\/$/);return page.url();
}
async function claim(page,slot=0){await expect(page.locator('#status')).toContainText('Claim a slot');await page.locator('#roster li',{hasText:`Slot ${slot}:`}).click();await expect(page.locator('#status')).toContainText(`You hold slot ${slot}`);}

test('coordinator identities, passwords, parallel games, filters and same-address restart',async({page,review},testInfo)=>{
  test.setTimeout(240000);
  const server=await isolatedServer(testInfo,()=>{},()=>{},['--coordinator']);review.afterClose(server.stop);
  await page.goto(server.url);await expect(page.locator('#identity')).toBeVisible();await review.capture('coordinator-first-visit-username',page);await identify(page,'Avery <sandbox> with a long unvalidated name');
  await expect(page.locator('#profile')).toContainText('Avery <sandbox>');
  await page.getByRole('button',{name:'Create game',exact:true}).click();await expect(page.locator('#bot-1 option')).toHaveText(['Human','Builder bot','Raider bot']);await page.locator('#bot-1').selectOption('raider');await expect(page.locator('#round-budget-label')).toBeHidden();await review.capture('coordinator-simple-create',page);await page.setViewportSize({width:1366,height:768});await page.locator('#submit-create').scrollIntoViewIfNeeded();await review.capture('coordinator-create-1366',page);await page.getByRole('button',{name:'Cancel',exact:true}).click();await page.setViewportSize({width:1920,height:1080});
  const first=await create(page,'Sandbox one',{password:'casual'});await expect(page).toHaveTitle('Sandbox one — Scoreboard');await expect(page.getByLabel('Username',{exact:true})).toBeHidden();await claim(page);await expect(page.locator('#roster li').filter({hasText:'Slot 0:'})).toContainText('Avery <sandbox> with a long unvalidated name');await page.locator('#roster li').filter({hasText:'Slot 0:'}).getByRole('button',{name:'Release slot',exact:true}).click();await expect(page.locator('#roster li').filter({hasText:'Slot 0:'})).toContainText('open');await claim(page);await expect(page.locator('#roster')).toContainText('Bot · Raider');
  await page.getByRole('button',{name:'Map 3',exact:true}).click();await review.capture('coordinator-human-bot-map-lobby',page);
  await page.getByRole('button',{name:'Start match',exact:true}).click();await revision(page,0);
  await page.locator('#commit').click();await revision(page,1);await seek(page,Math.min(100,await page.evaluate(()=>window.atemporal.rev().outcome.terminal_state_tick)));
  expect(await page.evaluate(()=>window.atemporal.exact.state.entities.some(e=>e.owner===1&&e.type_key==='factory'))).toBe(true);
  await review.capture('coordinator-bot-opening-replay',page);
  await page.getByRole('button',{name:'Switch / rejoin player',exact:true}).click();
  await expect(page.getByRole('button',{name:'Rejoin Avery <sandbox> with a long unvalidated name',exact:true})).toBeVisible();
  await review.capture('coordinator-switch-player-choice',page);
  await page.getByRole('button',{name:'Rejoin Avery <sandbox> with a long unvalidated name',exact:true}).click();await revision(page,1);
  const match=await page.evaluate(()=>window.atemporal.net.matchId);
  const secondPage=await page.context().newPage();await secondPage.goto(server.url);await expect(secondPage.locator('#identity')).not.toBeVisible();
  const second=await create(secondPage,'Sandbox two',{bot:'builder'});await claim(secondPage);await secondPage.getByRole('button',{name:'Map 2',exact:true}).click();
  // Switch games in the same tab; each URL keeps its own token.
  await page.goto(server.url);await page.locator('#mine').check();await page.locator('#refresh').click();await expect(page.locator('.card')).toHaveCount(2);
  await page.locator('#filter').selectOption('active');await expect(page.locator('.card')).toHaveCount(1);await expect(page.locator('.card')).toContainText('Sandbox one');
  await page.locator('#filter').selectOption('open');await expect(page.locator('.card')).toContainText('Sandbox two');
  await page.locator('#filter').selectOption('all');await review.capture('coordinator-directory-two-games-and-user',page);
  await page.goto(first);await revision(page,1);expect(await page.evaluate(()=>window.atemporal.player)).toBe(0);
  const rejoin=await page.context().newPage();await rejoin.goto(first);await rejoin.getByRole('button',{name:'Rejoin Avery <sandbox> with a long unvalidated name',exact:true}).click();await revision(rejoin,1);expect(await rejoin.evaluate(()=>window.atemporal.player)).toBe(0);
  const observer=await(await review.newContext('directory-observer')).newPage();await observer.goto(first);
  await observer.getByLabel('Password',{exact:true}).fill('wrong');await observer.getByRole('button',{name:'Enter lobby'}).click();await expect(observer.getByRole('alert')).toContainText('Incorrect');
  await observer.getByLabel('Password',{exact:true}).fill('casual');await observer.getByRole('button',{name:'Enter lobby'}).click();await identify(observer,'Sam');
  await expect(observer.locator('#status')).toContainText('Choose a saved player');await observer.getByRole('button',{name:'Spectate',exact:true}).click();await revision(observer,1);expect(await observer.evaluate(()=>window.atemporal.player)).toBe(null);
  await review.capture('coordinator-spectator-running-game',observer);
  // Wait until the open-lobby snapshot is on disk before exercising abrupt coordinator stop.
  await expect.poll(async()=>{const entries=await readdir(`${server.dir}/games`);for(const id of entries.filter(s=>s.startsWith('game-'))){const s=JSON.parse(await readFile(`${server.dir}/games/${id}/status.json`,'utf8'));if(s.phase==='lobby'){const l=JSON.parse(await readFile(`${server.dir}/games/${id}/lobby.json`,'utf8'));return l.lobby.selected_map;}}}).toBe(1);
  await server.stop();await server.start();await page.reload();await revision(page,1);expect(await page.evaluate(()=>window.atemporal.net.matchId)).toBe(match);expect(await page.evaluate(()=>window.atemporal.player)).toBe(0);
  await secondPage.reload();await expect(secondPage.locator('#status')).toContainText('You hold slot 0');await expect(secondPage.getByRole('button',{name:'Map 2',exact:true})).toHaveAttribute('aria-pressed','true');
  await page.locator('#commit').click();await revision(page,2);await review.capture('coordinator-resumed-game-bot-next-turn',page);
});

test('bot-only playtest archives at its budget and exposes read-only recent replay',async({page,review},testInfo)=>{
  test.setTimeout(180000);
  const server=await isolatedServer(testInfo,()=>{},()=>{},['--coordinator']);review.afterClose(server.stop);
  await page.goto(server.url);await identify(page,'Playtester');await create(page,'Raider versus Builder',{all:true,bot:'builder',rounds:3});
  await page.getByRole('button',{name:'Spectate',exact:true}).click();
  await expect.poll(()=>page.evaluate(()=>window.atemporal?.latest),{timeout:90000}).toBe(3);
  await page.goto(server.url);await page.locator('#filter').selectOption('recent');await expect(page.locator('.card')).toHaveCount(1);await expect(page.locator('.badge')).toHaveText('Archived');await review.capture('coordinator-recent-bot-playtest-replay',page);
  await page.locator('.replays a').click();await expect(page.getByRole('button',{name:'View replay',exact:true})).toBeEnabled();await expect(page.locator('#map-picker')).toBeHidden();await review.capture('coordinator-read-only-entry',page);await page.getByRole('button',{name:'View replay',exact:true}).click();await revision(page,3);expect(await page.evaluate(()=>window.atemporal.player)).toBe(null);await expect(page.locator('#commit')).toBeDisabled();await expect(page.locator('#new-game-menu')).toHaveCount(0);
  const parts=new URL(page.url()).pathname.split('/');const token=JSON.parse(await readFile(`${server.dir}/games/${parts[2]}/replays/${parts[4]}/lobby.json`,'utf8')).tokens[0];
  await page.evaluate(async token=>{const g=window.atemporal;g.net.send({kind:'hello',protocol_version:2,last_revision:null,slot_token:token});await g.net.request({kind:'get_exact_state',revision:g.current,tick:0},'exact_state');},token);expect(await page.evaluate(()=>window.atemporal.player)).toBe(null);
  await seek(page,Math.min(100,await page.evaluate(()=>window.atemporal.rev().outcome.terminal_state_tick)));await review.capture('coordinator-read-only-replay',page);await page.reload();await page.getByRole('button',{name:'View replay',exact:true}).click();await revision(page,3);
});

test('four bot teams obey single-order limits and advancing hybrid history',async({page,review},testInfo)=>{
  test.setTimeout(180000);
  const server=await isolatedServer(testInfo,()=>{},()=>{},['--coordinator']);review.afterClose(server.stop);
  await page.goto(server.url);await identify(page,'Mechanics test');
  await page.getByRole('button',{name:'Create game',exact:true}).click();await page.getByLabel('Game name',{exact:true}).fill('Team mechanics');
  await page.locator('#player-count').selectOption('4');await page.locator('#sides').selectOption('teams');await page.locator('#objective').selectOption('hybrid');await page.locator('#control').selectOption('single_order');await page.locator('#all-bots').check();await page.locator('#round-budget').fill('3');
  await page.locator('#advanced summary').click();const setup=JSON.parse(await page.locator('#setup').inputValue());setup.match_defaults.max_tick=700;setup.match_defaults.simulation_threads=1;setup.match_defaults.objective.lock_ticks_per_round=50;await page.locator('#setup').fill(JSON.stringify(setup));
  await page.getByRole('button',{name:'Create lobby',exact:true}).click();await expect(page).toHaveURL(/\/games\/game-[^/]+\/$/);
  const gameId=new URL(page.url()).pathname.split('/')[2];await page.getByRole('button',{name:'Spectate',exact:true}).click();await expect.poll(()=>page.evaluate(()=>window.atemporal?.latest),{timeout:90000}).toBe(3);
  const id=await page.evaluate(()=>window.atemporal.net.matchId);const dir=`${server.dir}/games/${gameId}/replays/${id}`;
  for(let round=1;round<=3;round++)for(let player=0;player<4;player++){const turn=JSON.parse(await readFile(`${dir}/turns/${round}-${player}.json`,'utf8'));expect(turn.commands.length).toBeLessThanOrEqual(1);expect(turn.tick).toBeGreaterThanOrEqual((round-1)*50);}
  const manifest=JSON.parse(await readFile(`${dir}/manifest.json`,'utf8'));expect(new Set(manifest.config.multiplayer.assignments.map(a=>a.team_id)).size).toBe(2);
  await expect(page.getByRole('button',{name:'Create another game',exact:true})).toBeEnabled();await review.capture('coordinator-four-bot-teams-hybrid-single-order',page);
});

test('simple win game type can be selected and started',async({page,review},testInfo)=>{
  const server=await isolatedServer(testInfo,()=>{},()=>{},['--coordinator']);review.afterClose(server.stop);
  await page.goto(server.url);await identify(page,'Simple win player');
  await page.getByRole('button',{name:'Create game',exact:true}).click();
  await page.getByLabel('Game name',{exact:true}).fill('Simple win test');
  await page.locator('#objective').selectOption('simple_win');
  await page.locator('#bot-1').selectOption('raider');
  await review.capture('simple-win-create',page);
  await page.getByRole('button',{name:'Create lobby',exact:true}).click();
  await expect(page).toHaveURL(/\/games\/game-[^/]+\/$/);
  await claim(page);await expect(page.locator('#lobby')).toContainText('Simple win');
  await review.capture('simple-win-lobby',page);
  await page.getByRole('button',{name:'Start match',exact:true}).click();await revision(page,0);
  expect(await page.evaluate(()=>window.atemporal.config.objective.kind)).toBe('simple_win');
});
