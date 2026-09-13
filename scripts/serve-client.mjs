// Local scaffold preview only. The server handoff owns game routes and WebSocket.
import { execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { resolve, extname, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
const root=fileURLToPath(new URL('../client/dist/',import.meta.url));
const port=Number(process.env.PORT ?? '8080');
if (!Number.isInteger(port)||port<1||port>65535) throw new Error('PORT must be 1..65535');
// Generate the static guide once from loaded content before publishing this origin.
execFileSync('cargo',['run','--locked','--quiet','-p','atemporal-tools','--','guide','--content',process.env.ATEMPORAL_CONTENT??'config/content.yaml','--out',resolve(root,'guide')],{cwd:fileURLToPath(new URL('../',import.meta.url)),stdio:'inherit'});
const server=createServer(async(req,res)=>{
  try {
    let path=resolve(root,'.'+decodeURIComponent(new URL(req.url,'http://localhost').pathname));
    if (path!==resolve(root)&&!path.startsWith(resolve(root)+sep)) {res.writeHead(403).end();return;}
    if ((await stat(path)).isDirectory()) path=resolve(path,'index.html');
    res.setHeader('Content-Type',({'.html':'text/html; charset=utf-8','.js':'text/javascript','.json':'application/json','.map':'application/json'})[extname(path)]??'application/octet-stream');
    res.setHeader('Cache-Control','no-store');res.end(await readFile(path));
  } catch {res.writeHead(404).end('Not found');}
});
server.on('error',error=>{console.error(error.message);process.exitCode=1;});
server.listen(port,'127.0.0.1',()=>console.log(`Scaffold preview: http://127.0.0.1:${port}`));
