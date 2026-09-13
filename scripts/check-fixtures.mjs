import Ajv2020 from '../client/node_modules/ajv/dist/2020.js';
import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
const root=new URL('../',import.meta.url);
const json=async path=>JSON.parse(await readFile(new URL(path,root),'utf8'));
const schema=await json('schemas/contracts-v1.json');
const ajv=new Ajv2020({strict:false,validateFormats:false,allErrors:true});
ajv.addSchema(schema,'contracts');
const validate=type=>ajv.compile({$ref:`contracts#/$defs/${type}`});
const manifest=await json('fixtures/manifest.json');
assert(manifest.length>0,'fixture manifest is empty');
for(const fixture of manifest) {
  const value=await json(fixture.file);
  const check=validate(fixture.type);
  assert(check(value),`${fixture.file}: ${JSON.stringify(check.errors)}`);
  const roundtrip=JSON.parse(JSON.stringify(value));
  assert.deepEqual(roundtrip,value);
}
for(const [type,value] of [['Version',2],['Version',0],['GroupSlot',10],['SafeInt',9007199254740992],['Order',{kind:'idle',extra:true}],['Tile',{x:65536,y:0}],['ClientEnvelope',{schema_version:1,message:{kind:'get_exact_state',revision:4294967296,tick:0}}]]) {
  assert(!validate(type)(value),`${type} wrongly accepted ${JSON.stringify(value)}`);
}
console.log(`Validated ${manifest.length} shared fixtures and 7 malformed boundary cases in JavaScript.`);
