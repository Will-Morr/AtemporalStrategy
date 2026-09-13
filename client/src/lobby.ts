import type { LobbyState, MatchConfig, Phase } from './contracts.generated';
import type { Net } from './net';

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

export interface Session {
  slot: number | null;
  token: string | null;
}

/** Landing page: profile inputs, live roster, start/spectate. Resolves when the match is running. */
export function runLobby(net: Net, config: MatchConfig, initial: LobbyState, phase: Phase, session: Session): Promise<{ session: Session; lobby: LobbyState }> {
  const username = $<HTMLInputElement>('username');
  const color = $<HTMLInputElement>('color');
  const teamWrap = $<HTMLSpanElement>('team-wrap');
  const team = $<HTMLSelectElement>('team');
  const roster = $<HTMLUListElement>('roster');
  const start = $<HTMLButtonElement>('start');
  const spectate = $<HTMLButtonElement>('spectate');
  const release = $<HTMLButtonElement>('release');
  const error = $<HTMLSpanElement>('lobby-error');
  const status = $<HTMLParagraphElement>('status');
  let lobby = initial;
  if (!username.value) username.value = localStorage.getItem('atemporal-username') ?? '';
  if (color.value === '#4fc3f7') color.value = localStorage.getItem('atemporal-color') ?? color.value;
  $<HTMLParagraphElement>('rules').textContent = lobby.rule_summary;
  if (lobby.available_teams.length) {
    teamWrap.style.display = '';
    team.innerHTML = lobby.available_teams.map(t => `<option value="${t.team_id}">${t.label}</option>`).join('');
  }
  const render = () => {
    roster.innerHTML = '';
    for (const slot of lobby.slots) {
      const li = document.createElement('li');
      const swatch = document.createElement('span');
      swatch.className = 'swatch';
      swatch.style.background = slot.profile?.color ?? '#555';
      li.append(swatch);
      const label = document.createElement('span');
      label.textContent = slot.profile
        ? `Slot ${slot.slot}: ${slot.profile.username}${slot.profile.team_id ? ` (${lobby.available_teams.find(t=>t.team_id===slot.profile!.team_id)?.label ?? slot.profile.team_id})` : ''} — ${slot.connected ? 'connected' : 'disconnected'}`
        : `Slot ${slot.slot}: open`;
      li.append(label);
      if (!slot.claimed && session.slot === null && phase === 'lobby') {
        const claim = document.createElement('button');
        claim.textContent = 'Claim';
        claim.onclick = () => {
          error.textContent = '';
          if (!username.value.trim()) {
            error.textContent = 'Enter a username first.';
            return;
          }
          localStorage.setItem('atemporal-username', username.value);
          localStorage.setItem('atemporal-color', color.value);
          net.send({ kind: 'claim_slot', slot: slot.slot, username: username.value, color: color.value, team_id: lobby.available_teams.length ? team.value : null });
        };
        li.append(claim);
      }
      if (session.slot === slot.slot) li.style.outline = '1px solid var(--accent)';
      roster.append(li);
    }
    const first = lobby.slots.find(s => s.claimed)?.slot ?? null;
    start.disabled = !(lobby.can_start && session.slot !== null && session.slot === first);
    start.textContent = session.slot === first && first !== null ? 'Start match' : 'Start match (first occupied slot starts)';
    release.style.display = session.slot !== null ? '' : 'none';
    status.textContent = session.slot !== null ? `You hold slot ${session.slot}.` : phase === 'lobby' ? 'Claim a slot to play or spectate.' : 'Match in progress — spectate or refresh with your slot token.';
  };
  const updateProfile = () => {
    if (!session.token) return;
    error.textContent = 'Updating profile…';
    localStorage.setItem('atemporal-username', username.value);
    localStorage.setItem('atemporal-color', color.value);
    net.send({ kind:'update_lobby_profile',request_id:`profile-${Date.now()}`,slot_token:session.token,username:username.value,color:color.value,team_id:lobby.available_teams.length ? team.value : null });
  };
  username.onchange = updateProfile; color.onchange = updateProfile; team.onchange = updateProfile;
  render();
  const objective = config.objective.kind === 'scoreboard' ? 'Scoreboard' : 'Timed';
  document.title = `Atemporal Strategy — ${objective}`;
  return new Promise(resolve => {
    net.on('welcome', m => {lobby = m.lobby; phase = m.phase; render();});
    net.on('lobby_updated', m => {
      lobby = m.lobby;
      error.textContent = '';
      render();
    });
    net.on('lobby_update_rejected', m => {
      lobby = m.lobby;
      error.textContent = m.message;
      render();
    });
    net.on('slot_claimed', m => {
      session.slot = m.slot;
      session.token = m.private_token;
      localStorage.setItem('atemporal-slot-token', m.private_token);
      render();
    });
    start.onclick = () => {
      if (session.token) net.send({ kind: 'start_match', slot_token: session.token, based_on_lobby_revision: lobby.revision });
    };
    release.onclick = () => {
      if (session.token) net.send({ kind: 'release_slot', slot_token: session.token });
      session.slot = null;
      session.token = null;
      localStorage.removeItem('atemporal-slot-token');
      render();
    };
    spectate.onclick = () => resolve({ session, lobby });
    net.on('revision_published', () => resolve({ session, lobby }));
    if (phase !== 'lobby') resolve({ session, lobby });
  });
}
