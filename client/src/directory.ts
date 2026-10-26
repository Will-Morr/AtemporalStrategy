import type {Setup, LobbySlot} from './contracts.generated';
type Style='builder'|'raider';
interface Entry {id:string;name:string;created:number;locked:boolean;players:number;mine:boolean;bots:(Style|null)[];status:{phase:string;round:number;slots?:LobbySlot[];objective?:{kind:string}};replays:string[]}
interface Directory {games:Entry[];user:string|null;defaults:Setup}
const $=<T extends HTMLElement>(id:string)=>document.getElementById(id) as T;
let directory:Directory;
let advancedDirty=false;
let initialized=false;
function continueToGame():void {const path=new URLSearchParams(location.search).get('join');if(path?.startsWith('/games/')&&!path.includes('..'))location.replace(path);}
async function api(path:string,body?:unknown):Promise<any>{const response=await fetch(path,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});if(!response.ok)throw new Error(`Server request failed (${response.status})`);const value=await response.json();if(value.error)throw new Error(value.error);return value;}
function render():void {
  const filter=$<HTMLSelectElement>('filter').value,mine=$<HTMLInputElement>('mine').checked;
  const list=$('games');list.replaceChildren();
  const games=[...directory.games].sort((a,b)=>b.created-a.created).filter(g=>(!mine||g.mine)&&(filter==='all'||filter==='open'&&g.status.phase==='lobby'||filter==='active'&&['planning','simulating'].includes(g.status.phase)||filter==='recent'&&g.replays.length));
  for(const game of games){
    const card=document.createElement('article');card.className='card';card.dataset.game=game.id;
    const body=document.createElement('div'),badge=document.createElement('span');badge.className='badge';badge.textContent=game.status.phase==='lobby'?'Open lobby':game.status.phase==='archived'?'Archived':game.status.phase==='finished'?'Finished':'Active';
    const title=document.createElement('h3');title.textContent=game.name;body.append(badge,title);
    const info=document.createElement('p');info.textContent=`${game.players} players · ${(game.status.objective?.kind==='simple_win'?'Simple win':game.status.objective?.kind)??'Preparing'}${game.status.round?` · Round ${game.status.round}`:''}${game.locked?' · Password required':''}${game.mine?' · Your game':''}`;
    const roster=document.createElement('p');roster.className='players';roster.textContent=game.status.slots?.map(s=>s.profile?.username??`Slot ${s.slot}: open`).join(' · ')??'Preparing lobby…';body.append(info,roster);
    const link=document.createElement('a');link.className='button';link.href=`/games/${game.id}/`;link.textContent=game.status.phase==='lobby'?'Enter lobby':['finished','archived'].includes(game.status.phase)?'View game':'Watch / rejoin';
    card.append(body,link);
    if(game.replays.length){const replays=document.createElement('details');replays.className='replays';replays.open=filter==='recent';const summary=document.createElement('summary');summary.textContent=`Recent replays (${game.replays.length})`;replays.append(summary);for(const id of game.replays){const a=document.createElement('a');a.href=`/games/${game.id}/replays/${id}/`;const stamp=Number(id.split('-')[1]);a.textContent=Number.isFinite(stamp)?`Replay · ${new Date(stamp).toLocaleString()}`:`Replay ${id}`;a.title=id;replays.append(a);}card.append(replays);}
    list.append(card);
  }
  if(!games.length){const empty=document.createElement('div');empty.className='empty';empty.textContent=filter==='all'?'No games yet. Create a lobby to get started.':'No games match these filters.';list.append(empty);}
}
async function refresh():Promise<void>{try{directory=await api('/api/directory');if(!initialized){const c=directory.defaults.match_defaults;for(const [id,value] of Object.entries({'player-count':c.player_count,'objective':c.objective.kind,'control':c.control_limit,'map-size':c.map_size,'symmetric':c.symmetric,'sides':c.multiplayer.kind})){const select=$<HTMLSelectElement>(id);if(!Array.from(select.options).some(o=>o.value===String(value)))select.add(new Option(`${value} (server default)`,String(value)));select.value=String(value);}initialized=true;}$('profile').textContent=directory.user===null?'Choose a username':`Playing as ${directory.user}`;render();$('error').textContent='';}catch(e){$('error').textContent=(e as Error).message;}}
function setup():Setup {
  const s=structuredClone(directory.defaults),c=s.match_defaults;
  c.player_count=Number($<HTMLSelectElement>('player-count').value);c.map_size=Number($<HTMLSelectElement>('map-size').value);c.symmetric=$<HTMLSelectElement>('symmetric').value==='true';c.control_limit=$<HTMLSelectElement>('control').value as typeof c.control_limit;
  const kind=$<HTMLSelectElement>('objective').value;
  const rules={victory_rule:{kind:'fixed_target' as const,points:5},tie_policy:'continue_until_unique' as const,draw_scoring:'none' as const,time_penalty:'none' as const};
  c.objective=kind===c.objective.kind?c.objective:kind==='timed'?{kind,lock_ticks_per_round:100}:kind==='hybrid'?{kind,lock_ticks_per_round:100,rules}:{kind:kind==='simple_win'?'simple_win':'scoreboard',rules};
  if($<HTMLSelectElement>('sides').value==='teams'){
    s.available_teams=[{team_id:'cyan',label:'Cyan',capacity:Math.ceil(c.player_count/2)},{team_id:'orange',label:'Orange',capacity:Math.ceil(c.player_count/2)}];
    c.multiplayer={kind:'teams',assignments:Array.from({length:c.player_count},(_,i)=>({player_id:i,team_id:i<Math.ceil(c.player_count/2)?'cyan':'orange'}))};
  }else{s.available_teams=[];c.multiplayer={kind:'ffa'};}
  return s;
}
function syncSetup():void {if(!advancedDirty)$<HTMLTextAreaElement>('setup').value=JSON.stringify(setup(),null,2);}
function slots():void {
  const container=$('bot-slots'),previous=Array.from(container.querySelectorAll('select')).map(s=>s.value),all=$<HTMLInputElement>('all-bots').checked;container.replaceChildren();
  for(let i=0;i<Number($<HTMLSelectElement>('player-count').value);i++){
    const label=document.createElement('label');label.className='field';label.textContent=`Slot ${i}`;const select=document.createElement('select');select.id=`bot-${i}`;
    for(const [value,text] of [['','Human'],['builder','Builder bot'],['raider','Raider bot']]){if(all&&!value||!all&&i===0&&value)continue;const o=document.createElement('option');o.value=value;o.textContent=text;select.append(o);}
    select.value=all?(previous[i]||['raider','builder'][i%2]):i===0?'':previous[i]??'';label.append(select);container.append(label);
  }
  $('round-budget-label').hidden=!all;
}
$('new-game').onclick=()=>{advancedDirty=false;slots();syncSetup();$('create-error').textContent='';$<HTMLDialogElement>('create').showModal();};
$('cancel-create').onclick=()=>$<HTMLDialogElement>('create').close();
$('profile').onclick=()=>{$<HTMLInputElement>('user-name').value=directory.user??'';$<HTMLDialogElement>('identity').showModal();};
$('identity').addEventListener('cancel',e=>{if(directory.user===null)e.preventDefault();});
$('identity-form').onsubmit=async e=>{e.preventDefault();try{const username=$<HTMLInputElement>('user-name').value;await api('/api/identity',{username});localStorage.setItem('atemporal-username',username);$<HTMLDialogElement>('identity').close();await refresh();continueToGame();}catch(e){$('identity-error').textContent=(e as Error).message;}};
for(const id of ['objective','player-count','sides','control','map-size','symmetric'])$(id).onchange=()=>{if(id==='player-count'){if($<HTMLSelectElement>('player-count').value==='3')$<HTMLSelectElement>('symmetric').value='false';slots();}syncSetup();};
$('all-bots').onchange=slots;
$('setup').oninput=()=>{advancedDirty=true;};
$('reset-setup').onclick=()=>{advancedDirty=false;syncSetup();};
$('create-form').onsubmit=async e=>{
  e.preventDefault();const submit=$<HTMLButtonElement>('submit-create');submit.disabled=true;$('create-error').textContent='';
  try{
    const config=advancedDirty?JSON.parse($<HTMLTextAreaElement>('setup').value):setup();
    const botSlots=Array.from($('bot-slots').querySelectorAll('select')).map(s=>s.value||null);
    const result=await api('/api/games',{name:$<HTMLInputElement>('game-name').value,password:$<HTMLInputElement>('password').value,setup:config,bots:{slots:botSlots,playtest_rounds:Number($<HTMLInputElement>('round-budget').value)}});location.href=result.url;
  }catch(e){$('create-error').textContent=(e as Error).message;submit.disabled=false;}
};
$('filter').onchange=render;$('mine').onchange=render;$('refresh').onclick=()=>void refresh();
void refresh().then(()=>{if(directory?.user===null){$<HTMLInputElement>('user-name').value=localStorage.getItem('atemporal-username')??'';$<HTMLDialogElement>('identity').showModal();}else if(directory)continueToGame();});
setInterval(()=>{if(!document.hidden&&!document.querySelector('dialog[open]'))void refresh();},4000);
