import {expect, test} from './fixtures.mjs';
import {isolatedServer, players, revision} from './game-helpers.mjs';

test('obsolete exact replies and failures leave the current seek intact', async ({review}, testInfo) => {
  const server = await isolatedServer(testInfo);
  review.afterClose(server.stop);
  const [page] = await players(review, server.url);
  await page.getByRole('button', {name:'Start match', exact:true}).click();
  await revision(page, 0);

  await page.evaluate(() => {
    const game = window.atemporal;
    const request = game.net.request.bind(game.net);
    const held = new Map();
    const counts = new Map();
    window.exactRequests = {held, counts};
    game.net.request = (message, ...args) => {
      const result = request(message, ...args);
      if (message.kind !== 'get_exact_state' || ![111, 122, 133].includes(message.tick)) return result;
      counts.set(message.tick, (counts.get(message.tick) ?? 0) + 1);
      return result.then(response => new Promise((resolve, reject) => {
        held.set(message.tick, fail => fail ? reject(new Error('obsolete replay mismatch')) : resolve(response));
      }));
    };
    game.seek(111);
    game.seek(122);
    game.seek(133);
  });
  await expect.poll(() => page.evaluate(() => window.exactRequests.held.size)).toBe(3);
  await page.evaluate(async () => {
    window.exactRequests.held.get(111)(true);
    await new Promise(resolve => setTimeout(resolve, 0));
    window.atemporal.seek(133);
    window.exactRequests.held.get(133)(false);
  });
  await expect.poll(() => page.evaluate(() => window.atemporal.exact?.tick)).toBe(133);
  await page.evaluate(async () => {
    window.exactRequests.held.get(122)(false);
    await new Promise(resolve => setTimeout(resolve, 0));
  });
  expect(await page.evaluate(() => window.exactRequests.counts.get(133))).toBe(1);
  expect(await page.evaluate(() => window.atemporal.exact.tick)).toBe(133);
  await expect(page.locator('#top-exact')).toHaveText(/^exact 133 in /);
  await expect(page.locator('#top-phase')).toContainText('planning');
  await expect(page.locator('#toast')).toBeHidden();
  await review.capture('current-exact-seek-after-obsolete-replies', page);
});

test('reconnect retries the initial state and preserves a seek during bootstrap', async ({review}, testInfo) => {
  const server = await isolatedServer(testInfo);
  review.afterClose(server.stop);
  const [page] = await players(review, server.url);
  let droppedInitial = false;
  await page.routeWebSocket('**/ws', socket => {
    const upstream = socket.connectToServer();
    socket.onMessage(raw => {
      const {message} = JSON.parse(raw.toString());
      if (!droppedInitial && message.kind === 'get_exact_state' && message.tick === 0) {
        droppedInitial = true;
        socket.close();
        upstream.close();
        return;
      }
      upstream.send(raw);
    });
  });
  // Route installation applies to subsequent connections.
  await page.reload();
  await page.getByRole('button', {name:'Start match', exact:true}).click();
  await expect.poll(() => droppedInitial).toBe(true);
  await page.evaluate(() => window.atemporal.seek(111));
  await expect.poll(() => page.evaluate(() => window.atemporal.exact?.tick)).toBe(111);
  await expect(page.locator('#connection')).toBeHidden();
  await expect(page.locator('#top-phase')).toContainText('planning');
  await review.capture('initial-state-recovered-at-requested-tick', page);
});
