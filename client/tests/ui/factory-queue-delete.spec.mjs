import {test, expect} from './fixtures.mjs';
import {isolatedServer, players, revision, seek, tile} from './game-helpers.mjs';

async function opening(review, testInfo) {
  const server = await isolatedServer(testInfo, config => {
    config.match_defaults.max_tick = 400;
    config.match_defaults.starting_matter = 5000;
  });
  review.afterClose(server.stop);
  const [a, b] = await players(review, server.url);
  await a.getByRole('button', {name: 'Start match', exact: true}).click();
  await revision(a, 0);
  await revision(b, 0);
  const constructor = await a.evaluate(() => window.atemporal.entities().find(e => e.owner === 0 && e.type_key === 'constructor'));
  return {a, b, constructor};
}

async function placeFactory(page, constructor) {
  await tile(page, constructor);
  const site = await page.evaluate(({x, y}) => {
    const g = window.atemporal;
    for (let row = y - 3; row <= y + 3; row++) {
      for (let col = x - 3; col <= x + 3; col++) {
        const location = {x: col, y: row};
        if (g.validPlacement(location, true)) return location;
      }
    }
  }, constructor);
  await page.keyboard.press('b');
  await page.locator('#mode-options button').filter({hasText: 'factory'}).click();
  await tile(page, site);
  await tile(page, site);
  return site;
}

async function selectBoth(page, sites) {
  await tile(page, sites[0]);
  const point = await page.evaluate(t => window.atemporal.renderer.screen(t.x + .5, t.y + .5), sites[1]);
  await page.keyboard.down('Shift');
  try {
    await page.mouse.click(...point);
  } finally {
    await page.keyboard.up('Shift');
  }
}

async function queues(page) {
  return page.evaluate(() => window.atemporal.selectedViews().map(v => v.settings.queue.length));
}

async function measure(page) {
  await page.evaluate(() => {
    const zero = {panelCalls: 0, panelMs: 0, draftWrites: 0, draftBytes: 0, largestDraft: 0, quotaErrors: 0};
    if (window.factoryDeleteStatistics) {
      Object.assign(window.factoryDeleteStatistics, zero);
      return;
    }
    const g = window.atemporal;
    const update = g.updatePanels;
    const setItem = Storage.prototype.setItem;
    const statistics = zero;
    window.factoryDeleteStatistics = statistics;
    g.updatePanels = function () {
      const start = performance.now();
      statistics.panelCalls++;
      try {
        return update.call(this);
      } finally {
        statistics.panelMs += performance.now() - start;
      }
    };
    Storage.prototype.setItem = function (key, value) {
      const record = this === sessionStorage && key === g.draftKey;
      if (record) {
        statistics.draftWrites++;
        statistics.draftBytes += value.length;
        statistics.largestDraft = Math.max(statistics.largestDraft, value.length);
      }
      try {
        return setItem.call(this, key, value);
      } catch (error) {
        if (record && error.name === 'QuotaExceededError') statistics.quotaErrors++;
        throw error;
      }
    };
  });
}

async function report(page, testInfo, name) {
  const statistics = await page.evaluate(() => window.factoryDeleteStatistics);
  await testInfo.attach(name, {body: JSON.stringify(statistics, null, 2), contentType: 'application/json'});
  return statistics;
}

async function fillStorage(page) {
  await page.evaluate(() => {
    // Use Chromium's actual quota and leave no room for a larger persisted draft.
    let low = 0;
    let high = 8 * 1024 * 1024;
    while (high - low > 1) {
      const size = Math.floor((low + high) / 2);
      try {
        sessionStorage.setItem('factory-delete-quota-test', 'x'.repeat(size));
        low = size;
      } catch (error) {
        if (error.name !== 'QuotaExceededError') throw error;
        high = size;
      }
    }
  });
}

