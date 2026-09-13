export interface SavedPlayer { match: string; slot: number; token: string; name: string }
const key = 'atemporal-players';
export const tabToken = () => sessionStorage.getItem('atemporal-slot-token') ?? (localStorage.getItem(key) === null ? localStorage.getItem('atemporal-slot-token') : null);
export function savedPlayers(): SavedPlayer[] {
  try { return JSON.parse(localStorage.getItem(key) ?? '[]'); } catch { return []; }
}
export function rememberPlayer(match: string, slot: number, token: string, name: string): void {
  if (!token) return;
  sessionStorage.setItem('atemporal-slot-token', token);
  localStorage.removeItem('atemporal-slot-token');
  const saved = savedPlayers().filter(p => !(p.match === match && p.slot === slot));
  saved.push({match, slot, token, name}); localStorage.setItem(key, JSON.stringify(saved.slice(-32)));
}
export function choosePlayer(token: string): void { sessionStorage.setItem('atemporal-slot-token', token); location.href = location.pathname; }
