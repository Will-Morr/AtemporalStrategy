import { test as base, expect } from '@playwright/test';
import { writeFile } from 'node:fs/promises';
export { expect };
export const test=base.extend({
  review:async({page},use,testInfo)=>{
    const log=[];
    const watched=new Set();
    const observe=target=>{
      if(watched.has(target)) return;
      watched.add(target);
      target.on('console',message=>log.push({kind:'console',url:target.url(),level:message.type(),text:message.text()}));
      target.on('pageerror',error=>log.push({kind:'pageerror',url:target.url(),text:error.message}));
      target.on('requestfailed',request=>log.push({kind:'requestfailed',url:request.url(),error:request.failure()?.errorText}));
    };
    observe(page);
    const capture=async(name,target=page)=>{
      const png=testInfo.outputPath(`${name}.png`);
      await target.screenshot({path:png,fullPage:true,animations:'disabled'});
      await testInfo.attach(name,{path:png,contentType:'image/png'});
      await writeFile(testInfo.outputPath(`${name}.aria.txt`),await target.locator('body').ariaSnapshot());
    };
    await use({capture,log,observe});
    await writeFile(testInfo.outputPath('browser-log.json'),JSON.stringify(log,null,2));
    await testInfo.attach('browser-log',{path:testInfo.outputPath('browser-log.json'),contentType:'application/json'});
    expect(log.filter(item=>item.kind==='pageerror'||item.kind==='requestfailed'||item.level==='error'),'browser errors').toEqual([]);
  }
});
