import type {EntityView, Game} from './game';
import {factoryPlan} from './factory';
import {idKey} from './game';
import type {MissileCapability, SiloPlan, Tile} from './contracts.generated';
import {unitIcon} from './icons';
export const missileName=(key:string)=>key==='tac_nuke'?'Tac nuke':key[0].toUpperCase()+key.slice(1);
export const flightTicks=(a:{x:number,y:number},b:Tile,m:MissileCapability)=>Math.max(1,Math.min(m.max_flight_ticks,Math.ceil(Math.sqrt((a.x-b.x)**2+(a.y-b.y)**2)/m.speed)));
export function siloPlan(g:Game,v:EntityView):SiloPlan {
  let plan=v.settings?.silo_plan ?? (v.silo??v.exact?.production?.silo)?.plan ?? {automatic:false,launches:[]};
  if(!v.blueprint && g.current===g.latest && g.draft.tick===Math.floor(g.playhead))for(const {command} of g.draft.commands){
    if(command.kind==='set_silo_plan' && command.silos.some(id=>idKey(id)===idKey(v.id)))plan=command.plan;
    if(command.kind==='edit_production' && command.edit.kind==='supply_launches' && command.factories.some(id=>idKey(id)===idKey(v.id)))plan=command.edit.plan;
  }
  return structuredClone(plan);
}
export function setSiloPlan(g:Game,v:EntityView,plan:SiloPlan):void {
  if(v.blueprint)g.stage({kind:'configure_blueprints',blueprint_ids:[v.blueprint],settings:{...structuredClone(v.settings!),silo_plan:plan}});
  else g.stage({kind:'set_silo_plan',silos:[v.id],plan});
}
export function queueLaunches(g:Game,v:EntityView,type:string,target:Tile,count:number):void {
  const production=factoryPlan(g,v), plan=production.silo;
  for(let i=0;i<count;i++)plan.launches.push({type_key:type,target});
  const pending=production.pending.filter(item=>item.type===type);
  const active=production.active?.type_key===type ? production.active : null;
  const repeats=pending.some(item=>item.loop) || active?.loop_enabled;
  const stock=production.inventory?.find(item=>item.type_key===type)?.count??0;
  const demand=plan.launches.filter(launch=>launch.type_key===type).length;
  const shortage=repeats?0:Math.max(0,demand-stock-pending.length-(active?1:0));
  if(!shortage){setSiloPlan(g,v,plan);return;}
  const items=Array<string>(shortage).fill(type);
  if(v.blueprint){
    const settings=structuredClone(v.settings!);
    settings.queue_loop_flags=settings.queue.map((_,i)=>settings.queue_loop_flags?.[i]??false);
    settings.queue.push(...items);
    settings.queue_loop_flags.push(...items.map(()=>settings.loop_enabled));
    settings.silo_plan=plan;
    g.stage({kind:'configure_blueprints',blueprint_ids:[v.blueprint],settings});
  } else {
    g.stage({kind:'edit_production',factories:[v.id],edit:{kind:'supply_launches',items,plan}});
  }
}
export function siloPanel(g:Game,v:EntityView):HTMLElement {
  const root=document.createElement('section');root.className='silo-controls';const enabled=g.canStage().ok,plan=siloPlan(g,v),def=g.types.get(v.type_key)!;
  const button=(label:string,fn:(event:MouseEvent)=>void)=>{const b=document.createElement('button');b.textContent=label;b.onclick=fn;b.disabled=!enabled;return b;};
  root.setAttribute('aria-label','Missile inventory and launches');
  const inventory=document.createElement('div');inventory.className='missile-inventory';const stockLabel=document.createElement('span');stockLabel.textContent='Stock';stockLabel.className='stock-label';inventory.append(stockLabel);
  for(const type of def.production!.recipes){const count=(v.silo??v.exact?.production?.silo)?.inventory.find(s=>s.type_key===type)?.count??0;const chip=document.createElement('span');chip.className='unit-chip';chip.title=`${missileName(type)}: ${count} stored`;chip.setAttribute('aria-label',chip.title);chip.dataset.missile=type;chip.dataset.count=String(count);chip.append(unitIcon(type,g.color(v.owner)),document.createTextNode(`×${count}`));inventory.append(chip);}
  root.append(inventory);
  const auto=button(`Auto launch: ${plan.automatic?'On':'Off'}`,()=>setSiloPlan(g,v,{...plan,automatic:!plan.automatic}));auto.setAttribute('aria-pressed',String(plan.automatic));auto.title=`Fire stock at visible enemies within ${def.silo!.auto_range} tiles when the manual queue is empty. Explosions also hit allies.`;inventory.append(auto);
  const targets=document.createElement('div');targets.className='missile-target-buttons';
  for(const type of def.production!.recipes){const b=button(`Target ${missileName(type)}`,event=>{g.mode={kind:'launch',type_key:type,silos:[v.id],batch:event.shiftKey};g.updateMode();document.getElementById('map')!.focus();});b.title='Click a target. Shift-click for five identical launches. Missing missiles are added to production.';targets.append(b);}root.append(targets);
  const launches=document.createElement('ol');launches.className='missile-launches';
  plan.launches.forEach((launch,index)=>{const li=document.createElement('li');const m=g.types.get(launch.type_key)!.missile!;li.append(unitIcon(launch.type_key,g.color(v.owner)),document.createTextNode(`${missileName(launch.type_key)} → ${launch.target.x},${launch.target.y} · ${flightTicks(v,launch.target,m)} ticks flight`));const remove=button('×',()=>{const next=structuredClone(plan);next.launches.splice(index,1);setSiloPlan(g,v,next);});remove.setAttribute('aria-label',`Remove launch ${index+1}`);li.append(remove);launches.append(li);});root.append(launches);
  const note=document.createElement('small');note.textContent=plan.launches.length?'Launches fire in order as stock becomes ready. Flight times start at launch.':plan.automatic?'Automatic targeting is active. Manual launches have unlimited range.':'Stockpile mode. Build missiles above, then target a launch when ready.';root.append(note);
  return root;
}
