import { build } from '../client/node_modules/esbuild/lib/main.js';
import { cp, mkdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const client=fileURLToPath(new URL('../client/',import.meta.url));
await rm(`${client}dist`,{recursive:true,force:true});
await mkdir(`${client}dist`,{recursive:true});
await cp(`${client}public`,`${client}dist`,{recursive:true});
await cp(`${client}index.html`,`${client}dist/index.html`);
await build({entryPoints:[`${client}src/main.ts`],bundle:true,format:'esm',target:'es2022',outfile:`${client}dist/app.js`,sourcemap:true});
