import type {Game} from './game';
let shown=false;
export function updateRematch(g:Game):void {
  let menu=document.getElementById('new-game-menu');
  let opener=document.getElementById('new-game-open') as HTMLButtonElement|null;
  if(!menu){
    menu=document.createElement('section');menu.id='new-game-menu';menu.setAttribute('role','dialog');menu.setAttribute('aria-label','Game complete');menu.hidden=true;
    const heading=document.createElement('h2');heading.textContent='Game complete';
    const note=document.createElement('p');note.id='new-game-note';
    const start=document.createElement('button');start.id='new-game-start';start.textContent='Choose maps for a new game';start.onclick=()=>{if(g.session.token&&g.net.matchId){start.disabled=true;note.textContent='Opening the new lobby for everyone…';g.net.send({kind:'new_match',slot_token:g.session.token,based_on_match_id:g.net.matchId});}};
    const close=document.createElement('button');close.textContent='Keep viewing replay';close.onclick=()=>{menu!.hidden=true;};
    menu.append(heading,note,start,close);document.getElementById('game')!.append(menu);
    opener=document.createElement('button');opener.id='new-game-open';opener.textContent='New game…';opener.onclick=()=>{menu!.hidden=false;};document.getElementById('top')!.append(opener);
  }
  opener!.hidden=!g.finished;
  if(!g.finished)return;
  const controller=g.profiles.find(p=>p.claimed);
  const own=g.player!==null&&controller?.slot===g.player;
  (document.getElementById('new-game-start') as HTMLButtonElement).disabled=!own||!g.net.connected;
  document.getElementById('new-game-note')!.textContent=own?'Keep these players and choose from five fresh maps. This match stays saved in the server archive.':`${controller?.profile?.username??'The controlling player'} can open the next game for everyone. You can keep reviewing this replay meanwhile.`;
  if(!shown){shown=true;menu.hidden=false;}
}