test('factory queue supports repeated single and shared deletions', async ({review}, testInfo) => {
  const {a, constructor} = await opening(review, testInfo);
  const sites = [await placeFactory(a, constructor), await placeFactory(a, constructor)];
  const add = a.getByRole('button', {name: 'Queue grunt', exact: true});
  const size = 30;
  await tile(a, sites[0]);
  for (let i = 0; i < size / 5; i++) await add.click({modifiers: ['Shift']});
  expect(await queues(a)).toEqual([size]);
  await measure(a);
  const remove = a.getByRole('button', {name: 'Remove one queued grunt', exact: true});
  for (let i = 0; i < size; i++) {
    await remove.focus();
    await a.keyboard.press('Delete');
  }
  expect(await queues(a)).toEqual([0]);
  await expect(a.locator('#queue')).toContainText('Queue empty');
  await report(a, testInfo, 'single-delete-measurements');
  await selectBoth(a, sites);
  for (let i = 0; i < size / 5; i++) await add.click({modifiers: ['Shift']});
  expect(await queues(a)).toEqual([size, size]);
  await expect(a.locator('.shared-factories .queue-item')).toHaveCount(1);
  await expect(a.locator('.shared-factories .queue-item')).toContainText('grunt ×30');
  await a.getByRole('button', {name:'Loop shared grunt batch'}).click();
  expect(await a.evaluate(()=>window.atemporal.selectedViews().every(v=>v.settings.queue_loop_flags.every(Boolean)))).toBe(true);
  await review.capture('shared-queue-before-delete', a);
  await measure(a);
  for (let i = size; i > 0; i--) {
    await a.getByRole('button', {name: `Remove shared item ${i}`, exact: true}).click();
  }
  expect(await queues(a)).toEqual([0, 0]);
  const sharedMeasurements = await report(a, testInfo, 'shared-delete-measurements');
  expect(sharedMeasurements.panelCalls).toBe(size);
  expect(sharedMeasurements.draftWrites).toBeLessThanOrEqual(size);
  await a.keyboard.press('Control+z');
  expect(await queues(a)).toEqual([1, 1]);
  await a.reload();
  await revision(a, 0);
  await selectBoth(a, sites);
  expect(await queues(a)).toEqual([1, 1]);
  await a.keyboard.press('Control+u');
  expect(await queues(a)).toEqual([0, 0]);
  await review.capture('shared-queue-after-delete', a);
});

test('factory queue deletion falls back to saving the current draft without history', async ({review}, testInfo) => {
  const {a, constructor} = await opening(review, testInfo);
  const site = await placeFactory(a, constructor);
  const queue = a.locator('#queue');
  await a.getByRole('button', {name: 'Queue grunt', exact: true}).click({modifiers: ['Shift']});
  const loop = a.getByRole('button', {name: /Loop new items/});
  // A longer history leaves enough space to replace it with only the current draft.
  for (let i = 0; i < 20; i++) await loop.click();
  await measure(a);
  await fillStorage(a);
  await a.getByRole('button', {name: 'Remove one queued grunt', exact: true}).click();
  const measurements = await report(a, testInfo, 'compact-fallback-measurements');
  expect(measurements.quotaErrors).toBe(1);
  await expect(queue).toContainText('grunt ×4');
  const history = await a.evaluate(() => {
    const game = window.atemporal;
    const saved = JSON.parse(sessionStorage.getItem(game.draftKey));
    return {savedUndo: saved.draft.undo.length, liveUndo: game.draft.undo.length};
  });
  expect(history.savedUndo).toBe(0);
  expect(history.liveUndo).toBeGreaterThan(0);
  await a.reload();
  await revision(a, 0);
  await tile(a, site);
  await expect(queue).toContainText('grunt ×4');
});

