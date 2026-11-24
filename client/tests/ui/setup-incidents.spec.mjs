import {readFile, readdir} from 'node:fs/promises';
import {join} from 'node:path';
import {test, expect} from './fixtures.mjs';
import {isolatedServer} from './game-helpers.mjs';

test('early setup exception saves a sanitized diagnosis and reload restores the lobby', async ({page, review}, testInfo) => {
  const server = await isolatedServer(testInfo, () => {}, () => {}, ['--coordinator']);
  review.afterClose(server.stop);
  const request = page.request;
  expect((await request.post(server.url+'api/identity', {data:{username:'Private display name'}})).ok()).toBe(true);
  const {defaults} = await (await request.get(server.url+'api/directory')).json();
  const game = await request.post(server.url+'api/games', {data:{name:'Setup diagnosis', setup:defaults, password:'Private lobby password'}});
  expect(game.ok()).toBe(true);
  const url = new URL((await game.json()).url, server.url);
  const id = url.pathname.split('/')[2];
  const endpoint = server.url+`api/games/${id}/client-incidents`;
  const root = join(server.dir,'games',id,'client-incidents');
  url.searchParams.set('credential', 'private-query-value');
  await page.addInitScript(() => {
    if (sessionStorage.getItem('setup-injected')) return;
    sessionStorage.setItem('setup-injected','yes');
    const original = window.fetch;
    window.fetch = async (resource, init) => {
      const response = await original(resource, init);
      if (resource === '/api/directory') {
        response.json = () => {
          const error = new TypeError(`can't access property "setAttribute", username.closest(...private-error-text...) is null`);
          error.stack = [
            `runLobby@${location.pathname}app.js:7:29`,
            `@/app.js:8:33`,
            `boot@${location.origin}/app.js?credential=private-stack-value:9:44`,
          ].join('\n');
          throw error;
        };
      }
      return response;
    };
  });
  const receipt = page.waitForResponse(endpoint);
  await page.goto(url.href);
  const accepted = await receipt;
  expect(accepted.ok()).toBe(true);
  expect(await accepted.request().headerValue('referer')).toBeNull();
  const {saved, report_id} = await accepted.json();
  expect(saved).toBe(true);
  expect(report_id).toMatch(/^client-[a-f0-9]{24}$/);
  await expect(page.locator('#connection')).toContainText(`Diagnostic report saved (${report_id})`);
  const raw = await readFile(join(root,`${report_id}.json`),'utf8');
  const record = JSON.parse(raw);
  expect(record.game_id).toBe(id);
  expect(record.setup).toEqual({
    stage:'directory', view:'game', category:'type', reason:'null_access', operation:'set_attribute',
    frames:[{line:7,column:29},{line:8,column:33}], round:null, revision:null,
  });
  for (const privateText of ['Private display name','Private lobby password','private-query-value','private-error-text','private-stack-value']) {
    expect(raw).not.toContain(privateText);
    expect(await page.locator('#connection').textContent()).not.toContain(privateText);
  }
  const duplicate = await request.post(endpoint,{data:record.setup});
  expect(await duplicate.json()).toEqual({saved:true,report_id});
  expect(await readdir(root)).toEqual([`${report_id}.json`]);
  const guest = await review.newContext('setup-guest');
  expect((await guest.request.post(endpoint,{data:record.setup})).status()).toBe(403);
  await review.capture('connection-setup-report-saved',page);
  await page.getByRole('button',{name:'Reload current server'}).click();
  await expect(page.locator('#status')).toContainText('Claim a slot');
  await expect(page.locator('#connection')).toBeHidden();
  await expect(page.locator('#lobby-username')).toBeHidden();
  await review.capture('reloaded-lobby-after-setup-failure',page);
});

test('setup failure claims a saved report only after a confirmed response', async ({page, review}, testInfo) => {
  const server = await isolatedServer(testInfo);
  review.afterClose(server.stop);
  await page.route(server.url, async route => {
    const response = await route.fetch();
    const body = (await response.text()).replace('id="top-controls"', 'id="fixture-removed-controls"');
    await route.fulfill({response,body});
  });
  await page.route(server.url+'api/client-incidents', route => {
    const report = route.request().postDataJSON();
    expect(report).toMatchObject({reason:'missing_ui',operation:'top_controls',stage:'environment'});
    expect(report.frames.length).toBeGreaterThan(0);
    return route.fulfill({json:{saved:false}});
  });
  await page.goto(server.url);
  await expect(page.locator('#connection')).toContainText('The diagnostic report could not be saved.');
  await review.capture('connection-setup-report-unconfirmed',page);
});
