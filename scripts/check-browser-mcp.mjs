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
assert.equal((await readdir(artifacts)).length,0,`Artifact directory must be empty: ${artifacts}`);
console.log(`MCP check artifacts: ${artifacts}`);
const transcript=[];
const diagnostics=[];
const status={startedAt:new Date().toISOString(),status:'running'};
let server, child, call;
let sequence=0;const pending=new Map();
const diagnostic=data=>{diagnostics.push(String(data));process.stderr.write(data);};
const rejectPending=error=>{for(const waiter of pending.values()){clearTimeout(waiter.timer);waiter.reject(error);}pending.clear();};
const stop=async process=>{
  if(!process || process.exitCode!==null || process.signalCode!==null) return;
  await new Promise(done=>{
    const timer=setTimeout(()=>process.kill('SIGKILL'),3000);
    process.once('close',()=>{clearTimeout(timer);done();});
    process.kill('SIGTERM');
  });
};
try {
let url=process.env.ATEMPORAL_UI_BASE_URL;
if(!url) {
  const probe=createServer();await new Promise((done,reject)=>{probe.once('error',reject);probe.listen(0,'127.0.0.1',done);});const port=probe.address().port;await new Promise(done=>probe.close(done));
  url=`http://127.0.0.1:${port}`;
  server=spawn('cargo',['run','--release','--quiet','-p','atemporal-server','--','--port',String(port),'--replays',`${artifacts}/replays`],{cwd:root,env:{...process.env,PORT:String(port)},stdio:['ignore','pipe','pipe']});
    // Wait for our own process to announce listening; never attach to a port squatter.
  await new Promise((done,reject)=>{
    let output='';
    const timer=setTimeout(()=>reject(new Error('Game server did not start within 120 seconds')),120000);
    server.once('error',error=>{clearTimeout(timer);reject(error);});
    server.once('exit',code=>{clearTimeout(timer);reject(new Error(`Game server exited ${code}`));});
    server.stdout.on('data',data=>{diagnostic(data);output+=data;if(output.includes(`listening on ${url}`)){clearTimeout(timer);done();}});
    server.stderr.on('data',diagnostic);
  });
}
const configuration=JSON.parse(await readFile(`${root}.mcp.json`,'utf8')).mcpServers['atemporal-browser'];
child=spawn(configuration.command,configuration.args,{cwd:root,env:{...configuration.env,...process.env,ATEMPORAL_MCP_ARTIFACTS:artifacts},stdio:['pipe','pipe','pipe']});
child.stderr.on('data',diagnostic);
child.on('error',rejectPending);
child.stdin.on('error',rejectPending);
createInterface({input:child.stdout}).on('line',line=>{
  let message;try{message=JSON.parse(line);}catch{rejectPending(new Error(`Non-JSON output polluted MCP stdout: ${line}`));child.kill();return;}
  transcript.push(message);
  const waiter=pending.get(message.id);
  if(waiter){clearTimeout(waiter.timer);pending.delete(message.id);if(message.error)waiter.reject(new Error(JSON.stringify(message.error)));else waiter.resolve(message.result);}
});
child.on('exit',code=>{for(const waiter of pending.values()){clearTimeout(waiter.timer);waiter.reject(new Error(`MCP exited ${code}`));}pending.clear();});
const request=(method,params={})=>new Promise((resolve,reject)=>{
  const id=++sequence;const timer=setTimeout(()=>{pending.delete(id);reject(new Error(`MCP timeout: ${method}`));},30000);
  const message={jsonrpc:'2.0',id,method,params};transcript.push({direction:'request',...message});
  pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify(message)+'\n');
});
call=async(name,args)=>{const result=await request('tools/call',{name,arguments:args});assert(!result.isError,JSON.stringify(result));return result;};
  status.baseURL=url;
  await request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'atemporal-browser-smoke',version:'1.0.0'}});
  child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
  const {tools}=await request('tools/list');
  for(const name of ['browser_navigate','browser_click','browser_tabs','browser_take_screenshot','browser_mouse_click_xy']) assert(tools.some(tool=>tool.name===name),`missing ${name}`);
  await call('browser_navigate',{url});
  await call('browser_click',{target:'a[href="/guide/"]',element:'How to play / Unit reference'});
  const tabs=await call('browser_tabs',{action:'list'});
  assert(JSON.stringify(tabs).includes('/guide/'),'guide opened in a browser tab');
  await call('browser_tabs',{action:'select',index:1});
  const snapshot=await call('browser_snapshot',{});
  assert(JSON.stringify(snapshot).includes('How to play Atemporal Strategy'));
  await call('browser_take_screenshot',{filename:`${artifacts}/mcp-guide.png`,fullPage:true,scale:'css'});
  const screenshot=(await readdir(artifacts,{recursive:true})).find(name=>name==='mcp-guide.png'||name.endsWith('/mcp-guide.png'));
  assert(screenshot,'MCP screenshot file was not saved');
  assert((await stat(`${artifacts}/${screenshot}`)).size>1000);
  await writeFile(`${artifacts}/transcript.json`,JSON.stringify(transcript,null,2));
  console.log(`MCP initialize, tool discovery, navigation, link click, snapshot and screenshot passed.\nArtifacts: ${artifacts}`);
  status.status='passed';
} catch(error) {
  status.status='failed';status.error=error.stack;process.exitCode=1;console.error(error);
} finally {
  if(call && child?.exitCode===null && child?.signalCode===null) {
    for(const [name,args] of [
      ['browser_console_messages',{level:'info',all:true,filename:`${artifacts}/console.log`}],
      ['browser_network_requests',{static:true,filename:`${artifacts}/network.log`}],
      ...(status.status==='failed'?[['browser_take_screenshot',{filename:`${artifacts}/failure.png`,fullPage:true}]]:[]),
      ['browser_close',{}]
    ]) {
      try {await call(name,args);} catch(error) {diagnostics.push(`Diagnostic ${name}: ${error.message}\n`);}
    }
  }
  child?.stdin.end();
  await Promise.all([stop(child),stop(server)]);
  status.finishedAt=new Date().toISOString();
  await writeFile(`${artifacts}/run.json`,JSON.stringify(status,null,2));
  await writeFile(`${artifacts}/diagnostics.log`,diagnostics.join(''));
  await writeFile(`${artifacts}/transcript.json`,JSON.stringify(transcript,null,2));
  for(const waiter of pending.values())clearTimeout(waiter.timer);
}
