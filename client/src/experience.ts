import type { AcceptedTurn, Command, ControlGroupState, EntityId, PlayerStats, ServerMessage, StatsSample } from './contracts.generated';
import { Game, idKey, type RevisionView } from './game';

type Round = Extract<ServerMessage, { kind: 'round_result' }>;
const $ = (id: string) => document.getElementById(id)!;
const button = (label: string, action: () => void) => {
  const b = document.createElement('button'); b.textContent = label; b.onclick = action; return b;
};
const metrics = ['bank', 'mined', 'spend', 'army', 'infrastructure', 'attrition', 'thinking'] as const;

/** Inspection and draft projections; all outcomes come from published server results. */
export class Experience {
  rounds = new Map<number, Round>();
  turns = new Map<number, AcceptedTurn[]>();
  stats = new Map<number, StatsSample[]>();
  destructions = new Map<number, number>();
  loading = new Map<number, Promise<void>>();
  focusDelete: (() => void) | null = null;
  metric: typeof metrics[number] = 'bank';
  graphHover: number | null = null;
  groupOwner: number;
  constructor(readonly g: Game) {
    this.groupOwner = g.player ?? 0;
    const toolbar = document.createElement('div'); toolbar.id = 'actions';
    const categories: [string,string,[string,string][]][] = [
      ['Orders','orders',[['F Attack','f'],['G Support','g'],['M Mine','m'],['C Construct','c'],['X Idle','x']]],
      ['Build & produce','production',[['B Build structure','b'],['Q Build units','q'],['P Priority','p'],['L Loop','l'],['R Rotate factory','r']]],
      ['Groups','groups',[['H Members','h'],['⇧H Add','H'],['J Output group','j']]],
      ['Planning','planning',[['O Future policy','o'],['Undo · Ctrl Z','Control+z'],['Redo · Ctrl U','Control+u'],['V Statistics','v'],['? Help','?']]],
    ];
    for (const [title,category,keys] of categories) {
      const column=document.createElement('div');column.className=`action-group ${category}`;
      const label=document.createElement('strong');label.textContent=title;column.append(label);
      for(const [label,key] of keys){const b=button(label,()=>{
        if(key==='Control+z')g.undo();else if(key==='Control+u')g.redo();else g.key(new KeyboardEvent('keydown',{key,shiftKey:key==='H'}));
      });b.dataset.key=key;column.append(b);}
      toolbar.append(column);
    }
    const advanced=document.createElement('details');advanced.innerHTML='<summary>More planning tools</summary>';const planning=document.createElement('div');planning.className='action-group';advanced.append(planning);$('draft-panel').append(advanced);
    planning.append(button('Rebase draft here', () => {
      if (g.current !== g.latest || g.playhead < g.editableFrom) { g.toast('Choose an editable live tick.'); return; }
      g.draft.undo.push({commands:[...g.draft.commands],tick:g.draft.tick}); g.draft.redo = [];
      g.draft.tick = g.draft.commands.length ? Math.floor(g.playhead) : null; g.updatePanels();
    }));
    planning.append(button('Clear draft', () => g.clearDraft()));
    planning.append(button('Delete selected', () => g.key(new KeyboardEvent('keydown', { key: 'Delete' }))));
    const guide = document.createElement('a'); guide.href = '/guide/'; guide.target = '_blank'; guide.textContent = 'Guide'; planning.append(guide);
    const options=document.createElement('div');options.id='mode-options';$('timeline-wrap').append(options);
    planning.append(button('Clear output binding', () => this.bind(null)),button('Esc Cancel', () => g.key(new KeyboardEvent('keydown',{key:'Escape'}))));
    $('timeline-wrap').append(toolbar);
    const groups = document.createElement('details'); groups.id = 'groups'; groups.innerHTML = '<summary>Groups 0–9</summary><div id="group-list"></div>';
    if (g.spectator) { const owner=document.createElement('select');owner.setAttribute('aria-label','Inspect player groups');for(let p=0;p<g.config.player_count;p++)owner.add(new Option(g.name(p),String(p)));owner.onchange=()=>{this.groupOwner=Number(owner.value);g.selection.clear();g.recalledGroup=null;g.updatePanels();};groups.append(owner); }
    $('selection').append(groups);
    const recipient = document.createElement('div'); recipient.id = 'recipient'; $('selection').prepend(recipient);
    const queue = document.createElement('div'); queue.id = 'queue'; $('selection').append(queue);
    const preview = document.createElement('div'); preview.id = 'lock-preview'; $('draft-panel').append(preview);
    const replay = document.createElement('details'); replay.id = 'replay';
    replay.innerHTML = '<summary>Rounds, inputs and rewrite results</summary><label>Round <select id="round-picker"></select></label><button id="return-live">Return to live</button><div id="round-summary"></div><div id="rewrite-summary"></div><div id="accepted-inputs"></div>';
    $('game').append(replay);
    $('return-live').onclick = () => void this.selectRound(g.latest);
    ($('round-picker') as HTMLSelectElement).onchange = e => void this.selectRound(Number((e.target as HTMLSelectElement).value));
    const rates = document.createElement('select'); rates.id = 'speed'; rates.setAttribute('aria-label', 'Playback speed');
    for (const n of [.25,.5,1,2,4,8]) rates.add(new Option(`${n}×`,String(n)));
    rates.value = '1'; rates.onchange = () => { g.rate = Number(rates.value); g.updatePanels(); }; $('transport').append(rates);
    $('transport').append(button('− Zoom', () => g.zoomTimeline(1.25)), button('+ Zoom', () => g.zoomTimeline(.8)), button('Pan ◀', () => g.panTimeline(-(g.view.t1-g.view.t0)/4)), button('Pan ▶', () => g.panTimeline((g.view.t1-g.view.t0)/4)));
    const overlay = document.createElement('div'); overlay.id = 'statistics'; overlay.className = 'overlay';
    overlay.innerHTML = '<button id="close-stats">Close statistics (V)</button><h2>Statistics</h2><label>Metric <select id="metric"></select></label> <label>Player <select id="graph-player"></select></label> <label>Opponent <select id="graph-opponent"></select></label> <label>Window <select id="graph-window"><option value="full">Full run</option><option value="visible">Visible timeline</option></select></label><p id="graph-caption"></p><canvas id="graph" width="900" height="330"></canvas><p id="graph-hover"></p><p>Thinking time is cumulative committed time by round, separate from the live planning timer. Attrition is this player’s spend divided by the selected opponent’s spend; no spend has no defined ratio.</p>';
    $('game').append(overlay);
    const metric = $('metric') as HTMLSelectElement;
    for (const m of metrics) metric.add(new Option(m === 'thinking' ? 'Thinking-time ratio' : m,m));
    for (const id of ['graph-player','graph-opponent']) {
      const select = $(id) as HTMLSelectElement;
      for (let p=0;p<g.config.player_count;p++) select.add(new Option(g.name(p),String(p)));
      select.value = String(id === 'graph-player' ? g.player ?? 0 : (g.player ?? 0) === 0 ? 1 : 0);
    }
    for (const id of ['metric','graph-player','graph-opponent','graph-window']) $(id).onchange = () => { this.metric = metric.value as typeof this.metric; this.drawGraph(); };
    $('close-stats').onclick = () => overlay.classList.remove('active');
    const canvas = $('graph') as HTMLCanvasElement;
    canvas.onmousemove = e => { this.graphHover = (e.clientX-canvas.getBoundingClientRect().left)/canvas.clientWidth; this.drawGraph(); };
    canvas.onmouseleave = () => { this.graphHover = null; };
    $('game').append(button('Stop and archive unfinished', () => {
      if (g.session.token && g.phase) g.net.send({ kind: 'stop_and_archive', request_id: `stop-${Date.now()}`, based_on_revision: g.latest, slot_token: g.session.token });
    }));
    const stop = $('game').lastElementChild as HTMLElement; stop.id = 'stop-archive';
    window.addEventListener('focusin', e => {
      if (!(e.target as HTMLElement).closest('#queue')) this.focusDelete = null;
      if ((e.target as HTMLElement).matches('input,select,textarea,[contenteditable]')) g.renderer.keys = { w:false,a:false,s:false,d:false };
    });
    window.addEventListener('blur', () => { g.renderer.keys = { w:false,a:false,s:false,d:false }; g.renderer.dragPan = null; });
  }
  loadRound(revision: number): Promise<void> {
    if (this.loading.has(revision)) return this.loading.get(revision)!;
    if (this.rounds.has(revision)) return Promise.resolve();
    const task = (async () => {
      try {
        const r = await this.g.net.request({ kind:'get_round', revision }, 'round_result', m => m.revision === revision);
        this.rounds.set(revision,r);
        const end = r.outcome.terminal_state_tick;
        const commands = await this.g.net.request({ kind:'get_commands', revision, from_tick:0, to_tick:Math.max(end,this.g.config.max_tick) }, 'commands', m => m.revision === revision);
        this.turns.set(revision,commands.turns);
        const stats = await this.g.net.request({ kind:'get_stats', revision, from_tick:0, to_tick:end, bucket_width:Math.max(this.g.config.snapshot_interval, Math.ceil(end / 1000 / this.g.config.snapshot_interval)*this.g.config.snapshot_interval) }, 'stats_range', m => m.revision === revision);
        this.stats.set(revision,stats.buckets);
        const events = await this.g.net.request({kind:'get_events',revision,from_tick:0,to_tick:end},'events',m => m.revision === revision);
        this.destructions.set(revision,events.events.filter(e => e.event.kind === 'destroyed').length);
        if (!this.g.revisions.has(revision)) this.g.revisions.set(revision, { revision, outcome:r.outcome, timeline:r.timeline_index, score:r.score ?? null, dictionary:[], samples:new Map(), events:[], chunks:new Set(), loading:new Set() });
        if (r.parent_revision !== null && r.parent_revision !== undefined) await this.loadRound(r.parent_revision);
        this.g.updatePanels(); this.drawGraph();
      } catch (e) { this.g.toast(`Replay data: ${(e as Error).message}`); }
      finally { this.loading.delete(revision); }
    })();
    this.loading.set(revision,task); return task;
  }
  async selectRound(revision: number): Promise<void> {
    await this.loadRound(revision);
    if (!this.g.revisions.has(revision)) return;
    const old = this.g.rev();
    if (old && old.revision !== revision) { old.samples.clear();old.events=[];old.chunks.clear(); }
    this.g.playing = false; this.g.current = revision; this.g.exact = null;
    this.g.selection.clear(); this.g.recalledGroup = null;
    this.g.view = { t0:0,t1:Math.max(1,this.g.rev()!.outcome.terminal_state_tick) };
    this.g.seek(Math.min(this.g.playhead,this.g.view.t1));
  }
  groups(): ControlGroupState[] {
    const g = this.g;
    const groups = structuredClone(g.exact?.revision === g.current && g.exact.tick === Math.floor(g.playhead) ? g.exact.state.control_groups : []);
    if (g.draft.tick !== Math.floor(g.playhead) || g.current !== g.latest) return groups;
    for (const d of g.draft.commands) {
      const c = d.command;
      if (c.kind !== 'edit_group_members' && c.kind !== 'assign_group_order') continue;
      let group = groups.find(v => v.id.owner === c.group.owner && v.id.slot === c.group.slot);
      if (!group) { group = { id:c.group, members:[], latest_order:null, order_locks:[] }; groups.push(group); }
      if (c.kind === 'edit_group_members') {
        if (c.edit.kind === 'replace') group.members = c.edit.entities;
        if (c.edit.kind === 'add') for (const e of c.edit.entities) { if (!group.members.some(m => idKey(e) === idKey(m))) group.members.push(e); }
        if (c.edit.kind === 'remove') group.members = group.members.filter(e => !c.edit.entities.some(m => idKey(e) === idKey(m)));
      } else group.latest_order = { order:c.order, tick:g.draft.tick!, source_command_id:{ round:g.round,player:g.player!,index:0 } };
    }
    return groups;
  }
  bind(slot: number | null): void {
    const factories = this.g.selectedIds(t => !!t.production);
    if (factories.length) this.g.stage({ kind:'bind_factory_group', factories, group:slot === null ? null : { owner:this.g.player!,slot } });
    else this.g.toast('Select a factory first.');
    this.g.mode = {kind:'none'}; this.g.updateMode();
  }
  deleteFocused(): boolean {
    if (this.focusDelete) { this.focusDelete(); return true; }
    if (this.g.draft.selected !== null) return false;
    const ids = this.g.selectedViews().filter(e => e.lifecycle === 'site' && e.owner === this.g.player).map(e => e.exact?.blueprint_id).filter((id): id is EntityId => !!id);
    if (!ids.length) return false;
    this.g.stage({ kind:'cancel_blueprints', blueprint_ids:ids.map(id => ({kind:'persistent',id})) }); return true;
  }
  update(): void {
    const g = this.g;
    this.actions();
    $('stop-archive').hidden = g.spectator || !!g.finished || g.current !== g.latest;
    $('recipient').textContent = g.recalledGroup === null ? 'Selected units · factories set newborn orders' : `Group ${g.recalledGroup}: current members + future spawns`;
    ($('speed') as HTMLSelectElement).value = String(g.rate);
    const list = $('group-list'); list.replaceChildren();
    const groups = this.groups();
    for (let slot=0;slot<10;slot++) {
      const group = groups.find(v => v.id.owner === this.groupOwner && v.id.slot === slot);
      const count = group?.members.filter(m => g.entities().some(e => idKey(e.id) === idKey(m))).length ?? 0;
      const bound = g.exact?.state.entities.filter(e => {
        let binding = e.production?.spawn_group;
        if (g.draft.tick === Math.floor(g.playhead) && g.current === g.latest) for (const d of g.draft.commands) if (d.command.kind === 'bind_factory_group' && d.command.factories.some(f => idKey(f) === idKey(e.id))) binding = d.command.group;
        return binding?.owner === this.groupOwner && binding.slot === slot;
      }).length ?? 0;
      list.append(button(`${slot}: ${count} living · ${bound} factories · ${group?.latest_order ? `${group.latest_order.order.kind} @${group.latest_order.tick}` : 'no saved order'}`, () => g.digit(slot)));
    }
    this.queue();
    $('lock-preview').textContent = this.lockPreview();
    const picker = $('round-picker') as HTMLSelectElement;
    const options = [...this.rounds.values()].sort((a,b) => a.revision-b.revision);
    if (picker.options.length !== options.length) { picker.replaceChildren(); for (const r of options) picker.add(new Option(r.round === 0 ? 'Opening' : `Round ${r.round} · revision ${r.revision}`,String(r.revision))); }
    picker.value = String(g.current);
    const r = this.rounds.get(g.current);
    if (!r) return;
    $('round-summary').textContent = `Round ${r.round} · simulation ${r.sim_duration_ms} ms · committed thinking ${r.time_totals.map(t => `${g.name(t.player_id)} ${(t.total_ms/1000).toFixed(1)}s`).join(', ')}${r.timed ? ` · finalized losses: ${r.timed.timed_lost_players.join(', ') || 'none'}` : ''}`;
    const inputs = $('accepted-inputs'); inputs.replaceChildren();
    for (const turn of this.turns.get(g.current) ?? []) {
      const row = document.createElement('div');
      row.append(button(`R${turn.round} ${g.name(turn.player)} @${turn.tick} (${turn.duration_ms} ms)`, () => g.seek(turn.tick)));
      for (const c of turn.commands) {
        const outcome = r.command_outcomes.find(o => JSON.stringify(o.command_id) === JSON.stringify(c.id));
        const label = document.createElement('div');
        label.textContent = `${c.command.kind} · ${outcome ? `${outcome.applied_entities.length} applied; ${outcome.skipped.map(s => s.reason).join(', ') || 'no skips'}` : 'no outcome recorded in this result'} · ${c.future_orders}`;
        row.append(label);
      }
      if (!turn.commands.length) row.append('Pass');
      inputs.append(row);
    }
    const before = r.parent_revision == null ? null : this.rounds.get(r.parent_revision);
    const previous = before ? this.stats.get(before.revision)?.at(-1) : null;
    const after = this.stats.get(g.current)?.at(-1);
    $('rewrite-summary').textContent = before ? `Before → after: ${before.outcome.kind} → ${r.outcome.kind}; survivors [${before.outcome.survivors}] → [${r.outcome.survivors}]. ${after?.players.map(p => { const old = previous?.players.find(o => o.player_id === p.player_id); return `${g.name(p.player_id)} spend ${old ? this.spend(old).toFixed(0) : '?'} → ${this.spend(p).toFixed(0)}, living ${old?.entity_count ?? '?'} → ${p.entity_count}`; }).join(' · ') ?? ''}` : 'Opening simulation';
    if (before && after && previous) {
      const count = (samples: StatsSample[]) => samples.at(-1)!.players.reduce((n,p) => n+p.entity_count,0)-samples[0].players.reduce((n,p) => n+p.entity_count,0);
      const oldDeaths = this.destructions.get(before.revision) ?? 0, deaths = this.destructions.get(r.revision) ?? 0;
      $('rewrite-summary').append(` · Entity births (including sites) ${count(this.stats.get(before.revision)!)+oldDeaths} → ${count(this.stats.get(r.revision)!)+deaths}; destructions/cancellations ${oldDeaths} → ${deaths}.`);
    }
    const transitions = r.outcome.survival_transitions;
    const survival = document.createElement('div');
    survival.textContent = `Survival requires active building AND constructor/factory. ${transitions.map(t => `${g.name(t.player_id)} ${t.status} @${t.resolved_tick}${t.reasons.length ? ` (${t.reasons.join(', ')})` : ' (recovered)'}`).join(' · ')}`;
    inputs.prepend(survival);
    const status = document.createElement('div');
    const entities = g.entities().filter(e => e.lifecycle === 'complete');
    status.textContent = Array.from({length:g.config.player_count},(_,p) => `${g.name(p)}: building ${entities.some(e=>e.owner===p && g.types.get(e.type_key)?.counts_for_survival)?'✓':'missing'}, constructor/factory ${entities.some(e=>e.owner===p && g.types.get(e.type_key)?.provides_build_ability)?'✓':'missing'}`).join(' · ');
    inputs.prepend(status);
  }
  actions(): void {
    const g=this.g, views=g.selectedViews().filter(v=>g.ownSelectable(v));
    const has=(cap:'movement'|'mining'|'construction'|'production')=>views.some(v=>!!g.types.get(v.type_key)?.[cap]);
    const factory=has('production');
    const allowed:Record<string,boolean>={f:has('movement')||factory||g.recalledGroup!==null,g:has('movement')||factory||g.recalledGroup!==null,m:has('mining')||factory||g.recalledGroup!==null,c:has('construction')||factory||g.recalledGroup!==null,x:views.length>0,b:has('construction'),q:factory,p:views.length>0,l:factory,r:g.mode.kind==='place'&&!!g.types.get(g.mode.type_key)?.production,h:views.some(v=>!v.blueprint),H:views.some(v=>!v.blueprint),j:views.some(v=>!v.blueprint&&!!g.types.get(v.type_key)?.production)};
    for(const b of Array.from(document.querySelectorAll<HTMLButtonElement>('#actions [data-key]'))) b.hidden=allowed[b.dataset.key!]===false || (g.spectator && !['v','?'].includes(b.dataset.key!));
    for(const col of Array.from(document.querySelectorAll<HTMLElement>('.action-group'))) col.hidden=!Array.from(col.querySelectorAll('button')).some(b=>!b.hidden);
    const options=$('mode-options');options.replaceChildren();
    const choices=g.mode.kind==='build'?g.structures():g.mode.kind==='recipe'?g.producible():g.mode.kind==='priority'?['High','Medium','Low']:g.mode.kind==='membership'||g.mode.kind==='binding'?Array.from({length:10},(_,n)=>String(n)):[];
    choices.forEach((label,index)=>{const digit=g.mode.kind==='membership'||g.mode.kind==='binding'?index:index+1;const cost=g.types.get(label)?.matter_cost;options.append(button(`${digit} · ${label}${cost===undefined?'':` · ${cost} matter`}`,()=>g.digit(digit)));});
  }
  queue(): void {
    const g = this.g, root = $('queue');
    // Preserve focused queue buttons through redraws caused by exact-state arrival.
    if (root.contains(document.activeElement)) return;
    root.replaceChildren(); this.focusDelete = null;
    const add = (label: string, remove: () => void) => {
      const b = button(label, () => { this.focusDelete = remove; b.focus(); });
      b.onfocus = () => { this.focusDelete = remove; g.draft.selected = null; };
      b.onkeydown = e => {
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); const all = Array.from(root.querySelectorAll('button')); all[(all.indexOf(b)+(e.key === 'ArrowDown'?1:all.length-1))%all.length]?.focus(); }
      }; root.append(b);
    };
    for (const v of g.selectedViews()) {
      if(v.blueprint && v.owner===g.player){const info=document.createElement('p');info.textContent=`Planned ${v.type_key} · ${v.settings?.priority} · queue: ${v.settings?.queue.join(', ')||'empty'} · newborn ${v.settings?.order.kind} · starts on completion`;root.append(info);root.append(button('Clear planned queue',()=>g.configureGhosts(s=>{s.queue=[];})));continue;}
      const p = structuredClone(v.exact?.production);
      if (!p || v.owner !== g.player) continue;
      if(g.draft.tick === Math.floor(g.playhead) && g.current === g.latest) for(const d of g.draft.commands) {const c=d.command;if('factories' in c && c.factories.some(id=>idKey(id)===idKey(v.id))) {if(c.kind==='bind_factory_group')p.spawn_group=c.group;if(c.kind==='set_stored_order')p.stored_order=c.order;}}
      const info = document.createElement('p');
      const group = this.groups().find(x => x.id.owner === p.spawn_group?.owner && x.id.slot === p.spawn_group?.slot);
      info.textContent = `Output group ${p.spawn_group?.slot ?? 'none'} · newborn ${group?.latest_order?.order.kind ?? p.stored_order.kind} · ${p.active_item?.awaiting_output ? 'blocked' : 'ready'}`; root.append(info);
      root.append(button('Cancel active (Delete on active)', () => g.stage({kind:'edit_production',factories:[v.id],edit:{kind:'cancel_active'}})));
      if (p.active_item) add(`Active: ${p.active_item.type_key}`, () => g.stage({kind:'edit_production',factories:[v.id],edit:{kind:'cancel_active'}}));
      for (const item of p.pending_items) add(`Pending: ${item.type_key}`, () => g.stage({kind:'edit_production',factories:[v.id],edit:{kind:'remove_pending',item_ids:[{kind:'persistent',id:item.item_id}]}}));
      root.append(button('Replace pending (empty)', () => g.stage({kind:'edit_production',factories:[v.id],edit:{kind:'replace_pending',items:[]}})));
    }
    for (const b of g.exact?.state.blueprints ?? []) if (b.owner === g.player) add(`Blueprint ${b.type_key} @${b.tile.x},${b.tile.y} (Delete)`, () => g.stage({kind:'cancel_blueprints',blueprint_ids:[{kind:'persistent',id:b.id}]}));
  }
  lockPreview(): string {
    const g = this.g, turns = this.turns.get(g.current);
    if (!turns) return 'Lock preview: loading scheduled commands…';
    let members = 0, slots = 0;
    const groups = structuredClone(g.exact?.state.control_groups ?? []);
    const key = (ids: EntityId[]) => new Set(ids.map(idKey));
    for (const draft of g.draft.commands) {
      const c = draft.command;
      if (c.kind === 'edit_group_members') {
        let group = groups.find(x => x.id.owner === c.group.owner && x.id.slot === c.group.slot);
        if (!group) { group = {id:c.group,members:[],order_locks:[]}; groups.push(group); }
        group.members = c.edit.kind === 'replace' ? c.edit.entities : c.edit.kind === 'add' ? [...group.members,...c.edit.entities] : group.members.filter(e => !key(c.edit.entities).has(idKey(e)));
      }
      if (draft.future_orders === 'keep' || (c.kind !== 'assign_order' && c.kind !== 'assign_group_order')) continue;
      const ids = c.kind === 'assign_order' ? c.entities : groups.find(x => x.id.owner === c.group.owner && x.id.slot === c.group.slot)?.members ?? [];
      const affected = key(ids.filter(id => g.exact?.state.entities.some(e => idKey(e.id) === idKey(id))));
      const end = draft.future_orders === 'drop_all' ? Infinity : g.draft.tick! + (g.config.future_orders.window_ticks ?? 0);
      for (const turn of turns) if (turn.round < g.round && turn.tick > g.draft.tick! && turn.tick <= end) for (const old of turn.commands) {
        const cmd = old.command;
        if (cmd.kind === 'assign_group_order') {
          if (c.kind === 'assign_group_order' && c.group.owner === cmd.group.owner && c.group.slot === cmd.group.slot) slots++;
          members += groups.find(x => x.id.owner === cmd.group.owner && x.id.slot === cmd.group.slot)?.members.filter(e => affected.has(idKey(e))).length ?? 0;
        } else if ('entities' in cmd) members += cmd.entities.filter(e => affected.has(idKey(e))).length;
        else if ('factories' in cmd) members += cmd.factories.filter(e => affected.has(idKey(e))).length;
      }
    }
    return `Non-authoritative lock preview: ${members} member deliveries; ${slots} group saved-order writes may skip. Following interval excludes draft tick${g.config.future_orders.window_ticks ? `; window ends at ${(g.draft.tick ?? Math.floor(g.playhead))+g.config.future_orders.window_ticks}` : ''}. Simultaneous replay, future births/membership and failed assignments can change this estimate.`;
  }
  toggleStats(): void { $('statistics').classList.toggle('active'); void this.loadRound(this.g.current).then(() => this.drawGraph()); }
  spend(p: PlayerStats): number { return p.counters.total_spend; }
  drawGraph(): void {
    if (!$('statistics').classList.contains('active')) return;
    const g = this.g, canvas = $('graph') as HTMLCanvasElement, ctx = canvas.getContext('2d')!;
    const player = Number(($('graph-player') as HTMLSelectElement).value), opponent = Number(($('graph-opponent') as HTMLSelectElement).value);
    const visible = ($('graph-window') as HTMLSelectElement).value === 'visible';
    const samples = this.stats.get(g.current) ?? [];
    const from = visible ? g.view.t0 : 0, to = Math.max(from+1,visible ? g.view.t1 : g.rev()?.outcome.terminal_state_tick ?? 1);
    const points = samples.filter(s => s.tick >= from && s.tick <= to);
    const value = (p: PlayerStats, other?: PlayerStats): number | null => {
      switch (this.metric) {
        case 'bank': return p.bank;
        case 'mined': return p.counters.mined;
        case 'spend': return this.spend(p);
        case 'army': return p.living_army_value;
        case 'infrastructure': return p.living_infrastructure_value;
        case 'attrition': return other && this.spend(other) > 0 ? this.spend(p)/this.spend(other) : null;
        default: return null;
      }
    };
    const series = [player,opponent].map(owner => ({ owner, points: this.metric === 'thinking' ? [...this.rounds.values()].filter(r => r.revision <= g.current).sort((a,b) => a.round-b.round).map(r => { const own = r.time_totals.find(t => t.player_id === owner)?.total_ms ?? 0; const other = r.time_totals.find(t => t.player_id === (owner === player ? opponent : player))?.total_ms ?? 0; return {x:r.round,y:other > 0 ? own/other : null}; }) : points.map(s => ({ x:s.tick, y: s.players.find(p => p.player_id === owner) ? value(s.players.find(p => p.player_id === owner)!,s.players.find(p => p.player_id === (owner === player ? opponent : player))) : null })) }));
    const x0 = this.metric === 'thinking' ? 0 : from, x1 = this.metric === 'thinking' ? Math.max(1,this.rounds.get(g.current)?.round ?? 1) : to;
    const max = Math.max(1,...series.flatMap(s => s.points.map(p => p.y ?? 0)));
    ctx.fillStyle = '#16161b'; ctx.fillRect(0,0,900,330); ctx.font = '13px system-ui';
    for (let i=0;i<=4;i++) { const y = 290-i*65; ctx.strokeStyle = '#393944'; ctx.beginPath();ctx.moveTo(65,y);ctx.lineTo(870,y);ctx.stroke();ctx.fillStyle = '#bbb';ctx.fillText((max*i/4).toFixed(1),5,y); }
    const px = (x:number) => 65+(x-x0)/(x1-x0)*805;
    for (const s of series) { ctx.strokeStyle = g.color(s.owner);ctx.lineWidth = 2;ctx.beginPath();let active = false;for (const p of s.points) { if(p.y === null) {active=false;continue;} const y=290-p.y/max*260;if(active)ctx.lineTo(px(p.x),y);else ctx.moveTo(px(p.x),y);active=true;}ctx.stroke(); if (this.metric === 'thinking') for(const p of s.points) if(p.y !== null){ctx.fillStyle=g.color(s.owner);ctx.beginPath();ctx.arc(px(p.x),290-p.y/max*260,4,0,Math.PI*2);ctx.fill();} }
    ctx.fillStyle='#ddd';ctx.fillText(`${this.metric === 'thinking' ? 'Round' : 'Tick'} ${Math.floor(x0)}`,65,318);ctx.fillText(String(Math.floor(x1)),820,318);
    $('graph-caption').textContent = `${this.metric} · ${g.name(player)} (${g.color(player)}) / ${g.name(opponent)} (${g.color(opponent)}) · ${this.metric === 'thinking' ? series[0].points.length+' rounds' : points.length+' sampled points'}`;
    if (this.graphHover !== null) { const x=x0+Math.max(0,Math.min(1,(this.graphHover*900-65)/805))*(x1-x0);ctx.strokeStyle='#fff';ctx.beginPath();ctx.moveTo(px(x),25);ctx.lineTo(px(x),290);ctx.stroke();$('graph-hover').textContent = `${this.metric === 'thinking' ? 'Round' : 'Tick'} ${Math.round(x)}: ${series.map(s => { const p=s.points.reduce<{x:number;y:number|null}|null>((best,p) => !best || Math.abs(p.x-x)<Math.abs(best.x-x)?p:best,null);return `${g.name(s.owner)} ${p?.y?.toFixed(2) ?? 'undefined (zero denominator)'}`; }).join(' · ')}`; }
  }
}
