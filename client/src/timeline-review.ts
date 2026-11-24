import type {CommittedCommand, Order, PresentationEvent, Tile, WorldEvent} from './contracts.generated';
import {type Game, idKey} from './game';
import {orderLabel} from './factory';
import {practice, practiceUrl} from './practice';

const words = (s: string) => s.replaceAll('_', ' ');
function describe(c: CommittedCommand['command']): string {
  if ('order' in c) return orderLabel(c.order);
  if (c.kind === 'place_blueprints') return `Plan ${c.tiles.length} × ${c.type_key}`;
  if (c.kind === 'edit_production') return `${words(c.edit.kind)}${'items' in c.edit ? ' · '+c.edit.items.join(', ') : ''}`;
  if (c.kind === 'set_priority') return `Set priority to ${c.priority}`;
  if (c.kind === 'set_queue_loop') return `Turn queue loop ${c.enabled ? 'on' : 'off'}`;
  if (c.kind === 'set_silo_plan') return 'Set missile launch plan';
  return words(c.kind);
}

function eventVisible(e: PresentationEvent, g: Game): boolean {
  if (g.fullVision) return true;
  const tiles = g.visibility();
  const visible = (t: Tile) => tiles.has(`${t.x},${t.y}`);
  switch (e.kind) {
    case 'attack': return visible(e.source_tile) && visible(e.target_tile);
    case 'move': return visible(e.from) && visible(e.to);
    case 'displacement': return visible(e.mover_from) && visible(e.mover_to) && visible(e.blocker_from) && visible(e.blocker_to);
    case 'destroyed': return visible(e.tile);
    case 'impact': return visible(e.target_tile);
    case 'missile_impact': return visible(e.target);
    case 'missile_launch': return visible(e.flight.origin);
    case 'survival': return e.transition.player_id === g.player;
  }
}

export class TimelineReview {
  private readonly enabled: HTMLInputElement;
  private readonly activity: HTMLElement;
  private readonly practiceBox: HTMLElement;
  private readonly practicePlayer: HTMLSelectElement;
  private cached = '';
  private requested = '';
  private eventKey = '';
  private events: WorldEvent[] = [];
  private failed = '';
  readonly recipients = new Map<string, Order|null>();
  readonly sites: Tile[] = [];

  constructor(readonly g: Game, root: HTMLElement) {
    const controls = document.createElement('section');
    controls.className = 'review-tools';
    controls.innerHTML = '<label><input type="checkbox" id="review-plans"> Show orders and events at playhead</label><div id="tick-activity" hidden></div><div class="review-jumps"><button type="button">Previous order tick</button><button type="button">Next order tick</button></div><div id="practice-options" hidden><label class="replay-field">Practice as <select id="practice-player"></select></label><button type="button" id="start-practice">Play this timeline</button><small>Try new orders against the recorded plans. Practice lasts until you leave or reload.</small></div>';
    root.querySelector('#return-live')!.after(controls);
    this.enabled = controls.querySelector('input')!;
    this.activity = controls.querySelector('#tick-activity')!;
    this.enabled.onchange = () => {this.cached=''; this.update();};
    controls.querySelectorAll<HTMLButtonElement>('.review-jumps button').forEach((b,i) => b.onclick = () => {
      const ticks = this.turns().filter(t => t.commands.length).map(t => t.tick).sort((a,b) => a-b);
      const tick = i ? ticks.find(t => t > Math.floor(g.playhead)) : ticks.reverse().find(t => t < Math.floor(g.playhead));
      if (tick === undefined) {g.toast(i?'No later order tick':'No earlier order tick');return;}
      this.enabled.checked = true;
      g.seek(tick);
    });
    this.practiceBox = controls.querySelector('#practice-options')!;
    this.practicePlayer = controls.querySelector('select')!;
    for (let p=0;p<g.config.player_count;p++) this.practicePlayer.add(new Option(g.name(p),String(p)));
    this.practicePlayer.value = String(g.player ?? 0);
    controls.querySelector<HTMLButtonElement>('#start-practice')!.onclick = () => {
      if (!this.practiceBox.hidden && g.net.matchId) location.assign(practiceUrl(g.net.matchId,g.current,Number(this.practicePlayer.value),g.playhead));
    };
  }

