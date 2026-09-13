import { unitIcon } from './icons';
import { factoryPlan, orderLabel } from './factory';
import type {
  BlueprintSettings, DraftItemRef, Command, Content, DraftCommand, EntityId, EntityRef, FutureOrderPolicy, LobbyState, MatchConfig, Order, Outcome, Priority, RoundScore, Sample, ServerMessage, Tile, TimelineBucket,
  TypeDefinition, WorldEvent, WorldState,
} from './contracts.generated';
import type { Net } from './net';
import type { Session } from './lobby';
import { Renderer } from './render';
import { Experience } from './experience';

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;
const CHUNK = 1000;
const RATES = [0.25, 0.5, 1, 2, 4, 8, 16];
export const idKey = (id: EntityId): string => JSON.stringify(id);

export interface RevisionView {
  revision: number;
  outcome: Outcome;
  timeline: TimelineBucket[];
  score: RoundScore | null;
  dictionary: EntityRef[];
  samples: Map<number, Sample>;
  events: WorldEvent[];
  chunks: Set<number>;
  loading: Set<number>;
}

export interface EntityView {
  index: number;
  id: EntityId;
  owner: number;
  type_key: string;
  x: number;
  y: number;
  hp: number;
  maxHp: number;
  facing: string;
  lifecycle: string;
  activity: string;
  engaged: number | null;
  blueprint?: DraftItemRef;
  settings?: BlueprintSettings;
  exact?: WorldState['entities'][number];
}

type Mode =
  | { kind: 'none' }
  | { kind: 'attack' }
  | { kind: 'support' }
  | { kind: 'area'; order: 'mine' | 'construct' }
  | { kind: 'build' }
  | { kind: 'place'; type_key: string }
  | { kind: 'recipe' }
  | { kind: 'priority' }
  | { kind: 'stored' }
  | { kind: 'stored-target'; order: 'attack' }
  | { kind: 'membership'; add: boolean }
  | { kind: 'binding' };

interface Draft {
  tick: number | null;
  commands: DraftCommand[];
  undo: {commands: DraftCommand[]; tick: number | null}[];
  redo: {commands: DraftCommand[]; tick: number | null}[];
  policy: FutureOrderPolicy;
  selected: number | null;
}

export class Game {
  readonly renderer: Renderer;
  experience!: Experience;
  stored = false;
  latest = -1;
  timelineCursor = 0;
  content!: Content;
  types = new Map<string, TypeDefinition>();
  revisions = new Map<number, RevisionView>();
  current = -1;
  replayErrors = new Map<number, string>();
  terrain: WorldState['terrain'] | null = null;
  initialOre: number[] = [];
  phase: ServerMessage & { kind: 'planning_opened' } | null = null;
  committed: number[] = [];
  round = 0;
  editableFrom = 0;
  availableThrough = 0;
  progress: { tick: number; end: number } | null = null;
  playhead = 0;
  playing = false;
  rate = 1;
  exact: { revision: number; tick: number; state: WorldState } | null = null;
  exactPending: { revision: number; tick: number } | null = null;
  selection = new Set<string>();
  recalledGroup: number | null = null;
  mode: Mode = { kind: 'none' };
  draft: Draft = { tick: null, commands: [], undo: [], redo: [], policy: 'keep', selected: null };
  drag: { x0: number; y0: number; x1: number; y1: number } | null = null;
  hover: Tile | null = null;
  profiles: LobbyState['slots'] = [];
  view = { t0: 0, t1: 1 };
  finished: string | null = null;
  toastTimer = 0;
  lastFrame = performance.now();
  lastTop = 0;
  planningSince = performance.now();

  constructor(readonly net: Net, readonly config: MatchConfig, readonly session: Session, lobby: LobbyState) {
    this.profiles = lobby.slots;
    this.renderer = new Renderer($('map'), $('minimap'), $('timeline'), this);
    $('game').classList.add('active');
    $('lobby').style.display = 'none';
  }

  get player(): number | null {
    return this.session.slot;
  }
  get spectator(): boolean {
    return this.session.slot === null;
  }
  color(owner: number): string {
    const team = this.profiles[owner]?.profile?.team_id;
    if (team) { const first = this.profiles.find(s => s.profile?.team_id === team); if (first?.profile) return first.profile.color; }
    return this.profiles[owner]?.profile?.color ?? ['#4fc3f7', '#ff8a65', '#aed581', '#ce93d8'][owner % 4];
  }
  name(owner: number): string {
    return this.profiles[owner]?.profile?.username ?? `Player ${owner}`;
  }

  async start(): Promise<void> {
    this.content = await (await fetch('/guide/content.json')).json();
    for (const t of this.content.types) this.types.set(t.key, t);
    this.net.on('revision_published', m => { void this.onPublished(m).catch(error => this.failReplay(m.revision, error)); });
    this.net.on('planning_opened', m => {
      if (this.phase?.round !== m.round) this.planningSince = performance.now();
      this.phase = m;
      this.round = m.round;
      this.editableFrom = m.editable_from;
      this.availableThrough = m.available_through;
      this.committed = m.committed_players;
      this.progress = null;
      this.updatePanels();
      if (this.player !== null && this.exact) this.net.send({ kind: 'planning_ready', round: m.round, revision: m.revision });
    });
    this.net.on('simulation_progress', m => {
      this.progress = { tick: m.tick, end: m.end_tick };
      this.updateTop();
    });
    this.net.on('match_finished', m => {
      this.finished = m.match_winners.length ? `Match winners: ${m.match_winners.map(s => (s.kind === 'player' ? this.name(s.player_id) : s.team_id)).join(', ')}` : `Match over: ${m.reason}`;
      this.updatePanels();
    });
    this.net.on('lobby_updated', m => {
      this.profiles = m.lobby.slots;
    });
    this.net.on('welcome', m => {
      this.profiles = m.lobby.slots;
    });
    this.net.on('match_archived', () => {
      this.finished = 'Match stopped and archived (unfinished).';
      this.updatePanels();
    });
    this.experience = new Experience(this);
    this.bindInput();
    this.buildHelp();
    requestAnimationFrame(t => this.frame(t));
  }

  // ---- revisions and data ------------------------------------------------------------------

  async onPublished(m: ServerMessage & { kind: 'revision_published' }): Promise<void> {
    if (this.revisions.has(m.revision) && this.terrain) { this.latest = Math.max(this.latest,m.revision); return; }
    const wasFull = this.view.t0 === 0 && this.view.t1 === this.rev()?.outcome.terminal_state_tick;
    const view: RevisionView = {
      revision: m.revision,
      outcome: m.outcome,
      timeline: m.timeline_index,
      score: m.score ?? null,
      dictionary: [],
      samples: new Map(),
      events: [],
      chunks: new Set(),
      loading: new Set(),
    };
    this.revisions.set(m.revision, view);
    this.latest = Math.max(this.latest, m.revision);
    const previous = this.current;
    this.current = m.revision;
    this.progress = null;
    if (previous < 0) {
      const t0 = performance.now();
      const exact = await this.net.request({ kind: 'get_exact_state', revision: m.revision, tick: 0 }, 'exact_state', e => e.tick === 0 && e.revision === m.revision);
      this.terrain = exact.snapshot.terrain;
      this.initialOre = exact.snapshot.ore;
      this.exact = { revision: m.revision, tick: 0, state: exact.snapshot };
      console.log(`initial exact state in ${Math.round(performance.now() - t0)} ms`);
      if (!this.renderer.fit() && this.player !== null) this.renderer.centerOn(this.startTile(this.player).x, this.startTile(this.player).y);
    }
    if (previous < 0 || wasFull) this.view = { t0:0,t1:Math.max(1,m.outcome.terminal_state_tick) };
    else this.panTimeline(0);
    $('toast').classList.remove('active');
    this.playhead = Math.min(this.playhead, m.outcome.terminal_state_tick);
    this.exact = this.exact && this.exact.revision === m.revision ? this.exact : null;
    this.requestExact();
    void this.ensureChunk(Math.floor(this.playhead / CHUNK));
    this.updatePanels();
    void this.experience.loadRound(m.revision);
    if (this.player !== null && this.phase && this.phase.revision === m.revision) this.net.send({ kind: 'planning_ready', round: this.phase.round, revision: m.revision });
  }

