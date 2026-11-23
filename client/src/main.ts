import {practice} from './practice';
import { Net } from './net';
import { runLobby, type Session } from './lobby';
import { Game } from './game';
import { tabToken, rememberPlayer, slotStorageKey, setFingerprint } from './session';

setFingerprint();

if (matchMedia('(pointer: coarse)').matches && !sessionStorage.getItem('atemporal-mobile-warned')) { sessionStorage.setItem('atemporal-mobile-warned', '1'); alert('Heads up: this is really not optimized for mobile, it might work a little but no promises'); }

const status = document.getElementById('status') as HTMLParagraphElement;
const controls = document.getElementById('top-controls')!;
const net = new Net();
let game: Game | null = null;
let stale = false;
let reconnecting = false;
let disconnectNotice: number | undefined;
const connection = document.createElement('div'); connection.id = 'connection'; document.body.append(connection);
const showConnection = (message: string) => {
  connection.replaceChildren(document.createTextNode(message));
  const reload = document.createElement('button'); reload.textContent = 'Reload current server'; reload.onclick = () => location.reload(); connection.append(reload);
  connection.hidden = false;
};
// Updates restart the server; reloading picks up the new release and restores the saved draft.
net.onInstanceChange = () => { stale = true; showConnection('Server updated. Reloading… '); location.reload(); };
net.onClose = () => {
  game?.updatePanels();
  if (stale || reconnecting) return;
  if (practice) {showConnection('Practice session closed. Reload to start again from the recording. '); return;}
  if (disconnectNotice === undefined) {
    // Give the first retry time to complete before covering the game with a warning.
    disconnectNotice = window.setTimeout(() => {
      if (!stale) showConnection('Disconnected. Reconnecting; your view and draft are retained. ');
    }, 2000);
  }
  reconnecting = true;
  window.setTimeout(async () => {
    try {
      if (!navigator.onLine) return;
      await net.connect();
      const w = await net.request({kind:'hello',protocol_version:2,last_revision:game?.terrain ? game.latest : null,slot_token:tabToken()},'welcome', undefined, 10000);
      clearTimeout(disconnectNotice);
      disconnectNotice = undefined;
      status.textContent = `Connected · ${w.phase}`;
      if (!stale && !connection.hidden) connection.hidden = true;
      game?.updatePanels();
    } catch { if (net.connected) net.disconnect(); }
    finally { reconnecting = false; if (!net.connected && !stale) net.onClose(); }
  }, 1000);
};
connection.hidden = true;
net.on('match_reset', () => {
  stale=true;
  showConnection('Opening the next game. Your player slot is retained. ');
  location.reload();
});
const switchPlayer = document.createElement('button'); switchPlayer.id = 'switch-player'; switchPlayer.textContent = 'Switch / rejoin player'; switchPlayer.onclick = () => { sessionStorage.removeItem(slotStorageKey); location.href = location.pathname; }; switchPlayer.hidden=!!practice || location.pathname.includes('/replays/'); controls.append(switchPlayer);

async function boot(): Promise<void> {
  if (location.pathname.startsWith('/games/')) {
    (document.getElementById('guide-link') as HTMLAnchorElement).href=`${location.pathname.match(/^\/games\/[^/]+/)![0]}/guide/`;
    for (const parent of [document.getElementById('lobby'),controls]) {
      const link=document.createElement('a');link.href=location.pathname.includes('/replays/')?'/replays/':'/lobbies/';link.textContent=location.pathname.includes('/replays/')?'Replays':'All games';link.style.pointerEvents='auto';parent?.prepend(link);
    }
    const identity=await fetch('/api/directory').then(r=>r.json());
    const replayPath=location.pathname.includes('/replays/');
    if(identity.user===null && !replayPath){location.replace(`/lobbies/?join=${encodeURIComponent(location.pathname)}`);return;}
    (document.getElementById('username') as HTMLInputElement).value=identity.user??'';
    document.getElementById('username')!.closest('label')!.hidden=true;
    const entry=identity.games.find((g:{id:string})=>g.id===location.pathname.split('/')[2]);
    const name=entry?.name??identity.replays?.find((r:{game:string})=>r.game===location.pathname.split('/')[2])?.name;
    if(name){document.body.dataset.gameName=name;document.querySelector('#lobby h1')!.textContent=name;}
    document.body.dataset.botOnly=String(!!entry?.bots?.length && entry.bots.every((bot:unknown)=>bot!==null));
    if(!practice && location.pathname.includes('/replays/')) sessionStorage.removeItem(slotStorageKey);
  }
  try {
    await net.connect();
  } catch {
    status.textContent = practice ? 'Practice replay unavailable. Return to the recording and try again.' : 'Cannot reach the game server. Is it running on this address?';
    return;
  }
  const session: Session = { slot: null, token: practice ? 'practice' : tabToken() };
  const welcome = net.request({ kind: 'hello', protocol_version: 2, last_revision: null, slot_token: session.token }, 'welcome');
  net.on('slot_claimed', m => {
    session.slot = m.slot;
    session.token = m.private_token;
  });
  const w = await welcome;
  // A token from an older server instance is not ours any more.
  // The server confirms the private slot before welcome; no timing guess is needed.
  if (session.slot === null) {
    session.token = null;
    if (!practice) sessionStorage.removeItem(slotStorageKey);
  }
  if (!practice && session.slot !== null && session.token) rememberPlayer(w.match_id, session.slot, session.token, w.lobby.slots[session.slot]?.profile?.username ?? `Player ${session.slot}`);
  status.textContent = `Connected · ${w.config.player_count} players · ${w.phase}`;
  const { session: settled, lobby } = practice ? {session, lobby:w.lobby} : await runLobby(net, w.config, net.lobby ?? w.lobby, net.published ? 'planning' : w.phase, session, w.match_id);
  game = new Game(net, w.config, settled, lobby);
  // Debug/test hook: agent-run browser checks read state through it; never used by gameplay code.
  (window as unknown as { atemporal: Game }).atemporal = game;
  if (practice) {
    game.playhead = practice.tick;
    const back = document.createElement('a');
    back.href = location.pathname; back.textContent = 'Exit practice'; back.id = 'exit-practice';
    controls.prepend(back);
    document.title = 'Practice replay · Atemporal Strategy';
  }
  await game.start();
  if (settled.slot !== null) game.toast(`Connected as ${lobby.slots[settled.slot]?.profile?.username ?? `Player ${settled.slot}`}. This tab keeps your player when refreshed.`);
  // Late joiners need the current revision; the server resends it on hello, so replay any queued one.
  net.send({ kind: 'hello', protocol_version: 2, last_revision: null, slot_token: settled.token });
}

void boot().catch(e => showConnection(`Connection setup failed: ${(e as Error).message} `));
