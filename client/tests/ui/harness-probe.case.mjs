// Only selected by scripts/check-ui-harness.mjs. These MUST fail.
import { test, expect } from './fixtures.mjs';

test('probe assertion failure preserves evidence for each identity', async ({ review }) => {
  for (const name of ['player-a', 'player-b', 'spectator']) {
    const context = await review.newContext(name);
    const page = await context.newPage();
    await page.goto('/');
    await expect(page.getByRole('heading', { name: 'Atemporal Strategy', exact: true })).toBeVisible();
  }
  expect('actual', 'intentional assertion probe').toBe('expected');
});

test('probe HTTP errors fail automatically', async ({ page }) => {
  await page.goto('/');
  await page.route('**/harness-probe', route => route.fulfill({ status: 503, body: 'intentional HTTP probe' }));
  await page.evaluate(() => fetch('/harness-probe'));
});

test('probe page errors fail automatically', async ({ page }) => {
  await page.goto('/');
  const error = page.waitForEvent('pageerror');
  await page.evaluate(() => { setTimeout(() => { throw new Error('intentional pageerror probe'); }, 0); });
  await error;
});

test('probe failed requests fail automatically', async ({ page }) => {
  await page.goto('/');
  await page.route('**/harness-probe', route => route.abort('failed'));
  await page.evaluate(() => fetch('/harness-probe').catch(() => {}));
});
