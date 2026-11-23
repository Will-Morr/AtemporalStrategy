import { test, expect } from './fixtures.mjs';
import { isolatedServer, players, revision } from './game-helpers.mjs';

test('a quick reconnect stays quiet and a sustained outage warns until recovery', async ({review}, testInfo) => {
  const server = await isolatedServer(testInfo);
  review.afterClose(server.stop);
  const [page] = await players(review, server.url);
  await page.getByRole('button', {name:'Start match', exact:true}).click();
  await revision(page, 0);

  const banner = page.locator('#connection');
  const connected = () => page.evaluate(() => window.atemporal.net.connected);
  await page.evaluate(() => {
    const banner = document.querySelector('#connection');
    const toast = document.querySelector('#toast');
    window.connectionNoticeCount = 0;
    window.connectionToasts = [];
    new MutationObserver(records => {
      window.connectionNoticeCount += records.filter(record => record.oldValue !== null).length;
    }).observe(banner, {attributes:true, attributeFilter:['hidden'], attributeOldValue:true});
    new MutationObserver(() => {
      if (toast.classList.contains('active') && /connection closed|disconnected/i.test(toast.textContent)) {
        window.connectionToasts.push(toast.textContent);
      }
    }).observe(toast, {attributes:true, childList:true});
    window.atemporal.seek(111);
    window.atemporal.net.disconnect();
  });
  await expect.poll(connected).toBe(false);
  await expect.poll(() => page.evaluate(() => window.atemporal.exact?.tick)).toBe(111);
  await expect(banner).toBeHidden();
  expect(await page.evaluate(() => window.connectionNoticeCount)).toBe(0);
  expect(await page.evaluate(() => window.connectionToasts)).toEqual([]);

  await page.context().setOffline(true);
  await expect.poll(connected).toBe(false);
  await expect(banner).toContainText('Disconnected. Reconnecting');
  await expect(banner.getByRole('button', {name:'Reload current server'})).toBeVisible();
  await review.capture('sustained-connection-outage', page);
  await page.context().setOffline(false);
  await expect(banner).toBeHidden();
  await expect.poll(connected).toBe(true);
  await revision(page, 0);
  await review.capture('connection-recovered', page);
});