  private turns() {
    const g = this.g;
    return (g.experience?.turns.get(g.current) ?? []).filter(t => g.fullVision || t.player === g.player);
  }

  get active(): boolean { return this.enabled.checked; }

  update(): void {
    const g = this.g;
    const vacant = g.phase && !g.progress && g.profiles.some(s=>s.claimed && !s.connected);
    const hidden = !!practice || !(g.finished || vacant) || g.current < 0 || g.viewingPreview;
    if (this.practiceBox.hidden !== hidden) this.practiceBox.hidden = hidden;
    const show = this.enabled.checked && !g.viewingPreview && g.current >= 0;
    if (this.activity.hidden === show) this.activity.hidden = !show;
    if (!show) {this.recipients.clear();this.sites.length=0;this.cached='';return;}
    const tick = Math.floor(g.playhead), revision = g.current, from = Math.floor(tick/24)*24;
    const key = `${revision}:${from}`;
    if (key !== this.eventKey && !this.requested && key !== this.failed) {
      this.requested=key;
      void g.net.request({kind:'get_events',revision,from_tick:from,to_tick:Math.min(from+23,g.rev()!.outcome.terminal_state_tick),effects_only:false},'events',m=>m.revision===revision)
        .then(m=>{this.events=m.events;this.eventKey=key;this.failed='';})
        .catch(()=>{this.failed=key;})
        .finally(()=>{this.requested='';this.cached='';});
    }
    const ledger = g.experience.turns.get(revision);
    const r = g.experience.rounds.get(revision);
    const cache = `${key}:${tick}:${this.eventKey}:${ledger?.length}:${r?.command_outcomes.length}:${g.fullVision}:${g.exact?.tick}:${this.failed}`;
    if (cache === this.cached) return;
    this.cached=cache; this.recipients.clear(); this.sites.length=0;
    this.activity.replaceChildren();
    const heading=document.createElement('strong');heading.textContent=`Tick ${tick}`;this.activity.append(heading);
    heading.title='Gold outlines mark units and buildings that received orders at this tick.';
    const outcomes=new Map(r?.command_outcomes.map(o=>[JSON.stringify(o.command_id),o]));
    const current=this.turns().filter(t=>t.tick===tick);
    for (const turn of current) {
      for (const c of turn.commands) {
        const row=document.createElement('button');
        row.type='button';
        row.style.borderColor=g.color(turn.player);row.className='review-command';
        const applied=outcomes.get(JSON.stringify(c.id));
        row.textContent=`${g.name(turn.player)} · ${describe(c.command)}${applied?.skipped.length ? ' · '+applied.skipped.length+' skipped' : ''}`;
        const recipients=new Set(applied?.applied_entities.map(idKey) ?? []);
        const site=c.command.kind==='place_blueprints' ? c.command.tiles[0] : null;
        row.title='Focus the units, buildings or planned site for this command';
        row.disabled=!site && !recipients.size;
        row.onclick=()=>{
          const actors=g.entities().filter(v=>!v.remembered && recipients.has(idKey(v.id)));
          if (actors.length) g.selectViews(actors);
          const center=actors[0] ?? site;
          if (center) g.renderer.centerOn(center.x,center.y);
          else g.toast('No recipients are visible at this tick.');
        };
        this.activity.append(row);
        for (const id of applied?.applied_entities ?? []) {
          const key=idKey(id);
          if ('order' in c.command) this.recipients.set(key,c.command.order);
          else if (!this.recipients.has(key)) this.recipients.set(key,null);
        }
        if(c.command.kind==='place_blueprints')this.sites.push(...c.command.tiles);
      }
    }
    const counts=new Map<string,number>();
    if(this.eventKey===key) for(const {event,tick:t} of this.events) if(t===tick && eventVisible(event,g)) counts.set(event.kind,(counts.get(event.kind)??0)+1);
    const effects=document.createElement('small');
    effects.textContent=this.failed===key?'Events unavailable':this.eventKey!==key?'Loading events…':[...counts].map(([kind,n])=>`${n} ${words(kind)}`).join(' · ') || 'No visible events';
    this.activity.append(effects);
  }
}
