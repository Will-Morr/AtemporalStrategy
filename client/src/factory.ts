import {siloPlan} from './silo';
import type { Game, EntityView } from './game';
import type { DraftItemRef2, Order, Priority } from './contracts.generated';
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
export function factoryPlan(g: Game, v: EntityView) {
  const p=v.exact?.production;
  let pending: {type: string; ref: DraftItemRef2 | null; loop: boolean}[]=v.blueprint ? (v.settings?.queue??[]).map((type,index)=>({type,ref:null,loop:v.settings?.queue_loop_flags?.[index]??false})) : (p?.pending_items??[]).map(i=>({type:i.type_key,loop:i.loop_enabled??false,ref:{kind:'persistent',id:i.item_id}}));
  let active=p?.active_item ? structuredClone(p.active_item) : null;
  let loop=v.settings?.loop_enabled ?? p?.loop_enabled ?? false;
  let priority:Priority=v.settings?.priority ?? v.exact?.priority ?? 'medium';
  let group=p?.spawn_group;
  if(!v.blueprint && g.current===g.latest && g.draft.tick===Math.floor(g.playhead)) for(const d of g.draft.commands) {
    const c=d.command;
    if(c.kind==='set_priority'&&c.entities.some(id=>same(id,v.id)))priority=c.priority;
    if('factories' in c && c.factories.some(id=>same(id,v.id))) {
      if(c.kind==='set_queue_loop')loop=c.enabled;
      if(c.kind==='bind_factory_group')group=c.group;
      if(c.kind==='edit_production') {
        const e=c.edit;
        if(e.kind==='append'||e.kind==='replace_pending') {
          const items=e.items.map((type,item_index)=>({type,loop,ref:{kind:'draft' as const,local_id:d.local_id,item_index}}));
          pending=e.kind==='append'?[...pending,...items]:items;
        } else if(e.kind==='remove_pending')pending=pending.filter(i=>!e.item_ids.some(ref=>same(ref,i.ref)));
        else if(e.kind==='set_item_loop'){for(const item of pending)if(e.item_ids.some(ref=>same(ref,item.ref)))item.loop=e.enabled;if(active&&e.item_ids.some(ref=>ref.kind==='persistent'&&same(ref.id,active!.item_id)))active.loop_enabled=e.enabled;}
        else if(e.kind==='cancel_active')active=null;
      }
    }
  }
  const saved=group ? g.experience.groups().find(x=>same(x.id,group))?.latest_order?.order : null;
  const order:Order=saved ?? g.effectiveOrder(v) ?? {kind:'idle'};
  return {pending,active,loop,priority,order,group,silo:siloPlan(g,v),inventory:(v.silo??p?.silo)?.inventory};
}
export function orderLabel(order: Order): string {
  switch(order.kind){case 'attack_move':return `Attack move → ${order.destination.x}, ${order.destination.y}`;case 'construct':return 'Construct in assigned area';case 'mine':return 'Mine in assigned area';case 'support':return 'Support assigned ally';default:return 'Idle';}
}
