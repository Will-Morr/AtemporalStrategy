import {test, expect} from './fixtures.mjs';
import {isolatedServer, players, revision, seek, tile} from './game-helpers.mjs';

// Read the pale outer barrel pixels, beyond the radius of the round turret cap.
async function barrel(page, shot) {
  return page.evaluate(shot => {
    const renderer = window.atemporal.renderer;
    renderer.centerOn(shot.x, shot.y);
    renderer.draw();
    const scale = renderer.camera.scale;
    const [cx, cy] = renderer.screen(shot.x + .5, shot.y + .5);
    const size = Math.ceil(scale);
    const left = Math.floor(cx - size / 2), top = Math.floor(cy - size / 2);
    const pixels = document.querySelector('#map').getContext('2d').getImageData(left, top, size, size).data;
    let dx = 0, dy = 0, count = 0;
    for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
      const i = (y * size + x) * 4;
      const vx = left + x + .5 - cx, vy = top + y + .5 - cy;
      if (Math.hypot(vx, vy) < scale * .28) continue;
      if (pixels[i] === 215 && pixels[i + 1] === 239 && pixels[i + 2] === 255) {
        dx += vx; dy += vy; count++;
      }
    }
    return {angle: Math.atan2(dy, dx), count};
  }, shot);
}

test('tank, artillery and fixed turret barrels face their recorded targets when seeking', async ({review}, testInfo) => {
  const server = await isolatedServer(testInfo, config => {
    Object.assign(config.match_defaults, {max_tick: 500, stop_when_decided: false});
  }, content => {
    content.starting_roster = ['tank', 'artillery', 'turret'];
    for (const unit of content.types.filter(type => content.starting_roster.includes(type.key))) {
      unit.max_hp = 900;
      if (unit.key === 'turret') {
        unit.vision = 16;
        unit.weapon.range = 16;
        unit.weapon.indirect = true;
        unit.weapon.damage = 1;
      }
    }
  });
  review.afterClose(server.stop);
  const [a, b] = await players(review, server.url);
  await a.getByRole('button', {name: 'Start match', exact: true}).click();
  await revision(a, 0); await revision(b, 0);
  const roster = page => page.evaluate(() => window.atemporal.entities().map(({x,y,id,owner,type_key}) => ({x,y,id,owner,type_key})));
  const home = await roster(a), enemy = await roster(b);
  expect(home.every(e => e.owner === 0)).toBe(true);
  for (const unit of home.filter(e => e.type_key !== 'turret')) {
    await tile(a, unit);
    await a.keyboard.press('f');
    await tile(a, enemy.find(e => e.type_key === 'turret'));
  }
  await a.locator('#commit').click(); await b.locator('#commit').click();
  await revision(a, 1);
  const viewer = await (await review.newContext('observer')).newPage();
  await viewer.goto(server.url); await viewer.locator('#spectate').click(); await revision(viewer, 1);
  await viewer.evaluate(() => window.atemporal.renderer.zoomAt(900,400,10));
  // Read authoritative events, including those outside the current streamed viewport.
  const attacks = await viewer.evaluate(async () => {
    const g = window.atemporal;
    const result = await g.net.request({kind:'get_events', revision:1, from_tick:0, to_tick:g.rev().outcome.terminal_state_tick, effects_only:true}, 'events');
    return result.events.filter(e => e.event.kind === 'attack');
  });
  await seek(viewer,0);
  for (const kind of ['tank', 'artillery', 'turret']) {
    const defender = enemy.find(e => e.type_key === kind);
    const shots = attacks.filter(e => JSON.stringify(e.event.attacker_id) === JSON.stringify(defender.id));
    expect(shots.length, kind + ' participated in combat').toBeGreaterThan(0);
    // Compare the idle barrel at tick zero with the shot furthest from its idle angle.
    const opening = await barrel(viewer, defender);
    expect(opening.count).toBeGreaterThan(2);
    const delta = (a, b) => Math.abs(Math.atan2(Math.sin(a-b), Math.cos(a-b)));
    const aim = ({event}) => {
      const dx=event.target_tile.x-event.source_tile.x, dy=event.target_tile.y-event.source_tile.y;
      if(event.visual_style!=='artillery')return Math.atan2(dy,dx);
      const length=Math.hypot(dx,dy), bow=Math.sign(dx||1)*Math.min(2,length*.22);
      return Math.atan2(dy/2-dx/length*bow,dx/2+dy/length*bow);
    };
    if(kind==='turret'){
      const firstShot=shots[0];
      await seek(viewer, firstShot.tick-1);
      const before=await barrel(viewer, defender);
      await viewer.locator('#play').click();
      await expect.poll(async()=>delta((await barrel(viewer, defender)).angle,before.angle)).toBeGreaterThan(.2);
      await viewer.locator('#play').click();
    }
    shots.sort((l,r) => delta(aim(r),opening.angle) - delta(aim(l),opening.angle));
    const shot = shots[0];
    await seek(viewer, shot.tick);
    await expect.poll(async () => delta((await barrel(viewer, shot.event.source_tile)).angle, aim(shot))).toBeLessThan(.12);
    await review.capture(kind + '-aim-at-tick-' + shot.tick, viewer);
    await seek(viewer, 0);
    expect(delta((await barrel(viewer, defender)).angle, opening.angle)).toBeLessThan(.03);
    expect(delta(opening.angle, aim(shot)), kind + ' barrel moved independently').toBeGreaterThan(.14);
  }
  await seek(viewer,1);
  const attacker=(await roster(viewer)).find(e=>e.owner===0 && e.type_key==='tank');
  const goal=enemy.find(e=>e.type_key==='turret');
  await tile(viewer, attacker);
  await expect.poll(()=>viewer.evaluate(()=>window.atemporal.renderer.orderPreviews.routes[0]?.points.at(-1))).toEqual({x:goal.x,y:goal.y});
  await viewer.evaluate(()=>window.atemporal.renderer.fit());
  await review.capture('attack-move-preview-to-enemy-turret',viewer);
  await viewer.keyboard.press('Escape');
  await seek(a, 0);
  expect((await roster(a)).every(e => e.owner === 0)).toBe(true);
});
