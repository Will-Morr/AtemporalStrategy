export interface SavedPlayer { match: string; slot: number; token: string; name: string; game?: string | null }
const key = 'atemporal-players';
const gameScope = location.pathname.match(/^\/games\/[^/]+/)?.[0] ?? null;
export const slotStorageKey = gameScope ? `atemporal-slot-token:${location.pathname}` : 'atemporal-slot-token';
export const tabToken = () => sessionStorage.getItem(slotStorageKey) ?? (!gameScope && localStorage.getItem(key) === null ? localStorage.getItem('atemporal-slot-token') : null);
function allSavedPlayers(): SavedPlayer[] {
  try { return JSON.parse(localStorage.getItem(key) ?? '[]'); } catch { return []; }
}
export function savedPlayers(): SavedPlayer[] {
  return allSavedPlayers().filter(p => (p.game ?? null) === gameScope);
}
export function rememberPlayer(match: string, slot: number, token: string, name: string): void {
  if (!token) return;
  sessionStorage.setItem(slotStorageKey, token);
  localStorage.removeItem('atemporal-slot-token');
  // Worker-local archive IDs can coincide; the coordinator game URL is the outer identity.
  const saved = allSavedPlayers().filter(p => !((p.game ?? null) === gameScope && p.match === match && p.slot === slot));
  saved.push({game: gameScope, match, slot, token, name}); localStorage.setItem(key, JSON.stringify(saved.slice(-128)));
}
export function choosePlayer(token: string): void { sessionStorage.setItem(slotStorageKey, token); location.href = location.pathname; }
export const guideUrl = `${gameScope ?? ''}/guide/`;