  failReplay(revision: number, error: unknown): void {
    const message = error instanceof Error ? error.message : String(error);
    this.replayErrors.set(revision, message);
    if (this.current === revision) { this.playing = false; this.updatePanels(); }
  }

  startTile(player: number): Tile {
    const e = this.exact?.state.entities.find(e => e.owner === player);
    return e?.tile ?? { x: 0, y: 0 };
  }

  rev(): RevisionView | null {
    return this.revisions.get(this.current) ?? null;
  }

  async ensureChunk(k: number): Promise<void> {
    const rev = this.rev();
    if (!rev || this.replayErrors.has(rev.revision) || rev.chunks.has(k) || rev.loading.has(k) || k * CHUNK > rev.outcome.terminal_state_tick) return;
    rev.loading.add(k);
    const from = k * CHUNK;
    const to = Math.min(from + CHUNK - 1, rev.outcome.terminal_state_tick);
    const t0 = performance.now();
    try {
      const range = await this.net.request({ kind: 'get_snapshot_range', revision: rev.revision, from_tick: from, to_tick: to, stride: this.config.snapshot_interval }, 'snapshot_range', s => s.revision === rev.revision && (s.samples.length === 0 || s.samples[0].tick >= from));
      rev.dictionary = range.entity_dictionary;
      for (const s of range.samples) rev.samples.set(s.tick, s);
      const events = await this.net.request({ kind: 'get_events', revision: rev.revision, from_tick: from, to_tick: to, effects_only: true }, 'events', e => e.revision === rev.revision);
      rev.events = rev.events.concat(events.events);
      rev.chunks.add(k);
      while (rev.chunks.size > 8) {
        const oldest = rev.chunks.values().next().value!; rev.chunks.delete(oldest);
        for (const tick of rev.samples.keys()) if (Math.floor(tick/CHUNK) === oldest) rev.samples.delete(tick);
        rev.events = rev.events.filter(e => Math.floor(e.tick/CHUNK) !== oldest);
      }
      console.log(`chunk ${k} of revision ${rev.revision}: ${range.samples.length} samples, ${events.events.length} events in ${Math.round(performance.now() - t0)} ms`);
    } catch (err) {
      if (/mismatch/i.test(String(err))) this.failReplay(rev.revision, err);
      else this.toast(`Loading replay: ${(err as Error).message}`);
    } finally {
      rev.loading.delete(k);
    }
  }

  requestExact(): void {
    const rev = this.rev();
    if (!rev || this.replayErrors.has(rev.revision)) return;
    const tick = Math.min(Math.floor(this.playhead), rev.outcome.terminal_state_tick);
    if (this.exact && this.exact.revision === rev.revision && this.exact.tick === tick) return;
    if (this.exactPending && this.exactPending.revision === rev.revision && this.exactPending.tick === tick) return;
    this.exactPending = { revision: rev.revision, tick };
    const t0 = performance.now();
    this.net
      .request({ kind: 'get_exact_state', revision: rev.revision, tick }, 'exact_state', e => e.revision === rev.revision && e.tick === tick)
      .then(e => {
        if (this.exactPending?.tick === tick && this.exactPending.revision === rev.revision) this.exactPending = null;
        if (Math.floor(this.playhead) === tick && this.current === rev.revision) {
          this.exact = { revision: rev.revision, tick, state: e.snapshot };
          if (this.player !== null && this.phase && this.phase.revision === rev.revision) this.net.send({ kind: 'planning_ready', round: this.phase.round, revision: rev.revision });
          this.updatePanels();
        }
        $('top-exact').textContent = `exact ${tick} in ${Math.round(performance.now() - t0)} ms`;
      })
      .catch(err => {
        this.exactPending = null;
        if (/mismatch/i.test(String(err))) this.failReplay(rev.revision, err);
        else this.toast(String(err.message ?? err));
      });
  }

  /** Entities to draw at the playhead: exact state when available, otherwise interpolated samples. */
  rawEntities(): EntityView[] {
    const rev = this.rev();
    if (!rev) return [];
    const tick = this.playhead;
    const whole = Math.floor(tick);
    if (this.exact && this.exact.revision === rev.revision && this.exact.tick === whole && (tick === whole || !this.playing)) {
      const dictionary = new Map(rev.dictionary.map((d, i) => [idKey(d.id), i]));
      return this.exact.state.entities.map(e => ({
        index: dictionary.get(idKey(e.id)) ?? -1,
        id: e.id,
        owner: e.owner,
        type_key: e.type_key,
        x: e.tile.x,
        y: e.tile.y,
        hp: e.hp,
        maxHp: this.maxHp(e.type_key, e.lifecycle, e.paid_matter),
        facing: e.last_move_direction,
        lifecycle: e.lifecycle,
        activity: e.action.kind === 'mine' ? 'mining' : e.action.kind === 'construct' ? 'construction' : e.engaged_target ? 'combat' : e.action.kind === 'idle' ? 'idle' : 'movement',
        engaged: e.engaged_target ? (dictionary.get(idKey(e.engaged_target)) ?? null) : null,
        exact: e,
      }));
    }
    const interval = this.config.snapshot_interval;
    const ta = Math.floor(tick / interval) * interval;
    const a = rev.samples.get(ta);
    if (!a) return [];
    const b = rev.samples.get(ta + interval) ?? rev.samples.get(rev.outcome.terminal_state_tick);
    const f = b && b.tick > ta ? Math.min(1, (tick - ta) / (b.tick - ta)) : 0;
    const next = new Map(b?.entities.map(e => [e.index, e]) ?? []);
    return a.entities.map(e => {
      const ref = rev.dictionary[e.index];
      const n = next.get(e.index);
      return {
        index: e.index,
        id: ref?.id,
        owner: ref?.owner ?? 0,
        type_key: ref?.type_key ?? '',
        x: n ? e.tile.x + (n.tile.x - e.tile.x) * f : e.tile.x,
        y: n ? e.tile.y + (n.tile.y - e.tile.y) * f : e.tile.y,
        hp: n ? e.hp + (n.hp - e.hp) * f : e.hp,
        maxHp: this.types.get(ref?.type_key ?? '')?.max_hp ?? e.hp,
        facing: e.facing,
        lifecycle: e.lifecycle,
        activity: e.activity,
        engaged: e.engaged ?? null,
      };
    });
  }

  entities(): EntityView[] {
    const all = this.rawEntities();
    const visible = this.visibility(all);
    const views = all.filter(e => this.spectator || e.owner === this.player || visible.has(`${Math.floor(e.x)},${Math.floor(e.y)}`));
    if (this.exact?.revision === this.current && this.exact.tick === Math.floor(this.playhead)) {
      for (const b of this.exact.state.blueprints) if (!b.site_id && (b.owner === this.player || this.spectator)) {
        views.push({index:-1,id:b.id,owner:b.owner,type_key:b.type_key,x:b.tile.x,y:b.tile.y,hp:0,maxHp:1,facing:'n',lifecycle:'blueprint',activity:'idle',engaged:null,blueprint:{kind:'persistent',id:b.id},settings:b.settings ?? {queue:[],order:{kind:'idle'},priority:b.priority,loop_enabled:false}});
      }
    }
    if (this.current === this.latest && this.draft.tick === Math.floor(this.playhead)) {
      this.draft.commands.forEach((d,index) => {
        const c = d.command;
        if (c.kind === 'place_blueprints') c.tiles.forEach((tile,item_index) => views.push({index:-1,id:{birth_command:{command:{round:this.round,player:this.player!,index},target_index:0},item_index,occurrence:0},owner:this.player!,type_key:c.type_key,x:tile.x,y:tile.y,hp:0,maxHp:1,facing:'n',lifecycle:'blueprint',activity:'idle',engaged:null,blueprint:{kind:'draft',local_id:d.local_id,item_index},settings:{queue:[],order:{kind:'idle'},priority:c.priority,loop_enabled:false}}));
        if (c.kind === 'cancel_blueprints') for (let i = views.length - 1; i >= 0; i--) if (c.blueprint_ids.some(b => JSON.stringify(b) === JSON.stringify(views[i].blueprint ?? (views[i].exact?.blueprint_id ? {kind:'persistent',id:views[i].exact!.blueprint_id} : null)))) views.splice(i, 1);
        if (c.kind === 'configure_blueprints') for (const v of views) if (v.blueprint && c.blueprint_ids.some(b => JSON.stringify(b) === JSON.stringify(v.blueprint))) v.settings = c.settings;
      });
    }
    return views;
  }

