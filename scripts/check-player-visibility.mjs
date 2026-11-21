import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {build} from '../client/node_modules/esbuild/lib/main.js';

const root = new URL('../', import.meta.url);
const read = path => readFile(new URL(path, root), 'utf8').then(JSON.parse);
const fixture = await read('fixtures/gameplay/player-visibility.json');
const content = await read('config/content.yaml');
const out = await build({entryPoints:[new URL('client/src/vision.ts', root).pathname],bundle:true,write:false,format:'esm'});
const {allied, SightMemory, GroundVision, playerSeesEntity, playerVisibility} =
  await import(`data:text/javascript;base64,${Buffer.from(out.outputFiles[0].text).toString('base64')}`);

const types = new Map(content.types.map(type => [type.key, type]));
for (const [type, radius] of Object.entries(fixture.observer_radii)) {
  if (type === 'satellite') types.get(type).missile.radius = radius;
  else types.get(type).vision = radius;
}
const terrain = {
  width:fixture.terrain[0].length,
  height:fixture.terrain.length,
  cells:fixture.terrain.flatMap(row => [...row].map(cell => cell === '#' ? 'wall' : 'floor')),
};
const ground = new GroundVision();
const memory = new SightMemory(terrain.width, terrain.height);
const silo = {...fixture.entities.find(e => e.type_key === 'silo'), index:4, hp:150, silo:{}, exact:{}, engaged:2};
const mask = tick => playerVisibility(ground, terrain, fixture.entities, fixture, types, fixture.multiplayer, 0, tick, false);
for (const {tick, visible, hidden} of fixture.frames) {
  const tiles = mask(tick);
  for (const tile of visible) assert.ok(tiles.has(tile), `tick ${tick} should see ${tile}`);
  for (const tile of hidden) assert.ok(!tiles.has(tile), `tick ${tick} should hide ${tile}`);
}
const alliedSite = fixture.entities.find(e => e.lifecycle === 'site');
assert.ok(allied(fixture.multiplayer, 0, alliedSite.owner));
assert.ok(!playerSeesEntity(0, mask(1), alliedSite), 'an unfinished allied site remains hidden in fog');
assert.ok(playerSeesEntity(0, mask(1), {...alliedSite,x:5,y:6}), 'allied vision reveals sites in its footprint');
assert.ok(playerSeesEntity(0, mask(1), {...alliedSite,owner:0}), 'the owner can always see their own site');
for (let tick = 1; tick <= 5; tick++) {
  if (tick === 3) silo.hp = 90;
  // The building disappears out of sight at 3. The empty tile is confirmed at 4.
  memory.observe(tick, mask(tick), tick < 3 && mask(tick).has('6,1') ? [silo] : []);
}
assert.deepEqual(memory.remembered(1, mask(1)), []);
const [lastSeen] = memory.remembered(3, mask(3));
assert.equal(lastSeen.hp, 150);
assert.equal(lastSeen.exact, undefined);
assert.equal(lastSeen.silo, undefined);
assert.equal(lastSeen.engaged, null);
assert.deepEqual(memory.remembered(5, mask(5)), []);
assert.deepEqual(new SightMemory(terrain.width, terrain.height).remembered(3, mask(3)), []);
const reverse = new SightMemory(terrain.width, terrain.height);
reverse.observe(4, mask(4), [], true);
reverse.observe(2, mask(2), [silo], true);
assert.equal(reverse.remembered(3, mask(3)).length, 1);
assert.deepEqual(reverse.remembered(5, mask(5)), [], 'the exact empty observation at tick 4 survives a rewind');
const scoutMemory = new SightMemory(terrain.width, terrain.height);
const scout = {...silo, id:{opening:1}, type_key:'scout', x:1, y:1, facing:'n'};
const fog = new Set();
scoutMemory.observe(8, new Set(['3,1']), [], true);
scoutMemory.observe(2, new Set(['1,1']), [scout], true);
scoutMemory.observe(5, new Set(['3,1']), [{...scout,x:3,facing:'e'}]);
assert.deepEqual(scoutMemory.remembered(6, fog).map(e => [e.x,e.facing]), [[3,'e']]);
assert.deepEqual(scoutMemory.remembered(9, fog), [], 'an empty tile clears the most recent scout sighting');
assert.deepEqual(scoutMemory.remembered(4, fog).map(e=>e.x), [1], 'rewinding restores the old position');
assert.deepEqual(scoutMemory.remembered(4, fog, new Set([JSON.stringify(scout.id)])), [], 'a currently visible scout has no ghost');
scoutMemory.observe(5, fog, [], true);
assert.deepEqual(scoutMemory.remembered(6, fog).map(e=>e.x), [1], 'exact sight supersedes a sampled location');

console.log('Player visibility: walls, team sight, flight/recon boundaries and time-indexed scout and building sightings passed.');
