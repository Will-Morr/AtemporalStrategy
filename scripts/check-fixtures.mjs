import Ajv2020 from '../client/node_modules/ajv/dist/2020.js';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
const root=new URL('../',import.meta.url);
const json=async path=>JSON.parse(await readFile(new URL(path,root),'utf8'));
const schema=await json('schemas/contracts-v2.json');
const ajv=new Ajv2020({strict:false,validateFormats:false,allErrors:true});
ajv.addSchema(schema,'contracts');
const validate=type=>ajv.compile({$ref:`contracts#/$defs/${type}`});
const manifest=await json('fixtures/manifest.json');
assert(manifest.length>0,'fixture manifest is empty');
const roundtripDirectory=new URL('target/fixtures-js-roundtrip/',root);
await mkdir(roundtripDirectory,{recursive:true});
for(const [index,fixture] of manifest.entries()) {
  const value=await json(fixture.file);
  const check=validate(fixture.type);
  assert(check(value),`${fixture.file}: ${JSON.stringify(check.errors)}`);
  const roundtrip=JSON.parse(JSON.stringify(value));
  assert.deepEqual(roundtrip,value);
  await writeFile(new URL(`${index}.json`,roundtripDirectory),JSON.stringify(roundtrip));

}
for(const [type,value] of [['Version',1],['Version',0],['GroupSlot',10],['SafeInt',9007199254740992],['Order',{kind:'idle',extra:true}],['Tile',{x:65536,y:0}],['ClientEnvelope',{schema_version:2,message:{kind:'get_exact_state',revision:4294967296,tick:0}}]]) {
  assert(!validate(type)(value),`${type} wrongly accepted ${JSON.stringify(value)}`);
}
console.log(`Validated ${manifest.length} shared fixtures and 7 malformed boundary cases in JavaScript.`);

execFileSync('cargo',['test','--locked','-p','atemporal-contracts','--test','fixtures','shared_fixtures_roundtrip'],{cwd:fileURLToPath(root),stdio:'inherit',env:{...process.env,ATEMPORAL_JS_FIXTURES:fileURLToPath(roundtripDirectory)}});
