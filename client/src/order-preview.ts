import type {Neighbors, Order, Tile} from './contracts.generated';
import {allied} from './vision';
import {factoryPlan} from './factory';
import {idKey, type EntityView, type Game} from './game';
import type {Renderer} from './render';
import {PreviewFields, miningTargets, targetCorners, type MiningPreview} from './path-preview';

export interface OrderRoute {entity: string; rule: Neighbors; actual: boolean; from: Tile; goal: Tile; points: Tile[]}
export class OrderPreviews {
  routes: OrderRoute[]=[];
  private fields: PreviewFields | null=null;
  private topology='';
  private breachFields = new Map<number,PreviewFields>();
  draw(g:Game,r:Renderer,ctx:CanvasRenderingContext2D,views:EntityView[]):void {
    this.routes=[];if(!g.terrain)return;
    const structures=views.filter(v=>!v.remembered&&!v.blueprint&&g.types.get(v.type_key)?.kind==='structure');
    const cell=(v:EntityView)=>Math.floor(v.y)*g.terrain!.width+Math.floor(v.x);
    const topology=structures.map(v=>`${cell(v)}:${v.owner}`).join(',');
    if(this.fields?.terrain!==g.terrain||topology!==this.topology){
      this.fields=new PreviewFields(g.terrain,new Set(structures.map(cell)));
      this.breachFields.clear();
      this.topology=topology;
    }
    const fields=this.fields!;
    const aiming=(g.mode.kind==='attack'||g.mode.kind==='stored-target')&&!!g.hover;
    const targets=new Map<string,Tile>();
    const previewMining=!aiming&&views.some(v=>g.selection.has(idKey(v.id))&&(g.types.get(v.type_key)?.production?factoryPlan(g,v).order:g.effectiveOrder(v))?.kind==='mine');
    const miners:MiningPreview[]=previewMining?views.flatMap(v=>{
      const order=g.effectiveOrder(v),def=g.types.get(v.type_key);
      return !v.blueprint&&v.lifecycle==='complete'&&def?.mining&&def.movement&&order?.kind==='mine'?[{key:idKey(v.id),from:{x:Math.floor(v.x),y:Math.floor(v.y)},area:order.area,rule:def.movement.neighbors,retained:order===v.exact?.action?v.exact.resolved_destination:null}]:[];
    }):[];
    const oreTargets=miningTargets(fields,miners,i=>g.oreAt(i));
    for(const v of views){
      if(v.remembered||!g.selection.has(idKey(v.id)))continue;
      if(!aiming&&g.experience.review.recipients.get(idKey(v.id)))continue;
      const def=g.types.get(v.type_key),factory=!!def?.production&&!def.silo;
      if(!factory&&!def?.movement)continue;
      const plan=factory?factoryPlan(g,v):null;
      const order:Order|undefined=aiming&&g.ownSelectable(v)?{kind:'attack_move',destination:g.hover!}:plan?.order??g.effectiveOrder(v);
      if(!order||!['attack_move','mine','construct'].includes(order.kind))continue;
      let from={x:Math.floor(v.x),y:Math.floor(v.y)};
      if(factory){const offsets={n:[0,-1],e:[1,0],s:[0,1],w:[-1,0]};const [dx,dy]=offsets[v.outputDirection??v.exact?.production?.output_direction??'e'];from=v.exact?.production?.output_tile??{x:from.x+dx,y:from.y+dy};}
      const types=plan?[...(plan.active?[plan.active.type_key]:[]),...plan.pending.map(i=>i.type)]:[v.type_key];
      const eligible=types.map(t=>g.types.get(t)).filter(t=>t?.movement&&(order.kind==='attack_move'||(order.kind==='mine'?t.mining:t.construction)));
      const profiles=new Map<string,{rule:Neighbors;breach:boolean}>();
      for (const type of eligible) {
        const rule=type!.movement!.neighbors;
        const breach=g.net.rulesVersion>=13 && order.kind==='attack_move' && (type!.weapon?.damage??0)>0;
        profiles.set(`${rule}:${breach}`,{rule,breach});
      }
      for(const {rule,breach} of profiles.values()){
        let routeFields=fields;
        if(breach){
          if(!this.breachFields.has(v.owner)){
            const blockers=structures.filter(b=>allied(g.config.multiplayer,v.owner,b.owner)).map(cell);
            this.breachFields.set(v.owner,new PreviewFields(g.terrain,new Set(blockers)));
          }
          routeFields=this.breachFields.get(v.owner)!;
        }
        let goal:Tile|undefined,adjacent=false;
        if(order.kind==='attack_move')goal=order.destination;
        else if(order.kind==='mine'){
          goal=factory?miningTargets(fields,[...miners,{key:'factory-preview',from,area:order.area,rule}],i=>g.oreAt(i)).get('factory-preview'):oreTargets.get(idKey(v.id));
        }
        else if(order.kind==='construct'){
          const inside=(t:Tile)=>t.x>=order.area.min.x&&t.y>=order.area.min.y&&t.x<=order.area.max.x&&t.y<=order.area.max.y;
          const source=fields.field(from,rule);
          const candidates:Tile[]=views.filter(b=>b.owner===v.owner&&(b.blueprint||b.lifecycle==='site')&&inside(b)).map(b=>({x:b.x,y:b.y}));
          adjacent=true;
          let best=Infinity;
          for(const t of candidates){let d=source[fields.index(t)];if(adjacent){d=Infinity;for(let dy=-1;dy<=1;dy++)for(let dx=-1;dx<=1;dx++){if(!dx&&!dy)continue;const p={x:t.x+dx,y:t.y+dy};if(fields.open(p)&&source[fields.index(p)]>=0)d=Math.min(d,source[fields.index(p)]);}}
            if(d>=0&&(d<best||(d===best&&goal&&(t.x<goal.x||(t.x===goal.x&&t.y<goal.y))))){goal=t;best=d;}}
        }
        if(!goal)continue;
        targets.set(`${goal.x},${goal.y}`,goal);
        const points=r.pathPreviews?routeFields.route(from,goal,rule,adjacent,factory?(v.outputDirection??v.exact?.production?.output_direction??'e'):v.facing,g.net.rulesVersion>=9):[from,goal];
        this.routes.push({entity:idKey(v.id),rule,actual:true,from,goal,points});
      }
    }
    ctx.save();
    // Offset walker dots so both colors remain visible on shared factory routes.
    for(const route of [...this.routes].sort((a,b)=>a.rule===b.rule?0:a.rule==='four'?-1:1)){
      const points=route.points;ctx.strokeStyle=r.pathPreviews?(route.rule==='four'?'#57d5ff':'#ffdb73'):'#ef5350';ctx.lineWidth=r.pathPreviews?(route.actual?3:1.5):1.5;ctx.globalAlpha=route.actual?1:.65;ctx.lineCap='round';ctx.setLineDash([0,7]);ctx.lineDashOffset=route.rule==='eight'?3.5:0;ctx.beginPath();
      points.forEach((p,i)=>{const [x,y]=r.screen(p.x+.5,p.y+.5);if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y);});ctx.stroke();
      if(!points.length){const [x,y]=r.screen(route.from.x+.5,route.from.y+.5);ctx.setLineDash([]);ctx.strokeStyle='#ff6262';ctx.beginPath();ctx.moveTo(x-4,y-4);ctx.lineTo(x+4,y+4);ctx.moveTo(x+4,y-4);ctx.lineTo(x-4,y+4);ctx.stroke();}
    }
    ctx.globalAlpha=1;ctx.setLineDash([]);ctx.lineDashOffset=0;
    if(aiming)targets.set('hover',g.hover!);
    for(const t of targets.values()){
      const [tx,ty]=r.screen(t.x,t.y),c=targetCorners(r.camera.scale),x=tx+c.inset,y=ty+c.inset,s=c.span,k=c.arm;
      ctx.lineJoin='miter';ctx.lineCap='butt';
      ctx.beginPath();for(const [dx,dy,sx,sy] of [[0,0,1,1],[s,0,-1,1],[0,s,1,-1],[s,s,-1,-1]]){ctx.moveTo(x+dx+sx*k,y+dy);ctx.lineTo(x+dx,y+dy);ctx.lineTo(x+dx,y+dy+sy*k);}
      ctx.strokeStyle='#230d16';ctx.lineWidth=c.backing;ctx.stroke();ctx.strokeStyle='#ff6262';ctx.lineWidth=c.stroke;ctx.stroke();
    }
    if(this.routes.length&&r.pathPreviews){ctx.font='12px system-ui';const label='Treads ··· cyan · Walker ··· gold · × no route';const y=document.getElementById('top')!.offsetHeight+22;ctx.fillStyle='#101d2aeb';ctx.fillRect((innerWidth-290)/2,y-15,290,24);ctx.fillStyle='#e5f3ff';ctx.fillText(label,(innerWidth-290)/2+8,y+1);}
    ctx.restore();
  }
}