  private visionCache: { revision: number; tick: number; exact: unknown; sample: unknown; nextSample: unknown; tiles: Set<string> } | null = null;
  visibility(all?: EntityView[]): Set<string> {
    const sample = this.rev()?.samples.get(Math.floor(this.playhead / this.config.snapshot_interval) * this.config.snapshot_interval);
    const nextSample = this.rev()?.samples.get((Math.floor(this.playhead / this.config.snapshot_interval) + 1) * this.config.snapshot_interval);
    const cached = this.visionCache;
    if (cached && cached.revision === this.current && cached.tick === this.playhead && cached.exact === this.exact && cached.sample === sample && cached.nextSample === nextSample) return cached.tiles;
    const visible = new Set<string>();
    this.visionCache = { revision: this.current, tick: this.playhead, exact: this.exact, sample, nextSample, tiles: visible };
    if (this.spectator || !this.terrain) return visible;
    const team = this.profiles[this.player!]?.profile?.team_id;
    for (const e of all ?? this.rawEntities()) {
      if (e.lifecycle !== 'complete' || (e.owner !== this.player && (!team || this.profiles[e.owner]?.profile?.team_id !== team))) continue;
      const radius = this.types.get(e.type_key)?.vision ?? 0;
      for (let y=Math.max(0,Math.ceil(e.y-radius));y<=Math.min(this.terrain.height-1,Math.floor(e.y+radius));y++)
        for (let x=Math.max(0,Math.ceil(e.x-radius));x<=Math.min(this.terrain.width-1,Math.floor(e.x+radius));x++)
          if ((x-e.x)**2+(y-e.y)**2<=radius**2) visible.add(`${x},${y}`);
    }
    return visible;
  }

  configureGhosts(change: (s: BlueprintSettings) => void): boolean {
    const ghosts = this.selectedViews().filter(v => v.blueprint && this.ownSelectable(v));
    for (const v of ghosts) {
      const settings = structuredClone(v.settings!); change(settings);
      this.stage({kind:'configure_blueprints',blueprint_ids:[v.blueprint!],settings});
    }
    return ghosts.length > 0;
  }

  effectiveOrder(v: EntityView): Order | undefined {
    let order = v.exact?.production?.stored_order ?? v.exact?.action ?? v.settings?.order;
    if (this.current === this.latest && this.draft.tick === Math.floor(this.playhead)) for (const d of this.draft.commands) {
      const c=d.command;
      if ((c.kind==='assign_order' && c.entities.some(id=>idKey(id)===idKey(v.id))) || (c.kind==='set_stored_order' && c.factories.some(id=>idKey(id)===idKey(v.id))) || (c.kind==='assign_group_order' && this.experience.groups().find(g=>g.id.owner===c.group.owner && g.id.slot===c.group.slot)?.members.some(id=>idKey(id)===idKey(v.id)))) order=c.order;
    }
    return order;
  }

  maxHp(type_key: string, lifecycle: string, paid: number): number {
    const t = this.types.get(type_key);
    if (!t) return 1;
    return lifecycle === 'site' ? Math.max(1e-9, (t.max_hp * paid) / t.matter_cost) : t.max_hp;
  }

  sampleAt(tick: number): Sample | null {
    const rev = this.rev();
    if (!rev) return null;
    const interval = this.config.snapshot_interval;
    return rev.samples.get(Math.floor(tick / interval) * interval) ?? null;
  }

  oreAt(index: number): number {
    if (this.exact && this.exact.revision === this.current && this.exact.tick === Math.floor(this.playhead)) return this.exact.state.ore[index];
    const s = this.sampleAt(this.playhead);
    if (!s) return this.initialOre[index] ?? 0;
    const cell = s.ore.find(o => o.index === index);
    return cell ? cell.remaining : 0;
  }

  eventsNear(tick: number, lookback: number): WorldEvent[] {
    const rev = this.rev();
    if (!rev) return [];
    return rev.events.filter(e => e.tick <= tick && e.tick > tick - lookback).slice(-Math.max(24, Math.floor(160 / this.rate)));
  }

  // ---- frame -------------------------------------------------------------------------------

  frame(now: number): void {
    const dt = (now - this.lastFrame) / 1000;
    this.lastFrame = now;
    const rev = this.rev();
    if (this.playing && rev) {
      this.playhead += dt * this.config.ticks_per_second * this.rate;
      if (this.playhead >= rev.outcome.terminal_state_tick) {
        this.playhead = rev.outcome.terminal_state_tick;
        this.playing = false;
        this.requestExact();
        this.updatePanels();
      }
      void this.ensureChunk(Math.floor(this.playhead / CHUNK));
      void this.ensureChunk(Math.floor(this.playhead / CHUNK) + 1);
      this.updateTop();
    }
    if (now - this.lastTop > 1000) { this.lastTop = now; this.updateTop(); }
    this.renderer.pan(dt);
    this.renderer.draw();
    requestAnimationFrame(t => this.frame(t));
  }

  seek(tick: number): void {
    const rev = this.rev();
    if (!rev) return;
    this.playhead = Math.max(0, Math.min(Math.floor(tick), rev.outcome.terminal_state_tick));
    void this.ensureChunk(Math.floor(this.playhead / CHUNK));
    this.requestExact();
    this.updatePanels();
  }

  // ---- selection and draft -----------------------------------------------------------------

  ownSelectable(e: EntityView): boolean {
    return this.player !== null && e.owner === this.player && !!e.id;
  }

  selectedViews(): EntityView[] {
    return this.entities().filter(e => e.id && this.selection.has(idKey(e.id)));
  }

  canStage(): { ok: boolean; reason: string } {
    if (this.replayErrors.has(this.current)) return {ok: false, reason: 'Replay verification failed. Restart the peripheral and refresh.'};
    if (!this.net.connected) return { ok:false, reason:'Disconnected. Draft retained; reconnect before staging.' };
    if (this.current !== this.latest) return { ok: false, reason: 'Historical replay is read-only. Return to live.' };
    if (this.spectator) return { ok: false, reason: 'Spectators cannot stage orders.' };
    if (!this.phase || this.phase.revision !== this.current) return { ok: false, reason: 'Planning is not open.' };
    if (this.committed.includes(this.player ?? -1)) return { ok: false, reason: 'You already committed this round.' };
    const tick = Math.floor(this.playhead);
    if (this.draft.tick !== null && tick !== this.draft.tick) return { ok: false, reason: `Draft is at tick ${this.draft.tick}; press T to return or clear the draft.` };
    if (tick < this.editableFrom || tick > this.availableThrough) return { ok: false, reason: `Editable ticks are ${this.editableFrom}–${this.availableThrough}.` };
    if (!this.exact || this.exact.revision !== this.current || this.exact.tick !== tick) return { ok: false, reason: 'Exact state is still loading.' };
    return { ok: true, reason: '' };
  }

