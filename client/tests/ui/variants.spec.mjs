import { test, expect } from './fixtures.mjs';
import { isolatedServer, players, revision, seek, tile, area } from './game-helpers.mjs';
import { readdir, readFile } from 'node:fs/promises';

function configure(c,{count,teams,timed,single}){
  const m=c.match_defaults;m.player_count=count;m.symmetric=count!==3;m.map_size=48;m.max_tick=1200;m.stall_ticks=100;
  m.control_limit=single?'single_order':'timestamp';
  if(timed)m.objective={kind:'timed',lock_ticks_per_round:100};
  if(teams){m.multiplayer={kind:'teams',assignments:Array.from({length:count},(_,player_id)=>({player_id,team_id:player_id<2?'cyan':'orange'}))};c.available_teams=[{team_id:'cyan',label:'Cyan',capacity:2},{team_id:'orange',label:'Orange',capacity:2}];}
}
for(const variant of [
  {label:'3-player FFA scoreboard single-order',count:3,teams:false,timed:false,single:true},
  {label:'4-player FFA timed timestamp',count:4,teams:false,timed:true,single:false},
  {label:'2v2 scoreboard timestamp',count:4,teams:true,timed:false,single:false},
  {label:'2v2 timed single-order',count:4,teams:true,timed:true,single:true},
]) test(`variants: ${variant.label}, simultaneous publication and durable refresh`,async({review},testInfo)=>{
  test.setTimeout(120000);
  const server=await isolatedServer(testInfo,c=>configure(c,variant),()=>{},['--memory-budget-mb','0','--results-budget-mb','0']);review.afterClose(server.stop);
  const ps=await players(review,server.url,variant.count,variant.teams),a=ps[0];
  const spectator=await(await review.newContext('spectator')).newPage();await spectator.goto(server.url);
  await ps[1].fill('#username','Updated teammate');await ps[1].locator('#color').focus();await expect(spectator.locator('#roster')).toContainText('Updated teammate');
  if(variant.teams){await expect(spectator.locator('#roster')).toContainText('Cyan');await expect(spectator.locator('#roster')).toContainText('Orange');}
  await review.capture('live-multiplayer-roster',spectator);
  await spectator.getByRole('button',{name:'Spectate',exact:true}).click();
  await a.getByRole('button',{name:'Start match',exact:true}).click();for(const p of [...ps,spectator])await revision(p,0);
  await expect(spectator.locator('#commit')).toBeDisabled();await expect(spectator.getByRole('button',{name:'B Build structure',exact:true})).toBeHidden();
  const constructor=await a.evaluate(()=>{const e=window.atemporal.entities().find(e=>e.owner===0&&e.type_key==='constructor');return{x:e.x,y:e.y};});
  await tile(a,constructor);await a.keyboard.press('h');await a.keyboard.press('2');
  await a.keyboard.press('p');await a.keyboard.press('1');
  if(variant.single){await expect(a.locator('#toast')).toContainText('Single-order mode');expect(await a.evaluate(()=>window.atemporal.draft.commands.length)).toBe(1);}
  else expect(await a.evaluate(()=>window.atemporal.draft.commands.length)).toBe(2);
  await a.locator('#commit').click();await expect(a.locator('#top-phase')).toContainText('committed');
  for(const p of [...ps,spectator])expect(await p.evaluate(()=>window.atemporal.current)).toBe(0);
  if(variant.count===3){
    const pendingMatch=(await readdir(`${server.dir}/replays`))[0];
    for(const p of [...ps,spectator])await p.context().setOffline(true);
    await server.stop();await server.start(['--resume',pendingMatch]);
    for(const p of [...ps,spectator]){await p.context().setOffline(false);await p.reload();await revision(p,0);}
    await expect(a.locator('#commit')).toBeDisabled();expect(await a.evaluate(()=>window.atemporal.committed)).toEqual([0]);
    await review.capture('partial-round-resumed',a);
  }
  for(const p of ps.slice(1))await p.locator('#commit').click();for(const p of [...ps,spectator])await revision(p,1);
  const tick=variant.timed?100:0;await seek(a,tick);await a.keyboard.press('2');await a.keyboard.press('f');await tile(a,{x:constructor.x+1,y:constructor.y+1});
  expect(await a.evaluate(()=>window.atemporal.draft.commands.map(d=>d.command.kind))).toEqual(['assign_group_order']);
  await review.capture('group-single-delivery-and-timeline',a);
  for(const p of ps){if(p!==a)await seek(p,tick);await p.locator('#commit').click();}for(const p of [...ps,spectator])await revision(p,2);
  await seek(a,tick+1);await a.keyboard.press('2');await expect(a.locator('#selection-body')).toContainText('attack_move');
  await expect.poll(()=>a.evaluate(()=>window.atemporal.experience.rounds.get(2)?.command_outcomes.find(o=>o.command_id.round===2&&o.command_id.player===0)?.applied_entities.length)).toBe(1);
  await expect(a.locator('#timeline')).toHaveAttribute('data-order-ticks',variant.timed?'0,100':'0');
  // Preserve tokens, accepted commands, revision, score and exact state across real archive resume.
  const before=await a.evaluate(()=>({round:window.atemporal.round,score:window.atemporal.rev().score,timed:window.atemporal.editableFrom,groups:window.atemporal.exact.state.control_groups}));
  const match=(await readdir(`${server.dir}/replays`))[0];
  for(const p of [...ps,spectator])await p.context().setOffline(true);
  await server.stop();await server.start(['--resume',match]);
  for(const p of [...ps,spectator])await p.context().setOffline(false);
  await expect(a.locator('#connection')).toContainText('Server restarted',{timeout:15000});await review.capture('archive-resumed-reload-prompt',a);
  for(const p of [...ps,spectator]){await p.reload();await revision(p,2);}
  await seek(a,tick+1);
  expect(await a.evaluate(()=>({round:window.atemporal.round,score:window.atemporal.rev().score,timed:window.atemporal.editableFrom,groups:window.atemporal.exact.state.control_groups}))).toEqual(before);
  await review.capture('archive-resumed-same-revision',a);
  await a.locator('#replay summary').click();await a.selectOption('#round-picker','0');await revision(a,0);
  await expect(a.locator('#commit')).toBeDisabled();await review.capture('evicted-historical-round-regenerated',a);
  expect(await readFile(`${server.dir}/server.log`,'utf8')).toMatch(/regenerat/i);

  const record=JSON.parse(await readFile(`${server.dir}/replays/${match}/rounds/2.json`,'utf8'));expect(record.revision).toBe(2);
});

