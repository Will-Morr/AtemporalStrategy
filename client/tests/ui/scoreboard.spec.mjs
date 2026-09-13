import {test,expect} from './fixtures.mjs';
import {isolatedServer,isolatedPeripheral,players,revision,tile,seek} from './game-helpers.mjs';

test('persistent scoreboard counts wins through history, refresh and match victory',async({review},testInfo)=>{
  test.setTimeout(120000);
  const server=await(process.env.ATEMPORAL_UI_PERIPHERAL?isolatedPeripheral:isolatedServer)(testInfo,c=>{c.match_defaults.max_tick=1200;});review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
  const card=(p,id)=>p.locator(`#scoreboard [data-player="${id}"]`);
  await expect(card(a,0)).toHaveAttribute('data-wins','0');await expect(card(a,1)).toHaveAttribute('data-wins','0');await expect(a.locator('#scoreboard')).toContainText('First to 5');await review.capture('scoreboard-opening-tie',a);
  const target=await a.evaluate(()=>{const v=window.atemporal.entities().find(e=>e.owner===0&&e.type_key==='turret');return{x:v.x,y:v.y}});
  const constructor=await b.evaluate(()=>{const v=window.atemporal.entities().find(e=>e.owner===1&&e.type_key==='constructor');return{x:v.x,y:v.y}});
  await tile(b,constructor);await b.keyboard.press('f');await tile(b,target);
  await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,1);await revision(b,1);
  await expect(card(a,0)).toHaveAttribute('data-wins','1');await expect(card(a,1)).toHaveAttribute('data-wins','0');await expect(card(a,0)).toHaveAttribute('data-status','leading');
  await seek(a,0);await expect(card(a,0)).toHaveAttribute('data-wins','1');await review.capture('scoreboard-leading-at-opening-tick',a);
  await a.locator('#replay summary').first().click();await a.selectOption('#round-picker','0');await expect.poll(()=>a.evaluate(()=>window.atemporal.current)).toBe(0);await expect(card(a,0)).toHaveAttribute('data-wins','1');await review.capture('scoreboard-persists-in-historical-round',a);
  await a.selectOption('#round-picker','1');await a.locator('#replay summary').first().click();
  await a.reload();await revision(a,1);await expect(card(a,0)).toHaveAttribute('data-wins','1');
  for(let n=2;n<=5;n++){await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,n);await revision(b,n);await expect(card(a,0)).toHaveAttribute('data-wins',String(n));}
  await expect(card(a,0)).toHaveAttribute('data-status','winner');await expect(card(a,1)).toHaveAttribute('data-wins','0');await review.capture('scoreboard-five-wins-match-finished',a);await review.capture('scoreboard-losing-player-view',b);
});
