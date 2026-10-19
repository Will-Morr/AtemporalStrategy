import {factoryPlan} from './factory';
import type {EntityView, Game} from './game';

export function orderWarnings(g: Game): {entity:EntityView;message:string}[] {
  if(g.spectator||g.current!==g.latest||g.finished||g.progress||g.preview||g.committed.includes(g.player??-1)||!g.exact)return [];
  const own=g.entities().filter(v=>g.ownSelectable(v));
  const builders=own.filter(v=>g.types.get(v.type_key)?.construction);
  const warnings:{entity:EntityView;message:string}[]=[];
  for(const v of own){
    const t=g.types.get(v.type_key)!;
    const order=g.effectiveOrder(v)??{kind:'idle'};
    if(t.production&&!t.silo){
      const plan=factoryPlan(g,v),items=[...plan.pending.map(i=>i.type),...(plan.active?[plan.active.type_key]:[])];
      if(!items.length)warnings.push({entity:v,message:'Factory has no production queue'});
      else if(plan.order.kind==='idle')warnings.push({entity:v,message:`${items.length} queued unit${items.length===1?'':'s'} will start idle — give this factory newborn orders`});
      else {
        const incompatible=items.filter(key=>{const unit=g.types.get(key);return (plan.order.kind==='mine'&&!unit?.mining)||(plan.order.kind==='construct'&&!unit?.construction);});
        if(incompatible.length)warnings.push({entity:v,message:`${[...new Set(incompatible)].join(', ')} cannot follow this factory’s newborn order`});
      }
    } else if(v.lifecycle==='complete'&&t.movement&&order.kind==='idle')warnings.push({entity:v,message:`${v.type_key} has no orders${t.mining||t.construction?' — assign a work area':''}`});
    if(v.lifecycle!=='complete'&&!builders.some(b=>{const o=g.effectiveOrder(b);return o?.kind==='construct'&&v.x>=o.area.min.x&&v.x<=o.area.max.x&&v.y>=o.area.min.y&&v.y<=o.area.max.y;}))warnings.push({entity:v,message:`${v.type_key} has no constructor assigned to its area`});
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
  const relevant=warnings.filter(w=>g.selection.has(JSON.stringify(w.entity.id)));
  const detail=document.createElement('span');detail.textContent=(relevant.length?relevant:warnings).slice(0,2).map(w=>w.message).join(' · ');
  root.title=`At tick ${Math.floor(g.playhead)}. Future orders may change this.\n${warnings.map(w=>w.message).join('\n')}`;
  const review=document.createElement('button');review.textContent='Review next';review.onclick=()=>{const entities=[...new Map(warnings.map(w=>[JSON.stringify(w.entity.id),w.entity])).values()];const current=entities.findIndex(v=>g.selection.has(JSON.stringify(v.id)));const next=entities[(current+1)%entities.length];g.renderer.centerOn(next.x,next.y);g.selectViews([next]);};
  root.append(label,detail,review);
}
