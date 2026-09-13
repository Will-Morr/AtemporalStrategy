import {test,expect} from './fixtures.mjs';
import {isolatedServer,isolatedPeripheral,players,revision,tile,seek,area} from './game-helpers.mjs';
const hover=async(p,t)=>{const point=await p.evaluate(t=>{const r=window.atemporal.renderer;r.centerOn(t.x,t.y);return r.screen(t.x+.5,t.y+.5)},t);await p.mouse.move(...point);return point;};

test('silo blueprint production, launch previews, satellite replay and stockpiling survive rewrites',async({review},testInfo)=>{
  test.setTimeout(150000);
  const server=await(process.env.ATEMPORAL_UI_PERIPHERAL?isolatedPeripheral:isolatedServer)(testInfo,c=>{c.match_defaults.max_tick=450;c.match_defaults.stop_when_decided=false;c.match_defaults.starting_matter=2000;});review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);await seek(a,0);
  const constructor=await a.evaluate(()=>{const e=window.atemporal.entities().find(e=>e.owner===0&&e.type_key==='constructor');return{x:e.x,y:e.y}});
  const enemy=await b.evaluate(()=>{const e=window.atemporal.entities().find(e=>e.owner===1&&e.type_key==='turret');return{x:e.x,y:e.y}});
  const build=await a.evaluate(c=>{const g=window.atemporal;for(let y=c.y-1;y<=c.y+1;y++)for(let x=c.x-1;x<=c.x+1;x++)if(g.validPlacement({x,y},false))return{x,y}},constructor);
  await tile(a,constructor);await a.keyboard.press('b');await a.locator('#mode-options button').filter({hasText:'turret'}).click();await hover(a,build);await expect(a.locator('#range-readout')).toContainText('Turret reach:');await review.capture('turret-placement-range',a);await a.keyboard.press('Escape');
  await a.keyboard.press('b');await a.locator('#mode-options button').filter({hasText:'silo'}).click();await hover(a,build);await expect(a.locator('#range-readout')).toContainText('Manual launches: unlimited range');await review.capture('silo-placement-auto-range',a);await tile(a,build);await tile(a,build);
  await a.locator('.silo-production>summary').click();
  await a.keyboard.press('q');await expect(a.locator('#mode-options button')).toHaveCount(3);await a.locator('#mode-options button').filter({hasText:'satellite'}).click();await a.getByRole('button',{name:'Queue cluster',exact:true}).click();await a.getByRole('button',{name:'Queue tac_nuke',exact:true}).click();
  await a.locator('.silo-production>summary').click();
  await a.getByRole('combobox',{name:'Selection priority'}).selectOption('high');
  for(const name of ['Satellite','Cluster','Tac nuke']){await a.getByRole('button',{name:`Target ${name}`,exact:true}).click();const point=await hover(a,enemy);await expect(a.locator('#range-readout')).toContainText('ticks to hit after launch');await expect(a.locator('#range-readout')).toContainText(name);if(name==='Tac nuke')await review.capture('nuke-target-flight-time-and-area',a);await a.mouse.click(...point);}
  await expect(a.locator('.missile-launches li')).toHaveCount(3);await a.getByRole('button',{name:'Remove launch 3',exact:true}).click();await expect(a.locator('.missile-launches li')).toHaveCount(2);
  await a.getByRole('button',{name:'Target Tac nuke',exact:true}).click();await tile(a,enemy);
  await a.getByRole('button',{name:'Auto launch: Off',exact:true}).click();await a.getByRole('button',{name:'Auto launch: On',exact:true}).click();
  await expect(a.locator('#actions [data-key="f"]')).toBeHidden();await review.capture('blueprint-missiles-and-queued-launches',a);
  await tile(a,constructor);await a.keyboard.press('c');await area(a,build);await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,1);await revision(b,1);
  const flights=await a.evaluate(()=>window.atemporal.revisions.get(window.atemporal.current).events.filter(e=>e.event.kind==='missile_launch').map(e=>e.event.flight));expect(flights.map(f=>f.type_key)).toEqual(['satellite','cluster','tac_nuke']);
  for(const f of flights)expect(f.impact_tick-f.launch_tick).toBeLessThanOrEqual(30);
  const satellite=flights[0],mid=satellite.launch_tick+Math.floor((satellite.impact_tick-satellite.launch_tick)/2);
  await seek(a,mid);const position=await a.evaluate(()=>{const g=window.atemporal,f=g.missileState().missiles.find(f=>f.type_key==='satellite'),k=(g.playhead-f.launch_tick)/(f.impact_tick-f.launch_tick);return{x:Math.round(f.origin.x+(f.target.x-f.origin.x)*k),y:Math.round(f.origin.y+(f.target.y-f.origin.y)*k)}});
  expect(await a.evaluate(t=>window.atemporal.visibility().has(`${t.x},${t.y}`),position)).toBe(true);await hover(a,position);await review.capture('satellite-moving-vision-and-flight',a);
  await seek(a,satellite.impact_tick+1);expect(await a.evaluate(t=>window.atemporal.visibility().has(`${t.x},${t.y}`),enemy)).toBe(true);await hover(a,enemy);await review.capture('satellite-destination-reveals-enemy',a);
  await seek(a,flights[1].impact_tick+1);const hp=await b.evaluate(()=>window.atemporal.types.get('turret').max_hp);const damaged=await a.evaluate(t=>window.atemporal.exact.state.entities.find(e=>e.tile.x===t.x&&e.tile.y===t.y)?.hp,enemy);expect(damaged).toBe(hp-100);await review.capture('cluster-impact-replay',a);
  await seek(a,flights[2].impact_tick+1);expect(await a.evaluate(t=>window.atemporal.exact.state.entities.some(e=>e.tile.x===t.x&&e.tile.y===t.y),enemy)).toBe(false);await review.capture('nuke-impact-replay',a);
  // Rewrite before the first launch to store all three completed missiles instead.
  await seek(a,satellite.launch_tick-1);await tile(a,build);for(let i=3;i>0;i--)await a.getByRole('button',{name:`Remove launch ${i}`,exact:true}).click();await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,2);await revision(b,2);await seek(a,350);await tile(a,build);
  for(const key of ['satellite','cluster','tac_nuke'])await expect(a.locator(`.missile-inventory [data-missile="${key}"]`)).toHaveAttribute('data-count','1');await a.locator('#selection').hover();await a.mouse.wheel(0,-1000);await review.capture('completed-silo-stockpile',a);
  await a.keyboard.press('q');await a.locator('#mode-options button').filter({hasText:'satellite'}).click();expect(await a.evaluate(()=>window.atemporal.draft.commands.at(-1).command.kind)).toBe('edit_production');await a.keyboard.press('Control+z');
  await a.reload();await revision(a,2);await seek(a,350);await tile(a,build);await expect(a.locator('.missile-inventory [data-missile="tac_nuke"]')).toHaveAttribute('data-count','1');
  await a.getByRole('button',{name:'Target Tac nuke',exact:true}).click();await tile(a,enemy);await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,3);await seek(a,351);await tile(a,build);await expect(a.locator('.missile-inventory [data-missile="tac_nuke"]')).toHaveAttribute('data-count','0');await expect(a.locator('.missile-inventory [data-missile="satellite"]')).toHaveAttribute('data-count','1');await review.capture('stored-missile-launch-after-refresh',a);
  // A one-tick flight can begin and end between compact samples; playback must still render it.
  await revision(b,3);await seek(a,350);await tile(a,build);await a.getByRole('button',{name:'Target Satellite',exact:true}).click();await tile(a,build);await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,4);await seek(a,350);
  await a.getByRole('combobox',{name:'Playback speed'}).selectOption('0.25');await a.locator('#play').click();
  await expect.poll(()=>a.evaluate(()=>{const g=window.atemporal;return g.playhead>350&&g.playhead<351&&g.missileState().missiles.some(f=>f.type_key==='satellite'&&f.impact_tick-f.launch_tick===1)}),{intervals:[50]}).toBe(true);
  await a.locator('#play').click();await seek(a,351);expect(await a.evaluate(()=>window.atemporal.missileState().missiles.some(f=>f.type_key==='satellite'))).toBe(true);

});

