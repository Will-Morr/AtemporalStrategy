const query = new URLSearchParams(location.search);
const source = query.get('source') ?? '';
const revision = query.get('practice') ?? '';
const player = query.get('as') ?? '';
export const practice = /^[a-zA-Z0-9-]{1,99}$/.test(source) && /^\d+$/.test(revision) && /^\d+$/.test(player)
  ? {source, revision:Number(revision), player:Number(player), tick:Math.max(0, Number(query.get('tick')) || 0)}
  : null;

export function practiceUrl(match: string, round: number, slot: number, tick: number): string {
  const url = new URL(location.href);
  url.search = new URLSearchParams({source:match, practice:String(round), as:String(slot), tick:String(Math.floor(tick))}).toString();
  return url.href;
}