test('factory queue deletion still works when no draft fits in storage', async ({review}, testInfo) => {
  const {a, constructor} = await opening(review, testInfo);
  const site = await placeFactory(a, constructor);
  const queue = a.locator('#queue');
  await a.getByRole('button', {name: 'Queue grunt', exact: true}).click({modifiers: ['Shift']});
  await expect(queue).toContainText('grunt ×5');
  await measure(a);
  await fillStorage(a);
  const remove = a.getByRole('button', {name: 'Remove one queued grunt', exact: true});
  await remove.click();
  const measurements = await report(a, testInfo, 'quota-delete-measurements');
  expect(measurements.quotaErrors).toBe(2);
  expect(await queues(a)).toEqual([4]);
  await expect(queue).toContainText('grunt ×4');
  await expect(a.locator('#toast')).toContainText('Browser storage is full or unavailable');
  await a.keyboard.press('Control+z');
  await expect(queue).toContainText('grunt ×5');
  await a.keyboard.press('Control+u');
  await expect(queue).toContainText('grunt ×4');
  expect(await a.evaluate(() => {
    const toast = document.querySelector('#toast').getBoundingClientRect();
    const orders = document.querySelector('#order-warnings').getBoundingClientRect();
    return toast.bottom > orders.top && toast.top < orders.bottom &&
      toast.right > orders.left && toast.left < orders.right;
  })).toBe(false);
  await review.capture('factory-queue-deleted-under-quota', a);
  await a.evaluate(() => sessionStorage.removeItem('factory-delete-quota-test'));
  await remove.click();
  await expect(queue).toContainText('grunt ×3');
  await a.reload();
  await revision(a, 0);
  await tile(a, site);
  await expect(queue).toContainText('grunt ×3');
});

test('completed factory queues retain the remaining items after deletion and commit', async ({review}, testInfo) => {
  const {a, b, constructor} = await opening(review, testInfo);
  const sites = [await placeFactory(a, constructor), await placeFactory(a, constructor)];
  await tile(a, constructor);
  await a.keyboard.press('c');
  const [from, to] = await a.evaluate(([first, second]) => {
    const renderer = window.atemporal.renderer;
    return [renderer.screen(first.x - 1, first.y - 1), renderer.screen(second.x + 2, second.y + 2)];
  }, sites);
  await a.mouse.move(...from);
  await a.mouse.down();
  await a.mouse.move(...to);
  await a.mouse.up();
  await a.locator('#commit').click();
  await b.locator('#commit').click();
  await revision(a, 1);
  await seek(a, 150);
  await tile(a, sites[0]);
  const add = a.getByRole('button', {name: 'Queue grunt', exact: true});
  await add.click({modifiers: ['Shift']});
  const remove = a.getByRole('button', {name: 'Remove one queued grunt', exact: true});
  await remove.focus();
  await a.keyboard.press('Delete');
  await expect(a.locator('#queue')).toContainText('grunt ×4');
  await a.getByRole('button', {name: 'Clear waiting queue', exact: true}).click();
  await selectBoth(a, sites);
  expect(await a.evaluate(() => window.atemporal.selectedViews().map(v => v.lifecycle))).toEqual(['complete', 'complete']);
  await add.click({modifiers: ['Shift']});
  await a.getByRole('button', {name: 'Remove shared item 5', exact: true}).click();
  await expect(a.getByRole('button', {name: 'Remove shared item 5', exact: true})).toHaveCount(0);
  await a.keyboard.press('Control+z');
  await expect(a.getByRole('button', {name: 'Remove shared item 5', exact: true})).toBeVisible();
  await a.keyboard.press('Control+u');
  await expect(a.getByRole('button', {name: 'Remove shared item 5', exact: true})).toHaveCount(0);
  await review.capture('completed-factories-after-delete', a);
  await a.locator('#commit').click();
  await b.locator('#commit').click();
  await revision(a, 2);
  await seek(a, 151);
  await selectBoth(a, sites);
  const remaining = await a.evaluate(() => window.atemporal.selectedViews().map(v => {
    const {pending_items, active_item} = v.exact.production;
    return pending_items.length + Number(!!active_item);
  }));
  expect(remaining).toEqual([4, 4]);
});
