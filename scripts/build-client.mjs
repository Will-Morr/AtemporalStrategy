import { build } from '../client/node_modules/esbuild/lib/main.js';
import { cp, mkdir, rm, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const client=fileURLToPath(new URL('../client/',import.meta.url));
await rm(`${client}dist`,{recursive:true,force:true});
await mkdir(`${client}dist`,{recursive:true});
await cp(`${client}index.html`,`${client}dist/index.html`);
await build({entryPoints:[`${client}src/main.ts`],bundle:true,format:'esm',target:'es2022',outfile:`${client}dist/app.js`,sourcemap:true});

await cp(`${client}directory.html`,`${client}dist/directory.html`);
await build({entryPoints:[`${client}src/directory.ts`],bundle:true,format:"esm",target:"es2022",outfile:`${client}dist/directory.js`,sourcemap:true});

// Repository-authored HTML fragment: edit the description without changing the page shell.
const landing=await readFile(`${client}landing.html`,'utf8');
const description=await readFile(`${client}landing-description.html`,'utf8');
await writeFile(`${client}dist/landing.html`,landing.replace('<!-- GAME_DESCRIPTION -->',description));
await build({entryPoints:[`${client}src/landing.ts`],bundle:true,format:'esm',target:'es2022',outfile:`${client}dist/landing.js`,sourcemap:true});