for(const timed of [false,true])test(`recovery: 2v2 ${timed?'timed':'scoreboard'} elimination reverses when delayed factory completes`,async({review},testInfo)=>{
  test.setTimeout(150000);
  const server=await isolatedServer(testInfo,c=>{configure(c,{count:4,teams:true,timed,single:false});c.match_defaults.starting_matter=1000;c.match_defaults.max_tick=2400;if(timed)c.match_defaults.objective.lock_ticks_per_round=500;},content=>{
    // Deterministic authored content, consumed normally by the real server and generated guide.
    // Harmless fragile turrets expose building-loss recovery; workers survive the test firefight.
    for(const t of content.types){if(t.key==='turret'){t.max_hp=1;t.weapon.damage=0;}if(['miner','constructor'].includes(t.key))t.max_hp=10000;}
  });review.afterClose(server.stop);
  const ps=await players(review,server.url,4,true),a=ps[0],attacker=ps[2];
  const spectator=await(await review.newContext('spectator')).newPage();await spectator.goto(server.url);
  await spectator.getByRole('button',{name:'Spectate',exact:true}).click();
  await a.getByRole('button',{name:'Start match',exact:true}).click();for(const p of [...ps,spectator])await revision(p,0);
  const ac=await attacker.evaluate(()=>{const e=window.atemporal.entities().find(e=>e.owner===2&&e.type_key==='constructor');return{x:e.x,y:e.y};});
  const ambush={x:2,y:4};
  await tile(attacker,ac);await attacker.keyboard.press('b');await attacker.keyboard.press('1');
  // Default east output is nearer the fragile turret than the workers.
  await tile(attacker,ambush);await tile(attacker,ambush);await attacker.keyboard.press('q');await attacker.keyboard.press('4');
  await tile(attacker,ac);await attacker.keyboard.press('c');await area(attacker,ambush);
  for(const p of ps)await p.locator('#commit').click();for(const p of [...ps,spectator])await revision(p,1);
  const loss=await a.evaluate(()=>window.atemporal.rev().outcome.survival_transitions.find(t=>t.player_id===0&&t.status==='eliminated'));
  expect(loss).toBeTruthy();expect(loss.resolved_tick).toBeLessThan(500);
  if(timed){expect(await a.evaluate(()=>window.atemporal.experience.rounds.get(1)?.timed?.timed_lost_players??[])).not.toContain(0);await seek(a,500);await expect(a.locator('#commit')).toBeEnabled();}
  await seek(a,loss.resolved_tick+1);await expect(a.locator('#outcome-banner')).toHaveAttribute('data-outcome','loss');await review.capture('temporarily-eliminated-player',a);
  await seek(spectator,loss.resolved_tick);await review.capture('turret-destruction-combat',spectator);
  const recoveryTick=timed?500:200;
  await seek(a,recoveryTick);
  const c=await a.evaluate(()=>{const e=window.atemporal.entities().find(e=>e.owner===0&&e.type_key==='constructor');return{x:e.x,y:e.y};}),rescue={x:8,y:5};
  await tile(a,c);await a.keyboard.press('b');await a.keyboard.press('1');await tile(a,rescue);await tile(a,c);await a.keyboard.press('c');await area(a,rescue);
  for(const p of ps){if(p!==a)await seek(p,recoveryTick);await p.locator('#commit').click();}for(const p of [...ps,spectator])await revision(p,2);
  const outcome=await a.evaluate(()=>window.atemporal.rev().outcome);
  expect(outcome.survival_transitions.some(t=>t.player_id===0&&t.status==='eliminated')).toBe(true);
  expect(outcome.survival_transitions.some(t=>t.player_id===0&&t.status==='alive'&&t.resolved_tick>recoveryTick)).toBe(true);
  expect(outcome.survivors).toHaveLength(4);expect(outcome.kind).toBe('stalemate');
  if(!timed)expect(await a.evaluate(()=>window.atemporal.rev().score.entries.map(e=>e.raw_delta))).toEqual([0,0]);
  await seek(a,outcome.terminal_state_tick);await a.locator('#replay summary').click();await expect(a.locator('#accepted-inputs')).toContainText('recovered');
  await review.capture('recovered-player-and-survival-history',a);
});
