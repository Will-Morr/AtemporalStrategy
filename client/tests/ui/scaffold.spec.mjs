import { test,expect } from './fixtures.mjs';
import { isolatedServer } from './game-helpers.mjs';

test('landing loads its real content, keyboard opens guide, and stats match',async({page,request,review},testInfo)=>{
  const server=await isolatedServer(testInfo);review.afterClose(server.stop);
  await page.goto(server.url);
  await expect(page.getByRole('heading',{name:'Atemporal Strategy',exact:true})).toBeVisible();
  await expect(page.locator('#status')).toContainText('Claim a slot');
  await review.capture('landing');
  const link=page.getByRole('link',{name:'How to play / Unit reference'});
  await page.keyboard.press('Tab');await expect(link).toBeFocused();
  const popup=page.waitForEvent('popup');await page.keyboard.press('Enter');page=await popup;await expect(page).toHaveURL(/\/guide\/$/);
  await expect(page.getByRole('heading',{name:'Game Docs'})).toBeVisible();
  const content=await (await request.get(`${server.url}guide/content.json`)).json();
  const cards=page.locator('.stat-card');await expect(cards).toHaveCount(content.types.length);
  for(const type of content.types) {
    const card=page.locator(`[data-unit="${type.key}"]`);
    await expect(card.locator('[data-stat="Cost"]')).toHaveText(String(type.matter_cost));
    if(type.kind==='missile') await expect(card.locator('[data-stat="Storage"]')).toContainText('no separate health');
    else await expect(card.locator('[data-stat="Health"]')).toHaveText(String(type.max_hp));
  }
  // Every stat card and the page must fit the desktop viewport.
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
  await review.capture('guide');
  await page.getByRole('link',{name:'← Atemporal'}).click();await expect(page).toHaveURL(/\/$/);
});

test('two players and spectator can use isolated browser contexts',async({review},testInfo)=>{
  const server=await isolatedServer(testInfo);review.afterClose(server.stop);
  const contexts=[];
  for(const role of ['player-a','player-b','spectator']) {
    const context=await review.newContext(role);contexts.push(context);
    const page=await context.newPage();await page.goto(server.url);
    expect(await page.evaluate(()=>localStorage.getItem('review-role'))).toBeNull();
    await page.evaluate(role=>localStorage.setItem('review-role',role),role);
  }
  for(const [index,role] of ['player-a','player-b','spectator'].entries()) {
    const page=contexts[index].pages()[0];await page.reload();
    expect(await page.evaluate(()=>localStorage.getItem('review-role'))).toBe(role);
    await expect(page.locator('#status')).toContainText('Claim a slot');
    await review.capture(role,page);
  }
});
