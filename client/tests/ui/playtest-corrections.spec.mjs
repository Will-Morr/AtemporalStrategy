import {test,expect} from './fixtures.mjs';
import {isolatedServer,players,revision,tile} from './game-helpers.mjs';

test('repeat buildings with Shift and keep live fog while inspecting final timeline results',async({review},testInfo)=>{
 const server=await isolatedServer(testInfo,c=>{c.match_defaults.max_tick=700;});review.afterClose(server.stop);
 const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
 for(const p of [a,b])expect(await p.evaluate(()=>{const g=window.atemporal;return g.entities().filter(e=>e.owner!==g.player).length<g.rawEntities().filter(e=>e.owner!==g.player).length;})).toBe(true);
 const constructor=await a.evaluate(()=>window.atemporal.entities().find(e=>e.owner===0&&e.type_key==='constructor'));
 await tile(a,constructor);await a.keyboard.press('b');await a.locator('#mode-options button').filter({hasText:'turret'}).click();
 const spots=await a.evaluate(()=>{const g=window.atemporal,tiles=[];for(let y=3;y<g.terrain.height-3;y++)for(let x=3;x<g.terrain.width-3;x++)if(g.validPlacement({x,y},false))tiles.push({x,y});return tiles.slice(0,2);});
 await a.keyboard.down('Shift');await tile(a,spots[0]);await a.keyboard.up('Shift');
 expect(await a.evaluate(()=>window.atemporal.mode.kind)).toBe('place');
 await review.capture('shift-repeat-placement-mode',a);
 await tile(a,spots[1]);expect(await a.evaluate(()=>window.atemporal.mode.kind)).toBe('none');
 expect(await a.evaluate(()=>window.atemporal.draft.commands.filter(d=>d.command.kind==='place_blueprints').length)).toBe(2);
 await expect(a.locator('#outcome-banner strong')).toContainText('TIMELINE 0');
 await expect(a.locator('#outcome-banner > div')).toContainText('Final result at tick');
 await a.getByRole('button',{name:'View final state',exact:true}).click();
 await expect.poll(()=>a.evaluate(()=>window.atemporal.playhead)).toBe(await a.evaluate(()=>window.atemporal.rev().outcome.terminal_state_tick));
 await review.capture('fogged-result-and-explanation',a);
});
