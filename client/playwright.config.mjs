import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const artifacts=process.env.ATEMPORAL_UI_ARTIFACTS;
if(!artifacts) throw new Error('Run npm run ui:review --prefix client so each run gets its own artifacts and port.');
export default defineConfig({
  testDir:'./tests/ui',
  testMatch:process.env.ATEMPORAL_UI_HARNESS_CHECK==='1'?'**/harness-probe.case.mjs':'**/*.spec.mjs',
  outputDir:resolve(artifacts,'results'),
  fullyParallel:true,
  forbidOnly:!!process.env.CI,
  workers:2,
  retries:0,
  // Built-in trace teardown has its own project timeout, separate from review cleanup.
  timeout:120000,
  expect:{timeout:5000},
  reporter:[['list'],['junit',{outputFile:resolve(artifacts,'junit.xml')}],['json',{outputFile:resolve(artifacts,'results.json')}],['html',{outputFolder:resolve(artifacts,'report'),open:'never'}]],
  use:{baseURL:process.env.ATEMPORAL_UI_RESOLVED_URL,headless:true,trace:'on',screenshot:'on',video:'retain-on-failure',locale:'en-US',timezoneId:'UTC',colorScheme:'dark',reducedMotion:'reduce',deviceScaleFactor:1},
  projects:[{name:'desktop-chromium',use:{browserName:'chromium',viewport:{width:1440,height:1000}}},{name:'narrow-chromium',use:{browserName:'chromium',viewport:{width:390,height:844}}}],
  webServer:process.env.ATEMPORAL_UI_BASE_URL?undefined:{
    command:process.env.ATEMPORAL_UI_SERVER_COMMAND??'npm run build --prefix client && cargo run --release -q -p atemporal-server -- --seed 42 --replays target/ui-replays',cwd:root,
    url:process.env.ATEMPORAL_UI_RESOLVED_URL,env:{PORT:process.env.ATEMPORAL_UI_PORT},
    reuseExistingServer:false,timeout:120000,stdout:'pipe',stderr:'pipe',gracefulShutdown:{signal:'SIGTERM',timeout:2000}
  }
});
