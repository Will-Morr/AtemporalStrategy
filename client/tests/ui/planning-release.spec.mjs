import {execFile} from 'node:child_process';
import {readdir} from 'node:fs/promises';
import {join} from 'node:path';
import {promisify} from 'node:util';
import {test,expect} from './fixtures.mjs';
import {isolatedServer,players,revision,tile,seek} from './game-helpers.mjs';

test('a silo supplies a single targeted launch and a staged reset preserves its previous timeline',async({review},testInfo)=>{
  const server=await isolatedServer(testInfo, cfg=>{
    Object.assign(cfg.match_defaults,{max_tick:160,stop_when_decided:false,control_limit:'single_order',starting_matter:2000});
  }, content=>{content.starting_roster=['silo','miner','miner'];});
  review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);
  await a.getByRole('button',{name:'Start match',exact:true}).click();
  await revision(a,0);await revision(b,0);
  const opening=await a.evaluate(()=>window.atemporal.entities().filter(e=>e.owner===0).map(e=>({type:e.type_key,x:e.x,y:e.y})));
  await tile(a,opening.find(e=>e.type==='miner'));
  await a.getByRole('button',{name:'Select all matching'}).click();
  await expect(a.locator('#selection-body')).toContainText('miner ×2');
  await a.setViewportSize({width:1366,height:768});
  await a.getByText('Game info',{exact:true}).click();
  await review.capture('game-info-menu-1366',a);
  const popup=a.waitForEvent('popup');
  await a.getByRole('link',{name:'Patch notes'}).click();
  const notes=await popup;
  await expect(notes).toHaveURL(/\/patch-notes\.html$/);
  await expect(notes.locator('article h2 a').first()).toHaveText(/^[a-f0-9]{7,12}$/);
  await notes.screenshot({path:testInfo.outputPath('game-info-patch-notes.png'),timeout:15000});await notes.close();
  await a.getByText('Game info',{exact:true}).click();
  await a.setViewportSize({width:1920,height:1080});
  const silo=opening.find(e=>e.type==='silo');
  await seek(a,12);await tile(a,silo);
  await a.getByRole('button',{name:'Target Satellite',exact:true}).click({modifiers:['Shift']});
  await a.keyboard.down('Shift');
  await tile(a,silo);
  await a.keyboard.up('Shift');
  expect(await a.evaluate(()=>window.atemporal.mode.kind)).toBe('launch');
  await a.keyboard.press('Escape');
  await expect(a.locator('.missile-launches li')).toHaveCount(1);
  await expect(a.locator('.silo-production summary')).toContainText('1 waiting');
  await expect(a.locator('#draft-title')).toContainText('1 changes');
  await a.keyboard.press('Control+z');await expect(a.locator('.missile-launches li')).toHaveCount(0);
  await a.keyboard.press('Control+y');await expect(a.locator('.missile-launches li')).toHaveCount(1);
  await a.locator('#commit').click();
  await expect(b.locator('#turn-banner')).toContainText('Player 0 is waiting for your turn');
  await review.capture('other-player-waiting',b);
  await b.getByRole('button',{name:'Minimize',exact:true}).click();
  const compact = b.getByRole('button',{name:'Player waiting',exact:true});
  await expect(compact).toHaveAttribute('aria-expanded','false');
  expect((await b.locator('#turn-banner').boundingBox()).width).toBeLessThan(150);
  await review.capture('other-player-waiting-minimized',b);
  await compact.click();
  await expect(b.locator('#turn-banner-message')).toBeVisible();
  await b.locator('#commit').click();await revision(a,1);await revision(b,1);
  await seek(a,13);await tile(a,silo);await expect(a.locator('.missile-launches li')).toHaveCount(1);
  await seek(a,0);await a.getByRole('button',{name:'Clear all future orders'}).click();
  await expect(a.locator('#draft-list')).toContainText('after this tick');
  await a.keyboard.press('Control+z');
  await expect(a.locator('#draft-list li')).toHaveCount(0);
  await a.keyboard.press('Control+y');
  await review.capture('staged-timeline-reset',a);
  await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,2);
  await seek(a,13);await tile(a,silo);
  await expect(a.locator('.missile-launches li')).toHaveCount(0);
  await expect(a.locator('.silo-production summary')).toContainText('0 waiting');
});

