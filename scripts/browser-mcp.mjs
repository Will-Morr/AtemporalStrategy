// Standard stdio MCP launcher usable by any MCP client. No agent-vendor dependency.
import { chromium } from '../client/node_modules/playwright/index.mjs';
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const executable=chromium.executablePath();
if(!existsSync(executable)) {console.error('Browser missing. Run npm run ui:install --prefix client.');process.exit(1);}
const output=resolve(process.env.ATEMPORAL_MCP_ARTIFACTS??`${root}artifacts/browser-mcp/${new Date().toISOString().replaceAll(':','-')}-${process.pid}`);
await mkdir(output,{recursive:true});
console.error(`Browser MCP artifacts: ${output}`);
const args=[`${root}client/node_modules/@playwright/mcp/cli.js`,'--browser','chrome','--executable-path',executable,'--isolated','--caps','vision','--save-session','--output-dir',output,'--viewport-size','1440,1000'];
if(process.env.ATEMPORAL_BROWSER_HEADED!=='1') args.push('--headless');
if(process.env.ATEMPORAL_BROWSER_NO_SANDBOX==='1') args.push('--no-sandbox');
args.push(...process.argv.slice(2));
const child=spawn(process.execPath,args,{cwd:root,stdio:'inherit'});
for(const signal of ['SIGINT','SIGTERM']) process.on(signal,()=>child.kill(signal));
child.on('error',error=>{console.error(error.message);process.exitCode=1;});
child.on('exit',code=>{process.exitCode=code??1;});
