// Agent-neutral entry point: one isolated run, artifacts, exit code, and optional external app.
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const runId=`${new Date().toISOString().replaceAll(':','-')}-${process.pid}`;
const artifacts=resolve(process.env.ATEMPORAL_UI_ARTIFACTS??`${root}artifacts/ui/${runId}`);
let port=process.env.ATEMPORAL_UI_PORT;
if(!process.env.ATEMPORAL_UI_BASE_URL && !port) {
  const probe=createServer();await new Promise((done,reject)=>{probe.once('error',reject);probe.listen(0,'127.0.0.1',done);});
  port=String(probe.address().port);await new Promise(done=>probe.close(done));
}
if(port && (!Number.isInteger(Number(port)) || Number(port)<1 || Number(port)>65535)) throw new Error('ATEMPORAL_UI_PORT must be 1..65535');
const baseURL=process.env.ATEMPORAL_UI_BASE_URL??`http://127.0.0.1:${port}`;
await mkdir(artifacts,{recursive:true});
await writeFile(`${artifacts}/run.json`,JSON.stringify({runId,baseURL,artifacts,command:process.argv.slice(2),externalServer:!!process.env.ATEMPORAL_UI_BASE_URL},null,2));
console.log(`UI review: ${baseURL}\nArtifacts: ${artifacts}`);
const child=spawn(process.execPath,[`${root}client/node_modules/@playwright/test/cli.js`,'test','--config',`${root}client/playwright.config.mjs`,...process.argv.slice(2)],{cwd:root,stdio:'inherit',env:{...process.env,ATEMPORAL_UI_PORT:port??'',ATEMPORAL_UI_ARTIFACTS:artifacts,ATEMPORAL_UI_RESOLVED_URL:baseURL}});
for(const signal of ['SIGINT','SIGTERM']) process.on(signal,()=>child.kill(signal));
child.on('error',error=>{console.error(error.message);process.exitCode=1;});
child.on('exit',code=>{process.exitCode=code??1;console.log(`Review artifacts: ${artifacts}`);});