test('an expired satellite leaves a gray last sighting which survives seeking and refresh',async({review},testInfo)=>{
  const server=await isolatedServer(testInfo,setup=>Object.assign(setup.match_defaults,{max_tick:250,stop_when_decided:false}),content=>{
    content.starting_roster=['silo','miner','turret'];
    content.types.find(t=>t.key==='satellite').missile.reveal_ticks=15;
  });
  review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);
  await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
  const position=(page,type)=>page.evaluate(type=>{const v=window.atemporal.entities().find(e=>e.owner===window.atemporal.player && e.type_key===type);return{x:v.x,y:v.y};},type);
  const home=await position(a,'silo'), enemy=await position(b,'turret');
  const sight=()=>a.evaluate(t=>{const g=window.atemporal;const building=g.entities().find(e=>e.type_key==='turret' && e.x===t.x && e.y===t.y);return !building?'unseen':building.remembered?'remembered':'visible';},enemy);
  expect(await sight()).toBe('unseen');
  await tile(a,home);await a.getByRole('button',{name:'Target Satellite',exact:true}).click();await tile(a,enemy);
  await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,1);
  await seek(a,100);await expect.poll(sight).toBe('remembered');
  const enemyMiner=await position(b,'miner');
  const minerMemory=()=>a.evaluate(()=>window.atemporal.entities().filter(e=>e.owner===1 && e.type_key==='miner' && e.remembered).map(e=>({x:e.x,y:e.y})));
  await expect.poll(minerMemory).toEqual([enemyMiner]);
  await tile(a,enemyMiner);expect(await a.evaluate(()=>window.atemporal.selectedViews().length)).toBe(0);
  await tile(a,enemy);expect(await a.evaluate(()=>window.atemporal.selectedViews().length)).toBe(0);
  await review.capture('gray-enemy-building-after-vision-expires',a);
  await seek(a,0);await expect.poll(sight).toBe('unseen');expect(await minerMemory()).toEqual([]);
  await a.reload();await revision(a,1);await seek(a,100);await expect.poll(sight).toBe('remembered');
});

test('archived replay shades tiles outside the combined player fields of view', async({review},testInfo)=>{
  const server=await isolatedServer(testInfo);
  review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);
  await a.getByRole('button',{name:'Start match',exact:true}).click();
  await Promise.all([revision(a,0),revision(b,0)]);
  const sight=page=>page.evaluate(()=>[...window.atemporal.visibility()].sort());
  const own=await sight(a), other=await sight(b);
  const union=[...new Set([...own,...other])].sort();
  expect(union.length).toBeGreaterThan(own.length);
  expect(union.length).toBeGreaterThan(other.length);
  await a.locator('#actions .planning>summary').click();
  await a.locator('#stop-archive').click();
  await expect(b.locator('#new-game-menu')).toBeVisible();
  await expect.poll(()=>sight(b)).toEqual(union);
  await b.getByRole('button',{name:'Keep viewing replay'}).click();
  const darkFloor=await b.evaluate(()=>{
    const g=window.atemporal, mask=g.visibility();
    return g.terrain.cells.some((cell,i)=>cell==='floor' && !mask.has([i%g.terrain.width,Math.floor(i/g.terrain.width)].join(',')));
  });
  expect(darkFloor).toBe(true);
  await b.evaluate(()=>{
    const {terrain, renderer}=window.atemporal;
    renderer.camera.scale=Math.min(20,(renderer.map.height-renderer.headerHeight-20)/terrain.height);
    renderer.centerOn(terrain.width/2,terrain.height/2);
    renderer.draw();
  });
  await review.capture('archived-combined-field-of-view',b);
});

