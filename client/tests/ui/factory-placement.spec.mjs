import {readFile, readdir} from 'node:fs/promises';
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
  return {a, b, home, server};
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
  await a.waitForFunction(() => window.atemporal.rev().samples.has(45));

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

async function eastFacingSites(page, home, count) {
  return page.evaluate(({home, count}) => {
    const game = window.atemporal;
    const sites = [];
    for (let y = home.y - 4; y <= home.y + 4; y++) {
      for (let x = home.x - 4; x <= home.x + 4; x++) {
        if (sites.some(site => Math.abs(site.x - x) <= 1 && site.y === y)) continue;
        if (game.entityAt({x, y}) || game.entityAt({x: x + 1, y})) continue;
        if (game.placementIssue('factory', [{x, y}])) continue;
        sites.push({x, y});
        if (sites.length === count) return sites;
      }
    }
    throw new Error('Not enough clear sites for independent east-facing factories');
  }, {home, count});
}

async function ownBlueprints(page) {
  return page.evaluate(() => window.atemporal.exact.state.blueprints.filter(b => b.owner === 0));
}

test('replacing a draft factory preserves independent targeting in one clock tick', async ({review}, testInfo) => {
  const {a, b, home} = await openMatch(review, testInfo);
  await center(a, home);
  const [first, removed, retained, added] = await eastFacingSites(a, home, 4);
  await a.evaluate(() => {
    const now = Date.now;
    window.restoreDraftClock = () => { Date.now = now; };
    Date.now = () => 1234567890000;
  });
  const stage = async body => {
    await chooseBuilding(a, 'factory');
    await a.mouse.click(...await point(a, body));
  };
  const draftCommands = () => a.evaluate(() => window.atemporal.draft.commands);
  for (const body of [first, removed, retained]) await stage(body);
  const original = await draftCommands();
  await a.mouse.click(...await point(a, removed));
  await a.getByRole('button', {name: 'Delete selected blueprints', exact: true}).click();
  await stage(added);
  await a.evaluate(() => window.restoreDraftClock());

  const placements = await draftCommands();
  expect(placements.map(draft => draft.command.tiles[0])).toEqual([first, retained, added]);
  expect(new Set(placements.map(draft => draft.local_id)).size).toBe(3);
  expect(placements[2].local_id).not.toBe(original[1].local_id);
  await a.keyboard.press('Control+z');
  expect(await draftCommands()).toEqual([original[0], original[2]]);
  await a.keyboard.press('Control+z');
  expect(await draftCommands()).toEqual(original);
  await a.keyboard.press('Control+u');
  await a.keyboard.press('Control+u');
  expect(await draftCommands()).toEqual(placements);

  await a.mouse.click(...await point(a, added));
  await a.getByRole('button', {name: 'Queue grunt', exact: true}).click();
  const configured = await draftCommands();
  await a.keyboard.press('Control+z');
  expect(await draftCommands()).toEqual(placements);
  await expect(a.locator('#queue')).toContainText('Queue empty');
  await a.keyboard.press('Control+u');
  expect(await draftCommands()).toEqual(configured);
  await expect(a.locator('#queue')).toContainText('grunt');
  await a.mouse.click(...await point(a, retained));
  await expect(a.locator('#queue')).toContainText('Queue empty');
  await review.capture('retained-factory-keeps-empty-queue', a);

  await a.locator('#commit').click();
  await b.locator('#commit').click();
  await revision(a, 1);
  await seek(a, 1);
  const blueprints = await ownBlueprints(a);
  expect(blueprints.map(blueprint => [blueprint.tile, blueprint.settings?.queue ?? []])).toEqual([
    [first, []], [retained, []], [added, ['grunt']],
  ]);
});

test('redo survives an empty draft reload and a new edit discards it', async ({review}, testInfo) => {
  const {a, home} = await openMatch(review, testInfo);
  await center(a, home);
  const [first, second] = await eastFacingSites(a, home, 2);
  const rows = a.locator('#draft-list li');
  const commands = () => a.evaluate(() => window.atemporal.draft.commands);
  const place = async site => {
    await chooseBuilding(a, 'factory');
    await a.mouse.click(...await point(a, site));
    await expect(rows).toHaveCount(1);
  };

  await place(first);
  const original = await commands();
  await a.keyboard.press('Control+z');
  await expect(rows).toHaveCount(0);
  await a.reload();
  await revision(a, 0);
  await expect(rows).toHaveCount(0);
  await a.keyboard.press('Control+u');
  await expect(rows).toHaveCount(1);
  expect(await commands()).toEqual(original);
  await review.capture('factory-draft-redone-after-empty-reload', a);

  await a.keyboard.press('Control+z');
  await expect(rows).toHaveCount(0);
  await center(a, home);
  await place(second);
  const replacement = await commands();
  expect(replacement[0].command.tiles).toEqual([second]);
  expect(replacement[0].local_id).not.toBe(original[0].local_id);
  await a.keyboard.press('Control+u');
  expect(await commands()).toEqual(replacement);
  await a.reload();
  await revision(a, 0);
  await expect(rows).toHaveCount(1);
  await a.keyboard.press('Control+u');
  expect(await commands()).toEqual(replacement);
});

