import { test as base, expect } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';
export { expect };

// Automatic diagnostics also cover tests that forget to request `review`.
// Allow recorded player contexts more than the default 30 seconds to flush evidence.
export const test = base.extend({
  review: [async ({ page, context, browser, baseURL, viewport }, use, testInfo) => {
    const log = [];
    const watched = new Set();
    const owned = [];
    const cleanups = [];
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
      const videos = [];
      target.on('page', opened => {
        const video = opened.video();
        if (video) videos.push(video);
      });
      owned.push({ name, context: target, videos });
      observeContext(target);
      return target;
    };
    const errors = () => log.filter(item => ['pageerror', 'requestfailed', 'http-error', 'cleanup-error'].includes(item.kind) || item.level === 'error');
    try {
      await use({ capture, log, observe, newContext, afterClose: fn => cleanups.push(fn) });
    } finally {
      const failed = testInfo.status !== testInfo.expectedStatus || errors().length > 0;
      if (failed) {
        for (const [index, target] of [...watched].entries()) {
          if (!target.isClosed()) await capture(`failure-page-${index}`, target).catch(error => record({ kind: 'capture-error', text: error.message }));
        }
      }
      for (const item of owned) {
        // Keep collecting evidence even if one context or video cannot be finalized.
        await item.context.close().catch(error => record({ kind: 'cleanup-error', context: item.name, text: error.message }));
        for (const [index, video] of item.videos.entries()) {
          try {
            if (failed || errors().length > 0) await testInfo.attach(`${item.name}-video-${index}`, { path: await video.path(), contentType: 'video/webm' });
            else await video.delete();
          } catch (error) {
            record({ kind: 'cleanup-error', context: item.name, text: error.message });
          }
        }
      }
      for (const cleanup of cleanups.reverse()) await cleanup().catch(error => record({kind:'cleanup-error',text:error.message}));
      const path = testInfo.outputPath('browser-log.json');
      await writeFile(path, JSON.stringify(log, null, 2));
      await testInfo.attach('browser-log', { path, contentType: 'application/json' });
      expect(errors(), 'browser errors').toEqual([]);
    }
  }, { auto: true, timeout: 120000 }]
});