test('automatic silo launches use allied spotting and can be rewritten into stored inventory',async({review},testInfo)=>{
  const config=c=>{c.match_defaults.max_tick=220;c.match_defaults.stop_when_decided=false;c.match_defaults.starting_matter=2000;};
  const content=c=>{c.starting_roster=['silo','scout','turret'];};
  const server=await(process.env.ATEMPORAL_UI_PERIPHERAL?isolatedPeripheral(testInfo,config,{},content):isolatedServer(testInfo,config,content));review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);await seek(a,0);await seek(b,0);
  const own=await a.evaluate(()=>window.atemporal.entities().filter(e=>e.owner===0).map(e=>({type:e.type_key,x:e.x,y:e.y})));
  const enemyScout=await b.evaluate(()=>{const v=window.atemporal.entities().find(e=>e.owner===1&&e.type_key==='scout');return{x:v.x,y:v.y}});
  const pair=await a.evaluate(()=>{const g=window.atemporal,t=g.terrain;for(let y=20;y<28;y++)for(let x=20;x<28;x++)if(t.cells[y*t.width+x]==='floor'&&t.cells[y*t.width+x+1]==='floor')return{observer:{x,y},enemy:{x:x+1,y}};});expect(pair).toBeTruthy();
  const silo=own.find(e=>e.type==='silo');await tile(a,silo);await a.keyboard.press('q');await a.locator('#mode-options button').filter({hasText:'cluster'}).click({modifiers:['Shift']});await a.getByRole('button',{name:'Auto launch: Off',exact:true}).click();
  await tile(a,own.find(e=>e.type==='scout'));await a.keyboard.press('f');await tile(a,pair.observer);
  await tile(b,enemyScout);await b.keyboard.press('f');await tile(b,pair.enemy);await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,1);await revision(b,1);
  const flights=await a.evaluate(()=>window.atemporal.revisions.get(window.atemporal.current).events.filter(e=>e.event.kind==='missile_launch'&&e.event.flight.owner===0).map(e=>e.event.flight));expect(flights.length).toBeGreaterThan(0);
  const f=flights[0];expect(Math.hypot(f.target.x-f.origin.x,f.target.y-f.origin.y)).toBeLessThanOrEqual(36);
  await seek(a,f.launch_tick);expect(await a.evaluate(t=>window.atemporal.visibility().has(`${t.x},${t.y}`),f.target)).toBe(true);
  await seek(a,f.launch_tick+1);await hover(a,f.target);await review.capture('automatic-missile-with-allied-spotter',a);
  await seek(a,1);await tile(a,silo);await a.getByRole('button',{name:'Auto launch: On',exact:true}).click();await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,2);await seek(a,210);await tile(a,silo);
  await expect(a.locator('.missile-inventory [data-missile="cluster"]')).toHaveAttribute('data-count','5');
  expect(await a.evaluate(()=>window.atemporal.revisions.get(window.atemporal.current).events.some(e=>e.event.kind==='missile_launch'))).toBe(false);await review.capture('automatic-launch-disabled-stockpile',a);
});
