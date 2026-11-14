import {test,expect} from './fixtures.mjs';
import {isolatedServer,players,revision,tile,area,seek} from './game-helpers.mjs';

test('order paths use actual movement rules, include utilities and follow factory queues',async({review},testInfo)=>{
  const server=await isolatedServer(testInfo,c=>{c.match_defaults.max_tick=300;});review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
  const units=await a.evaluate(()=>window.atemporal.entities().filter(e=>e.owner===0));
  const constructor=units.find(e=>e.type_key==='constructor'),miner=units.find(e=>e.type_key==='miner');
  await tile(a,constructor);
  const mp=await a.evaluate(t=>window.atemporal.renderer.screen(t.x+.5,t.y+.5),miner);await a.keyboard.down('Shift');await a.mouse.click(...mp);await a.keyboard.up('Shift');
  await a.keyboard.press('f');
  const goal=await a.evaluate(()=>{const g=window.atemporal,c=g.selectedViews()[0];for(let y=c.y+4;y<c.y+10;y++)for(let x=c.x+4;x<c.x+10;x++)if(g.terrain.cells[y*g.terrain.width+x]==='floor')return{x,y};});
  const point=await a.evaluate(t=>window.atemporal.renderer.screen(t.x+.5,t.y+.5),goal);await a.mouse.move(...point);
  await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.orderPreviews.routes.length)).toBe(2);
  await review.capture('utility-attack-aim-paths',a);await a.mouse.click(...point);
  await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.orderPreviews.routes.length)).toBe(2);
  const routes=await a.evaluate(()=>window.atemporal.renderer.orderPreviews.routes);expect(new Set(routes.map(r=>r.entity)).size).toBe(2);expect(routes.every(r=>JSON.stringify(r.goal)===JSON.stringify(goal))).toBe(true);
  await review.capture('selected-utility-order-paths',a);
  await a.getByText('Settings',{exact:true}).click();await a.getByRole('checkbox',{name:'Show path previews'}).uncheck();await a.getByText('Settings',{exact:true}).click();
  await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.orderPreviews.routes.every(r=>r.points.length===2))).toBe(true);await review.capture('straight-order-lines',a);
  await a.reload();await revision(a,0);await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.pathPreviews)).toBe(false);
  await a.getByText('Settings',{exact:true}).click();await a.getByRole('checkbox',{name:'Show path previews'}).check();await a.getByText('Settings',{exact:true}).click();
  await tile(a,constructor);await a.keyboard.press('b');await a.locator('#mode-options button').filter({hasText:'factory'}).click();await a.keyboard.press('r');
  const site=await a.evaluate(()=>{const g=window.atemporal,c=g.entities().find(e=>e.owner===0&&e.type_key==='constructor');for(let y=c.y-3;y<=c.y+3;y++)for(let x=c.x-3;x<=c.x+3;x++)if(g.validPlacement({x,y},true))return{x,y};});
  await tile(a,site);await tile(a,site);await a.getByRole('button',{name:'Queue tank',exact:true}).click();await a.keyboard.press('f');await tile(a,goal);
  await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.orderPreviews.routes.map(r=>r.rule))).toEqual(['four']);
  await a.getByRole('button',{name:'Queue grunt',exact:true}).click();
  await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.orderPreviews.routes.map(r=>r.rule))).toEqual(['four','eight']);
  expect(await a.evaluate(()=>{const g=window.atemporal,v=g.selectedViews()[0],r=g.renderer.orderPreviews.routes[0];return r.from.x===v.x&&r.from.y===v.y+1;})).toBe(true);
  await review.capture('factory-mixed-queue-paths',a);
  await tile(a,constructor);await a.keyboard.press('c');await area(a,site);
  await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.orderPreviews.routes.length)).toBe(1);await review.capture('constructor-work-path',a);
  await tile(a,miner);await a.keyboard.press('m');
  const ore=await a.evaluate(()=>{const g=window.atemporal,i=g.initialOre.findIndex(n=>n>0);return{x:i%g.terrain.width,y:Math.floor(i/g.terrain.width)};});await area(a,ore);
  await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.orderPreviews.routes.length)).toBe(1);await review.capture('miner-work-path',a);
  await tile(a,constructor);
  await a.setViewportSize({width:1366,height:768});await a.getByText('Settings',{exact:true}).click();await review.capture('path-settings-laptop',a);
});


test('mining previews follow retained targets after authoritative movement',async({review},testInfo)=>{
  const server=await isolatedServer(testInfo,c=>{c.match_defaults.max_tick=120;});review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
  await a.getByRole('button',{name:'All units',exact:true}).click();await a.keyboard.press('m');
  const box=await a.evaluate(()=>{const g=window.atemporal,r=g.renderer,t=g.terrain;r.camera.scale=4;r.centerOn(t.width/2,t.height/2);return [r.screen(.5,.5),r.screen(t.width-.5,t.height-.5)];});
  await a.mouse.move(...box[0]);await a.mouse.down();await a.mouse.move(...box[1]);await a.mouse.up();
  await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,1);await seek(a,1);
  const workers=await a.evaluate(()=>window.atemporal.entities().filter(v=>v.owner===0&&v.exact?.action.kind==='mine'));
  expect(workers.length).toBe(2);expect(new Set(workers.map(v=>JSON.stringify(v.exact.resolved_destination))).size).toBe(2);
  for(const worker of workers){
    expect(worker.exact.resolved_destination).toBeTruthy();await tile(a,worker);
    await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.orderPreviews.routes.map(r=>r.goal))).toEqual([worker.exact.resolved_destination]);
    await a.evaluate(()=>{const g=window.atemporal,r=g.renderer,v=g.selectedViews()[0];r.camera.scale=26;r.centerOn(v.x,v.y);});
    await review.capture(`${worker.type_key}-retained-mining-target`,a);
  }
});
