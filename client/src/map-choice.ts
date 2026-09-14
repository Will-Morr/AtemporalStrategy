import type {LobbyState} from './contracts.generated';

export function renderMapChoices(lobby:LobbyState, slot:number|null, choose:(index:number)=>void):void {
  const root=document.getElementById('map-choices')!;
  root.replaceChildren();
  const controller=lobby.slots.find(s=>s.claimed);
  document.getElementById('map-choice-status')!.textContent=controller
    ? `${controller.profile?.username ?? `Player ${controller.slot}`} chooses the map · everyone sees the same options`
    : 'Claim a slot to choose the map. Everyone can preview the five options.';
  (lobby.map_candidates??[]).forEach((map,index)=>{
    const button=document.createElement('button');button.className='map-choice';button.setAttribute('aria-label',`Map ${index+1}`);button.setAttribute('aria-pressed',String(index===lobby.selected_map));button.disabled=slot===null||slot!==controller?.slot;
    button.onclick=()=>choose(index);
    const canvas=document.createElement('canvas');canvas.width=240;canvas.height=240;canvas.setAttribute('aria-hidden','true');
    const ctx=canvas.getContext('2d')!,t=map.terrain,sx=240/t.width,sy=240/t.height;
    t.cells.forEach((cell,i)=>{ctx.fillStyle=cell==='floor'?'#77777e':'#24252b';ctx.fillRect((i%t.width)*sx,Math.floor(i/t.width)*sy,sx+.5,sy+.5);if(map.ore[i]>0){ctx.fillStyle='#c5ad58';ctx.fillRect((i%t.width)*sx+1,Math.floor(i/t.width)*sy+1,Math.max(1,sx-2),Math.max(1,sy-2));}});
    map.starts.forEach((p,owner)=>{ctx.fillStyle=lobby.slots[owner]?.profile?.color??['#4fc3f7','#aed581','#ff8a65','#ce93d8'][owner];ctx.strokeStyle='#111';ctx.lineWidth=2;ctx.beginPath();ctx.arc((p.x+.5)*sx,(p.y+.5)*sy,6,0,Math.PI*2);ctx.fill();ctx.stroke();ctx.fillStyle='#fff';ctx.font='bold 11px system-ui';ctx.fillText(String(owner+1),(p.x+.5)*sx+7,(p.y+.5)*sy+4);});
    const label=document.createElement('strong');label.textContent=`Map ${index+1}${index===lobby.selected_map?' · Selected':''}`;button.append(canvas,label);root.append(button);
  });
}