test('reload preserves distinct legacy blueprint placements and reports an ambiguous edit', async ({review}, testInfo) => {
  const {a, b, home, server} = await openMatch(review, testInfo);
  const matchId = await a.evaluate(() => window.atemporal.net.matchId);
  const reports = `${server.dir}/replays/${matchId}/checker-incidents`;
  const sites = await eastFacingSites(a, home, 3);
  await a.evaluate(sites => {
    const game = window.atemporal;
    const placement = (local_id, body) => ({
      local_id, future_orders: 'keep', command: {
        kind: 'place_blueprints', type_key: 'factory', tiles: [body], priority: 'medium', output_directions: ['e'],
      },
    });
    const configure = (local_id, target, unit) => ({
      local_id, future_orders: 'keep', command: {
        kind: 'configure_blueprints',
        blueprint_ids: [{kind: 'draft', local_id: target, item_index: 0}],
        settings: {queue: [unit], queue_loop_flags: [], order: {kind: 'idle'}, priority: 'medium', loop_enabled: false},
      },
    });
    const commands = [
      placement('aliased', sites[0]), placement('aliased', sites[1]), placement('distinct', sites[2]),
      configure('ambiguous-edit', 'aliased', 'miner'), configure('retained-edit', 'distinct', 'grunt'),
    ];
    sessionStorage.setItem(game.draftKey, JSON.stringify({
      round: game.round, draft: {
        states: [{keep: 0, append: commands.slice(0, 1), tick: 0}, {keep: 1, append: commands.slice(1), tick: 0}],
        current: 1, policy: 'keep', selected: null,
      },
    }));
  }, sites);
  await a.reload();
  await revision(a, 0);
  await expect.poll(() => a.evaluate(() => window.atemporal.draft.commands.length)).toBe(4);
  await expect(a.locator('#draft-recovery')).toContainText('Removed 1 blueprint or queue reference that could not be matched safely');
  const backup = await a.evaluate(() => JSON.parse(sessionStorage.getItem(window.atemporal.draftKey)).recovery.original);
  expect(backup.states.flatMap(state => state.append.map(entry => entry.local_id))).toEqual([
    'aliased', 'aliased', 'distinct', 'ambiguous-edit', 'retained-edit',
  ]);
  let incidentFiles;
  await expect.poll(async () => {
    incidentFiles = await readdir(reports).catch(() => []);
    return incidentFiles.length;
  }).toBe(1);
  const incidentPath = `${reports}/${incidentFiles[0]}`;
  const incidentJson = await readFile(incidentPath, 'utf8');
  const report = JSON.parse(incidentJson);
  expect(report.checker.failure).toBe('draft_recovery');
  expect(report.checker.reason).toContain('Draft commands 1 and 2 share a local ID');
  expect(report.state.tick).toBe(0);
  const reported = report.checker.draft.commands;
  expect(reported).toHaveLength(5);
  expect(reported[0].local_id).toBe(reported[1].local_id);
  expect(reported[0].local_id).not.toBe('aliased');
  expect(reported[3].command.blueprint_ids[0].local_id).toBe(reported[0].local_id);
  await a.keyboard.press('Control+z');
  await expect.poll(() => a.evaluate(() => window.atemporal.draft.commands.length)).toBe(1);
  await a.keyboard.press('Control+u');
  await expect.poll(() => a.evaluate(() => window.atemporal.draft.commands.length)).toBe(4);
  await a.reload();
  await revision(a, 0);
  await expect(a.locator('#draft-recovery')).toContainText('Removed 1 blueprint or queue reference');
  const restored = await a.evaluate(() => window.atemporal.draft.commands);
  expect(new Set(restored.map(draft => draft.local_id)).size).toBe(4);
  expect(restored.slice(0, 3).map(draft => draft.command.tiles[0])).toEqual(sites);
  expect(restored[3].command.blueprint_ids.map(ref => ref.local_id)).toEqual([restored[2].local_id]);
  expect(restored[3].command.settings.queue).toEqual(['grunt']);
  await review.capture('restored-draft-drops-ambiguous-edit', a);

  await a.locator('#commit').click();
  await expect(a.locator('#commit')).toHaveText('Uncommit · edit my moves');
  await a.locator('#commit').click();
  await expect.poll(() => a.evaluate(() => window.atemporal.draft.commands)).toEqual(restored);
  await expect(a.locator('#draft-recovery')).toBeHidden();
  await a.locator('#commit').click();
  await expect(a.locator('#commit')).toHaveText('Uncommit · edit my moves');
  await b.locator('#commit').click();
  await revision(a, 1);
  await seek(a, 1);
  expect((await ownBlueprints(a)).map(blueprint => [blueprint.tile, blueprint.settings?.queue ?? []])).toEqual([
    [sites[0], []], [sites[1], []], [sites[2], ['grunt']],
  ]);
  await server.stop();
  await server.start(['--resume', matchId]);
  await a.reload();
  await revision(a, 1);
  expect(await readdir(reports)).toEqual(incidentFiles);
  expect(await readFile(incidentPath, 'utf8')).toBe(incidentJson);
});
