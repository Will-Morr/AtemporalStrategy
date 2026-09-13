import type { Content, GuideManifest, ClientEnvelope } from './contracts.generated';
// Compile a real boundary value to ensure generated unions stay usable by the client.
export const hello: ClientEnvelope = {schema_version:1,message:{kind:'hello',protocol_version:1,last_revision:null,slot_token:null}};
const status=document.querySelector<HTMLParagraphElement>('#status');
try {
  const responses=await Promise.all([fetch('/guide/content.json'),fetch('/guide/manifest.json')]);
  if (responses.some(response=>!response.ok)) throw new Error('Unit reference unavailable');
  const content:Content=await responses[0].json();
  const manifest:GuideManifest=await responses[1].json();
  if(status) status.textContent=`${content.types.length} unit and structure types · content ${manifest.content_hash.slice(0,12)}`;
} catch {
  if(status) status.textContent='Unit reference could not be loaded. Rebuild the browser assets and refresh.';
}
