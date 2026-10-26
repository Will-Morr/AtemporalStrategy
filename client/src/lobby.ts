import {renderMapChoices} from './map-choice';
import type { LobbyState, MatchConfig, Phase } from './contracts.generated';
import type { Net } from './net';
import { savedPlayers, rememberPlayer, choosePlayer, slotStorageKey } from './session';

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

export interface Session {
  slot: number | null;
  token: string | null;
}

/** Landing page: profile inputs, live roster, start/spectate. Resolves when the match is running. */
export function runLobby(net: Net, config: MatchConfig, initial: LobbyState, phase: Phase, session: Session, matchId: string): Promise<{ session: Session; lobby: LobbyState }> {
  const readonly = location.pathname.includes('/replays/');
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
  if(readonly){username.closest('.row')!.setAttribute('hidden','');start.hidden=true;$('map-picker').hidden=true;spectate.textContent='View replay';}
  if (!location.pathname.startsWith('/games/') && !username.value) username.value = localStorage.getItem('atemporal-username') ?? '';
  if (color.value === '#4fc3f7') color.value = localStorage.getItem('atemporal-color') ?? color.value;
  $<HTMLParagraphElement>('rules').textContent = lobby.rule_summary;
  if (lobby.available_teams.length) {
    teamWrap.style.display = '';
    team.replaceChildren(...lobby.available_teams.map(t => new Option(t.label, t.team_id)));
  }
  const render = () => {
    if(!readonly)renderMapChoices(lobby, session.slot, index => {if(session.token)net.send({kind:'select_map',slot_token:session.token,based_on_lobby_revision:lobby.revision,index});});
    release.remove();
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
      if (!readonly && !slot.claimed && session.slot === null && phase === 'lobby') {
        const claim = document.createElement('button');
        claim.textContent = 'Claim';
        const claimSlot = () => {
          error.textContent = '';
          localStorage.setItem('atemporal-username', username.value);
          localStorage.setItem('atemporal-color', color.value);
          net.send({ kind: 'claim_slot', slot: slot.slot, username: username.value, color: color.value, team_id: lobby.available_teams.length ? team.value : null });
        };
        claim.onclick = e => {e.stopPropagation();claimSlot();};
        li.classList.add('open-slot');
        li.tabIndex=0;
        li.onclick=claimSlot;
        li.onkeydown=e=>{if(e.target===li && (e.key==='Enter'||e.key===' ')){e.preventDefault();claimSlot();}};
        li.append(claim);
      }
      const saved = savedPlayers().find(p => p.match === matchId && p.slot === slot.slot);
      if (!readonly && slot.claimed && saved && session.slot === null) {
        const rejoin = document.createElement('button'); rejoin.textContent = `Rejoin ${slot.profile?.username ?? saved.name}`;
        rejoin.onclick = () => choosePlayer(saved.token); li.append(rejoin);
      }
      if (session.slot === slot.slot) {
        li.style.outline = '1px solid var(--accent)';
        if(!readonly && phase==='lobby')li.append(release);
      }
      roster.append(li);
    }
    const first = lobby.slots.find(s => s.claimed)?.slot ?? null;
    start.disabled = !(lobby.can_start && session.slot !== null && session.slot === first);
    start.textContent = session.slot === first && first !== null ? 'Start match' : 'Start match (first occupied slot starts)';
    release.style.display = session.slot !== null ? '' : 'none';
    status.textContent = readonly ? 'Read-only replay. Spectate to inspect every recorded round.' : session.slot !== null ? `You hold slot ${session.slot}.` : phase === 'lobby' ? 'Claim a slot to play or spectate.' : 'Match in progress. Choose a saved player to rejoin, or spectate. Each tab keeps its own player.';
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
  const objective = config.objective.kind === 'simple_win' ? 'Simple win' : config.objective.kind === 'scoreboard' ? 'Scoreboard' : config.objective.kind === 'hybrid' ? 'Hybrid' : 'Timed';
  document.title = `${document.body.dataset.gameName ?? 'Atemporal Strategy'} — ${readonly ? 'Replay' : objective}`;
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
      rememberPlayer(matchId, m.slot, m.private_token, username.value || `Player ${m.slot}`);
      render();
    });
    start.onclick = () => {
      if (session.token) net.send({ kind: 'start_match', slot_token: session.token, based_on_lobby_revision: lobby.revision });
    };
    release.onclick = () => {
      if (session.token) net.send({ kind: 'release_slot', slot_token: session.token });
      session.slot = null;
      session.token = null;
      sessionStorage.removeItem(slotStorageKey);
      render();
    };
    spectate.onclick = () => resolve({ session, lobby });
    spectate.disabled = false;
    net.on('replay_progress', m => { if(m.preview && session.slot !== null) resolve({session,lobby}); });
    net.on('revision_published', () => { if(session.slot !== null) resolve({ session, lobby }); });
    if (phase !== 'lobby' && session.slot !== null) resolve({ session, lobby });
  });
}
