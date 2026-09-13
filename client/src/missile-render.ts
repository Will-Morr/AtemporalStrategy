import {idKey, type Game} from './game';
import type {Renderer} from './render';
import type {Tile} from './contracts.generated';
import {flightPosition,flightTicks,missileName,siloPlan} from './silo';

export function drawMissileOverlays(g:Game,r:Renderer,ctx:CanvasRenderingContext2D):void {
  const scale=r.camera.scale,visible=g.visibility(),team=g.profiles[g.player!]?.profile?.team_id;
  const allied=(owner:number)=>owner===g.player || (!!team&&g.profiles[owner]?.profile?.team_id===team);
  const sees=(t:Tile)=>g.spectator||visible.has(`${Math.round(t.x)},${Math.round(t.y)}`);
  const circle=(t:Tile,radius:number,color:string,fill=false)=>{const [x,y]=r.screen(t.x+.5,t.y+.5);ctx.strokeStyle=color;ctx.beginPath();ctx.arc(x,y,radius*scale,0,Math.PI*2);if(fill){ctx.save();ctx.globalAlpha=.08;ctx.fillStyle=color;ctx.fill();ctx.restore();}ctx.stroke();};
  const line=(a:{x:number,y:number},b:Tile,color:string)=>{ctx.strokeStyle=color;const [x,y]=r.screen(a.x+.5,a.y+.5),[tx,ty]=r.screen(b.x+.5,b.y+.5);ctx.beginPath();ctx.moveTo(x,y);ctx.lineTo(tx,ty);ctx.stroke();};
  const color=(key:string)=>key==='tac_nuke'?'#ff8677':key==='cluster'?'#ffda69':'#91eee4';
  const selected=g.selectedViews().filter(v=>g.ownSelectable(v));
  ctx.save();ctx.lineWidth=1.5;ctx.setLineDash([5,5]);
  if(selected.length===1){const v=selected[0],d=g.types.get(v.type_key);const radius=d?.silo?.auto_range??(!d?.movement?d?.weapon?.range:undefined);if(radius)circle(v,radius,d?.silo?'#80b4d7':'#cad8e8');}
  for(const v of selected.filter(v=>g.types.get(v.type_key)?.silo))for(const launch of siloPlan(g,v).launches){line(v,launch.target,color(launch.type_key));circle(launch.target,g.types.get(launch.type_key)!.missile!.radius,color(launch.type_key));}
  const readout=document.getElementById('range-readout')!;readout.hidden=true;
  const hover=g.hover;
  if(hover && (g.mode.kind==='place'||g.mode.kind==='launch')){
    let text='';
    if(g.mode.kind==='place'){
      const def=g.types.get(g.mode.type_key),radius=def?.silo?.auto_range??def?.weapon?.range;
      if(radius){circle(hover,radius,'#c5eff7',true);text=def?.silo?`Auto targeting: ${radius} tiles\nManual launches: unlimited range`:`Turret reach: ${radius} tiles`;}
    }else{
      const m=g.types.get(g.mode.type_key)!.missile!;
      const ids=g.mode.silos;const silos=selected.filter(v=>ids.some(id=>idKey(id)===idKey(v.id)));
      for(const v of silos)line(v,hover,color(g.mode.type_key));circle(hover,m.radius,color(g.mode.type_key),true);
      const ticks=silos.map(v=>flightTicks(v,hover,m));
      text=`${missileName(g.mode.type_key)} · ${hover.x}, ${hover.y}\n${ticks.length?Math.min(...ticks):'—'} ticks to hit after launch · radius ${m.radius}\n${m.effect==='satellite'?'Reveals in flight + '+m.reveal_ticks+' ticks at destination':m.effect==='cluster'?m.damage+' damage · also hits allies':'Destroys all units & buildings · also hits allies'}`;
    }
    if(text){readout.hidden=false;readout.textContent=text;const [x,y]=r.screen(hover.x+1,hover.y+1);readout.style.left=`${Math.max(6,Math.min(ctx.canvas.width-275,x+8))}px`;readout.style.top=`${Math.max(document.getElementById('top')!.offsetHeight+8,Math.min(ctx.canvas.height-90,y+6))}px`;}
  }
  ctx.setLineDash([]);
  for(const f of g.missileState().missiles){const p=flightPosition(f,g.playhead);if(!g.spectator&&!allied(f.owner)&&!sees(p))continue;
    const [x,y]=r.screen(p.x+.5,p.y+.5);const c=color(f.type_key);ctx.save();ctx.translate(x,y);ctx.rotate(Math.atan2(f.target.y-f.origin.y,f.target.x-f.origin.x)+Math.PI/2);ctx.fillStyle=g.color(f.owner);ctx.strokeStyle='#101b25';ctx.beginPath();ctx.moveTo(0,-9);ctx.lineTo(5,5);ctx.lineTo(-5,5);ctx.closePath();ctx.fill();ctx.stroke();ctx.strokeStyle=c;ctx.beginPath();ctx.moveTo(0,6);ctx.lineTo(0,15);ctx.stroke();ctx.restore();
    if(allied(f.owner)||g.spectator){circle(f.target,g.types.get(f.type_key)!.missile!.radius,c);ctx.fillStyle=c;ctx.font='11px system-ui';ctx.fillText(`${missileName(f.type_key)} · ${Math.max(0,Math.ceil(f.impact_tick-g.playhead))}t`,x+10,y-8);}
  }
  for(const event of g.missileEvents())if(event.tick<=g.playhead&&event.tick>g.playhead-6&&event.event.kind==='missile_impact'&&(allied(event.event.owner)||sees(event.event.target))){
    const e=event.event;ctx.globalAlpha=Math.max(0,1-(g.playhead-event.tick)/6);circle(e.target,e.radius,color(e.type_key),true);ctx.globalAlpha=1;
  }
  ctx.restore();
}
