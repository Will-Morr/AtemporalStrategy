import {test,expect} from './fixtures.mjs';
import {isolatedServer,players,revision,tile,seek,area} from './game-helpers.mjs';

test('artillery silhouette and icons remain distinct from tanks at game zoom',async({review},testInfo)=>{
  const server=await isolatedServer(testInfo,c=>{c.match_defaults.max_tick=400;c.match_defaults.starting_matter=5000;});review.afterClose(server.stop);
  const [a,b]=await players(review,server.url);await a.getByRole('button',{name:'Start match',exact:true}).click();await revision(a,0);await revision(b,0);
  const constructor=await a.evaluate(()=>window.atemporal.entities().find(e=>e.owner===0&&e.type_key==='constructor'));
  await tile(a,constructor);await a.keyboard.press('b');await a.locator('#mode-options button').filter({hasText:'factory'}).click();
  const site=await a.evaluate(()=>{const g=window.atemporal,c=g.selectedViews()[0];for(let y=c.y-3;y<=c.y+3;y++)for(let x=c.x-3;x<=c.x+3;x++)if(g.validPlacement({x,y},true))return{x,y};});
  await tile(a,site);await tile(a,site);await a.getByRole('button',{name:'Queue artillery',exact:true}).click();await a.getByRole('button',{name:'Queue tank',exact:true}).click();await review.capture('artillery-and-tank-queue-icons',a);
  await tile(a,constructor);await a.keyboard.press('c');await area(a,site);await a.locator('#commit').click();await b.locator('#commit').click();await revision(a,1);await seek(a,300);
  const units=await a.evaluate(()=>window.atemporal.entities().filter(e=>e.owner===0&&['tank','artillery'].includes(e.type_key)));expect(units.map(e=>e.type_key).sort()).toEqual(['artillery','tank']);
  await tile(a,units.find(e=>e.type_key==='artillery'));
  for(const scale of [26,18,40]){await a.evaluate(scale=>{const g=window.atemporal,r=g.renderer,v=g.selectedViews()[0];r.camera.scale=scale;r.centerOn(v.x,v.y);},scale);await review.capture(`artillery-versus-tank-zoom-${scale}`,a);}
  await a.setViewportSize({width:1366,height:768});await a.evaluate(()=>{const g=window.atemporal,r=g.renderer,v=g.selectedViews()[0];r.camera.scale=26;r.centerOn(v.x,v.y);});await review.capture('artillery-laptop',a);
  await a.setViewportSize({width:1920,height:1080});
  for(const unit of units){
    await tile(a,unit);await a.keyboard.press('f');
    const goal=await a.evaluate(v=>({x:v.x+3,y:v.y+3}),unit);await tile(a,goal);
    await expect.poll(()=>a.evaluate(()=>window.atemporal.renderer.orderPreviews.routes.map(({rule,goal,points})=>({
      rule,goal,
      hasMultipleSteps:points.length>2,
      orthogonal:points.slice(1).every((p,i)=>Math.abs(p.x-points[i].x)+Math.abs(p.y-points[i].y)===1),
    })))).toEqual([{rule:'four',goal,hasMultipleSteps:true,orthogonal:true}]);
    for(const scale of [26,8,2]){await a.evaluate(scale=>{const g=window.atemporal,r=g.renderer,v=g.selectedViews()[0];r.camera.scale=scale;r.centerOn(v.x,v.y);},scale);await review.capture(`${unit.type_key}-orthogonal-path-and-inset-target-${scale}`,a);}
  }
  // Review the authored SVG in the generated guide as well as battlefield canvas rendering.
  const guide=await (await review.newContext('guide')).newPage();await guide.goto(`${server.url}guide/`);await guide.locator('.role-card').filter({hasText:'Artillery'}).scrollIntoViewIfNeeded();await review.capture('artillery-guide-icon',guide);await guide.screenshot({path:testInfo.outputPath('artillery-guide-viewport.png'),animations:'disabled'});
});