async function exportedFrames(server, revisionNumber) {
  const exporter = process.env.ATEMPORAL_PLAYER_REPLAY_BIN;
  if (!exporter) return null;
  const root = join(server.dir, 'replays');
  const archives = (await readdir(root, {withFileTypes:true})).filter(entry => entry.isDirectory());
  expect(archives).toHaveLength(1);
  const {stdout} = await promisify(execFile)(
    exporter, [join(root, archives[0].name), '0', String(revisionNumber)], {maxBuffer:16*1024*1024},
  );
  const [header, ...rows] = stdout.trim().split('\n').map(JSON.parse);
  expect(header.revision).toBe(revisionNumber);
  const frames = new Map();
  let tiles = [];
  for (const row of rows) {
    if (row.visible_tile_ranges) {
      tiles = row.visible_tile_ranges.flatMap(([first, end]) => Array.from({length:end-first}, (_, i)=>first+i));
    }
    frames.set(row.frame.tick, {...row.frame, tiles});
  }
  return frames;
}

async function expectBrowserProjection(page, frames, tick, sampled=false) {
  if (!frames) return;
  const frame = frames.get(tick);
  const browser = await page.evaluate(sampled=>{
    const g=window.atemporal;
    const exact = g.exact;
    if (sampled) g.exact = null;
    const tiles=[...g.visibility()].map(tile=>{
      const [x,y]=tile.split(',').map(Number);
      return y*g.terrain.width+x;
    });
    const entities=g.entities().filter(e=>!e.remembered && e.lifecycle!=='blueprint').map(e=>({
      id:e.id, owner:e.owner, type_key:e.type_key, lifecycle:e.lifecycle, hp:e.hp, activity:e.activity, facing:e.facing, tile:{x:e.x,y:e.y},
    }));
    g.exact = exact;
    return {tiles,entities};
  },sampled);
  expect(browser.tiles.sort((a,b)=>a-b)).toEqual(frame.tiles);
  const sorted = entries => [...entries].sort((a,b)=>JSON.stringify(a.id).localeCompare(JSON.stringify(b.id)));
  const expected = [...frame.own_entities,...frame.visible_entities].map(({id,owner,type_key,lifecycle,hp,activity,last_move_direction,tile})=>({
    id,owner,type_key,lifecycle,hp,activity,facing:last_move_direction,tile,
  }));
  expect(sorted(browser.entities)).toEqual(sorted(expected));
}

