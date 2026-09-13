import { Net } from './net';
import { runLobby, type Session } from './lobby';
import { Game } from './game';

const status = document.getElementById('status') as HTMLParagraphElement;
const net = new Net();
net.onClose = () => {
  status.textContent = 'Disconnected. Refresh to reconnect to the current server.';
  const toast = document.getElementById('toast');
  if (toast) {
    toast.textContent = 'Connection lost. Refresh the page to reconnect.';
    toast.classList.add('active');
  }
};

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
  const { session: settled, lobby } = await runLobby(net, w.config, w.lobby, w.phase, session);
  const game = new Game(net, w.config, settled, lobby);
  // Debug/test hook: agent-run browser checks read state through it; never used by gameplay code.
  (window as unknown as { atemporal: Game }).atemporal = game;
  await game.start();
  // Late joiners need the current revision; the server resends it on hello, so replay any queued one.
  net.send({ kind: 'hello', protocol_version: 2, last_revision: null, slot_token: settled.token });
}

void boot();
