import { test, expect } from './fixtures.mjs';
import { isolatedServer, players, revision, seek, tile, area } from './game-helpers.mjs';

test('playtest fixes: reachable map, contextual production, ghosts, fog and applied ticks',async({review},testInfo)=>{
  test.setTimeout(120000);
  const server=await isolatedServer(testInfo,c=>{c.match_defaults.starting_matter=2000;c.match_defaults.max_tick=2000;});
  review.afterClose(server.stop);
  {
    const [a,b]=await players(review,server.url);
    await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
    const start=await a.evaluate(()=>{const g=window.atemporal;const c=g.entities().find(e=>e.type_key==='constructor'&&e.owner===g.player);return {x:c.x,y:c.y};});
    expect(await a.evaluate(()=>window.atemporal.entities().some(e=>e.owner===1))).toBe(false);
    // Every bottom-row tile transforms into the unobstructed canvas after ordinary camera panning.
    const mini=await a.locator('#minimap').boundingBox();await a.mouse.click(mini.x+mini.width*.5,mini.y+mini.height-2);
    expect(await a.evaluate(()=>{const g=window.atemporal;const [,y]=g.renderer.screen(24,47.5);return y<document.getElementById('panels').getBoundingClientRect().top&&y>64;})).toBe(true);
    await review.capture('bottom-edge-uncovered',a);
    await tile(a,start);
    await expect(a.getByRole('button',{name:'Q Build units',exact:true})).toBeHidden();
    await expect(a.getByRole('button',{name:'B Build structure',exact:true})).toBeVisible();
    await a.keyboard.press('f');await tile(a,{x:start.x+1,y:start.y});
    await a.keyboard.press('c');await area(a,start);
    expect(await a.evaluate(()=>window.atemporal.draft.commands.filter(d=>d.command.kind==='assign_order').map(d=>d.command.order.kind))).toEqual(['construct']);
    expect(await a.evaluate(()=>window.atemporal.effectiveOrder(window.atemporal.selectedViews()[0]).kind)).toBe('construct');
    await a.keyboard.press('Control+z');expect(await a.evaluate(()=>window.atemporal.effectiveOrder(window.atemporal.selectedViews()[0]).kind)).toBe('attack_move');
    await a.keyboard.press('Control+u');expect(await a.evaluate(()=>window.atemporal.effectiveOrder(window.atemporal.selectedViews()[0]).kind)).toBe('construct');
    await a.keyboard.press('Control+z');await a.keyboard.press('Control+z');
    const build=await a.evaluate(()=>{const g=window.atemporal,c=g.selectedViews()[0];for(let y=Math.floor(c.y)-2;y<=c.y+2;y++)for(let x=Math.floor(c.x)-2;x<=c.x+2;x++)if(g.validPlacement({x,y},true))return{x,y};});
    await a.getByRole('button',{name:'B Build structure',exact:true}).click();await a.locator('#mode-options button').filter({hasText:'factory'}).click();
    const direction=await a.evaluate(()=>window.atemporal.renderer.outputDirection);await a.keyboard.press('r');expect(await a.evaluate(()=>window.atemporal.renderer.outputDirection)).not.toBe(direction);
    // Choose a valid output after rotation.
    await tile(a,build);await expect(a.locator('#draft-list')).toContainText('place factory');await tile(a,build);
    await a.getByRole('button',{name:'Q Build units',exact:true}).click();await a.locator('#mode-options button').filter({hasText:'grunt'}).click();
    await a.getByRole('button',{name:'P Priority',exact:true}).click();await a.locator('#mode-options button').filter({hasText:'High'}).click();
    await a.keyboard.press('f');await tile(a,{x:start.x+4,y:start.y+2});
    await expect(a.locator('#queue')).toContainText('queue: grunt');await expect(a.locator('#queue')).toContainText('high');await expect(a.locator('#queue')).toContainText('attack_move');
    await review.capture('ghost-production-configured',a);
    await tile(a,start);await a.keyboard.press('c');await area(a,build);
    await a.locator('#commit').click();await expect(a.locator('#top-phase')).toContainText('committed');await b.locator('#commit').click();await revision(a,1);
    await seek(a,153);
    const actual=await a.evaluate(()=>{const g=window.atemporal;return g.exact.state.entities.filter(e=>e.owner===0).map(e=>({type:e.type_key,priority:e.priority,order:e.action.kind,stored:e.production?.stored_order.kind}));});
    expect(actual).toContainEqual({type:'factory',priority:'high',order:'idle',stored:'attack_move'});
    expect(actual.some(e=>e.type==='grunt'&&e.order==='attack_move')).toBe(true);
    await tile(a,await a.evaluate(()=>{const e=window.atemporal.entities().find(e=>e.owner===0&&e.type_key==='constructor');return{x:e.x,y:e.y};}));await expect(a.locator('#timeline')).toHaveAttribute('data-order-ticks','0');
    await expect(a.locator('#top-bank')).toContainText('MATTER');await expect(a.locator('#top-bank')).toContainText('tick 153');
    const tl=await a.locator('#timeline').boundingBox();await a.mouse.move(tl.x+tl.width*.5,tl.y+20);await a.mouse.wheel(0,-500);
    const before=await a.evaluate(()=>({...window.atemporal.view}));await a.keyboard.down('Shift');await a.mouse.wheel(0,120);await a.keyboard.up('Shift');
    await expect.poll(()=>a.evaluate(()=>window.atemporal.view.t0)).not.toBe(before.t0);
    await a.mouse.move(700,350);const camera=await a.evaluate(()=>({...window.atemporal.renderer.camera}));await a.keyboard.down('Shift');await a.mouse.wheel(0,120);await a.keyboard.up('Shift');
    await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.camera.y)).not.toBe(camera.y);
    expect(await a.evaluate(()=>window.atemporal.renderer.camera.scale)).toBe(camera.scale);
    await review.capture('configured-factory-live-and-timeline',a);
  }
});