  stage(command: Command, policy: FutureOrderPolicy = 'keep'): void {
    const gate = this.canStage();
    if (!gate.ok) {
      this.toast(gate.reason);
      return;
    }
    if (this.config.control_limit === 'single_order' && this.draft.commands.length >= 1) {
      this.toast('Single-order mode: delete the staged command first.');
      return;
    }
    this.draft.undo.push({commands:[...this.draft.commands],tick:this.draft.tick});
    this.draft.redo = [];
    this.draft.tick = Math.floor(this.playhead);
    $('toast').classList.remove('active');
    // A later draft assignment replaces the same recipients' earlier action; undo restores it.
    if (command.kind === 'assign_order' || command.kind === 'set_stored_order') {
      const ids = command.kind === 'assign_order' ? command.entities : command.factories;
      this.draft.commands = this.draft.commands.flatMap(d => {
        const c=d.command;
        if(c.kind==='assign_order' && command.kind==='assign_order') {const entities=c.entities.filter(id=>!ids.some(v=>idKey(v)===idKey(id)));return entities.length?[{...d,command:{...c,entities}}]:[];}
        if(c.kind==='set_stored_order' && command.kind==='set_stored_order') {const factories=c.factories.filter(id=>!ids.some(v=>idKey(v)===idKey(id)));return factories.length?[{...d,command:{...c,factories}}]:[];}
        return [d];
      });
    }
    this.draft.commands.push({ local_id: `d${Date.now()}-${this.draft.commands.length}`, command: command as DraftCommand['command'], future_orders: policy });
    this.updatePanels();
  }

  undo(): void {
    const previous = this.draft.undo.pop();
    if (!previous) return;
    this.draft.redo.push({commands:[...this.draft.commands],tick:this.draft.tick});
    this.draft.commands = previous.commands;
    this.draft.tick = previous.tick;
    this.updatePanels();
  }
  redo(): void {
    const next = this.draft.redo.pop();
    if (!next) return;
    this.draft.undo.push({commands:[...this.draft.commands],tick:this.draft.tick});
    this.draft.commands = next.commands;
    this.draft.tick = next.tick;
    this.updatePanels();
  }
  clearDraft(): void {
    this.draft = { tick: null, commands: [], undo: [], redo: [], policy: this.draft.policy, selected: null };
    this.updatePanels();
  }

  selectedIds(filter?: (t: TypeDefinition) => boolean): EntityId[] {
    return this.selectedViews()
      .filter(e => this.ownSelectable(e) && e.lifecycle !== 'blueprint' && (!filter || filter(this.types.get(e.type_key)!)))
      .map(e => e.id)
      .sort((a, b) => idKey(a).localeCompare(idKey(b)));
  }

  assign(order: Order, filter: (t: TypeDefinition) => boolean): void {
    const ghosts = this.configureGhosts(s => { s.order = order; });
    this.stored = false;
    if (this.recalledGroup !== null && this.player !== null) {
      this.stage({ kind: 'assign_group_order', group: { owner: this.player, slot: this.recalledGroup }, order }, this.draft.policy);
      return;
    }
    const entities = this.selectedIds(t => !!t.production || filter(t));
    if (!entities.length) {
      if (ghosts) return;
      this.toast('No selected unit can take that order.');
      return;
    }
    this.stage({ kind: 'assign_order', entities, order }, this.draft.policy);
  }

  private turnRequestPending = false;

  async uncommit(): Promise<void> {
    if (this.turnRequestPending || !this.net.connected || !this.session.token || !this.phase || this.committed.length >= this.config.player_count) return;
    this.turnRequestPending = true;
    this.updatePanels();
    const request_id = `uncommit-${this.player}-${this.round}-${Date.now()}`;
    try {
      const reply = await this.net.request({ kind: 'uncommit', request_id, slot_token: this.session.token, round: this.round, revision: this.latest }, 'uncommitted', m => m.request_id === request_id);
      if (reply.round === this.round) {
        this.committed = this.committed.filter(p => p !== this.player);
        this.clearDraft();
        if (reply.draft) {
          this.draft.tick = reply.draft.tick;
          this.draft.commands = reply.draft.commands;
          this.draft.undo = reply.draft.commands.map((_, index) => ({commands:reply.draft!.commands.slice(0,index),tick:index ? reply.draft!.tick : null}));
          this.current = reply.draft.based_on_revision;
          this.seek(reply.draft.tick);
          this.toast('Turn uncommitted. Your moves are ready to edit.');
        } else this.toast('Turn uncommitted. This older archive has no editable draft; enter your moves again.');
      }
    } catch (err) { this.toast(`Could not uncommit: ${(err as Error).message}`); }
    finally { this.turnRequestPending = false; this.updatePanels(); }
  }

  async commit(): Promise<void> {
    if (!this.net.connected || this.spectator || !this.session.token || !this.phase) return;
    if (this.committed.includes(this.player ?? -1)) { await this.uncommit(); return; }
    if (this.current !== this.latest) return;
    if (this.turnRequestPending) return;
    const tick = this.draft.tick ?? Math.floor(this.playhead);
    if (tick < this.editableFrom || tick > this.availableThrough) {
      this.toast(`Pass tick must be within ${this.editableFrom}–${this.availableThrough}.`);
      return;
    }
    this.turnRequestPending = true;
    this.updatePanels();
    const submittedRound = this.round;
    const request_id = `${this.player}-${this.round}-${Date.now()}`;
    try {
      await this.net.request({ kind: 'commit', request: { request_id, slot_token: this.session.token, draft: { based_on_revision: this.current, tick, commands: this.draft.commands } } }, 'commit_accepted', m => m.request_id === request_id);
      if (this.round === submittedRound && !this.committed.includes(this.player!)) this.committed.push(this.player!);
      this.clearDraft();
      this.toast('Turn committed. Waiting for the other players…');
    } catch (err) {
      this.toast(`Commit rejected: ${(err as Error).message}`);
    }
    this.turnRequestPending = false;
    this.updatePanels();
  }

  // ---- input ---------------------------------------------------------------------------------

