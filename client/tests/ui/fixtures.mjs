import { test as base, expect } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';
export { expect };

// Automatic diagnostics also cover tests that forget to request `review`.
export const test = base.extend({
  review: [async ({ page, context, browser, baseURL, viewport }, use, testInfo) => {
    const log = [];
    const watched = new Set();
    const owned = [];
    const record = entry => log.push({ time: new Date().toISOString(), ...entry });
    const observe = target => {
      if (watched.has(target)) return;
      watched.add(target);
      target.on('console', message => record({ kind: 'console', url: target.url(), level: message.type(), text: message.text() }));
      target.on('pageerror', error => record({ kind: 'pageerror', url: target.url(), text: error.stack ?? error.message }));
      target.on('requestfailed', request => record({ kind: 'requestfailed', url: request.url(), error: request.failure()?.errorText }));
      target.on('response', response => {
        if (response.status() >= 400) record({ kind: 'http-error', url: response.url(), status: response.status() });
      });
    };
    const observeContext = target => {
      target.pages().forEach(observe);
      target.on('page', observe);
    };
    observeContext(context);
    observe(page);
    const capture = async (name, target = page) => {
      if (!/^[a-z0-9][a-z0-9_-]*$/i.test(name)) throw new Error('Capture names must be filename-safe');
      const png = testInfo.outputPath(`${name}.png`);
      await target.screenshot({ path: png, fullPage: true, animations: 'disabled', timeout: 5000 });
      await testInfo.attach(name, { path: png, contentType: 'image/png' });
      const aria = testInfo.outputPath(`${name}.aria.txt`);
      await writeFile(aria, await target.locator('body').ariaSnapshot({ timeout: 5000 }));
      await testInfo.attach(`${name}-aria`, { path: aria, contentType: 'text/plain' });
    };
    // Let the fixture own extra identities so evidence is saved before contexts close.
    const newContext = async name => {
      if (!/^[a-z0-9][a-z0-9_-]*$/i.test(name) || owned.some(item => item.name === name)) throw new Error('Context names must be unique and filename-safe');
      const videoDir = testInfo.outputPath(`${name}-video`);
      await mkdir(videoDir, { recursive: true });
      const target = await browser.newContext({ baseURL, viewport, locale: 'en-US', timezoneId: 'UTC', colorScheme: 'dark', reducedMotion: 'reduce', recordVideo: { dir: videoDir } });
      owned.push({ name, context: target });
      observeContext(target);
      return target;
    };
    const errors = () => log.filter(item => ['pageerror', 'requestfailed', 'http-error'].includes(item.kind) || item.level === 'error');
    try {
      await use({ capture, log, observe, newContext });
    } finally {
      const failed = testInfo.status !== testInfo.expectedStatus || errors().length > 0;
      if (failed) {
        for (const [index, target] of [...watched].entries()) {
          if (!target.isClosed()) await capture(`failure-page-${index}`, target).catch(error => record({ kind: 'capture-error', text: error.message }));
        }
      }
      for (const item of owned) {
        const videos = item.context.pages().map(target => target.video()).filter(Boolean);
        await item.context.close();
        for (const [index, video] of videos.entries()) {
          if (failed) await testInfo.attach(`${item.name}-video-${index}`, { path: await video.path(), contentType: 'video/webm' });
          else await video.delete();
        }
      }
      const path = testInfo.outputPath('browser-log.json');
      await writeFile(path, JSON.stringify(log, null, 2));
      await testInfo.attach('browser-log', { path, contentType: 'application/json' });
      expect(errors(), 'browser errors').toEqual([]);
    }
  }, { auto: true }]
});
