import type {
  Command, Content, DraftCommand, EntityId, EntityRef, FutureOrderPolicy, LobbyState, MatchConfig, Order, Outcome, Priority, RoundScore, Sample, ServerMessage, Tile, TimelineBucket,
  TypeDefinition, WorldEvent, WorldState,
} from './contracts.generated';
import type { Net } from './net';
import type { Session } from './lobby';
import { Renderer } from './render';

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;
const CHUNK = 1000;
const RATES = [0.25, 0.5, 1, 2, 4, 8];
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
  | { kind: 'stored-target'; order: 'attack' };

interface Draft {
  tick: number | null;
  commands: DraftCommand[];
  undo: DraftCommand[][];
  redo: DraftCommand[][];
  policy: FutureOrderPolicy;
  selected: number | null;
}

export class Game {
  readonly renderer: Renderer;
  content!: Content;
  types = new Map<string, TypeDefinition>();
  revisions = new Map<number, RevisionView>();
  current = -1;
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
    return this.profiles[owner]?.profile?.color ?? ['#4fc3f7', '#ff8a65', '#aed581', '#ce93d8'][owner % 4];
  }
  name(owner: number): string {
    return this.profiles[owner]?.profile?.username ?? `Player ${owner}`;
  }

  async start(): Promise<void> {
    this.content = await (await fetch('/guide/content.json')).json();
    for (const t of this.content.types) this.types.set(t.key, t);
    this.net.on('revision_published', m => this.onPublished(m));
    this.net.on('planning_opened', m => {
      this.phase = m;
      this.round = m.round;
      this.editableFrom = m.editable_from;
      this.availableThrough = m.available_through;
      this.committed = m.committed_players;
      this.progress = null;
      if (this.draft.tick !== null && this.current !== m.revision) this.clearDraft();
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
    this.bindInput();
    this.buildHelp();
    requestAnimationFrame(t => this.frame(t));
  }

  // ---- revisions and data ------------------------------------------------------------------

  async onPublished(m: ServerMessage & { kind: 'revision_published' }): Promise<void> {
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
    this.view = { t0: 0, t1: Math.max(1, m.outcome.terminal_state_tick) };
    this.playhead = Math.min(this.playhead, m.outcome.terminal_state_tick);
    this.exact = this.exact && this.exact.revision === m.revision ? this.exact : null;
    this.requestExact();
    void this.ensureChunk(Math.floor(this.playhead / CHUNK));
    this.updatePanels();
    if (this.player !== null && this.phase && this.phase.revision === m.revision) this.net.send({ kind: 'planning_ready', round: this.phase.round, revision: m.revision });
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
    if (!rev || rev.chunks.has(k) || rev.loading.has(k) || k * CHUNK > rev.outcome.terminal_state_tick) return;
    rev.loading.add(k);
    const from = k * CHUNK;
    const to = Math.min(from + CHUNK - 1, rev.outcome.terminal_state_tick);
    const t0 = performance.now();
    try {
      const range = await this.net.request({ kind: 'get_snapshot_range', revision: rev.revision, from_tick: from, to_tick: to, stride: this.config.snapshot_interval }, 'snapshot_range', s => s.revision === rev.revision && (s.samples.length === 0 || s.samples[0].tick >= from));
      rev.dictionary = range.entity_dictionary;
      for (const s of range.samples) rev.samples.set(s.tick, s);
      const events = await this.net.request({ kind: 'get_events', revision: rev.revision, from_tick: from, to_tick: to }, 'events', e => e.revision === rev.revision);
      rev.events.push(...events.events);
      rev.chunks.add(k);
      console.log(`chunk ${k} of revision ${rev.revision}: ${range.samples.length} samples, ${events.events.length} events in ${Math.round(performance.now() - t0)} ms`);
    } finally {
      rev.loading.delete(k);
    }
  }

  requestExact(): void {
    const rev = this.rev();
    if (!rev) return;
    const tick = Math.min(Math.floor(this.playhead), rev.outcome.terminal_state_tick);
    if (this.exact && this.exact.revision === rev.revision && this.exact.tick === tick) return;
    if (this.exactPending && this.exactPending.revision === rev.revision && this.exactPending.tick === tick) return;
    this.exactPending = { revision: rev.revision, tick };
    const t0 = performance.now();
    this.net
      .request({ kind: 'get_exact_state', revision: rev.revision, tick }, 'exact_state', e => e.revision === rev.revision && e.tick === tick)
      .then(e => {
        if (this.exactPending?.tick === tick) this.exactPending = null;
        if (Math.floor(this.playhead) === tick && this.current === rev.revision) {
          this.exact = { revision: rev.revision, tick, state: e.snapshot };
          if (this.player !== null && this.phase && this.phase.revision === rev.revision) this.net.send({ kind: 'planning_ready', round: this.phase.round, revision: rev.revision });
          this.updatePanels();
        }
        $('top-exact').textContent = `exact ${tick} in ${Math.round(performance.now() - t0)} ms`;
      })
      .catch(err => {
        this.exactPending = null;
        this.toast(String(err.message ?? err));
      });
  }

  /** Entities to draw at the playhead: exact state when available, otherwise interpolated samples. */
  entities(): EntityView[] {
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
    return rev.events.filter(e => e.tick <= tick && e.tick > tick - lookback);
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
    this.draft.undo.push([...this.draft.commands]);
    this.draft.redo = [];
    this.draft.tick = Math.floor(this.playhead);
    this.draft.commands.push({ local_id: `d${Date.now()}-${this.draft.commands.length}`, command: command as DraftCommand['command'], future_orders: policy });
    this.updatePanels();
  }

  undo(): void {
    const previous = this.draft.undo.pop();
    if (!previous) return;
    this.draft.redo.push([...this.draft.commands]);
    this.draft.commands = previous;
    if (!this.draft.commands.length) this.draft.tick = null;
    this.updatePanels();
  }
  redo(): void {
    const next = this.draft.redo.pop();
    if (!next) return;
    this.draft.undo.push([...this.draft.commands]);
    this.draft.commands = next;
    if (this.draft.commands.length) this.draft.tick = Math.floor(this.playhead);
    this.updatePanels();
  }
  clearDraft(): void {
    this.draft = { tick: null, commands: [], undo: [], redo: [], policy: this.draft.policy, selected: null };
    this.updatePanels();
  }

  selectedIds(filter?: (t: TypeDefinition) => boolean): EntityId[] {
    return this.selectedViews()
      .filter(e => this.ownSelectable(e) && e.lifecycle === 'complete' && (!filter || filter(this.types.get(e.type_key)!)))
      .map(e => e.id)
      .sort((a, b) => idKey(a).localeCompare(idKey(b)));
  }

  assign(order: Order, filter: (t: TypeDefinition) => boolean): void {
    if (this.recalledGroup !== null && this.player !== null) {
      this.stage({ kind: 'assign_group_order', group: { owner: this.player, slot: this.recalledGroup }, order }, this.draft.policy);
      return;
    }
    const entities = this.selectedIds(filter);
    if (!entities.length) {
      this.toast('No selected unit can take that order.');
      return;
    }
    this.stage({ kind: 'assign_order', entities, order }, this.draft.policy);
  }

  async commit(): Promise<void> {
    if (this.spectator || !this.session.token || !this.phase) return;
    if (this.committed.includes(this.player ?? -1)) return;
    const tick = this.draft.tick ?? Math.floor(this.playhead);
    if (tick < this.editableFrom || tick > this.availableThrough) {
      this.toast(`Pass tick must be within ${this.editableFrom}–${this.availableThrough}.`);
      return;
    }
    const request_id = `${this.player}-${this.round}-${Date.now()}`;
    try {
      await this.net.request({ kind: 'commit', request: { request_id, slot_token: this.session.token, draft: { based_on_revision: this.current, tick, commands: this.draft.commands } } }, 'commit_accepted', m => m.request_id === request_id);
      this.committed.push(this.player!);
      this.clearDraft();
      this.toast('Turn committed. Waiting for the other players…');
    } catch (err) {
      this.toast(`Commit rejected: ${(err as Error).message}`);
    }
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
      const tile = this.renderer.tileAt(e.clientX, e.clientY);
      if (this.mode.kind === 'attack' || this.mode.kind === 'stored-target') {
        this.applyTileMode(tile);
        return;
      }
      if (this.mode.kind === 'place') {
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
      this.hover = this.renderer.tileAt(e.clientX, e.clientY);
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
      this.renderer.zoomAt(e.clientX, e.clientY, e.deltaY < 0 ? 1.15 : 1 / 1.15);
    }, { passive: false });
    $<HTMLCanvasElement>('minimap').addEventListener('mousedown', e => {
      const t = this.renderer.minimapTile(e.clientX, e.clientY);
      if (t) this.renderer.centerOn(t.x, t.y);
    });
    const timeline = $<HTMLCanvasElement>('timeline');
    timeline.addEventListener('mousedown', e => this.seek(this.renderer.timelineTick(e.clientX)));
    timeline.addEventListener('wheel', e => {
      e.preventDefault();
      const at = this.renderer.timelineTick(e.clientX);
      const factor = e.deltaY < 0 ? 0.8 : 1.25;
      const rev = this.rev();
      const end = rev ? rev.outcome.terminal_state_tick : 1;
      let t0 = at - (at - this.view.t0) * factor;
      let t1 = at + (this.view.t1 - at) * factor;
      t0 = Math.max(0, t0);
      t1 = Math.min(end, Math.max(t0 + 10, t1));
      this.view = { t0, t1 };
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
    if (this.mode.kind === 'stored-target') {
      const factories = this.selectedIds(t => !!t.production);
      if (factories.length) this.stage({ kind: 'set_stored_order', factories, order: { kind: 'attack_move', destination: tile } });
      else this.toast('Select a factory first.');
    }
    this.mode = { kind: 'none' };
    this.updateMode();
  }

  placeAt(tile: Tile): void {
    if (this.mode.kind !== 'place') return;
    const type_key = this.mode.type_key;
    const def = this.types.get(type_key);
    const command: Command = { kind: 'place_blueprints', type_key, tiles: [tile], priority: 'medium', output_directions: def?.production ? [this.renderer.outputDirection] : null };
    this.stage(command);
    this.mode = { kind: 'none' };
    this.updateMode();
  }

  producible(): string[] {
    return [...this.types.values()].filter(t => t.kind === 'unit').map(t => t.key).sort();
  }
  structures(): string[] {
    return ['factory', 'turret', 'wall'].filter(k => this.types.has(k)).concat([...this.types.values()].filter(t => t.kind === 'structure' && !['factory', 'turret', 'wall'].includes(t.key)).map(t => t.key));
  }

  key(e: KeyboardEvent): void {
    const target = e.target as HTMLElement;
    if (target && (target.tagName === 'INPUT' || target.tagName === 'SELECT' || target.tagName === 'TEXTAREA')) {
      if (e.key === 'Escape') target.blur();
      return;
    }
    const k = e.key;
    if (e.ctrlKey || e.metaKey) {
      if (k.toLowerCase() === 'z' && e.shiftKey) this.redo();
      else if (k.toLowerCase() === 'z') this.undo();
      else if (k.toLowerCase() === 'y') this.redo();
      else return;
      e.preventDefault();
      return;
    }
    if (k === 'Escape') {
      if ($('help').classList.contains('active')) $('help').classList.remove('active');
      else if (this.mode.kind !== 'none') this.mode = { kind: 'none' };
      else {
        this.selection.clear();
        this.recalledGroup = null;
      }
      this.updateMode();
      this.updatePanels();
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
      case 'r': this.mode = { kind: 'stored' }; break;
      case 'z': if (this.mode.kind === 'place') this.renderer.cycleOutput(); break;
      case 'x': this.assign({ kind: 'idle' }, () => true); break;
      case 'l': {
        const factories = this.selectedIds(t => !!t.production);
        if (factories.length) this.stage({ kind: 'set_queue_loop', factories, enabled: !this.selectedViews().some(v => v.exact?.production?.loop_enabled) });
        else this.toast('Select a factory first.');
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
        if (this.draft.selected !== null && this.draft.commands[this.draft.selected]) {
          this.draft.undo.push([...this.draft.commands]);
          this.draft.commands.splice(this.draft.selected, 1);
          this.draft.selected = null;
          if (!this.draft.commands.length) this.draft.tick = null;
          this.updatePanels();
        }
        break;
      }
      case 'Enter': void this.commit(); break;
      case ' ': this.togglePlay(); e.preventDefault(); break;
      case ',': this.seek(this.playhead - 1); break;
      case '.': this.seek(this.playhead + 1); break;
      case '<': this.seek(this.playhead - 100); break;
      case '>': this.seek(this.playhead + 100); break;
      case 'Home': this.seek(this.editableFrom); break;
      case 'End': this.seek(this.availableThrough); break;
      case '[': this.rate = RATES[Math.max(0, RATES.indexOf(this.rate) - 1)]; this.updatePanels(); break;
      case ']': this.rate = RATES[Math.min(RATES.length - 1, RATES.indexOf(this.rate) + 1)]; this.updatePanels(); break;
      case 't': if (this.draft.tick !== null) this.seek(this.draft.tick); break;
      default: return;
    }
    this.updateMode();
  }

  digit(n: number): void {
    const mode = this.mode;
    if (mode.kind === 'build') {
      const key = this.structures()[n - 1];
      if (key) this.mode = { kind: 'place', type_key: key };
      else this.mode = { kind: 'none' };
    } else if (mode.kind === 'recipe') {
      const key = this.producible()[n - 1];
      const factories = this.selectedIds(t => !!t.production);
      if (key && factories.length) this.stage({ kind: 'edit_production', factories, edit: { kind: 'append', items: [key] } });
      else this.toast(key ? 'Select a factory first.' : 'No such recipe.');
      this.mode = { kind: 'none' };
    } else if (mode.kind === 'priority') {
      const priority = (['high', 'medium', 'low'] as Priority[])[n - 1];
      const entities = this.selectedIds();
      if (priority && entities.length) this.stage({ kind: 'set_priority', entities, priority });
      this.mode = { kind: 'none' };
    } else if (mode.kind === 'stored') {
      this.mode = { kind: 'none' };
    } else {
      // Recall a control group from exact state; membership is simulation state.
      const group = this.exact?.state.control_groups.find(g => g.id.owner === this.player && g.id.slot === n);
      this.selection.clear();
      this.recalledGroup = null;
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
        case 'place': return `Place ${this.mode.type_key}: click a floor tile${this.types.get(this.mode.type_key)?.production ? ` (output ${this.renderer.outputDirection.toUpperCase()}, Z cycles)` : ''}`;
        case 'recipe': return `Queue: ${this.producible().map((k, i) => `${i + 1}=${k}`).join('  ')}`;
        case 'priority': return 'Priority: 1=high 2=medium 3=low';
        case 'stored': return 'Stored order for selected factories: F then click destination';
        case 'stored-target': return 'Stored attack-move: click a destination tile';
      }
    })();
    el.textContent = text;
    el.classList.toggle('active', text !== '');
  }

  updateTop(): void {
    const rev = this.rev();
    const phase = this.finished ? this.finished : this.committed.includes(this.player ?? -1) ? `Round ${this.round}: committed, waiting` : this.phase && this.phase.revision === this.current ? `Round ${this.round}: planning` : this.progress ? `Simulating ${this.progress.tick}/${this.progress.end}` : 'Simulating…';
    $('top-phase').innerHTML = `<b>${this.spectator ? 'Spectator' : this.name(this.player!)}</b> · ${phase} · revision ${this.current}`;
    $('top-tick').innerHTML = `tick <b>${Math.floor(this.playhead)}</b> / ${rev?.outcome.terminal_state_tick ?? 0} · editable ${this.editableFrom}–${this.availableThrough}`;
    const sample = this.sampleAt(this.playhead);
    const banks = this.exact && this.exact.tick === Math.floor(this.playhead) && this.exact.revision === this.current
      ? this.exact.state.players.map(p => `${this.name(p.player_id)} ${p.bank.toFixed(0)}${p.currently_eliminated ? ' (eliminated)' : ''}`)
      : sample?.players.map(p => `${this.name(p.player_id)} ${p.bank.toFixed(0)}${p.currently_eliminated ? ' (eliminated)' : ''}`) ?? [];
    $('top-bank').textContent = `bank: ${banks.join(' · ')}`;
    const score = rev?.score?.entries.map(e => `${e.side_id.kind === 'player' ? this.name(e.side_id.player_id) : e.side_id.team_id} ${e.raw_total}`).join(' · ');
    $('top-score').textContent = score ? `score: ${score}` : '';
  }

  updatePanels(): void {
    this.updateTop();
    const rev = this.rev();
    $('play').textContent = this.playing ? 'Pause' : 'Play';
    $('rate').textContent = `${this.rate}×`;
    $<HTMLInputElement>('tick-input').value = String(Math.floor(this.playhead));
    $('timeline-info').textContent = rev ? `${rev.outcome.kind} · ${rev.outcome.stop_reason.replace('_', ' ')} at ${rev.outcome.terminal_state_tick}` : '';
    // Result panel.
    const result = $('result');
    if (rev) {
      const o = rev.outcome;
      const lines = [`<b>Revision ${rev.revision}</b>: ${o.kind} (${o.stop_reason.replace('_', ' ')} at tick ${o.terminal_state_tick}, last progress ${o.last_progress_tick})`];
      lines.push(`survivors: ${o.survivors.map(p => this.name(p)).join(', ') || 'none'}`);
      if (rev.score) lines.push(rev.score.entries.map(e => `${e.side_id.kind === 'player' ? this.name(e.side_id.player_id) : e.side_id.team_id}: +${e.raw_delta} → ${e.raw_total}`).join(' · '));
      if (this.phase) lines.push(`committed: ${this.committed.map(p => this.name(p)).join(', ') || 'nobody yet'}`);
      if (this.finished) lines.push(`<b>${this.finished}</b>`);
      result.innerHTML = lines.join('<br>');
    }
    // Selection panel.
    const body = $('selection-body');
    const views = this.selectedViews();
    if (!views.length) {
      body.innerHTML = '<span class="muted">Nothing selected. Click or drag on the map. Press ? for hotkeys.</span>';
    } else {
      const rows = views.slice(0, 12).map(v => {
        const t = this.types.get(v.type_key);
        let extra = '';
        const e = v.exact;
        if (e) {
          extra = ` · ${e.action.kind}${e.action.kind === 'attack_move' ? ` → ${e.action.destination.x},${e.action.destination.y}` : ''} · ${e.priority}`;
          if (e.production) {
            const p = e.production;
            const active = p.active_item ? `${p.active_item.type_key} ${p.active_item.paid_matter.toFixed(0)}/${this.types.get(p.active_item.type_key)?.matter_cost}${p.active_item.awaiting_output ? ' (output blocked)' : ''}` : 'idle';
            extra += `<br>queue: ${active}; pending ${p.pending_items.map(i => i.type_key).join(', ') || '—'}; loop ${p.loop_enabled ? 'on' : 'off'}; stored ${p.stored_order.kind}; output ${p.output_tile.x},${p.output_tile.y}`;
          }
          if (t?.mining && v.type_key === 'constructor') extra += ' · mines at half rate';
        }
        return `<div><span class="swatch" style="background:${this.color(v.owner)}"></span> ${v.type_key} ${v.lifecycle === 'site' ? '(site)' : ''} hp ${v.hp.toFixed(0)}/${v.maxHp.toFixed(0)} @${Math.round(v.x)},${Math.round(v.y)}${extra}</div>`;
      });
      if (views.length > 12) rows.push(`<div class="muted">…and ${views.length - 12} more</div>`);
      if (this.recalledGroup !== null) rows.unshift(`<div><b>Group ${this.recalledGroup}</b>: orders go to current members + future spawns</div>`);
      body.innerHTML = rows.join('');
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
    const gate = this.canStage();
    $('draft-title').textContent = `Draft${this.draft.tick !== null ? ` @ tick ${this.draft.tick}` : ''} · policy ${this.draft.policy}${gate.ok ? '' : ` · ${gate.reason}`}`;
    const commit = $<HTMLButtonElement>('commit');
    const canCommit = !this.spectator && !!this.phase && this.phase.revision === this.current && !this.committed.includes(this.player ?? -1) && !this.finished;
    commit.disabled = !canCommit;
    commit.textContent = this.draft.commands.length ? `Commit turn (${this.draft.commands.length} at tick ${this.draft.tick})` : `Pass turn (tick ${Math.floor(this.playhead)})`;
  }

  toast(text: string): void {
    const el = $('toast');
    el.textContent = text;
    el.classList.add('active');
    clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => el.classList.remove('active'), 4000);
  }

  buildHelp(): void {
    const rows: [string, string][] = [
      ['Left click / drag', 'Select / box select (Shift adds)'], ['0–9', 'Recall control group'], ['WASD, middle drag, wheel', 'Pan / zoom'],
      ['F then click', 'Attack-move'], ['G then click ally', 'Support'], ['M then drag', 'Mine area'], ['C then drag', 'Construct area'],
      ['B then 1/2/3 then click', 'Place factory / turret / wall (Z cycles output)'], ['X', 'Idle'], ['O', 'Cycle future-order policy'],
      ['P then 1/2/3', 'Priority high / medium / low'], ['Q then 1–7', 'Queue a unit at selected factories'], ['L', 'Toggle factory loop'],
      ['R then F then click', 'Stored attack-move for newborns'], ['Delete', 'Remove selected draft command'], ['Ctrl+Z / Ctrl+Shift+Z', 'Undo / redo draft'],
      ['Enter', 'Commit / pass'], ['Space', 'Play / pause'], [', .', 'Step one tick'], ['< >', 'Jump 100 ticks'], ['Home / End', 'Editable start / end'],
      ['[ ]', 'Playback speed'], ['T', 'Return to draft tick'], ['Esc', 'Cancel mode, then clear selection'], ['?', 'This help'],
    ];
    $('help').innerHTML = `<b>Hotkeys</b><table>${rows.map(([k, v]) => `<tr><td><kbd>${k}</kbd></td><td>${v}</td></tr>`).join('')}</table>`;
  }
}

function describe(c: DraftCommand): string {
  const cmd = c.command;
  const policy = c.future_orders === 'keep' ? '' : ` [${c.future_orders}]`;
  switch (cmd.kind) {
    case 'assign_order': return `${cmd.order.kind} × ${cmd.entities.length}${policy}`;
    case 'assign_group_order': return `group ${cmd.group.slot}: ${cmd.order.kind}${policy}`;
    case 'place_blueprints': return `place ${cmd.type_key} at ${cmd.tiles.map(t => `${t.x},${t.y}`).join(' ')}`;
    case 'edit_production': return `queue ${cmd.edit.kind}${'items' in cmd.edit ? ` ${cmd.edit.items.join(',')}` : ''} × ${cmd.factories.length}`;
    case 'set_queue_loop': return `loop ${cmd.enabled ? 'on' : 'off'} × ${cmd.factories.length}`;
    case 'set_stored_order': return `stored ${cmd.order.kind} × ${cmd.factories.length}`;
    case 'set_priority': return `priority ${cmd.priority} × ${cmd.entities.length}`;
    default: return cmd.kind;
  }
}
