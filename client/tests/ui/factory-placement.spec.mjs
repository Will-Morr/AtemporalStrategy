import {test, expect} from './fixtures.mjs';
import {isolatedServer, players, revision, seek} from './game-helpers.mjs';

const directions = [['e', 1, 0], ['s', 0, 1], ['w', -1, 0], ['n', 0, -1]];

async function openMatch(review, testInfo) {
  const server = await isolatedServer(testInfo, config => {
    Object.assign(config.match_defaults, {max_tick: 180, stop_when_decided: false, starting_matter: 1500});
  }, content => {
    content.types.find(type => type.key === 'constructor').construction.rate = 20;
  });
  review.afterClose(server.stop);
  const [a, b] = await players(review, server.url);
  await a.getByRole('button', {name: 'Start match', exact: true}).click();
  await revision(a, 0);
  await revision(b, 0);
  const home = await a.evaluate(() => window.atemporal.exact.state.entities.find(e => e.owner === 0 && e.type_key === 'constructor').tile);
  return {a, b, home};
}

async function chooseBuilding(page, type) {
  await page.keyboard.press('b');
  await page.locator('#mode-options button').filter({hasText: type}).click();
}

async function point(page, tile) {
  return page.evaluate(({x, y}) => window.atemporal.renderer.screen(x + .5, y + .5), tile);
}

async function center(page, home) {
  await page.evaluate(({x, y}) => {
    const renderer = window.atemporal.renderer;
    renderer.camera.scale = 40;
    renderer.centerOn(x + .5, y + .5);
  }, home);
}

async function checkCanvas(page, sites, marker, gate = false) {
  await expect.poll(() => page.evaluate(({sites, marker, gate}) => {
    const renderer = window.atemporal.renderer;
    const ctx = document.getElementById('map').getContext('2d');
    const s = renderer.camera.scale;
    const matches = (worldX, worldY, target, tolerance) => {
      const [x, y] = renderer.screen(worldX, worldY);
      const {data} = ctx.getImageData(Math.floor(x) - 1, Math.floor(y) - 1, 3, 3);
      for (let i = 0; i < data.length; i += 4) {
        if (target.every((channel, c) => Math.abs(channel - data[i + c]) <= tolerance)) return true;
      }
      return false;
    };
    return sites.map(({body, output, dx, dy}) => {
      const inset = Math.max(2, s * .12) / s;
      const outline = matches(output.x + inset, output.y + .5, marker, 35);
      const bay = !gate || (matches(body.x + .5 + dx * .36, body.y + .5 + dy * .36, [215, 239, 255], 35) &&
        matches(body.x + .5 + dx * .26, body.y + .5 + dy * .26, [7, 16, 25], 35));
      return outline && bay;
    });
  }, {sites, marker, gate})).toEqual(sites.map(() => true));
}

test('factory placement marks a blocked output on the exact adjacent tile', async ({review}, testInfo) => {
  const {a, home} = await openMatch(review, testInfo);
  await center(a, home);
  const body = await a.evaluate(home => {
    const game = window.atemporal;
    for (let y = home.y - 3; y <= home.y + 3; y++) {
      for (let x = home.x - 3; x <= home.x + 3; x++) {
        if (!game.placementIssue('factory', [{x, y}]) && !game.entityAt({x: x + 1, y})) return {x, y};
      }
    }
    throw new Error('No east-facing factory site near opening constructor');
  }, home);
  const output = {x: body.x + 1, y: body.y};
  await chooseBuilding(a, 'turret');
  await a.mouse.click(...await point(a, output));
  await chooseBuilding(a, 'factory');
  await a.mouse.move(...await point(a, body));
  await a.mouse.click(...await point(a, body));
  await expect(a.locator('#toast')).toContainText(/output.*turret/i);
  expect(await a.evaluate(() => window.atemporal.draft.commands.map(draft => draft.command.type_key))).toEqual(['turret']);
  await checkCanvas(a, [{body, output, dx: 1, dy: 0}], [255, 82, 82]);
  await review.capture('blocked-east-output-by-draft-turret', a);
});

