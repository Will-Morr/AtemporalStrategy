import { execFileSync } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { compile } from '../client/node_modules/json-schema-to-typescript/dist/src/index.js';
const root = fileURLToPath(new URL('../', import.meta.url));
execFileSync('cargo', ['run','--locked','--quiet','-p','atemporal-tools','--','schema'], {cwd:root,stdio:'inherit'});
const schema = JSON.parse(await readFile(new URL('../schemas/contracts-v2.json', import.meta.url),'utf8'));
// json-schema-to-typescript consumes draft-07 definitions; reference conversion is mechanical.
const converted = JSON.parse(JSON.stringify(schema).replaceAll('"$defs"','"definitions"').replaceAll('#/$defs/','#/definitions/'));
delete converted.$schema;
await writeFile(new URL('../client/src/contracts.generated.ts', import.meta.url), await compile(converted,'ContractCatalog',{bannerComment:'/* Generated from Rust contracts by npm run generate. Do not edit. */',additionalProperties:false,unreachableDefinitions:true}));
