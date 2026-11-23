import {mkdir, readFile, readdir, writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {test, expect} from './fixtures.mjs';
import {isolatedPeripheral, isolatedServer, players, revision, seek, tile} from './game-helpers.mjs';

async function openMatch(review, testInfo) {
  const launch = process.env.ATEMPORAL_UI_PERIPHERAL ? isolatedPeripheral : isolatedServer;
  const server = await launch(testInfo, config => {
    config.match_defaults.max_tick = 150;
    config.match_defaults.simulation_threads = 1;
  });
  review.afterClose(server.stop);
  const [player, opponent] = await players(review, server.url);
  await player.getByRole('button', {name: 'Start match', exact: true}).click();
  await revision(player, 0);
  await revision(opponent, 0);
  const match = await player.evaluate(() => window.atemporal.net.matchId);
  return {player, opponent, server, match, archive: join(server.dir, 'replays', match)};
}

async function openPlots(page) {
  return page.evaluate(() => {
    const game = window.atemporal;
    const {tile: home} = game.exact.state.entities.find(e => e.owner === 0 && e.type_key === 'constructor');
    const sites = [];
    for (let y = home.y - 3; y <= home.y + 3; y++) {
      for (let x = home.x - 3; x <= home.x + 3; x++) {
        if (game.entityAt({x, y}) || game.placementIssue('turret', [{x, y}])) continue;
        sites.push({x, y});
        if (sites.length === 2) return sites;
      }
    }
    throw new Error('Could not find two building plots');
  });
}

async function place(page, type, body) {
  await page.keyboard.press('b');
  await page.locator('#mode-options button').filter({hasText: type}).click();
  await tile(page, body);
}

const REPORT = /checker-[a-f0-9]{24}/;

async function reportFromToast(page, archive) {
  await expect(page.locator('#toast')).toContainText('this is a failure on the checker, reported');
  await expect(page.locator('#toast')).toHaveClass(/\bactive\b/);
  const text = await page.locator('#toast').textContent();
  const id = text.match(REPORT)?.[0];
  expect(id).toBeTruthy();
  const raw = await readFile(join(archive, 'checker-incidents', `${id}.json`), 'utf8');
  return {id, raw, record: JSON.parse(raw)};
}

test('rejected checked commit saves exact checker evidence without disclosing it to players', async ({review}, testInfo) => {
  const {player, server, match, archive} = await openMatch(review, testInfo);
  const [first, second] = await openPlots(player);
  await place(player, 'turret', first);
  await place(player, 'turret', second);
  const originalIds = await player.evaluate(() => window.atemporal.draft.commands.map(draft => draft.local_id));
  const secret = 'browser-private-alias';
  await player.evaluate(secret => {
    const draft = window.atemporal.draft;
    draft.commands[0].local_id = secret;
    draft.commands[1].local_id = secret;
  }, secret);
  await player.locator('#commit').click();
  await expect(player.locator('#toast')).toContainText('Draft commands 1 and 2 share a local ID');
  const {id, raw, record} = await reportFromToast(player, archive);
  expect(record.report_id).toBe(id);
  expect(record.checker).toMatchObject({
    match_id: match, round: 1, player: 0, revision: 0, tick: 0, failure: 'compiler_rejected',
  });
  expect(record.checker.draft.commands.map(d => d.command.tiles[0])).toEqual([first, second]);
  expect(record.checker.draft.commands[0].local_id).toBe(record.checker.draft.commands[1].local_id);
  expect(record.state).toEqual(await player.evaluate(() => window.atemporal.exact.state));
  expect(record.config.max_tick).toBe(150);
  expect(record.checker.fingerprint.sim_build).toContain('rules 15');
  expect(raw.includes(secret)).toBe(false);
  expect(raw.includes('slot_token')).toBe(false);
  expect(await player.evaluate(raw => raw.includes(window.atemporal.session.token), raw)).toBe(false);
  for (const path of [`checker-incidents/${id}.json`, `replays/${match}/checker-incidents/${id}.json`]) {
    const response = await player.request.get(server.url + path);
    expect(response.status()).toBe(404);
  }
  await expect(player.locator('#draft-list li')).toHaveCount(2);
  await review.capture('reported-compiler-mismatch-retains-draft', player);

  // A checked commit can also fail the independent exact-state check after compilation.
  await player.evaluate(({first, originalIds}) => {
    const game = window.atemporal;
    const drafts = game.draft.commands;
    drafts.forEach((draft, index) => { draft.local_id = originalIds[index]; });
    drafts[1].command.tiles = [first];
    game.blueprintDraftIssue = () => null;
  }, {first, originalIds});
  await player.locator('#commit').click();
  await expect(player.locator('#toast')).toContainText('A turret already occupies that tile');
  const exactFailure = await reportFromToast(player, archive);
  expect(exactFailure.id).not.toBe(id);
  expect(exactFailure.record.checker.failure).toBe('state_rejected');
  await expect(player.locator('#draft-list li')).toHaveCount(2);
  await review.capture('reported-exact-state-mismatch-retains-draft', player);
});

test('ordinary placement conflict stays local, but a false veto reports and continues a legal plan', async ({review}, testInfo) => {
  const {player, opponent, archive} = await openMatch(review, testInfo);
  const [first, second] = await openPlots(player);
  await player.evaluate(() => {
    const game = window.atemporal;
    game.net.on('draft_check_compared', reply => { window.lastDraftComparison = reply; });
  });
  await place(player, 'turret', first);
  await place(player, 'turret', first);
  await expect(player.locator('#toast')).toContainText('already holds a turret blueprint');
  await expect.poll(() => player.evaluate(() => window.lastDraftComparison?.issue)).toBe(null);
  expect(await readdir(archive)).not.toContain('checker-incidents');
  await player.keyboard.press('Escape');

  const injectedReason = 'Browser checker incorrectly marked this clear tile unavailable.';
  await player.evaluate(reason => {
    window.atemporal.placementPlanAtPlayhead = () => ({issue: () => reason});
  }, injectedReason);
  await place(player, 'turret', second);
  await expect(player.locator('#toast')).toContainText(injectedReason);
  const {raw, record} = await reportFromToast(player, archive);
  expect(record.checker).toMatchObject({failure: 'client_rejected', client_check_source: 'placement'});
  expect(record.checker.draft.commands.map(d => d.command.tiles[0])).toEqual([first, second]);
  expect(raw.includes(injectedReason)).toBe(false);
  await expect(player.locator('#draft-list li')).toHaveCount(2);
  await review.capture('reported-false-placement-veto', player);

  await player.keyboard.press('Escape');
  const originalCommands = await player.evaluate(() => window.atemporal.draft.commands);
  await seek(player, 5);
  await player.evaluate(() => {
    window.atemporal.blueprintDraftIssue = () => 'Browser checker incorrectly refused to rebase this draft.';
  });
  await player.locator('#actions .planning>summary').click();
  await player.getByRole('button', {name: 'Rebase draft here', exact: true}).click();
  await expect(player.locator('#toast')).toContainText('incorrectly refused to rebase');
  const rebase = await reportFromToast(player, archive);
  expect(rebase.record.checker.client_check_source).toBe('rebase');
  expect(await player.evaluate(() => window.atemporal.draft.tick)).toBe(5);
  expect(await player.evaluate(() => window.atemporal.draft.commands)).toEqual(originalCommands);
  await review.capture('reported-false-rebase-veto', player);

  await player.evaluate(() => {
    window.atemporal.blueprintDraftIssue = () => 'Browser checker incorrectly refused to submit this draft.';
  });
  await player.locator('#commit').click();
  await expect(player.locator('#toast')).toContainText('incorrectly refused to submit');
  const preflight = await reportFromToast(player, archive);
  expect(preflight.record.checker.client_check_source).toBe('commit_preflight');
  expect(new Set([record.report_id, rebase.id, preflight.id]).size).toBe(3);
  await expect(player.locator('#toast')).toContainText('Turn committed');
  await review.capture('reported-false-commit-preflight-veto', player);
  await opponent.locator('#commit').click();
  await revision(player, 1);
  await seek(player, 6);
  expect(await player.evaluate(() => window.atemporal.exact.state.blueprints
    .filter(b => b.owner === 0).map(b => b.tile))).toEqual([first, second]);
});

test('a late checker approval cannot stage a placement the player cancelled', async ({review}, testInfo) => {
  const {player, archive} = await openMatch(review, testInfo);
  const [clear] = await openPlots(player);
  await player.evaluate(() => {
    const game = window.atemporal;
    const send = game.net.request.bind(game.net);
    const hold = new Promise(resolve => { window.releaseChecker = resolve; });
    game.net.request = async (...args) => {
      const reply = await send(...args);
      if (args[0].kind === 'compare_draft_check') await hold;
      return reply;
    };
    game.placementPlanAtPlayhead = () => ({issue: () => 'Browser checker incorrectly rejected this tile.'});
  });
  await place(player, 'turret', clear);
  await player.keyboard.press('Escape');
  await player.evaluate(() => window.releaseChecker());
  await reportFromToast(player, archive);
  await expect(player.locator('#toast')).toContainText('plan changed before the action could be retried');
  await expect(player.locator('#draft-list li')).toHaveCount(0);
  await review.capture('reported-late-approval-does-not-change-draft', player);
});

test('a failed incident write never claims the checker was reported', async ({review}, testInfo) => {
  const {player, archive, server} = await openMatch(review, testInfo);
  const [clear] = await openPlots(player);
  await mkdir(archive, {recursive: true});
  await writeFile(join(archive, 'checker-incidents'), 'test fixture blocks directory creation');
  await player.evaluate(() => {
    window.atemporal.placementPlanAtPlayhead = () => ({issue: () => 'Browser checker incorrectly rejected this tile.'});
  });
  await place(player, 'turret', clear);
  await expect(player.locator('#toast')).toContainText('the report could not be saved');
  await expect(player.locator('#toast')).not.toContainText('checker, reported');
  await expect.poll(async () => await readFile(join(server.dir, 'server.log'), 'utf8')).toMatch(
    /checker_incident status=save_failed report=checker-[a-f0-9]{24}/
  );
  await review.capture('checker-write-failure-is-explicit', player);
});
