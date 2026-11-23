import {practice} from './practice';
import type {Game} from './game';
import {feedbackForm} from './feedback';

function roundResults(g: Game, list: HTMLElement, menu: HTMLElement): void {
  const rounds = [...g.experience.rounds.values()].filter(r => r.round > 0).sort((a,b) => a.round-b.round);
  const signature = JSON.stringify([g.current,g.latest,rounds.map(r => r.revision),g.profiles.map(p=>p.profile)]);
  if (list.dataset.signature === signature) return;
  list.dataset.signature = signature;
  list.replaceChildren();
  for (const r of rounds) {
    const row = document.createElement('button');
    const {kind,survivors,terminal_state_tick} = r.outcome;
    const won = g.player !== null && survivors.includes(g.player);
    const verdict = kind === 'win' ? g.spectator ? 'Winner' : won ? 'Win' : 'Loss' : kind === 'stalemate' ? 'Stalemate' : 'Draw';
    row.dataset.result = kind === 'win' && !g.spectator ? won ? 'win' : 'loss' : kind;
    if (g.spectator && kind === 'win' && survivors.length) row.style.setProperty('--result-color',g.color(survivors[0]));
    row.setAttribute('aria-current', String(r.revision === g.current));
    const number = document.createElement('strong');
    number.textContent = 'Round '+r.round;
    const result = document.createElement('span');
    result.textContent = verdict;
    const detail = document.createElement('small');
    const names = survivors.map(p => g.name(p)).join(', ');
    detail.textContent = kind === 'win' ? names+' won' : 'Tick '+terminal_state_tick;
    row.title = 'View round '+r.round+' at tick '+terminal_state_tick;
    row.append(number,result,detail);
    row.onclick = async () => {
      await g.experience.selectRound(r.revision,terminal_state_tick);
      if (g.current === r.revision) menu.hidden=true;
    };
    list.append(row);
  }
  if (!rounds.length) list.textContent = g.latest === 0 ? 'No completed rounds.' : 'Loading round results…';
}

export function updateRematch(g:Game):void {
  if (practice || !g.finished || !g.experience) return;
  const archive = location.pathname.includes('/replays/');
  const botOnly = document.body.dataset.botOnly === 'true';
  let menu = document.getElementById('new-game-menu');
  if (!menu) {
    menu=document.createElement('section');
    menu.id='new-game-menu';
    menu.setAttribute('role','dialog');
    menu.setAttribute('aria-label','Game complete');
    menu.innerHTML='<h2>Match review</h2><p id="match-verdict"></p><h3>Round results</h3><div id="round-results"></div><div class="review-actions"></div>';
    const actions=menu.querySelector<HTMLElement>('.review-actions')!;
    const action=(label:string,run:()=>void)=>{
      const b=document.createElement('button');b.textContent=label;b.onclick=run;actions.append(b);return b;
    };
    action('Keep viewing replay',()=>{menu!.hidden=true;});
    action('Try a different timeline',()=>{
      menu!.hidden=true;
      (document.getElementById('replay') as HTMLDetailsElement).open=true;
      document.getElementById('practice-player')?.focus();
    });
    if (!archive) {
      const start=action(botOnly?'Create another game':'Choose maps for a new game',()=>{
        if(botOnly) {location.href='/lobbies/';return;}
        if(g.session.token && g.net.matchId) {
          start.disabled=true;
          g.net.send({kind:'new_match',slot_token:g.session.token,based_on_match_id:g.net.matchId});
        }
      });
      start.id='new-game-start';
    }
    if(!archive && location.pathname.startsWith('/games/')) actions.append(feedbackForm(g));
    document.getElementById('game')!.append(menu);
    const opener=document.createElement('button');
    opener.id='new-game-open';opener.textContent='Match review';opener.onclick=()=>{menu!.hidden=false;};
    document.getElementById('top-controls')!.append(opener);
  }
  const start=document.getElementById('new-game-start') as HTMLButtonElement|null;
  if(start) start.disabled=!botOnly && (g.player===null || g.profiles.find(p=>p.claimed)?.slot!==g.player || !g.net.connected);
  const winners=g.experience.rounds.get(g.latest)?.score?.match_winners ?? [];
  document.getElementById('match-verdict')!.textContent=winners.length
    ? 'Match won by '+winners.map(s=>s.kind==='player'?g.name(s.player_id):s.team_id).join(', ')
    : g.finished;
  roundResults(g,document.getElementById('round-results')!,menu);
}