  bindInput(): void {
    const map = $<HTMLCanvasElement>('map');
    map.addEventListener('contextmenu', e => e.preventDefault());
    map.addEventListener('mousedown', e => {
      if (e.button === 1) {
        this.renderer.dragPan = { x: e.clientX, y: e.clientY };
        e.preventDefault();
        return;
      }
      if (e.button !== 0) return;
      map.focus();
      const tile = this.renderer.tileAt(e.clientX, e.clientY);
      if (this.mode.kind === 'attack' || this.mode.kind === 'stored-target') {
        this.applyTileMode(tile);
        return;
      }
      if (this.mode.kind === 'place' && this.mode.type_key !== 'wall') {
        this.placeAt(tile);
        return;
      }
      if (this.mode.kind === 'support') {
        const target = this.entityAt(tile);
        if (target && target.id && target.owner === this.player) this.assign({ kind: 'support', target: target.id }, t => !!t.movement);
        else this.toast('Support needs one of your own units as the target.');
        this.mode = { kind: 'none' };
        this.updateMode();
        return;
      }
      this.drag = { x0: e.clientX, y0: e.clientY, x1: e.clientX, y1: e.clientY };
    });
    window.addEventListener('mousemove', e => {
      if (e.target === map) this.hover = this.renderer.tileAt(e.clientX, e.clientY);
      if (this.drag) {
        this.drag.x1 = e.clientX;
        this.drag.y1 = e.clientY;
      }
      if (this.renderer.dragPan) {
        this.renderer.panBy(e.clientX - this.renderer.dragPan.x, e.clientY - this.renderer.dragPan.y);
        this.renderer.dragPan = { x: e.clientX, y: e.clientY };
      }
    });
    window.addEventListener('mouseup', e => {
      if (e.button === 1) this.renderer.dragPan = null;
      if (e.button !== 0 || !this.drag) return;
      const drag = this.drag;
      this.drag = null;
      const a = this.renderer.tileAt(drag.x0, drag.y0);
      const b = this.renderer.tileAt(drag.x1, drag.y1);
      const rect = { min: { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y) }, max: { x: Math.max(a.x, b.x), y: Math.max(a.y, b.y) } };
      if (this.mode.kind === 'place') {
        this.placeAt(a, b);
        return;
      }
      if (this.mode.kind === 'area') {
        const order: Order = this.mode.order === 'mine' ? { kind: 'mine', area: rect } : { kind: 'construct', area: rect };
        this.assign(order, t => (this.mode.kind === 'area' && this.mode.order === 'mine' ? !!t.mining : !!t.construction));
        this.mode = { kind: 'none' };
        this.updateMode();
        return;
      }
      const moved = Math.hypot(drag.x1 - drag.x0, drag.y1 - drag.y0) > 4;
      if (!e.shiftKey) this.selection.clear();
      this.recalledGroup = null;
      const views = this.entities().filter(v => v.id && (moved ? v.x >= rect.min.x && v.x <= rect.max.x && v.y >= rect.min.y && v.y <= rect.max.y : Math.round(v.x) === a.x && Math.round(v.y) === a.y));
      const own = views.filter(v => this.ownSelectable(v));
      for (const v of moved ? own : views.slice(0, 1)) {
        const key = idKey(v.id);
        if (e.shiftKey && this.selection.has(key) && !moved) this.selection.delete(key);
        else this.selection.add(key);
      }
      this.updatePanels();
    });
    map.addEventListener('wheel', e => {
      e.preventDefault();
      if (e.shiftKey) this.renderer.panBy(-e.deltaX, -e.deltaY);
      else this.renderer.zoomAt(e.clientX, e.clientY, Math.exp(-e.deltaY * .002));
    }, { passive: false });
    $<HTMLCanvasElement>('minimap').addEventListener('mousemove', e => {
      if (e.buttons !== 1) return;
      const t = this.renderer.minimapTile(e.clientX, e.clientY);
      if (t) this.renderer.centerOn(t.x, t.y);
    });
    $<HTMLCanvasElement>('minimap').addEventListener('mousedown', e => {
      const t = this.renderer.minimapTile(e.clientX, e.clientY);
      if (t) this.renderer.centerOn(t.x, t.y);
    });
    const timeline = $<HTMLCanvasElement>('timeline');
    let timelineDrag: { x: number; t0: number; t1: number } | null = null;
    timeline.addEventListener('mousedown', e => {
      e.preventDefault(); map.focus();
      if (e.button === 1 || e.offsetY >= timeline.clientHeight - 16) timelineDrag = { x: e.clientX, ...this.view };
      else if (e.button === 0) this.seek(this.renderer.timelineTick(e.clientX));
    });
    window.addEventListener('mouseup', () => { timelineDrag = null; });
    window.addEventListener('mousemove', e => {
      if (timelineDrag) {
        const delta = (timelineDrag.x - e.clientX) / timeline.clientWidth * (timelineDrag.t1 - timelineDrag.t0);
        this.view = { t0: timelineDrag.t0, t1: timelineDrag.t1 };
        this.panTimeline(delta);
      }
    });
    timeline.addEventListener('mousemove', e => {
      this.timelineCursor = this.renderer.timelineTick(e.clientX);
      const tolerance = 4 / timeline.clientWidth * (this.view.t1 - this.view.t0);
      const turns = (this.experience.turns.get(this.current) ?? []).filter(t => t.commands.length && Math.abs(t.tick - this.timelineCursor) <= tolerance);
      timeline.title = `Tick ${Math.floor(this.timelineCursor)} · ${turns.map(t => `${this.name(t.player)}: ${t.commands.length} orders at tick ${t.tick} (round ${t.round})`).join(' · ')} · White outline: latest write · Purple: selected units · Shift + scroll or Drag bottom ruler to pan`;
    });
    timeline.addEventListener('wheel', e => {
      e.preventDefault();
      if (e.shiftKey) this.panTimeline((e.deltaX || e.deltaY) / timeline.clientWidth * (this.view.t1-this.view.t0));
      else this.zoomTimeline(Math.exp(e.deltaY * .002), this.renderer.timelineTick(e.clientX));
    }, { passive: false });
    $('play').onclick = () => this.togglePlay();
    $('step-back').onclick = () => this.seek(this.playhead - 1);
    $('step-fwd').onclick = () => this.seek(this.playhead + 1);
    $('jump-back').onclick = () => this.seek(this.playhead - 100);
    $('jump-fwd').onclick = () => this.seek(this.playhead + 100);
    $('to-draft').onclick = () => this.draft.tick !== null && this.seek(this.draft.tick);
    $<HTMLInputElement>('tick-input').onchange = e => this.seek(Number((e.target as HTMLInputElement).value));
    $('commit').onclick = () => void this.commit();
    window.addEventListener('keydown', e => this.key(e));
    window.addEventListener('resize', () => this.renderer.resize());
    this.renderer.resize();
  }

  togglePlay(): void {
    const rev = this.rev();
    if (!rev) return;
    if (!this.playing && this.playhead >= rev.outcome.terminal_state_tick) this.playhead = 0;
    this.playing = !this.playing;
    if (!this.playing) {
      this.playhead = Math.floor(this.playhead);
      this.requestExact();
    }
    this.updatePanels();
  }

  entityAt(tile: Tile): EntityView | undefined {
    return this.entities().find(v => Math.round(v.x) === tile.x && Math.round(v.y) === tile.y);
  }

  applyTileMode(tile: Tile): void {
    if (this.mode.kind === 'attack') this.assign({ kind: 'attack_move', destination: tile }, t => !!t.movement);
    if (this.mode.kind === 'stored-target') this.assign({ kind:'attack_move', destination:tile }, t => !!t.movement);
    this.mode = { kind: 'none' };
    this.updateMode();
  }

  placeAt(tile: Tile, end: Tile = tile): void {
    if (this.mode.kind !== 'place') return;
    const type_key = this.mode.type_key;
    const def = this.types.get(type_key);
    const tiles = this.placementTiles(tile, end);
    if (tiles.some(t => !this.validPlacement(t, !!def?.production))) { this.toast('Placement blocked: choose clear floor and an open factory output.'); return; }
    const command: Command = { kind: 'place_blueprints', type_key, tiles, priority: 'medium', output_directions: def?.production ? tiles.map(() => this.renderer.outputDirection) : null };
    this.stage(command);
    this.mode = { kind: 'none' };
    this.updateMode();
  }

  placementTiles(a: Tile, b: Tile): Tile[] {
    const tiles: Tile[] = [{ ...a }];
    let { x, y } = a;
    // Follow the dragged line with axis-connected steps (no diagonal holes).
    const nx = Math.abs(b.x-a.x), ny = Math.abs(b.y-a.y);
    let ix = 0, iy = 0;
    while (ix < nx || iy < ny) {
      if (ix < nx && (iy === ny || (1+2*ix)*ny <= (1+2*iy)*nx)) { x += Math.sign(b.x-a.x); ix++; }
      else { y += Math.sign(b.y-a.y); iy++; }
      tiles.push({x,y});
    }
    return tiles.sort((a, b) => a.y - b.y || a.x - b.x);
  }
  validPlacement(tile: Tile, factory: boolean): boolean {
    const terrain = this.terrain;
    const clear = (t: Tile) => !!terrain && t.x >= 0 && t.y >= 0 && t.x < terrain.width && t.y < terrain.height && terrain.cells[t.y * terrain.width + t.x] === 'floor' && !this.entityAt(t) && !this.exact?.state.blueprints.some(b => b.tile.x === t.x && b.tile.y === t.y) && !this.draft.commands.some(c => c.command.kind === 'place_blueprints' && c.command.tiles.some(b => b.x === t.x && b.y === t.y));
    const offsets = { n: [0,-1], e: [1,0], s: [0,1], w: [-1,0] };
    const [dx,dy] = offsets[this.renderer.outputDirection];
    return clear(tile) && (!factory || clear({ x: tile.x + dx, y: tile.y + dy }));
  }
  timelineEnd(): number {
    return Math.max(1, this.rev()?.outcome.terminal_state_tick ?? 1, ...(this.experience?.turns.get(this.current) ?? []).filter(t => t.commands.length).map(t => t.tick));
  }
  panTimeline(delta: number): void {
    const end = this.timelineEnd();
    const span = Math.min(end, this.view.t1 - this.view.t0);
    const t0 = Math.max(0, Math.min(end - span, this.view.t0 + delta));
    this.view = { t0, t1: t0 + span };
  }
  zoomTimeline(factor: number, at = this.timelineCursor || this.playhead): void {
    const end = this.timelineEnd();
    const old = this.view.t1 - this.view.t0;
    const span = Math.max(Math.min(10, end), Math.min(end, old * factor));
    const fraction = Math.max(0, Math.min(1, (at - this.view.t0) / old));
    this.view = { t0: at - fraction * span, t1: at + (1 - fraction) * span };
    this.panTimeline(0);
  }

  producible(): string[] {
    return [...this.types.values()].filter(t => t.kind === 'unit').map(t => t.key).sort();
  }
  structures(): string[] {
    return ['factory', 'turret', 'wall'].filter(k => this.types.has(k)).concat([...this.types.values()].filter(t => t.kind === 'structure' && !['factory', 'turret', 'wall'].includes(t.key)).map(t => t.key));
  }

  key(e: KeyboardEvent): void {
    const target = e.target as HTMLElement;
    if (target && (target.tagName === 'INPUT' || target.tagName === 'SELECT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) {
      if (e.key === 'Escape') target.blur();
      return;
    }
    if (e.altKey) return;
    const k = e.key.length === 1 && /^[a-z]$/i.test(e.key) ? e.key.toLowerCase() : e.key;
    if (e.ctrlKey || e.metaKey) {
      if (k.toLowerCase() === 'z' && e.shiftKey) this.redo();
      else if (k.toLowerCase() === 'z') this.undo();
      else if (k.toLowerCase() === 'y' || k.toLowerCase() === 'u') this.redo();
      else return;
      e.preventDefault();
      return;
    }
    if (target?.tagName === 'BUTTON' && (k === 'Enter' || k === ' ')) return;
    if (k === 'Escape') {
      if ($('statistics').classList.contains('active')) $('statistics').classList.remove('active');
      else if ($('help').classList.contains('active')) $('help').classList.remove('active');
      else if (this.mode.kind !== 'none') { this.mode = { kind: 'none' }; this.stored = false; this.drag = null; }
      else {
        this.selection.clear();
        this.recalledGroup = null;
      }
      this.updateMode();
      this.updatePanels();
      $('map').focus();
      return;
    }
    if (k === '?') {
      $('help').classList.toggle('active');
      return;
    }
    if (/^[0-9]$/.test(k)) {
      this.digit(Number(k));
      return;
    }
    switch (k) {
      case 'w': this.renderer.keys.w = true; break;
      case 'a': this.renderer.keys.a = true; break;
      case 's': this.renderer.keys.s = true; break;
      case 'd': this.renderer.keys.d = true; break;
      case 'f': this.mode = this.mode.kind === 'stored' ? { kind: 'stored-target', order: 'attack' } : { kind: 'attack' }; break;
      case 'g': this.mode = { kind: 'support' }; break;
      case 'm': this.mode = { kind: 'area', order: 'mine' }; break;
      case 'c': this.mode = { kind: 'area', order: 'construct' }; break;
      case 'b': this.mode = { kind: 'build' }; break;
      case 'q': this.mode = { kind: 'recipe' }; break;
      case 'p': this.mode = { kind: 'priority' }; break;
      case 'r': if (this.mode.kind === 'place') this.renderer.cycleOutput(); break;
      case 'h': this.mode = { kind: 'membership', add: true }; break;
      case 'j': this.mode = { kind: 'binding' }; break;
      case 'v': this.experience.toggleStats(); break;
      case 'z': if (this.mode.kind === 'place') this.renderer.cycleOutput(); break;
      case 'x': this.assign({ kind: 'idle' }, () => true); this.mode = { kind: 'none' }; break;
      case 'l': {
        const enabled = !this.selectedViews().filter(v=>this.types.get(v.type_key)?.production).every(v=>factoryPlan(this,v).loop);
        const ghosts = this.configureGhosts(s => { s.loop_enabled = enabled; });
        const factories = this.selectedIds(t => !!t.production);
        if (factories.length) this.stage({ kind: 'set_queue_loop', factories, enabled });
        else if (!ghosts) this.toast('Select a factory first.');
        break;
      }
      case 'o': {
        const policies: FutureOrderPolicy[] = this.config.future_orders.window_ticks ? ['keep', 'drop_all', 'drop_window'] : ['keep', 'drop_all'];
        this.draft.policy = policies[(policies.indexOf(this.draft.policy) + 1) % policies.length];
        this.updatePanels();
        break;
      }
      case 'Delete':
      case 'Backspace': {
        e.preventDefault();
        if (this.mode.kind === 'binding') { this.experience.bind(null); break; }
        if (this.experience.deleteFocused()) break;
        if (this.draft.selected !== null && this.draft.commands[this.draft.selected]) {
          this.draft.undo.push({commands:[...this.draft.commands],tick:this.draft.tick});
          this.draft.redo = [];
          this.draft.commands.splice(this.draft.selected, 1);
          this.draft.selected = null;
          if (!this.draft.commands.length) this.draft.tick = null;
          this.updatePanels();
        }
        break;
      }
      case 'Enter': if (this.mode.kind === 'none' && !this.drag) void this.commit(); else this.toast('Finish or cancel the action before committing.'); break;
      case ' ': this.togglePlay(); e.preventDefault(); break;
      case ',': this.seek(this.playhead - (e.shiftKey ? 100 : 1)); break;
      case '.': this.seek(this.playhead + (e.shiftKey ? 100 : 1)); break;
      case '<': this.seek(this.playhead - 100); break;
      case '>': this.seek(this.playhead + 100); break;
      case 'Home': this.seek(this.editableFrom); break;
      case 'End': this.seek(this.availableThrough); break;
      case '[': if (e.shiftKey) {this.panTimeline(-(this.view.t1-this.view.t0)/4);break;} this.rate = RATES[Math.max(0, RATES.indexOf(this.rate) - 1)]; this.updatePanels(); break;
      case ']': if (e.shiftKey) {this.panTimeline((this.view.t1-this.view.t0)/4);break;} this.rate = RATES[Math.min(RATES.length - 1, RATES.indexOf(this.rate) + 1)]; this.updatePanels(); break;
      case '-': this.zoomTimeline(1.25); e.preventDefault(); break;
      case '=': this.zoomTimeline(0.8); e.preventDefault(); break;
      case '{': this.panTimeline(-(this.view.t1-this.view.t0)/4); break;
      case '}': this.panTimeline((this.view.t1-this.view.t0)/4); break;
      case 't': if (this.draft.tick !== null) this.seek(this.draft.tick); break;
      default: return;
    }
    this.updateMode();
  }

  digit(n: number): void {
    const mode = this.mode;
    if (mode.kind === 'membership') {
      if (this.player !== null) this.stage({ kind: 'edit_group_members', group: { owner: this.player, slot: n }, edit: { kind: mode.add ? 'add' : 'replace', entities: mode.add ? this.selectedIds() : [] } });
      this.mode = { kind: 'none' };
    } else if (mode.kind === 'binding') {
      this.experience.bind(n);
    } else if (mode.kind === 'build') {
      const key = this.structures()[n - 1];
      if (key) this.mode = { kind: 'place', type_key: key };
      else this.mode = { kind: 'none' };
    } else if (mode.kind === 'recipe') {
      const key = this.producible()[n - 1];
      const factories = this.selectedIds(t => !!t.production);
      const ghosts = key ? this.configureGhosts(s => { s.queue.push(key); }) : false;
      if (key && factories.length) this.stage({ kind: 'edit_production', factories, edit: { kind: 'append', items: [key] } });
      else if (!ghosts) this.toast(key ? 'Select a factory first.' : 'No such recipe.');
      this.mode = { kind: 'none' };
    } else if (mode.kind === 'priority') {
      const priority = (['high', 'medium', 'low'] as Priority[])[n - 1];
      if (priority) this.configureGhosts(s => { s.priority = priority; });
      const entities = this.selectedViews().filter(v=>this.ownSelectable(v) && !v.blueprint).map(v=>v.id);
      if (priority && entities.length) this.stage({ kind: 'set_priority', entities, priority });
      this.mode = { kind: 'none' };
    } else if (mode.kind === 'stored') {
      this.mode = { kind: 'none' };
    } else {
      // Recall a control group from exact state; membership is simulation state.
      const group = this.experience.groups().find(g => g.id.owner === this.experience.groupOwner && g.id.slot === n);
      this.selection.clear();
      this.recalledGroup = n;
      if (group && this.exact) {
        const living = new Set(this.exact.state.entities.map(e => idKey(e.id)));
        for (const m of group.members) if (living.has(idKey(m))) this.selection.add(idKey(m));
        this.recalledGroup = n;
      }
      this.updatePanels();
    }
    this.updateMode();
  }

  // ---- panels --------------------------------------------------------------------------------

  updateMode(): void {
    const el = $('mode');
    const text = (() => {
      switch (this.mode.kind) {
        case 'none': return '';
        case 'attack': return 'Attack-move: click a destination tile (Esc cancels)';
        case 'support': return 'Support: click one of your units';
        case 'area': return `${this.mode.order === 'mine' ? 'Mine' : 'Construct'}: drag a rectangle`;
        case 'build': return `Build: ${this.structures().map((k, i) => `${i + 1}=${k}`).join('  ')}`;
        case 'place': return `Place ${this.mode.type_key}: click a floor tile${this.types.get(this.mode.type_key)?.production ? ` (output ${this.renderer.outputDirection.toUpperCase()}, R rotates)` : ''}`;
        case 'recipe': return `Queue: ${this.producible().map((k, i) => `${i + 1}=${k}`).join('  ')}`;
        case 'priority': return 'Priority: 1=high 2=medium 3=low';
        case 'membership': return `${this.mode.add ? 'Add selection to' : 'Clear'} group: 0–9`;
        case 'binding': return 'Factory output group: 0–9, Backspace clears';
        case 'stored': return 'Stored order: F destination, G ally, M mine area, C construct area, X idle';
        case 'stored-target': return 'Stored attack-move: click a destination tile';
      }
    })();
    el.textContent = `${this.stored && this.mode.kind !== 'stored' ? 'Stored order · ' : ''}${text}`;
    el.classList.toggle('active', text !== '');
    this.experience?.actions();
  }

  updateTop(): void {
    const rev = this.rev();
    const phase = this.replayErrors.has(this.current) ? `Replay unavailable: ${/mismatch/i.test(this.replayErrors.get(this.current)!) ? 'local replay mismatch' : 'loading failed'}` : this.current !== this.latest ? `Historical round ${this.experience?.rounds.get(this.current)?.round ?? '…'} (read-only)` : this.finished ? this.finished : this.committed.length >= this.config.player_count ? `Round ${this.round}: simulating…` : this.committed.includes(this.player ?? -1) ? `Round ${this.round}: committed, waiting` : this.phase && this.phase.revision === this.current ? `Round ${this.round}: planning` : this.progress ? `Simulating ${this.progress.tick}/${this.progress.end}` : 'Simulating…';
    $('top-phase').textContent = `${this.spectator ? 'Spectator' : this.name(this.player!)} · ${phase} · revision ${this.current}`;
    $('top-tick').innerHTML = `tick <b>${Math.floor(this.playhead)}</b> / ${rev?.outcome.terminal_state_tick ?? 0} · editable ${this.editableFrom}–${this.availableThrough}`;
    const sample = this.sampleAt(this.playhead);
    const banks = this.exact && this.exact.tick === Math.floor(this.playhead) && this.exact.revision === this.current
      ? this.exact.state.players.filter(p => this.spectator || p.player_id === this.player).map(p => `${this.name(p.player_id)} ${p.bank.toFixed(0)}${p.currently_eliminated ? ' (eliminated)' : ''}`)
      : sample?.players.filter(p => this.spectator || p.player_id === this.player).map(p => `${this.name(p.player_id)} ${p.bank.toFixed(0)}${p.currently_eliminated ? ' (eliminated)' : ''}`) ?? [];
    $('top-bank').textContent = `MATTER · ${banks.join(' · ')} · tick ${Math.floor(this.playhead)}${this.exact?.tick === Math.floor(this.playhead) ? '' : ' (sampled)'}`;
    const score = rev?.score?.entries.map(e => `${e.side_id.kind === 'player' ? this.name(e.side_id.player_id) : e.side_id.team_id} ${e.raw_total} (adjusted ${e.adjusted_total.toFixed(2)})`).join(' · ');
    $('top-score').textContent = score ? `score: ${score}` : '';
    const round = this.experience?.rounds.get(this.current);
    if (round) {
      const fastest = Math.max(1000,Math.min(...round.time_totals.map(t=>t.total_ms)));
      const penalty = this.config.objective.kind !== 'timed' ? this.config.objective.rules.time_penalty : 'none';
      $('top-sim').textContent = `Sim ${round.sim_duration_ms}ms · committed time ratio ${round.time_totals.map(t=>`${this.name(t.player_id)} ${(Math.max(1000,t.total_ms)/fastest).toFixed(2)}×`).join(' / ')} · penalty ${penalty}${!this.finished && this.current === this.latest && this.phase && !this.committed.includes(this.player ?? -1) && !this.spectator ? ` · live planning ${((performance.now()-this.planningSince)/1000).toFixed(0)}s` : ''}`;
    }
  }

  updatePanels(): void {
    this.updateTop();
    this.experience?.update();
    const rev = this.rev();
    $('play').textContent = this.playing ? 'Pause' : 'Play';
    $('rate').textContent = `${this.rate}×`;
    $<HTMLInputElement>('tick-input').value = String(Math.floor(this.playhead));
    $('timeline-info').textContent = rev ? `${rev.outcome.kind} · ${rev.outcome.stop_reason.replace('_', ' ')} at ${rev.outcome.terminal_state_tick}` : '';
    // Result panel.
    const result = $('result');
    if (rev) {
      const o = rev.outcome;
      const verdict = o.kind === 'win' ? this.spectator ? 'WIN' : o.survivors.includes(this.player!) ? 'WIN' : 'LOSS' : o.kind.toUpperCase();
      result.dataset.outcome = verdict.toLowerCase();
      const banner = $('outcome-banner');
      banner.dataset.outcome = verdict.toLowerCase();
      banner.textContent = `${this.finished ? 'MATCH ENDED · ' : 'TIMELINE RESULT · '}${verdict} · ${this.finished ? 'Replay remains available' : o.stop_reason === 'elimination' ? 'Simulation stopped: remaining opposition cannot act · Plan a new timeline or replay' : 'Simulation finished · Final survival determines this result · Replay available'}`;
      const lines = [`<b>Revision ${rev.revision}</b>: ${o.kind} (${o.stop_reason.replace('_', ' ')} at tick ${o.terminal_state_tick}, last progress ${o.last_progress_tick})`];
      lines.push(`survivors: ${o.survivors.map(p => this.name(p)).join(', ') || 'none'}`);
      if (rev.score) lines.push(rev.score.entries.map(e => `${e.side_id.kind === 'player' ? this.name(e.side_id.player_id) : e.side_id.team_id}: +${e.raw_delta} → ${e.raw_total} (adjusted ${e.adjusted_total.toFixed(2)})`).join(' · '));
      if (this.phase) lines.push(`committed: ${this.committed.map(p => this.name(p)).join(', ') || 'nobody yet'}`);
      if (this.finished) lines.push(this.finished);
      result.replaceChildren(); for (const line of lines) {const row=document.createElement('div');row.textContent=line.replace(/<\/?b>/g,'');result.append(row);}
    }
    if (this.replayErrors.has(this.current)) {
      const banner = $('outcome-banner');
      banner.dataset.outcome = 'loss';
      banner.textContent = /mismatch/i.test(this.replayErrors.get(this.current)!) ? 'REPLAY MISMATCH · Local replay differs from the controller. Orders disabled. Restart the peripheral, then refresh.' : 'REPLAY UNAVAILABLE · Loading failed. Orders disabled. Reconnect and refresh.';
      result.textContent = this.replayErrors.get(this.current)!;
    }
    // Selection panel.
    const body = $('selection-body');
    const views = this.selectedViews();
    $('selection').classList.toggle('factory-selected', views.length === 1 && !!this.types.get(views[0].type_key)?.production);

    if (!views.length) {
      body.innerHTML = this.finished ? '<p>Match ended. Select units, seek the timeline or open Statistics to inspect the replay.</p>' : this.spectator ? '<span class="muted">Spectating. Click or drag to inspect units, or seek any replay tick.</span>' : '<p>Select units to see their stats and orders.</p><p>Start an army: constructor → B Factory → C Construct.</p><a href="/guide/" target="_blank">How to play ↗</a>';
    } else {
      body.replaceChildren();
      const icons=document.createElement('div'); icons.id='selection-icons';
      const counts=new Map<string,{v:EntityView,count:number}>();
      for(const v of views){const key=`${v.owner}:${v.type_key}`;const entry=counts.get(key);if(entry)entry.count++;else counts.set(key,{v,count:1});}
      for(const {v,count} of counts.values()){const chip=document.createElement('span');chip.className='unit-chip';chip.title=`${count} ${v.type_key}`;chip.append(unitIcon(v.type_key,this.color(v.owner)),document.createTextNode(`×${count}`));icons.append(chip);}
      body.append(icons);
      if(views.length===1){const v=views[0],t=this.types.get(v.type_key)!;const title=document.createElement('strong');title.textContent=`${v.type_key} · ${v.lifecycle==='site'?'Under construction':v.lifecycle==='blueprint'?'Planned':this.name(v.owner)}`;body.append(title);
        const stats=document.createElement('div');stats.id='unit-stats';stats.className='unit-stats';
        const values=[['Health',v.blueprint ? `${t.max_hp} when built` : `${v.hp.toFixed(0)} / ${v.maxHp.toFixed(0)}`],['Cost',`${t.matter_cost} matter`],['Vision',`${t.vision} tiles`]];
        if(t.weapon)values.push(['Damage',`${t.weapon.damage} / ${t.weapon.cooldown} ticks`],['Range',`${t.weapon.range} tiles`]);
        if(t.movement)values.push(['Move',`1 tile / ${t.movement.cooldown} ticks`]);
        if(t.mining)values.push(['Mining',`${t.mining.rate} / ${t.mining.cooldown} ticks`]);
        if(t.construction)values.push(['Build',`${t.construction.rate} / ${t.construction.cooldown} ticks`]);
        for(const [label,value] of values){const item=document.createElement('span');item.textContent=`${label}: ${value}`;stats.append(item);}if(t.production){const details=document.createElement('details');details.innerHTML='<summary>Unit stats</summary>';details.append(stats);body.append(details);}else { body.append(stats); const order=document.createElement('div');order.className='unit-order';order.textContent=`Order: ${orderLabel(this.effectiveOrder(v) ?? {kind:'idle'})}`;body.append(order); }
      } else {const label=document.createElement('strong');label.textContent=`${views.length} units selected`;body.append(label);}
    }
    // Draft panel.
    const list = $('draft-list');
    list.innerHTML = '';
    this.draft.commands.forEach((c, i) => {
      const li = document.createElement('li');
      li.textContent = describe(c);
      if (this.draft.selected === i) li.classList.add('selected');
      li.onclick = () => {
        this.draft.selected = this.draft.selected === i ? null : i;
        this.updatePanels();
      };
      list.append(li);
    });
    $('draft-title').textContent = `Plan${this.draft.tick !== null ? ` · tick ${this.draft.tick}` : ''} · ${this.draft.commands.length} changes`;
    const commit = $<HTMLButtonElement>('commit');
    const canCommit = this.net.connected && !this.spectator && !!this.phase && this.phase.revision === this.current && !this.committed.includes(this.player ?? -1) && !this.finished;
    const commitTick = this.draft.tick ?? Math.floor(this.playhead);
    commit.disabled = !canCommit || this.current !== this.latest || commitTick < this.editableFrom || commitTick > this.availableThrough;
    commit.textContent = this.draft.commands.length ? `Commit turn (${this.draft.commands.length} at tick ${this.draft.tick})` : `Pass turn (tick ${Math.floor(this.playhead)})`;
    if (this.committed.includes(this.player ?? -1)) {
      const waiting = !!this.phase && this.phase.revision === this.latest && !this.progress && this.committed.length < this.config.player_count && !this.finished;
      commit.textContent = waiting ? 'Uncommit · edit my moves' : 'Turn started';
      commit.disabled = !waiting || !this.net.connected || this.spectator;
    }
    if (this.finished) { commit.textContent = 'Match ended'; commit.disabled = true; }
    if (this.turnRequestPending) commit.disabled = true;
  }

  toast(text: string): void {
    const el = $('toast');
    el.textContent = `${this.stored && this.mode.kind !== 'stored' ? 'Stored order · ' : ''}${text}`;
    el.classList.add('active');
    clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => el.classList.remove('active'), 4000);
  }

  buildHelp(): void {
    const rows: [string, string][] = [
      ['Left click / drag', 'Select / box select (Shift adds)'], ['0–9', 'Recall control group'], ['WASD, middle drag, wheel', 'Pan / zoom; Shift+wheel pans'],
      ['F then click', 'Attack-move'], ['G then click ally', 'Support'], ['M then drag', 'Mine area'], ['C then drag', 'Construct area'],
      ['B then 1/2/3 then click', 'Place factory / turret / wall (R rotates output)'], ['X', 'Idle'], ['O', 'Cycle future-order policy'],
      ['P then 1/2/3', 'Priority high / medium / low'], ['Q then 1–7', 'Queue a unit at selected factories'], ['L', 'Toggle factory loop'],
      ['H then digit / Clear group', 'Add members / empty a group'], ['J then digit / Backspace', 'Bind / clear factory output group'], ['Factory + F/G/M/C/X', 'Starting order for newborns'], ['V', 'Statistics graphs'], ['- / =, Shift+[ / Shift+]', 'Timeline zoom / pan'], ['Delete', 'Remove selected draft / blueprint / queue item'], ['Ctrl+Z / Ctrl+U', 'Undo / redo draft'],
      ['Enter', 'Commit / pass'], ['Space', 'Play / pause'], [', .', 'Step one tick'], ['< >', 'Jump 100 ticks'], ['Home / End', 'Editable start / end'],
      ['[ ]', 'Playback speed'], ['T', 'Return to draft tick'], ['Esc', 'Cancel mode, then clear selection'], ['?', 'This help'],
    ];
    $('help').innerHTML = `<b>Hotkeys</b><table>${rows.map(([k, v]) => `<tr><td><kbd>${k}</kbd></td><td>${v}</td></tr>`).join('')}</table>`;
  }
}