test('an exact mid-sample satellite sighting becomes revision-local building memory',async({review},testInfo)=>{
  const server=await isolatedServer(testInfo, setup=>{
    Object.assign(setup.match_defaults,{snapshot_interval:60,max_tick:180,stop_when_decided:false});
  },content=>{
    content.starting_roster=['silo','miner','turret'];
    const satellite=content.types.find(t=>t.key==='satellite').missile;
    Object.assign(satellite,{radius:0.1,reveal_ticks:1});
  });
  review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);
  await a.getByRole('button',{name:'Start match',exact:true}).click();
  await revision(a,0);await revision(b,0);
  const position=(page,type)=>page.evaluate(type=>{
    const game=window.atemporal;
    const v=game.entities().find(e=>e.owner===game.player && e.type_key===type);
    return {x:v.x,y:v.y};
  },type);
  const home=await position(a,'silo'), miner=await position(a,'miner'), enemy=await position(b,'silo');
  const sight=()=>a.evaluate(t=>{
    const v=window.atemporal.entities().find(e=>e.type_key==='silo' && e.x===t.x && e.y===t.y);
    return !v?'unseen':v.remembered?'remembered':'visible';
  },enemy);
  expect(await sight()).toBe('unseen');
  await seek(a,12);await tile(a,home);
  await a.getByRole('button',{name:'Target Satellite',exact:true}).click();await tile(a,enemy);
  await tile(a,miner);await a.keyboard.press('f');await tile(a,miner);
  await tile(a,home);
  await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,1);
  const observed=await exportedFrames(server,1);
  const impact=await a.evaluate(async()=>{
    const g=window.atemporal, revision=g.current;
    const reply=await g.net.request({kind:'get_events',revision,from_tick:0,to_tick:180,effects_only:true},
      'events',m=>m.revision===revision);
    return reply.events.find(e=>e.event.kind==='missile_impact' && e.event.type_key==='satellite').tick;
  });
  expect(impact%60).not.toBe(0);
  expect((impact+1)%60).not.toBe(0);
  await seek(a,60);expect(await sight()).toBe('unseen');
  await a.waitForFunction(()=>window.atemporal.rev()?.samples.has(60));
  await expectBrowserProjection(a,observed,60,true);
  await seek(a,impact);expect(await sight()).toBe('visible');
  await a.evaluate(tile=>window.atemporal.renderer.centerOn(tile.x,tile.y),enemy);
  await expectBrowserProjection(a,observed,impact);
  if (observed) {
    const settled = observed.get(impact).own_entities.find(e=>e.type_key==='miner');
    expect(settled.action.kind).toBe('attack_move');
    expect(settled.activity).toBe('idle');
  }
  await review.capture('exact-mid-sample-satellite-sighting',a);
  await seek(a,100);expect(await sight()).toBe('remembered');
  await expectBrowserProjection(a,observed,100);
  if (observed) {
    const isEnemySilo = e => e.owner===1 && e.type_key==='silo' && e.tile.x===enemy.x && e.tile.y===enemy.y;
    const last = [...observed.values()]
      .filter(frame=>frame.tick<=100)
      .flatMap(frame=>frame.visible_entities.filter(isEnemySilo))
      .at(-1);
    const remembered = await a.evaluate(tile=>{
      const v=window.atemporal.entities().find(e=>e.remembered && e.type_key==='silo' && e.x===tile.x && e.y===tile.y);
      return {id:v.id, hp:v.hp, lifecycle:v.lifecycle};
    },enemy);
    expect(remembered).toEqual({id:last.id, hp:last.hp, lifecycle:last.lifecycle});
  }
  await review.capture('mid-sample-satellite-building-memory',a);
  await seek(a,0);expect(await sight()).toBe('unseen');
  await expectBrowserProjection(a,observed,0);
  // A new published revision removes the launch. Its memory cannot inherit the old sighting.
  await a.getByRole('button',{name:'Clear all future orders'}).click();
  await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,2);
  const reset=await exportedFrames(server,2);
  await seek(a,100);expect(await sight()).toBe('unseen');
  await expectBrowserProjection(a,reset,100);
  await a.evaluate(()=>window.atemporal.experience.selectRound(1));
  await revision(a,1);await seek(a,impact);
  await expectBrowserProjection(a,observed,impact);
  expect(await sight()).toBe('visible');
  await tile(a,enemy);
  await expect(a.locator('#selection-body')).toContainText('silo ×1');
  await expect(a.locator('#queue .factory-plan')).toHaveCount(0);
});


test('the opening shares vision with the roster-selected allies',async({review},testInfo)=>{
  const server=await isolatedServer(testInfo,setup=>{
    Object.assign(setup.match_defaults,{
      max_tick:20,
      stop_when_decided:false,
      player_count:4,
      multiplayer:{
        kind:'teams',
        assignments:[0,1,2,3].map(player_id=>({player_id,team_id:player_id%2===0?'cyan':'orange'})),
      },
    });
    setup.available_teams=['cyan','orange'].map(team_id=>({team_id,label:team_id,capacity:2}));
  },content=>{content.starting_roster=['constructor','miner'];});
  review.afterClose(server.stop);
  const [a]=await players(review,server.url,4,true);
  await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);
  expect(await a.evaluate(()=>[...new Set(window.atemporal.entities().map(e=>e.owner))].sort())).toEqual([0,1]);
  const observed=await exportedFrames(server,0);
  await expectBrowserProjection(a,observed,0);
  await review.capture('roster-selected-team-vision',a);
});
