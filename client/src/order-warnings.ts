import {factoryPlan} from './factory';
import type {EntityView, Game} from './game';

type OrderWarning = {entity:EntityView;message:string;construction?:true};

export function orderWarnings(g: Game): OrderWarning[] {
  if(g.spectator||g.current!==g.latest||g.finished||g.progress||g.preview||g.committed.includes(g.player??-1)||!g.exact)return [];
  if(!g.draft.commands.length||g.draft.tick!==Math.floor(g.playhead))return [];
  const own=g.entities().filter(v=>g.ownSelectable(v));
  const builders=own.flatMap(v=>{
    if(v.lifecycle!=='complete'||!g.types.get(v.type_key)?.construction||g.effectivePriority(v)==='off')return [];
    const order=g.effectiveOrder(v);
    return order?.kind==='construct'?[order.area]:[];
  });
  const warnings:OrderWarning[]=[];
  for(const v of own){
    const t=g.types.get(v.type_key)!;
    const order=g.effectiveOrder(v)??{kind:'idle'};
    if(t.production&&!t.silo){
      const plan=factoryPlan(g,v),items=[...plan.pending.map(i=>i.type),...(plan.active?[plan.active.type_key]:[])];
      if(!items.length)warnings.push({entity:v,message:'Factory has no production queue'});
      else if(plan.order.kind==='idle')warnings.push({entity:v,message:`${items.length} queued unit${items.length===1?'':'s'} will start idle — give this factory newborn orders`});
    } else if(v.lifecycle==='complete'&&t.movement&&order.kind==='idle')warnings.push({entity:v,message:`${v.type_key} has no orders${t.mining||t.construction?' — assign a work area':''}`});
    if(v.lifecycle!=='complete'&&g.effectivePriority(v)!=='off'&&!builders.some(area=>v.x>=area.min.x&&v.x<=area.max.x&&v.y>=area.min.y&&v.y<=area.max.y)){
      warnings.push({entity:v,message:`${v.type_key} has no constructor assigned to its area`,construction:true});
    }
  }
  return warnings;
}

export function updateOrderWarnings(g: Game): void {
  const root=document.getElementById('order-warnings')!;
  const warnings=orderWarnings(g);
  root.hidden=!warnings.length;
  root.replaceChildren();
  if(!warnings.length)return;
  const label=document.createElement('strong');label.textContent=`Needs orders · ${warnings.length}`;
  const construction=warnings.filter(w=>w.construction);
  const other=warnings.filter(w=>!w.construction);
  const relevant=other.filter(w=>g.selection.has(JSON.stringify(w.entity.id)));
  const detail=document.createElement('span');
  const messages=(relevant.length?relevant:other).slice(0,construction.length?1:2).map(w=>w.message);
  if(construction.length)messages.unshift(`No active Construct order covers ${construction.length===1?'1 planned building':`${construction.length} planned buildings`} at this tick — select a constructor and press C.`);
  detail.textContent=messages.join(' · ');
  root.title=`At tick ${Math.floor(g.playhead)}. Future orders may change this.\n${[...new Set(warnings.map(w=>w.message))].join('\n')}`;
  const review=document.createElement('button');review.textContent='Review next';review.onclick=()=>{const entities=[...new Map([...construction,...other].map(w=>[JSON.stringify(w.entity.id),w.entity])).values()];const current=entities.findIndex(v=>g.selection.has(JSON.stringify(v.id)));const next=entities[(current+1)%entities.length];g.renderer.centerOn(next.x,next.y);g.selectViews([next]);};
  root.append(label,detail,review);
}
