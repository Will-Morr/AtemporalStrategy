import {Net} from './net';
import type {Setup} from './contracts.generated';
import {setFingerprint} from './session';

setFingerprint();

const play=document.getElementById('play') as HTMLButtonElement;
const status=document.getElementById('play-status')!;
const resume=document.getElementById('resume') as HTMLAnchorElement;
const pendingKey='atemporal-quick-play';
async function api(path:string,body?:unknown):Promise<any>{
  const response=await fetch(path,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  if(!response.ok)throw new Error(`Server request failed (${response.status}).`);
  const value=await response.json();if(value.error)throw new Error(value.error);return value;
}
async function askName():Promise<string|null>{
  const dialog=document.getElementById('name-dialog') as HTMLDialogElement,input=document.getElementById('name-input') as HTMLInputElement;
  input.value=localStorage.getItem('atemporal-username')??'';dialog.returnValue='';dialog.showModal();
  await new Promise(r=>dialog.addEventListener('close',r,{once:true}));
  return dialog.returnValue==='ok'?input.value.trim():null;
}
play.onclick=async()=>{
  if(play.disabled)return;
  play.disabled=true;resume.hidden=true;status.textContent='Preparing your game…';
  let url=sessionStorage.getItem(pendingKey);
  let net:Net|null=null;
  try {
    const directory=await api('/api/directory');
    // Earlier quick-play visitors were silently named "Player"; let them choose too.
    let username:string|null=directory.user==='Player'?null:directory.user;
    if(username===null){
      status.textContent='';username=await askName();
      if(!username){status.textContent='';play.disabled=false;return;}
      await api('/api/identity',{username});localStorage.setItem('atemporal-username',username);
      status.textContent='Preparing your game…';
    }
    if(url&&!directory.games.some((g:{id:string})=>url===`/games/${g.id}/`))url=null;
    if(!url){
      const setup:Setup=structuredClone(directory.defaults);
      setup.match_defaults.player_count=2;setup.match_defaults.multiplayer={kind:'ffa'};
      setup.match_defaults.transport='authoritative';setup.available_teams=[];
      const result=await api('/api/games',{name:`${username} vs Bot`,setup,bots:{slots:[null,directory.default_bot],playtest_rounds:20}});
      url=result.url;sessionStorage.setItem(pendingKey,url!);
    }
    const path=url!;
    const tokenKey=`atemporal-slot-token:${path}`;
    let token=sessionStorage.getItem(tokenKey);
    net=new Net(path);
    await net.connect();
    let rejected:string|null=null;
    net.on('lobby_update_rejected',m=>{rejected=m.message;});
    let welcome=await net.request({kind:'hello',protocol_version:2,last_revision:null,slot_token:token},'welcome');
    if(welcome.phase==='lobby'){
      if(!token){
        const claimed=await net.request({kind:'claim_slot',slot:0,username,color:'#4fc3f7',team_id:null},'slot_claimed');
        token=claimed.private_token;sessionStorage.setItem(tokenKey,token);
      }
      welcome=await net.request({kind:'hello',protocol_version:2,last_revision:null,slot_token:token},'welcome');
      status.textContent='Starting your game…';
      net.send({kind:'start_match',slot_token:token,based_on_lobby_revision:welcome.lobby.revision});
      // A hello on the same connection is handled after start, so navigation cannot
      // race sending the command. The game page rejoins the confirmed player token.
      const started=await net.request({kind:'hello',protocol_version:2,last_revision:null,slot_token:token},'welcome');
      if(started.phase==='lobby')throw new Error(rejected??'Could not start the game. Try again or open your game.');
    }
    sessionStorage.removeItem(pendingKey);
    location.assign(path);
  }catch(error){
    status.textContent=(error as Error).message;play.disabled=false;
    if(url){resume.href=url;resume.hidden=false;}
  }finally{net?.disconnect();}
};
