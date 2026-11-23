import { test, expect } from './fixtures.mjs';
import { isolatedServer, players, revision } from './game-helpers.mjs';

test('map panning brings each edge to the center of the battlefield', async ({review}, testInfo) => {
  const server = await isolatedServer(testInfo);
  review.afterClose(server.stop);
  const [page] = await players(review, server.url);
  await page.getByRole('button', {name:'Start match', exact:true}).click();
  await revision(page, 0);

  const battlefield = await page.evaluate(() => {
    const map = document.querySelector('#map');
    const top = document.querySelector('#top').getBoundingClientRect().bottom;
    return {x:map.width / 2, y:(top + map.height) / 2, top, bottom:map.height};
  });
  const dimensions = await page.evaluate(() => {
    const {width, height} = window.atemporal.terrain;
    return {width, height};
  });
  const center = () => page.evaluate(() => {
    const {x, y} = window.atemporal.renderer.camera;
    return {x, y};
  });

  await page.mouse.move(battlefield.x, battlefield.y);
  await page.keyboard.down('Shift');
  await page.mouse.wheel(-100000, -100000);
  await expect.poll(center).toEqual({x:0, y:0});
  await page.keyboard.up('Shift');
  await review.capture('map-northwest-edge', page);
  await page.mouse.wheel(0, 10000);
  await expect.poll(() => page.evaluate(() => window.atemporal.renderer.camera.scale)).toBe(1);
  await expect.poll(center).toEqual({x:0, y:0});
  await page.keyboard.down('Shift');
  await page.mouse.wheel(100000, 100000);
  await expect.poll(center).toEqual({x:dimensions.width, y:dimensions.height});
  await page.keyboard.up('Shift');
  await review.capture('map-southeast-edge-zoomed-out', page);

  // Continue a middle-button drag over the command deck and then onto the top bar.
  await page.mouse.down({button:'middle'});
  await page.mouse.move(battlefield.x, battlefield.bottom + 20, {steps:5});
  await expect.poll(center).toEqual({x:dimensions.width, y:0});
  await page.mouse.move(battlefield.x + 250, battlefield.top / 2, {steps:8});
  await page.mouse.up({button:'middle'});
  await expect.poll(center).toEqual({x:0, y:dimensions.height});
});