function describe(c: DraftCommand): string {
  const cmd = c.command;
  const policy = ` [${c.future_orders}]`;
  switch (cmd.kind) {
    case 'assign_order': return `Selected units only: ${cmd.order.kind} × ${cmd.entities.length}${policy}`;
    case 'assign_group_order': return `group ${cmd.group.slot}: ${cmd.order.kind} (members + future spawns)${policy}`;
    case 'place_blueprints': return `place ${cmd.type_key} at ${cmd.tiles.map(t => `${t.x},${t.y}`).join(' ')}`;
    case 'edit_production': return `queue ${cmd.edit.kind}${'items' in cmd.edit ? ` ${cmd.edit.items.join(',')}` : ''} × ${cmd.factories.length}`;
    case 'set_queue_loop': return `loop ${cmd.enabled ? 'on' : 'off'} × ${cmd.factories.length}`;
    case 'set_stored_order': return `stored ${cmd.order.kind} × ${cmd.factories.length}`;
    case 'configure_blueprints': return `Factory plan: ${cmd.settings.queue.length} queued · ${cmd.settings.priority} · loop ${cmd.settings.loop_enabled?'on':'off'} · ${cmd.settings.order.kind.replace('_',' ')}`;
    case 'set_priority': return `priority ${cmd.priority} × ${cmd.entities.length}`;
    default: return cmd.kind;
  }
}
