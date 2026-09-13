import { test,expect } from './fixtures.mjs';

test('landing loads its real content, keyboard opens guide, and stats match',async({page,request,review})=>{
  await page.goto('/');
  await expect(page.getByRole('heading',{name:'Atemporal Strategy',exact:true})).toBeVisible();
  await expect(page.locator('#status')).toContainText('10 unit and structure types');
  await review.capture('landing');
  const link=page.getByRole('link',{name:'How to play / Unit reference'});
  await page.keyboard.press('Tab');await expect(link).toBeFocused();
  await page.keyboard.press('Enter');await expect(page).toHaveURL(/\/guide\/$/);
  await expect(page.getByRole('heading',{name:'How to play Atemporal Strategy'})).toBeVisible();
  const content=await (await request.get('/guide/content.json')).json();
  const rows=page.locator('tbody tr');await expect(rows).toHaveCount(content.types.length);
  for(const type of content.types) {
    const row=rows.filter({has:page.getByRole('cell',{name:type.key,exact:true})});
    await expect(row.locator('td').nth(2)).toHaveText(String(type.matter_cost));
    await expect(row.locator('td').nth(3)).toHaveText(String(type.max_hp));
  }
  // Tables may scroll internally; the page itself must fit a narrow viewport.
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
  await review.capture('guide');
  await page.getByRole('link',{name:'Back to lobby'}).click();await expect(page).toHaveURL(/\/$/);
});

test('two players and spectator can use isolated browser contexts',async({review})=>{
  const contexts=[];
  for(const role of ['player-a','player-b','spectator']) {
    const context=await review.newContext(role);contexts.push(context);
    const page=await context.newPage();await page.goto('/');
    expect(await page.evaluate(()=>localStorage.getItem('review-role'))).toBeNull();
    await page.evaluate(role=>localStorage.setItem('review-role',role),role);
  }
  for(const [index,role] of ['player-a','player-b','spectator'].entries()) {
    const page=contexts[index].pages()[0];await page.reload();
    expect(await page.evaluate(()=>localStorage.getItem('review-role'))).toBe(role);
    await expect(page.locator('#status')).toContainText('10 unit and structure types');
    await review.capture(role,page);
  }
});
