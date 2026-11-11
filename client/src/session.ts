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
// Stable per-machine hash so the server's visit log can count unique users across cleared cookies.
export function setFingerprint(): void {
  const c = document.createElement('canvas'), g = c.getContext('2d');
  if (g) { g.textBaseline = 'top'; g.font = '14px Arial'; g.fillStyle = '#f60'; g.fillRect(2, 2, 60, 20); g.fillStyle = '#069'; g.fillText('Atemporal ✓', 4, 4); }
  const gl = document.createElement('canvas').getContext('webgl'), info = gl?.getExtension('WEBGL_debug_renderer_info');
  const parts = [c.toDataURL(), info ? gl!.getParameter(info.UNMASKED_RENDERER_WEBGL) : '', screen.width, screen.height, screen.colorDepth, Intl.DateTimeFormat().resolvedOptions().timeZone, navigator.languages.join(), navigator.platform, navigator.hardwareConcurrency, navigator.maxTouchPoints].join('|');
  let h = 0x811c9dc5;
  for (let i = 0; i < parts.length; i++) h = Math.imul(h ^ parts.charCodeAt(i), 0x01000193);
  document.cookie = `atemporal-fp=${(h >>> 0).toString(16)}; Path=/; SameSite=Lax; Max-Age=31536000`;
}
