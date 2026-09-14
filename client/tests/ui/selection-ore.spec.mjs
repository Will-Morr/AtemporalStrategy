import {test,expect} from './fixtures.mjs';
import {isolatedServer,isolatedPeripheral,players,revision,tile,seek} from './game-helpers.mjs';

test('fogged ore amounts and type selection helpers preserve ownership and order recipients',async({review},testInfo)=>{
  const config=c=>{c.match_defaults.max_tick=600;};
  const content=c=>{c.starting_roster=['miner','factory','grunt'];};
  const server=await(process.env.ATEMPORAL_UI_PERIPHERAL?isolatedPeripheral(testInfo,config,{},content):isolatedServer(testInfo,config,content));review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);await seek(a,0);
  const ore=await a.evaluate(()=>{const g=window.atemporal,t=g.terrain,visible=g.visibility();const i=g.initialOre.findIndex((v,i)=>v>0&&!visible.has(`${i%t.width},${Math.floor(i/t.width)}`));return{x:i%t.width,y:Math.floor(i/t.width),value:g.oreAt(i)};});
  expect(ore.value).toBeGreaterThan(0);
  const point=await a.evaluate(t=>{const r=window.atemporal.renderer;r.centerOn(t.x,t.y);return r.screen(t.x+.5,t.y+.5)},ore);await a.mouse.move(...point);
  await expect(a.locator('#ore-readout')).toContainText(`Ore: ${ore.value.toLocaleString('en-US',{maximumFractionDigits:1})} matter`);
  expect(await a.evaluate(t=>window.atemporal.visibility().has(`${t.x},${t.y}`),ore)).toBe(false);
  await expect.poll(()=>a.evaluate(([x,y])=>{const p=document.querySelector('#map').getContext('2d').getImageData(Math.floor(x),Math.floor(y),1,1).data;return p[0]>120&&p[0]<180&&p[1]>110&&p[2]<120&&p[0]-p[2]<80;},point)).toBe(true);
  expect(await a.evaluate(()=>window.atemporal.entities().some(v=>v.owner===1))).toBe(false);
  await review.capture('fogged-ore-visible-amount',a);
  await a.getByRole('button',{name:'All units',exact:true}).click();
  const all=await a.evaluate(()=>window.atemporal.selectedViews().map(v=>({type:v.type_key,owner:v.owner})));
  expect(all.length).toBeGreaterThan(2);expect(all.every(v=>v.owner===0)).toBe(true);
  await review.capture('all-owned-types-selection',a);
  await a.getByRole('button',{name:'Remove miner from selection',exact:true}).click();
  expect(await a.evaluate(()=>window.atemporal.selectedViews().some(v=>v.type_key==='miner'))).toBe(false);
  await a.getByRole('button',{name:'Only factory',exact:true}).click();
  expect(await a.evaluate(()=>window.atemporal.selectedViews().map(v=>v.type_key))).toEqual(['factory']);
  await a.getByRole('button',{name:'All army',exact:true}).click();
  const army=await a.evaluate(()=>window.atemporal.selectedViews().map(v=>({id:v.id,type:v.type_key,owner:v.owner})));
  expect(army.length).toBeGreaterThan(0);expect(army.every(v=>v.owner===0&&!['miner','constructor','turret','factory'].includes(v.type))).toBe(true);
  await review.capture('all-army-ready-to-order',a);
  // Group recall must not silently widen the recipients after filtering a type.
  await a.keyboard.press('h');await a.keyboard.press('1');await a.keyboard.press('Escape');await a.keyboard.press('1');
  await a.getByRole('button',{name:'Only '+army[0].type,exact:true}).click();
  await a.keyboard.press('f');
  const destination=await a.evaluate(()=>{const v=window.atemporal.selectedViews()[0];return{x:v.x,y:v.y}});await tile(a,destination);
  const staged=await a.evaluate(()=>window.atemporal.draft.commands.map(d=>d.command));
  const order=staged.findLast(c=>c.kind==='assign_order');expect(order).toBeTruthy();
  expect(order.entities).toEqual(army.filter(v=>v.type===army[0].type).map(v=>v.id));
  await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,1);await seek(a,0);
  // S[0] precedes tick-zero commands; their authoritative result is visible at S[1].
  await seek(a,1);
  expect(await a.evaluate(ids=>window.atemporal.exact.state.entities.filter(e=>ids.some(id=>JSON.stringify(id)===JSON.stringify(e.id))).map(e=>e.action.kind),order.entities)).toEqual(order.entities.map(()=>'attack_move'));
});
