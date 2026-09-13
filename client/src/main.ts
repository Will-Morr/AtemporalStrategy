import { Net } from './net';
import { runLobby, type Session } from './lobby';
import { Game } from './game';

const status = document.getElementById('status') as HTMLParagraphElement;
const net = new Net();
let game: Game | null = null;
let stale = false;
let reconnecting = false;
const connection = document.createElement('div'); connection.id = 'connection'; document.body.append(connection);
const showConnection = (message: string) => {
  connection.replaceChildren(document.createTextNode(message));
  const reload = document.createElement('button'); reload.textContent = 'Reload current server'; reload.onclick = () => location.reload(); connection.append(reload);
  connection.hidden = false;
};
net.onInstanceChange = () => { stale = true; showConnection('Server restarted. Reload to obtain its current match and guide. '); };
net.onClose = () => {
  game?.updatePanels();
  if (stale || reconnecting) return;
  showConnection('Disconnected. Reconnecting; your view and draft are retained. ');
  reconnecting = true;
  window.setTimeout(async () => {
    try {
      if (!navigator.onLine) return;
      await net.connect();
      const w = await net.request({kind:'hello',protocol_version:2,last_revision:game?.latest ?? null,slot_token:localStorage.getItem('atemporal-slot-token')},'welcome');
      status.textContent = `Connected · ${w.phase}`;
      if (!stale) connection.hidden = true;
      game?.updatePanels();
    } catch { /* Retry while the same server is unavailable. */ }
    finally { reconnecting = false; if (!net.connected && !stale) net.onClose(); }
  }, 1000);
};
connection.hidden = true;

async function boot(): Promise<void> {
  try {
    await net.connect();
  } catch {
    status.textContent = 'Cannot reach the game server. Is it running on this address?';
    return;
  }
  const session: Session = { slot: null, token: localStorage.getItem('atemporal-slot-token') };
  const welcome = net.request({ kind: 'hello', protocol_version: 2, last_revision: null, slot_token: session.token }, 'welcome');
  net.on('slot_claimed', m => {
    session.slot = m.slot;
    session.token = m.private_token;
  });
  const w = await welcome;
  // A token from an older server instance is not ours any more.
  await new Promise(r => setTimeout(r, 50));
  if (session.slot === null) {
    session.token = null;
    localStorage.removeItem('atemporal-slot-token');
  }
  status.textContent = `Connected · ${w.config.player_count} players · ${w.phase}`;
  const { session: settled, lobby } = await runLobby(net, w.config, net.lobby ?? w.lobby, net.published ? 'planning' : w.phase, session);
  game = new Game(net, w.config, settled, lobby);
  // Debug/test hook: agent-run browser checks read state through it; never used by gameplay code.
  (window as unknown as { atemporal: Game }).atemporal = game;
  await game.start();
  // Late joiners need the current revision; the server resends it on hello, so replay any queued one.
  net.send({ kind: 'hello', protocol_version: 2, last_revision: null, slot_token: settled.token });
}

void boot().catch(e => showConnection(`Connection setup failed: ${(e as Error).message} `));