test('factory loading bays and spawn markers follow all four rotations through playback', async ({review}, testInfo) => {
  const {a, b, home} = await openMatch(review, testInfo);
  await center(a, home);
  const sites = [];
  for (const [direction, dx, dy] of directions) {
    await chooseBuilding(a, 'factory');
    if (sites.length) await a.keyboard.press('r');
    const body = await a.evaluate(({home, sites, dx, dy}) => {
      const game = window.atemporal;
      for (let radius = 2; radius <= 4; radius++) {
        for (let y = home.y - radius; y <= home.y + radius; y++) {
          for (let x = home.x - radius; x <= home.x + radius; x++) {
            if (sites.some(({body}) => Math.max(Math.abs(body.x - x), Math.abs(body.y - y)) <= 1)) continue;
            if (game.entityAt({x, y}) || game.entityAt({x: x + dx, y: y + dy})) continue;
            if (!game.placementIssue('factory', [{x, y}])) return {x, y};
          }
        }
      }
      throw new Error('No clear factory site near opening constructor');
    }, {home, sites, dx, dy});
    const output = {x: body.x + dx, y: body.y + dy};
    const site = {direction, dx, dy, body, output};
    sites.push(site);
    await a.mouse.move(...await point(a, body));
    await a.waitForFunction(body => window.atemporal.hover?.x === body.x && window.atemporal.hover.y === body.y, body);
    await checkCanvas(a, [site], [129, 228, 173]);
    await review.capture(`placement-preview-${direction}`, a);
    await a.mouse.click(...await point(a, body));
  }
  await checkCanvas(a, sites, [215, 239, 255]);
  await review.capture('four-draft-factory-ghosts', a);
  for (const {body} of sites) {
    await a.mouse.click(...await point(a, body));
    await a.getByRole('button', {name: 'Queue grunt', exact: true}).click();
  }
  await a.mouse.click(...await point(a, home));
  await a.keyboard.press('c');
  const min = {x: Math.min(...sites.map(s => s.body.x)), y: Math.min(...sites.map(s => s.body.y))};
  const max = {x: Math.max(...sites.map(s => s.body.x)), y: Math.max(...sites.map(s => s.body.y))};
  await a.mouse.move(...await point(a, min));
  await a.mouse.down();
  await a.mouse.move(...await point(a, max));
  await a.mouse.up();
  await a.locator('#commit').click();
  await b.locator('#commit').click();
  await revision(a, 1);
  await a.waitForFunction(() => window.atemporal.rev().chunks.has(0));

  const stage = await a.evaluate(() => {
    const game = window.atemporal;
    const rev = game.rev();
    const ownFactories = sample => sample.entities.filter(e => rev.dictionary[e.index]?.owner === 0 && rev.dictionary[e.index]?.type_key === 'factory');
    const samples = [...rev.samples.values()].sort((a, b) => a.tick - b.tick);
    const site = samples.find(sample => ownFactories(sample).some(e => e.lifecycle === 'site'));
    const complete = samples.find(sample => {
      const factories = ownFactories(sample);
      return factories.length === 4 && factories.every(e => e.lifecycle === 'complete');
    });
    return {siteTick: site?.tick, builtTick: complete?.tick, headings: complete && ownFactories(complete).map(e => e.output_direction)};
  });
  expect(stage.siteTick).toBeGreaterThan(0);
  expect(stage.builtTick).toBeGreaterThan(stage.siteTick);
  expect(stage.headings).toEqual(sites.map(site => site.direction));
  await seek(a, stage.siteTick);
  await center(a, home);
  const underConstruction = await a.evaluate(() => window.atemporal.entities()
    .filter(e => e.owner === 0 && e.type_key === 'factory' && e.lifecycle === 'site')
    .map(e => [e.x, e.y, e.outputDirection]));
  expect(underConstruction.length).toBeGreaterThan(0);
  for (const [x, y, direction] of underConstruction) {
    expect(sites.find(site => site.body.x === x && site.body.y === y)?.direction).toBe(direction);
  }
  await checkCanvas(a, sites, [215, 239, 255]);
  await review.capture('construction-site-bay-and-output', a);
  await seek(a, stage.builtTick);
  await checkCanvas(a, sites, [215, 239, 255], true);
  await review.capture('completed-factory-bays-and-outputs', a);

  await a.locator('#play').click();
  await a.waitForFunction(() => {
    const game = window.atemporal;
    return game.playing && game.exact?.tick !== Math.floor(game.playhead) &&
      game.rawEntities().filter(e => e.owner === 0 && e.type_key === 'factory' && e.outputDirection).length === 4;
  });
  await checkCanvas(a, sites, [215, 239, 255], true);
  await a.locator('#play').click();
  await seek(a, stage.builtTick + 40);
  const actual = await a.evaluate(() => {
    const entities = window.atemporal.exact.state.entities.filter(e => e.owner === 0);
    return {
      outputs: entities.filter(e => e.type_key === 'factory').map(e => [e.tile, e.production.output_tile]),
      newborns: entities.filter(e => e.type_key === 'grunt' && e.born_at_tick !== null).map(e => e.tile),
    };
  });
  expect(actual.outputs).toEqual(sites.map(({body, output}) => [body, output]));
  expect(actual.newborns).toEqual(expect.arrayContaining(sites.map(site => site.output)));
  await review.capture('newborns-on-four-marked-output-tiles', a);
});
