// Vendor-neutral MCP protocol smoke test; this client uses JSON-RPC over stdio directly.
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { createInterface } from 'node:readline';
import { mkdir, writeFile, stat, readFile, readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import assert from 'node:assert/strict';
const root=fileURLToPath(new URL('../',import.meta.url));
const artifacts=resolve(process.env.ATEMPORAL_MCP_ARTIFACTS??`${root}artifacts/browser-mcp/smoke-${Date.now()}-${process.pid}`);
await mkdir(artifacts,{recursive:true});
let server;
let url=process.env.ATEMPORAL_UI_BASE_URL;
if(!url) {
  const probe=createServer();await new Promise(done=>probe.listen(0,'127.0.0.1',done));const port=probe.address().port;await new Promise(done=>probe.close(done));
  url=`http://127.0.0.1:${port}`;
  server=spawn(process.execPath,[`${root}scripts/serve-client.mjs`],{cwd:root,env:{...process.env,PORT:String(port)},stdio:['ignore','pipe','pipe']});
  server.stdout.on('data',data=>process.stderr.write(data));server.stderr.on('data',data=>process.stderr.write(data));
  let ready=false;
  for(let i=0;i<100;i++) {try{if((await fetch(url)).ok){ready=true;break;}}catch{} await new Promise(done=>setTimeout(done,50));}
  if(!ready) {server.kill();throw new Error('Preview server did not start');}
}
const configuration=JSON.parse(await readFile(`${root}.mcp.json`,'utf8')).mcpServers['atemporal-browser'];
const child=spawn(configuration.command,configuration.args,{cwd:root,env:{...process.env,...configuration.env,ATEMPORAL_MCP_ARTIFACTS:artifacts},stdio:['pipe','pipe','pipe']});
let sequence=0;const pending=new Map();const transcript=[];
child.stderr.on('data',data=>process.stderr.write(data));
createInterface({input:child.stdout}).on('line',line=>{
  let message;try{message=JSON.parse(line);}catch{throw new Error(`Non-JSON output polluted MCP stdout: ${line}`);}
  transcript.push(message);
  const waiter=pending.get(message.id);
  if(waiter){clearTimeout(waiter.timer);pending.delete(message.id);if(message.error)waiter.reject(new Error(JSON.stringify(message.error)));else waiter.resolve(message.result);}
});
child.on('exit',code=>{for(const waiter of pending.values()){clearTimeout(waiter.timer);waiter.reject(new Error(`MCP exited ${code}`));}pending.clear();});
const request=(method,params={})=>new Promise((resolve,reject)=>{
  const id=++sequence;const timer=setTimeout(()=>{pending.delete(id);reject(new Error(`MCP timeout: ${method}`));},30000);
  pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');
});
const call=async(name,args)=>{const result=await request('tools/call',{name,arguments:args});assert(!result.isError,JSON.stringify(result));return result;};
try {
  await request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'atemporal-browser-smoke',version:'1.0.0'}});
  child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
  const {tools}=await request('tools/list');
  for(const name of ['browser_navigate','browser_click','browser_take_screenshot','browser_mouse_click_xy']) assert(tools.some(tool=>tool.name===name),`missing ${name}`);
  await call('browser_navigate',{url});
  await call('browser_click',{target:'a[href="/guide/"]',element:'How to play / Unit reference'});
  const snapshot=await call('browser_snapshot',{});
  assert(JSON.stringify(snapshot).includes('How to play Atemporal Strategy'));
  await call('browser_take_screenshot',{filename:`${artifacts}/mcp-guide.png`,fullPage:true,scale:'css'});
  const screenshot=(await readdir(artifacts,{recursive:true})).find(name=>name==='mcp-guide.png'||name.endsWith('/mcp-guide.png'));
  assert(screenshot,'MCP screenshot file was not saved');
  assert((await stat(`${artifacts}/${screenshot}`)).size>1000);
  await writeFile(`${artifacts}/transcript.json`,JSON.stringify(transcript,null,2));
  console.log(`MCP initialize, tool discovery, navigation, link click, snapshot and screenshot passed.\nArtifacts: ${artifacts}`);
} finally {
  await writeFile(`${artifacts}/transcript.json`,JSON.stringify(transcript,null,2));
  child.stdin.end();child.kill('SIGTERM');server?.kill('SIGTERM');
  for(const waiter of pending.values())clearTimeout(waiter.timer);
}
