import {packDraft, unpackDraft, type Draft} from './draft-storage';
import {practice} from './practice';
import {PlacementPlan} from './placement';
import {GroundVision, SightMemory, playerSeesEntity, playerVisibility} from './vision';
import {guideUrl} from './session';
import {updateOrderWarnings} from './order-warnings';
import {updateRematch} from './rematch';
import {missileName, queueLaunch} from './silo';
import { unitIcon } from './icons';
import { factoryPlan, factorySignature, orderLabel } from './factory';
import type {
  Activity, CardinalDirection, ReplayFrontier, BlueprintSettings, DraftItemRef, Command, Content, DraftCommand, EntityId, EntityRef, FutureOrderPolicy, LobbyState, MatchConfig, Order, Outcome, Priority, RoundScore, Sample, ServerMessage, Tile, TimelineBucket,
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

// Mirrors the exact-state activity projected by atemporal_sim::output::activity_of.
function activityOf(entity: WorldState['entities'][number]): Activity {
  const kind = entity.action.kind;
  if (kind === 'mine') return 'mining';
  if (kind === 'construct') return 'construction';
  if (entity.engaged_target) return 'combat';
  if ((kind === 'attack_move' || kind === 'support') && !entity.goal_settled) return 'movement';
  return 'idle';
}

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
  remembered?: boolean;
  settings?: BlueprintSettings;
  outputDirection?: CardinalDirection;
  silo?: import('./contracts.generated').SiloState | null;
  exact?: WorldState['entities'][number];
}

type Mode =
  | { kind: 'none' }
  | { kind: 'attack' }
  | { kind: 'launch'; type_key: string; silos: EntityId[] }
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
  preview: ReplayFrontier | null = null;
  previewView: RevisionView | null = null;
  private previewBusy = false;
  private previewSourceReady = true;
  private previewFollowTick: number | null = null;
  private previewRequest = 0;
  private previewCoverage = new Map<number, number>();
  get viewingPreview(): boolean { return !!this.preview && this.current === this.preview.revision; }

  playhead = 0;
  playing = false;
  rate = 1;
  exact: { revision: number; tick: number; state: WorldState } | null = null;
  exactPending: { revision: number; tick: number } | null = null;
  selection = new Set<string>();
  recalledGroup: number | null = null;
  mode: Mode = { kind: 'none' };
  draft: Draft = { tick: null, commands: [], undo: [], redo: [], policy: 'keep', selected: null };
  // Uncommitted orders survive a reload (e.g. a server update restart) in this tab.
  draftRestored = false;
  get draftKey(): string { return `atemporal-draft:${location.pathname}:${this.net.matchId}:${this.player}`; }
  drag: { x0: number; y0: number; x1: number; y1: number } | null = null;
  hover: Tile | null = null;
  profiles: LobbyState['slots'] = [];
  view = { t0: 0, t1: 1 };
  finished: string | null = null;
  toastTimer = 0;
  lastFrame = performance.now();
  lastTop = 0;
  planningSince = performance.now();
  private waitingNoticeCompact = false;

  constructor(readonly net: Net, readonly config: MatchConfig, readonly session: Session, lobby: LobbyState) {
    this.updateLobby(lobby);
    const noticeKey = `atemporal-waiting-notice:${location.pathname}:${this.net.matchId}`;
    try { this.waitingNoticeCompact = sessionStorage.getItem(noticeKey) === 'compact'; } catch {}
    $('turn-banner-toggle').onclick = () => {
      this.waitingNoticeCompact = !this.waitingNoticeCompact;
      try { sessionStorage.setItem(noticeKey, this.waitingNoticeCompact ? 'compact' : 'expanded'); } catch {}
      this.updateTop();
    };
    this.renderer = new Renderer($('map'), $('minimap'), $('timeline'), this);
    $('game').classList.add('active');
    $('lobby').style.display = 'none';
  }

  private updateLobby(lobby: LobbyState): void {
    this.profiles = lobby.slots;
    if (!lobby.available_teams.length) return;
    const assignments = lobby.slots.flatMap(({slot, profile}) =>
      profile?.team_id == null ? [] : [{player_id:slot, team_id:profile.team_id}],
    );
    if (assignments.length === this.config.player_count) {
      this.config.multiplayer = {kind:'teams', assignments};
      this.visionCache = null;
    }
  }

  get player(): number | null {
    return this.session.slot;
  }
  get spectator(): boolean {
    return this.session.slot === null;
  }
  get fullVision(): boolean { return this.spectator || this.finished !== null; }
  canInspectPlan(owner: number): boolean { return this.fullVision || owner === this.player; }
  color(owner: number): string {
    const team = this.profiles[owner]?.profile?.team_id;
    if (team) { const first = this.profiles.find(s => s.profile?.team_id === team); if (first?.profile) return first.profile.color; }
    return this.profiles[owner]?.profile?.color ?? ['#4fc3f7', '#ff8a65', '#aed581', '#ce93d8'][owner % 4];
  }
  name(owner: number): string {
    return this.profiles[owner]?.profile?.username ?? `Player ${owner}`;
  }

  async start(): Promise<void> {
    $('game').style.setProperty('--timeline-height',`${Math.max(38,this.config.player_count*12+14)}px`);
    this.content = await (await fetch(`${guideUrl}content.json`)).json();
    for (const t of this.content.types) this.types.set(t.key, t);
    this.net.on('revision_published', m => { void this.onPublished(m).catch(error => this.failReplay(m.revision, error)); });
    this.net.on('planning_opened', m => {
      if(this.preview && m.revision<this.preview.revision)this.clearPreview();
      if (this.phase?.round !== m.round) this.planningSince = performance.now();
      this.phase = m;
      this.round = m.round;
      this.editableFrom = m.editable_from;
      this.availableThrough = m.available_through;
      this.committed = m.committed_players;
      this.progress = null;
      if (!this.draftRestored && this.player !== null) {
        try {
          const saved = JSON.parse(sessionStorage.getItem(this.draftKey) ?? 'null');
          if (saved?.round === m.round && !this.draft.commands.length) this.draft = unpackDraft(saved.draft);
        } catch { this.toast('Could not restore the saved draft in this tab.'); }
        this.draftRestored = true;
      }
      this.updatePanels();
      if (this.player !== null && this.exact) this.net.send({ kind: 'planning_ready', round: m.round, revision: m.revision });
    });
    this.net.on('round_inputs', m => { this.experience.turns.set(m.revision,[...(this.experience.turns.get(m.revision-1)??[]),...m.turns]); });
    this.net.on('replay_progress', m => this.onReplayProgress(m.preview ?? null));
    this.net.on('simulation_progress', m => {
      this.progress = { tick: m.tick, end: m.end_tick };
      this.updateTop();
    });
    this.net.on('match_finished', m => {
      this.finished = m.match_winners.length ? `Match winners: ${m.match_winners.map(s => (s.kind === 'player' ? this.name(s.player_id) : s.team_id)).join(', ')}` : `Match over: ${m.reason}`;
      this.updatePanels();
    });
    this.net.on('lobby_updated', m => {
      this.updateLobby(m.lobby);
      this.experience?.review.update();
    });
    this.net.on('welcome', m => {
      this.updateLobby(m.lobby);
      if(m.phase==='finished'||m.phase==='archived'){this.finished=m.phase==='archived'?'Match archived.':'Match complete.';this.updatePanels();}
      this.net.send({kind:'get_replay_progress'});
    });
    this.net.on('match_archived', () => {
      this.clearPreview();
      this.finished = 'Match stopped and archived (unfinished).';
      this.updatePanels();
    });
    this.experience = new Experience(this);
    this.bindInput();
    this.buildHelp();
    requestAnimationFrame(t => this.frame(t));
  }

  // ---- revisions and data ------------------------------------------------------------------

  onReplayProgress(frontier: ReplayFrontier | null): void {
    if (frontier && frontier.revision <= this.latest) return;
    if (!frontier) {
      // Completion may precede durable publication: retain the viewed frame during that handoff.
      if(this.preview){this.previewSourceReady=false;this.updatePreviewStatus();}
      return;
    }
    this.previewSourceReady=true;
    if(this.preview?.generation===frontier.generation && frontier.through_tick<this.preview.through_tick)return;
    const fresh = this.preview?.generation !== frontier.generation;
    this.preview = frontier;
    if (fresh) {
      this.previewCoverage.clear();
      this.previewView = {revision:frontier.revision,
        // Bounds only: provisional results are never presented as an outcome.
        outcome:{kind:'stalemate',stop_reason:'inactivity',terminal_state_tick:frontier.through_tick,last_progress_tick:0,survivors:[],eliminated:[],surviving_sides:[],survival_transitions:[]},
        timeline:[],score:null,dictionary:[],samples:new Map(),events:[],chunks:new Set(),loading:new Set()};
      this.current = frontier.revision; this.exact = null; this.exactPending = null;
      this.clearDraft(); $('toast').classList.remove('active');
      this.previewFollowTick ??= this.playhead;
      this.playhead = Math.min(this.previewFollowTick,frontier.through_tick);
      this.view = {t0:0,t1:Math.max(1,frontier.end_tick)};
    }
    this.previewView!.outcome.terminal_state_tick = frontier.through_tick;
    if(this.viewingPreview && this.previewFollowTick!==null && frontier.through_tick>=this.previewFollowTick){this.playhead=this.previewFollowTick;this.previewFollowTick=null;}
    if (this.viewingPreview) { void this.loadPreview(); if(fresh)this.updatePanels();else {this.updateTop();this.updatePreviewStatus();} }
  }

  clearPreview(): void {
    if(!this.preview)return;
    if(this.viewingPreview)this.current=this.latest;
    this.preview=null;this.previewView=null;this.previewCoverage.clear();
    if(this.previewFollowTick!==null){this.playhead=Math.min(this.previewFollowTick,this.rev()?.outcome.terminal_state_tick??0);this.previewFollowTick=null;}
    this.exact=null;this.updatePanels();this.requestExact();
  }

  async loadPreview(): Promise<void> {
    const frontier = this.preview, rev = this.previewView;
    if (!frontier || !rev || !this.previewSourceReady || !this.viewingPreview || this.previewBusy || !this.net.connected) return;
    const tick = Math.min(Math.floor(this.playhead),frontier.through_tick);
    const k = Math.floor(tick/CHUNK), from = k*CHUNK, to = Math.min(from+CHUNK-1,frontier.through_tick);
    const loaded = (this.previewCoverage.get(k) ?? -1) >= to;
    if (loaded && this.playing && tick < frontier.through_tick) return;
    if (loaded && this.exact?.revision===frontier.revision && this.exact.tick===tick) return;
    this.previewBusy = true;
    const request_id = `preview-${++this.previewRequest}`;
    try {
      const m = await this.net.request({kind:'get_replay_preview',request_id,generation:frontier.generation,tick,from_tick:loaded?tick:from,to_tick:loaded?tick:to},'replay_preview',m=>m.request_id===request_id);
      if (this.preview?.generation !== m.generation || this.previewView !== rev) return;
      rev.dictionary=m.entity_dictionary;
      for(const sample of m.samples) rev.samples.set(sample.tick,sample);
      if(!loaded) { rev.events=rev.events.filter(e=>e.tick<from||e.tick>to).concat(m.events);this.previewCoverage.set(k,to); }
      while(this.previewCoverage.size>8){const old=this.previewCoverage.keys().next().value!;this.previewCoverage.delete(old);for(const t of rev.samples.keys())if(Math.floor(t/CHUNK)===old)rev.samples.delete(t);rev.events=rev.events.filter(e=>Math.floor(e.tick/CHUNK)!==old);}
      if(!this.terrain){this.terrain=m.snapshot.terrain;this.initialOre=m.snapshot.ore;this.renderer.fit();const own=m.snapshot.entities.find(e=>e.owner===this.player);if(own)this.renderer.centerOn(own.tile.x,own.tile.y);}
      const changed=this.exact?.revision!==m.revision || this.exact.tick!==tick;
      if (!this.fullVision) this.recordSightSamples(rev, m.samples);
      if(this.viewingPreview && Math.floor(this.playhead)===tick) {
        this.exact={revision:m.revision,tick,state:m.snapshot};
        if (!this.fullVision) this.recordExactSightings(rev, m.snapshot);
      }
      if(changed)this.updatePanels();else this.updatePreviewStatus();
    } catch(error) {
      // Publication or retry can expire an in-flight provisional query.
      if(this.preview?.generation===frontier.generation) this.net.send({kind:'get_replay_progress'});
    } finally { this.previewBusy=false; }
  }

  async onPublished(m: ServerMessage & { kind: 'revision_published' }): Promise<void> {
    if (this.revisions.has(m.revision)) { this.latest = Math.max(this.latest,m.revision); return; }
    const finishingPreview = this.preview?.revision === m.revision;
    if (finishingPreview && this.previewFollowTick!==null){this.playhead=this.previewFollowTick;this.previewFollowTick=null;}
    if (finishingPreview) { this.preview=null;this.previewView=null;this.previewCoverage.clear(); }
    const wasFull = finishingPreview || this.view.t0 === 0 && this.view.t1 === this.rev()?.outcome.terminal_state_tick;
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
    if(!this.preview || m.revision>=this.preview.revision)this.current = m.revision;
    if(!this.viewingPreview)this.progress = null;
    if (!this.terrain) {
      const t0 = performance.now();
      const exact = await this.net.request({ kind: 'get_exact_state', revision: m.revision, tick: 0 }, 'exact_state', e => e.tick === 0 && e.revision === m.revision);
      const needsFit = !this.terrain;
      this.terrain = exact.snapshot.terrain;
      this.initialOre = exact.snapshot.ore;
      if(this.current===m.revision && !this.viewingPreview) {
        this.exact = { revision: m.revision, tick: 0, state: exact.snapshot };
        if (!this.fullVision) this.recordExactSightings(view, exact.snapshot);
      }
      console.log(`initial exact state in ${Math.round(performance.now() - t0)} ms`);
      if(needsFit){this.renderer.fit();const own=exact.snapshot.entities.find(e=>e.owner===this.player);if(own)this.renderer.centerOn(own.tile.x,own.tile.y);}
    }
    if(this.current!==m.revision || this.viewingPreview){void this.experience.loadRound(m.revision);return;}
    if (previous < 0 || wasFull) this.view = { t0:0,t1:Math.max(1,m.outcome.terminal_state_tick) };
    else this.panTimeline(0);
    $('toast').classList.remove('active');
    if(previous<0){
      const key=`atemporal-entry-tick:${location.pathname}`,entry=sessionStorage.getItem(key);
      if(entry!==null){const tick=Number(entry);if(Number.isFinite(tick)&&tick>=0)this.playhead=tick;sessionStorage.removeItem(key);}
    }
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
    return this.viewingPreview ? this.previewView : this.revisions.get(this.current) ?? null;
  }

  async ensureChunk(k: number): Promise<void> {
    if(this.viewingPreview){if(k===Math.floor(this.playhead/CHUNK))void this.loadPreview();return;}
    const rev = this.rev();
    if (!rev || this.replayErrors.has(rev.revision) || rev.loading.has(k) || k * CHUNK > rev.outcome.terminal_state_tick) return;
    if (rev.chunks.has(k)) {
      void this.loadSightHistory(rev);
      return;
    }
    rev.loading.add(k);
    const from = k * CHUNK;
    const to = Math.min(from + CHUNK - 1, rev.outcome.terminal_state_tick);
    const stride = this.config.snapshot_interval;
    const sampleStart = from - (from % stride);
    const t0 = performance.now();
    try {
      const range = await this.net.request({ kind: 'get_snapshot_range', revision: rev.revision, from_tick: sampleStart, to_tick: to, stride }, 'snapshot_range', s => s.revision === rev.revision);
      rev.dictionary = range.entity_dictionary;
      for (const s of range.samples) rev.samples.set(s.tick, s);

      const events = await this.net.request({ kind: 'get_events', revision: rev.revision, from_tick: from, to_tick: to, effects_only: true }, 'events', e => e.revision === rev.revision);
      rev.events = rev.events.concat(events.events);
      rev.chunks.add(k);
      void this.loadSightHistory(rev);
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
    if(this.viewingPreview){void this.loadPreview();return;}
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
          if (!this.fullVision) this.recordExactSightings(rev, e.snapshot);
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

  private exactAtPlayhead(): WorldState | null {
    const tick = Math.floor(this.playhead);
    return this.exact?.revision === this.current && this.exact.tick === tick && (this.playhead === tick || !this.playing)
      ? this.exact.state : null;
  }

  /** Entities to draw at the playhead: exact state when available, otherwise interpolated samples. */
  rawEntities(): EntityView[] {
    const rev = this.rev();
    if (!rev) return [];
    const tick = this.playhead;
    const exact = this.exactAtPlayhead();
    if (exact) return this.exactEntities(rev, exact);
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
        outputDirection: this.sampleOutputDirection(rev, ref, e),
        silo: e.silo,
      };
    });
  }

  private exactEntities(rev:RevisionView, state:WorldState):EntityView[] {
    const dictionary = new Map(rev.dictionary.map((d, i) => [idKey(d.id), i]));
    let outputs = this.exactOutputs.get(rev);
    if (!outputs) {
      outputs = new Map();
      this.exactOutputs.set(rev, outputs);
    }
    for (const e of state.entities) {
      if (e.production && !this.types.get(e.type_key)?.silo) outputs.set(idKey(e.id), e.production.output_direction);
    }
    return state.entities.map(e => ({
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
      activity: activityOf(e),
      engaged: e.engaged_target ? (dictionary.get(idKey(e.engaged_target)) ?? null) : null,
      exact: e,
      outputDirection: e.production && !this.types.get(e.type_key)?.silo ? e.production.output_direction : undefined,
      silo: e.production?.silo,
    }));
  }

  private exactOutputs = new WeakMap<RevisionView, Map<string, CardinalDirection>>();

  private sampleOutputDirection(rev: RevisionView, ref: EntityRef | undefined, sample: Sample['entities'][number]): CardinalDirection | undefined {
    if (sample.output_direction || !ref) return sample.output_direction ?? undefined;
    const def = this.types.get(ref.type_key);
    if (!def?.production || def.silo) return undefined;
    // Older cached samples omitted the orientation. It is fixed once the placement succeeds.
    const known = this.exactOutputs.get(rev)?.get(idKey(ref.id));
    if (known) return known;
    const birth = ref.id.birth_command.command;
    if (birth.round === 0) return 'e';
    const turn = this.experience.turns.get(rev.revision)?.find(t => t.round === birth.round && t.player === birth.player);
    const placement = turn?.commands.find(c => c.id.index === birth.index)?.command;
    return placement?.kind === 'place_blueprints' ? placement.output_directions?.[ref.id.item_index] : undefined;
  }

  entities(): EntityView[] {
    const all = this.rawEntities();
    const visible = this.visibility(all);
    const views = all
      .filter(e => this.fullVision || playerSeesEntity(this.player, visible, e))
      .map(e => this.canInspectPlan(e.owner) ? e : {...e, exact:undefined, silo:undefined});
    if (this.exact?.revision === this.current && this.exact.tick === Math.floor(this.playhead)) {
      for (const b of this.exact.state.blueprints) if (!b.site_id && this.canInspectPlan(b.owner)) {
        views.push({index:-1,id:b.id,owner:b.owner,type_key:b.type_key,x:b.tile.x,y:b.tile.y,hp:0,maxHp:1,facing:'n',lifecycle:'blueprint',activity:'idle',engaged:null,blueprint:{kind:'persistent',id:b.id},outputDirection:b.output_direction??undefined,settings:b.settings ?? {queue:[],order:{kind:'idle'},priority:b.priority,loop_enabled:false}});
      }
    }
    if (this.current === this.latest && this.draft.tick === Math.floor(this.playhead)) {
      this.draft.commands.forEach((d,index) => {
        const c = d.command;
        if (c.kind === 'place_blueprints') c.tiles.forEach((tile,item_index) => views.push({index:-1,id:{birth_command:{command:{round:this.round,player:this.player!,index},target_index:0},item_index,occurrence:0},owner:this.player!,type_key:c.type_key,x:tile.x,y:tile.y,hp:0,maxHp:1,facing:'n',lifecycle:'blueprint',activity:'idle',engaged:null,blueprint:{kind:'draft',local_id:d.local_id,item_index},outputDirection:c.output_directions?.[item_index]??undefined,settings:{queue:[],order:{kind:'idle'},priority:c.priority,loop_enabled:false}}));
        if (c.kind === 'cancel_blueprints') for (let i = views.length - 1; i >= 0; i--) if (c.blueprint_ids.some(b => JSON.stringify(b) === JSON.stringify(views[i].blueprint ?? (views[i].exact?.blueprint_id ? {kind:'persistent',id:views[i].exact!.blueprint_id} : null)))) views.splice(i, 1);
        if (c.kind === 'configure_blueprints') for (const v of views) if (v.blueprint && c.blueprint_ids.some(b => JSON.stringify(b) === JSON.stringify(v.blueprint))) v.settings = c.settings;
      });
    }
    const revision=this.rev();
    if (!this.fullVision && revision && this.terrain) {
      const memory = this.memoryFor(revision);
      views.push(...memory.remembered(Math.floor(this.playhead), visible, new Set(views.map(e => idKey(e.id)))));
    }
    return views;
  }

  private missileEventCache: {source: WorldEvent[]; events: WorldEvent[]} | null=null;
  missileEvents(): WorldEvent[] {
    const source=this.rev()?.events;if(!source)return [];
    if(this.missileEventCache?.source!==source)this.missileEventCache={source,events:source.filter(e=>e.event.kind==='missile_launch'||e.event.kind==='missile_impact')};
    return this.missileEventCache.events;
  }
  missileState() {
    if(this.exact?.revision===this.current && this.exact.tick===Math.floor(this.playhead) && (this.playhead===Math.floor(this.playhead)||!this.playing))return {missiles:this.exact.state.missiles??[],recon:this.exact.state.recon??[]};
    const interval=this.config.snapshot_interval;
    const samples=[this.sampleAt(this.playhead),this.sampleAt(this.playhead+interval)];
    const events=this.missileEvents();
    const shortFlights=events.flatMap(e=>e.event.kind==='missile_launch'?[e.event.flight]:[]);
    const flights=new Map(samples.flatMap(s=>(s?.missiles??[]).map(f=>[`${idKey(f.silo_id)}:${f.sequence}`,f] as const)));
    for(const f of shortFlights)flights.set(`${idKey(f.silo_id)}:${f.sequence}`,f);
    const zones=new Map(samples.flatMap(s=>(s?.recon??[]).map(r=>[JSON.stringify(r),r] as const)));
    return {missiles:[...flights.values()].filter(f=>f.launch_tick<this.playhead && f.impact_tick>=this.playhead),recon:[...zones.values()].filter(r=>r.starts_at<=this.playhead && r.expires_at>this.playhead)};
  }

  private sightHistory=new WeakMap<RevisionView,{next:number,busy:boolean}>();
  private async loadSightHistory(rev:RevisionView):Promise<void> {
    if(this.fullVision || this.viewingPreview || !this.terrain)return;
    const history=this.sightHistory.get(rev)??{next:0,busy:false};
    if(history.busy || history.next>Math.floor(this.playhead/CHUNK))return;
    this.sightHistory.set(rev,history);history.busy=true;
    try {
      while(this.rev()===rev && !this.fullVision && history.next<=Math.floor(this.playhead/CHUNK)){
        const k=history.next,from=k*CHUNK,to=Math.min(from+CHUNK-1,rev.outcome.terminal_state_tick);
        let samples:Sample[];
        if(rev.chunks.has(k))samples=[...rev.samples.values()].filter(s=>s.tick>=from && s.tick<=to).sort((a,b)=>a.tick-b.tick);
        else {
          const stride=this.config.snapshot_interval;
          const range=await this.net.request({kind:'get_snapshot_range',revision:rev.revision,from_tick:from-(from%stride),to_tick:to,stride},'snapshot_range',m=>m.revision===rev.revision);
          if(this.rev()!==rev)return;
          rev.dictionary=range.entity_dictionary;samples=range.samples;
        }
        this.recordSightSamples(rev,samples);
        history.next++;
        await new Promise(resolve=>setTimeout(resolve,0));
      }
    } catch(error){console.warn('Could not restore sightings',error);}
    finally {history.busy=false;}
  }
  private sampleGroundVision = new GroundVision();
  private groundVision = new GroundVision();
  private sightMemory = new WeakMap<RevisionView,SightMemory>();

  private memoryFor(rev:RevisionView):SightMemory {
    let memory=this.sightMemory.get(rev);
    if(!memory){
      const terrain=this.terrain!;
      memory=new SightMemory(terrain.width,terrain.height);
      this.sightMemory.set(rev,memory);
    }
    return memory;
  }

  private recordExactSightings(rev:RevisionView, state:WorldState):void {
    const views = this.exactEntities(rev, state);
    const visible = playerVisibility(this.sampleGroundVision, state.terrain, views,
      {missiles:state.missiles??[], recon:state.recon??[]}, this.types, this.config.multiplayer,
      this.player, state.tick, this.net.rulesVersion===7);
    this.memoryFor(rev).observe(state.tick, visible, views.filter(v => v.owner !== this.player), true);
  }

  private recordSightSamples(rev:RevisionView,samples:Sample[]):void {
    const memory=this.memoryFor(rev);
    for(const sample of samples){
      const views=sample.entities.flatMap(entity=>{
        const ref=rev.dictionary[entity.index];
        const def=ref && this.types.get(ref.type_key);
        return def ? [{...ref,...entity,...entity.tile,maxHp:def.max_hp,engaged:null,outputDirection:this.sampleOutputDirection(rev,ref,entity)}] : [];
      });
      const visible = playerVisibility(this.sampleGroundVision, this.terrain!, views,
        {missiles:sample.missiles??[], recon:sample.recon??[]}, this.types, this.config.multiplayer,
        this.player, sample.tick, this.net.rulesVersion===7);
      memory.observe(sample.tick, visible, views.filter(v => v.owner !== this.player));
    }
  }

  private visionCache: { perspective: number | null; revision: number; tick: number; exact: unknown; sample: unknown; nextSample: unknown; events: unknown; tiles: Set<string> } | null = null;
  visibility(all?: EntityView[]): Set<string> {
    const sample = this.rev()?.samples.get(Math.floor(this.playhead / this.config.snapshot_interval) * this.config.snapshot_interval);
    const nextSample = this.rev()?.samples.get((Math.floor(this.playhead / this.config.snapshot_interval) + 1) * this.config.snapshot_interval);
    const cached = this.visionCache;
    const events = this.rev()?.events;
    const perspective = this.fullVision ? null : this.player;
    if (cached && cached.perspective === perspective && cached.revision === this.current && cached.tick === this.playhead && cached.exact === this.exact && cached.sample === sample && cached.nextSample === nextSample && cached.events === events) return cached.tiles;
    const visible = !this.terrain ? new Set<string>() : playerVisibility(
      this.groundVision, this.terrain, all ?? this.rawEntities(), this.missileState(),
      this.types, this.config.multiplayer, perspective, this.playhead, this.net.rulesVersion===7,
    );
    this.visionCache = { perspective, revision: this.current, tick: this.playhead, exact: this.exact, sample, nextSample, events, tiles: visible };
    return visible;
  }

  configureGhosts(change: (s: BlueprintSettings) => void, accepts: (v: EntityView) => boolean = () => true): boolean {
    const ghosts = this.selectedViews().filter(v => v.blueprint && this.ownSelectable(v) && accepts(v));
    for (const v of ghosts) {
      const settings = structuredClone(v.settings!); settings.queue_loop_flags=settings.queue.map((_,i)=>settings.queue_loop_flags?.[i]??false); change(settings);
      this.stage({kind:'configure_blueprints',blueprint_ids:[v.blueprint!],settings});
    }
    return ghosts.length > 0;
  }

  effectivePriority(v: EntityView): Priority {
    let priority = v.settings?.priority ?? v.exact?.priority ?? 'medium';
    if(this.current===this.latest && this.draft.tick===Math.floor(this.playhead)) for(const d of this.draft.commands) {
      if(d.command.kind==='set_priority' && d.command.entities.some(id=>idKey(id)===idKey(v.id))) priority=d.command.priority;
    }
    return priority;
  }

  blueprintRef(v: EntityView): DraftItemRef | null {
    if(v.blueprint)return v.blueprint;
    if(v.lifecycle!=='site')return null;
    const id=v.exact?.blueprint_id ?? this.exact?.state.blueprints.find(b=>b.site_id && idKey(b.site_id)===idKey(v.id))?.id;
    return id ? {kind:'persistent',id} : null;
  }

  cancelSelectedBlueprints(): boolean {
    const selected = this.selectedViews().filter(v => this.ownSelectable(v));
    const refs = selected.map(v => this.blueprintRef(v)).filter((ref): ref is DraftItemRef => ref !== null);
    if (!refs.length) return false;
    const persistent = refs.filter(ref => ref.kind === 'persistent');
    if (persistent.length === refs.length) {
      this.stage({kind: 'cancel_blueprints', blueprint_ids: persistent});
      return true;
    }
    const gate = this.canStage();
    if (!gate.ok) { this.toast(gate.reason); return true; }

    const removed = new Map<string, Set<number>>();
    for (const ref of refs) {
      if (ref.kind !== 'draft') continue;
      const indices = removed.get(ref.local_id) ?? new Set<number>();
      indices.add(ref.item_index);
      removed.set(ref.local_id, indices);
    }

    // A partial wall-line deletion shifts item indices for later settings and cancellations.
    const commands: DraftCommand[] = [];
    const newIndices = new Map<string, Map<number, number>>();
    for (const draft of this.draft.commands) {
      const command = draft.command;
      const excluded = removed.get(draft.local_id);
      if (command.kind === 'place_blueprints' && excluded) {
        const indices = new Map<number, number>();
        const tiles = command.tiles.filter((_, index) => {
          if (excluded.has(index)) return false;
          indices.set(index, indices.size);
          return true;
        });
        newIndices.set(draft.local_id, indices);
        const output_directions = command.output_directions?.filter((_, index) => indices.has(index)) ?? null;
        if (tiles.length) commands.push({...draft, command: {...command, tiles, output_directions}});
      } else if ((command.kind === 'configure_blueprints' || command.kind === 'cancel_blueprints') &&
                 command.blueprint_ids.some(ref => ref.kind === 'draft' && newIndices.has(ref.local_id))) {
        const blueprint_ids = command.blueprint_ids.flatMap<DraftItemRef>(ref => {
          if (ref.kind !== 'draft' || !newIndices.has(ref.local_id)) return [ref];
          const item_index = newIndices.get(ref.local_id)!.get(ref.item_index);
          return item_index === undefined ? [] : [{...ref, item_index}];
        });
        if (blueprint_ids.length) commands.push({...draft, command: {...command, blueprint_ids}});
      } else commands.push(draft);
    }
    if (persistent.length && this.config.control_limit === 'single_order' && commands.length) {
      this.toast('Single-order mode: delete the staged command first.');
      return true;
    }
    if (persistent.length) {
      commands.push({local_id: `d${Date.now()}-${this.draft.commands.length}`, future_orders: 'keep',
        command: {kind: 'cancel_blueprints', blueprint_ids: persistent}});
    }
    this.draft.undo.push({commands: this.draft.commands, tick: this.draft.tick});
    this.draft.redo = [];
    this.draft.commands = commands;
    this.draft.tick = commands.length ? Math.floor(this.playhead) : null;
    this.draft.selected = null;
    for (const v of selected) if (this.blueprintRef(v)) this.selection.delete(idKey(v.id));
    $('toast').classList.remove('active');
    this.updatePanels();
    return true;
  }

  removeDraftCommand(index: number): void {
    const removed=this.draft.commands[index];if(!removed)return;
    // Removing a placement/queue append also removes its local references, never dangling them.
    const commands=this.draft.commands.flatMap((d,i)=>{
      if(i===index)return [];
      const c=d.command;
      if(c.kind==='configure_blueprints' || c.kind==='cancel_blueprints') {
        const blueprint_ids=c.blueprint_ids.filter(ref=>ref.kind!=='draft'||ref.local_id!==removed.local_id);
        return blueprint_ids.length ? [{...d,command:{...c,blueprint_ids}}] : [];
      }
      if(c.kind==='edit_production' && (c.edit.kind==='remove_pending'||c.edit.kind==='set_item_loop')) {
        const item_ids=c.edit.item_ids.filter(ref=>ref.kind!=='draft'||ref.local_id!==removed.local_id);
        return item_ids.length ? [{...d,command:{...c,edit:{...c.edit,item_ids}}}] : [];
      }
      return [d];
    });
    if (removed.command.kind === 'cancel_blueprints' && this.current === this.latest &&
        this.exact?.revision === this.current && this.exact.tick === this.draft.tick) {
      const issue = this.blueprintDraftIssue(this.exact.state, commands);
      if (issue) { this.toast(issue); return; }
    }
    this.draft.undo.push({commands:this.draft.commands,tick:this.draft.tick});
    this.draft.redo=[];
    this.draft.commands=commands;
    this.draft.selected=null;if(!commands.length)this.draft.tick=null;this.updatePanels();
  }

  effectiveOrder(v: EntityView): Order | undefined {
    let order = v.exact?.production?.stored_order ?? v.exact?.action ?? v.settings?.order;
    if (this.current === this.latest && this.draft.tick === Math.floor(this.playhead)) for (const [index,d] of this.draft.commands.entries()) {
      const c=d.command;
      if ((c.kind==='assign_order' && c.entities.some(id=>idKey(id)===idKey(v.id))) || (c.kind==='set_stored_order' && c.factories.some(id=>idKey(id)===idKey(v.id))) || (c.kind==='assign_group_order' && this.experience.groups(index).find(g=>g.id.owner===c.group.owner && g.id.slot===c.group.slot)?.members.some(id=>idKey(id)===idKey(v.id)))) order=c.order;
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
        if(!this.viewingPreview) this.playing = false;
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
    this.previewFollowTick=null;
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
    return this.entities().filter(e => !e.remembered && e.id && this.selection.has(idKey(e.id)));
  }

  selectViews(views: EntityView[]): void {
    this.selection = new Set(views.map(v => idKey(v.id)));
    this.recalledGroup = null;
    this.mode = {kind: 'none'};
    this.updateMode();
    this.updatePanels();
    $('map').focus();
  }

  canStage(rebasing = false): { ok: boolean; reason: string } {
    if (this.turnRequestPending) return {ok: false, reason: 'Waiting for the server to confirm your turn.'};
    if (this.preview) return {ok:false,reason:'Simulation is still running. Replay is read-only until verification finishes.'};
    if (this.replayErrors.has(this.current)) return {ok: false, reason: 'Replay verification failed. Restart the peripheral and refresh.'};
    if (!this.net.connected) return { ok:false, reason:'Disconnected. Draft retained; reconnect before staging.' };
    if (this.current !== this.latest) return { ok: false, reason: 'Historical replay is read-only. Return to live.' };
    if (this.spectator) return { ok: false, reason: 'Spectators cannot stage orders.' };
    if (!this.phase || this.phase.revision !== this.current) return { ok: false, reason: 'Planning is not open.' };
    if (this.committed.includes(this.player ?? -1)) return { ok: false, reason: 'You already committed this round.' };
    const tick = Math.floor(this.playhead);
    if (!rebasing && this.draft.tick !== null && tick !== this.draft.tick) return { ok: false, reason: `Draft is at tick ${this.draft.tick}; press T to return or clear the draft.` };
    if (tick < this.editableFrom || tick > this.availableThrough) return { ok: false, reason: `Editable ticks are ${this.editableFrom}–${this.availableThrough}.` };
    if (!this.exact || this.exact.revision !== this.current || this.exact.tick !== tick) return { ok: false, reason: 'Exact state is still loading.' };
    return { ok: true, reason: '' };
  }

  factoryEditing = false;
  factorySelection(): EntityView[] {
    return this.selectedViews().filter(v=>this.ownSelectable(v)&&!!this.types.get(v.type_key)?.production&&!this.types.get(v.type_key)?.silo);
  }
  editFactories(action: () => void): void {
    if(this.factoryEditing){action();return;}
    const factories=this.factorySelection();
    if(factories.length<2){action();return;}
    if(!this.canStage().ok){this.toast(this.canStage().reason);return;}
    const mixed=new Set(factories.map(v=>factorySignature(this,v))).size>1;
    const before=this.draft;
    this.draft={...before,commands:[...before.commands]};
    this.factoryEditing=true;
    try {
      if(mixed)for(const v of factories){
        if(v.blueprint){const settings=structuredClone(v.settings!);settings.queue=[];settings.queue_loop_flags=[];settings.loop_enabled=false;settings.priority='medium';settings.order={kind:'idle'};this.stage({kind:'configure_blueprints',blueprint_ids:[v.blueprint],settings});}
        else {
          this.stage({kind:'edit_production',factories:[v.id],edit:{kind:'replace_pending',items:[]}});
          if(factoryPlan(this,v).active)this.stage({kind:'edit_production',factories:[v.id],edit:{kind:'cancel_active'}});
          this.stage({kind:'set_queue_loop',factories:[v.id],enabled:false});
          this.stage({kind:'bind_factory_group',factories:[v.id],group:null});
          this.stage({kind:'assign_order',entities:[v.id],order:{kind:'idle'}});
          this.stage({kind:'set_priority',entities:[v.id],priority:'medium'});
        }
      }
      action();
      if(this.config.control_limit==='single_order'&&this.draft.commands.length>1){this.draft=before;this.toast('This shared edit needs multiple commands. In single-order mode, select factories individually.');}
      else {this.draft.undo=[...before.undo,{commands:before.commands,tick:before.tick}];this.draft.redo=[];}
    } catch (error) { this.draft=before; throw error; }
    finally {this.factoryEditing=false;this.updatePanels();}
  }

  stage(command: Command, policy: FutureOrderPolicy = 'keep'): boolean {
    const gate = this.canStage();
    if (!gate.ok) {
      this.toast(gate.reason);
      return false;
    }
    if (!this.factoryEditing && this.config.control_limit === 'single_order' && this.draft.commands.length >= 1) {
      this.toast('Single-order mode: delete the staged command first.');
      return false;
    }
    if (command.kind === 'place_blueprints') {
      const issue = this.placementPlanAtPlayhead()?.issue(command);
      if (issue) { this.toast(issue); return false; }
    }
    // One shared payload remains one protocol command for disjoint recipients.
    if(this.factoryEditing){
      const previous=this.draft.commands.at(-1);
      const field='factories' in command?'factories':'entities' in command?'entities':command.kind==='configure_blueprints'?'blueprint_ids':null;
      if(previous&&field&&previous.future_orders===policy){
        const old=previous.command as unknown as Record<string,unknown>, next=command as unknown as Record<string,unknown>;
        const left=old[field] as unknown[]|undefined,right=next[field] as unknown[];
        const payload=(c:Record<string,unknown>)=>JSON.stringify({...c,[field]:undefined});
        if(left&&payload(old)===payload(next)&&!left.some(a=>right.some(b=>JSON.stringify(a)===JSON.stringify(b)))){
          this.draft.commands[this.draft.commands.length-1]={...previous,command:{...previous.command,[field]:[...left,...right]} as DraftCommand['command']};
          return true;
        }
      }
    }
    if (!this.factoryEditing) {
      this.draft.undo.push({commands:this.draft.commands,tick:this.draft.tick});
      this.draft.redo = [];
    }
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
    this.draft.commands = [...this.draft.commands, { local_id: `d${Date.now()}-${this.draft.commands.length}`, command: command as DraftCommand['command'], future_orders: policy }];
    if (!this.factoryEditing) this.updatePanels();
    return true;
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
    if(!this.factoryEditing && this.factorySelection().length>1){this.editFactories(()=>this.assign(order,filter));return;}
    const ghosts = this.configureGhosts(s => { s.order = order; },v=>!!this.types.get(v.type_key)?.production&&!this.types.get(v.type_key)?.silo);
    this.stored = false;
    if (this.recalledGroup !== null && this.player !== null) {
      this.stage({ kind: 'assign_group_order', group: { owner: this.player, slot: this.recalledGroup }, order }, this.draft.policy);
      return;
    }
    const entities = this.selectedIds(t => !t.silo && (!!t.production || filter(t)));
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

  rebaseDraft(): void {
    if (!this.draft.commands.length) return;
    const gate = this.canStage(true);
    if (!gate.ok) { this.toast(gate.reason); return; }
    const issue = this.blueprintDraftIssue(this.exact!.state, this.draft.commands);
    if (issue) { this.toast(issue); return; }
    this.draft.undo.push({commands: this.draft.commands, tick: this.draft.tick});
    this.draft.redo = [];
    this.draft.tick = Math.floor(this.playhead);
    this.updatePanels();
  }

  private blueprintDraftIssue(state: WorldState, commands: DraftCommand[]): string | null {
    if (!commands.some(d => ['place_blueprints', 'cancel_blueprints', 'configure_blueprints'].includes(d.command.kind))) return null;
    const placement = this.placementPlan(state);
    const key=(ref:DraftItemRef)=>ref.kind==='persistent'?idKey(ref.id):`${ref.local_id}:${ref.item_index}`;
    const available=new Map(state.blueprints.filter(b=>b.owner===this.player).map(b=>[idKey(b.id),!b.site_id]));
    for(const d of commands){const c=d.command;
      if(c.kind==='place_blueprints') {
        const issue = placement.issue(c);
        if (issue) return issue;
        c.tiles.forEach((_,i)=>available.set(`${d.local_id}:${i}`,true));
      }
      if(c.kind==='cancel_blueprints'||c.kind==='configure_blueprints')for(const ref of c.blueprint_ids){
        const exists=available.get(key(ref));
        if(exists===undefined || (c.kind==='configure_blueprints'&&!exists))return `A blueprint in this plan is unavailable at tick ${state.tick}. Review that blueprint change before continuing.`;
        if(c.kind==='cancel_blueprints')available.delete(key(ref));
      }
      placement.apply(d);
    }
    return null;
  }

  async checkBlueprintDraft(tick: number, commands = this.draft.commands, revision = this.current): Promise<void> {
    if (!commands.some(d => ['place_blueprints', 'cancel_blueprints', 'configure_blueprints'].includes(d.command.kind))) return;
    const state = this.exact?.revision === revision && this.exact.tick === tick ? this.exact.state :
      (await this.net.request({kind:'get_exact_state',revision,tick},'exact_state',m=>m.revision===revision&&m.tick===tick)).snapshot;
    const issue = this.blueprintDraftIssue(state, commands);
    if (issue) throw new Error(issue);
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
      const commands = this.draft.commands;
      const revision = this.current;
      const draftTick = this.draft.tick;
      await this.checkBlueprintDraft(tick, commands, revision);
      if (this.draft.commands !== commands || this.draft.tick !== draftTick || this.current !== revision || this.round !== submittedRound) {
        throw new Error('The plan changed during validation. Review your draft and commit again.');
      }
      await this.net.request({ kind: 'commit', request: { request_id, slot_token: this.session.token, draft: { based_on_revision: revision, tick, commands } } }, 'commit_accepted', m => m.request_id === request_id);
      if (this.round === submittedRound && !this.committed.includes(this.player!)) this.committed.push(this.player!);
      this.clearDraft();
      this.toast('Turn committed. Waiting for the other players…');
    } catch (err) {
      this.toast(`Turn not submitted. Your draft is saved. ${(err as Error).message}`);
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
      if (this.mode.kind === 'launch') {
        const mode=this.mode;
        for(const v of this.entities().filter(v=>this.ownSelectable(v)&&this.types.get(v.type_key)?.silo&&mode.silos.some(id=>idKey(id)===idKey(v.id)))) {
          queueLaunch(this,v,mode.type_key,tile);
        }
        if (!e.shiftKey) this.mode={kind:'none'};
        this.updateMode();return;
      }
      if (this.mode.kind === 'attack' || this.mode.kind === 'stored-target') {
        this.applyTileMode(tile);
        return;
      }
      if (this.mode.kind === 'place' && this.mode.type_key !== 'wall') {
        this.placeAt(tile, tile, e.shiftKey);
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
      this.hover = e.target === map ? this.renderer.tileAt(e.clientX, e.clientY) : null;
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
        this.placeAt(a, b, e.shiftKey);
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
      const views = this.entities().filter(v => !v.remembered && v.id && (moved ? v.x >= rect.min.x && v.x <= rect.max.x && v.y >= rect.min.y && v.y <= rect.max.y : Math.round(v.x) === a.x && Math.round(v.y) === a.y));
      const own = views.filter(v => this.ownSelectable(v));
      for (const v of moved ? (this.fullVision ? views : own) : views.slice(0, 1)) {
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
        const delta = (timelineDrag.x - e.clientX) / this.renderer.timelinePlotWidth() * (timelineDrag.t1 - timelineDrag.t0);
        this.view = { t0: timelineDrag.t0, t1: timelineDrag.t1 };
        this.panTimeline(delta);
      }
    });
    timeline.addEventListener('mousemove', e => {
      this.timelineCursor = this.renderer.timelineTick(e.clientX);
      const tolerance = 4 / this.renderer.timelinePlotWidth() * (this.view.t1 - this.view.t0);
      const turns = (this.experience.turns.get(this.current) ?? []).filter(t => t.commands.length && Math.abs(t.tick - this.timelineCursor) <= tolerance);
      timeline.title = `Tick ${Math.floor(this.timelineCursor)} · ${turns.map(t => `${this.name(t.player)}: ${t.commands.length} orders at tick ${t.tick} (round ${t.round})`).join(' · ')} · White outline: latest write · Purple: selected units · Shift + scroll or Drag bottom ruler to pan`;
    });
    timeline.addEventListener('wheel', e => {
      e.preventDefault();
      if (e.shiftKey) this.panTimeline((e.deltaX || e.deltaY) / this.renderer.timelinePlotWidth() * (this.view.t1-this.view.t0));
      else this.zoomTimeline(Math.exp(e.deltaY * .002), this.renderer.timelineTick(e.clientX));
    }, { passive: false });
    $('play').onclick = () => this.togglePlay();
    const panels = $('panels');
    const releaseStepLayout = () => panels.style.removeProperty('--step-row-height');
    $('game').addEventListener('click', e => {
      if (!(e.target as Element).closest('#step-back,#step-fwd,#jump-back,#jump-fwd')) releaseStepLayout();
    }, true);
    window.addEventListener('keydown', e => { if (![',','.','<','>'].includes(e.key)) releaseStepLayout(); }, true);
    window.addEventListener('resize', releaseStepLayout);
    $('step-back').onclick = () => this.step(-1);
    $('step-fwd').onclick = () => this.step(1);
    $('jump-back').onclick = () => this.step(-100);
    $('jump-fwd').onclick = () => this.step(100);
    $('to-draft').onclick = () => this.draft.tick !== null && this.seek(this.draft.tick);
    $<HTMLInputElement>('tick-input').onchange = e => this.seek(Number((e.target as HTMLInputElement).value));
    $('commit').onclick = () => void this.commit();
    window.addEventListener('keydown', e => this.key(e));
    window.addEventListener('resize', () => this.renderer.resize());
    new ResizeObserver(()=>{
      $('game').style.setProperty('--workspace-height',`${$('panels').getBoundingClientRect().height}px`);
      this.renderer.resize();
    }).observe($('panels'));
    this.renderer.resize();
  }

  private step(delta: number): void {
    // Exact-state replies can add or remove the selected unit's queue between clicks.
    $('panels').style.setProperty('--step-row-height', `${$('selection').getBoundingClientRect().height}px`);
    this.seek(this.playhead + delta);
  }

  togglePlay(): void {
    this.previewFollowTick=null;
    const rev = this.rev();
    if (!rev) return;
    if (!this.viewingPreview && !this.playing && this.playhead >= rev.outcome.terminal_state_tick) this.playhead = 0;
    this.playing = !this.playing;
    if (!this.playing) {
      this.playhead = Math.floor(this.playhead);
      this.requestExact();
    }
    this.updatePanels();
  }

  entityAt(tile: Tile): EntityView | undefined {
    return this.entities().find(v => !v.remembered && Math.round(v.x) === tile.x && Math.round(v.y) === tile.y);
  }

  applyTileMode(tile: Tile): void {
    if (this.mode.kind === 'attack') this.assign({ kind: 'attack_move', destination: tile }, t => !!t.movement);
    if (this.mode.kind === 'stored-target') this.assign({ kind:'attack_move', destination:tile }, t => !!t.movement);
    this.mode = { kind: 'none' };
    this.updateMode();
  }

  placeAt(tile: Tile, end: Tile = tile, repeat = false): void {
    if (this.mode.kind !== 'place') return;
    const command = this.placementCommand(this.mode.type_key, this.placementTiles(tile, end));
    if (!this.stage(command)) return;
    if (!repeat) this.mode = { kind: 'none' };
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
  private placementCommand(type_key: string, tiles: Tile[]): Extract<Command, {kind: 'place_blueprints'}> {
    const def = this.types.get(type_key);
    const output_directions = def?.production && !def.silo ? tiles.map(() => this.renderer.outputDirection) : null;
    return {kind: 'place_blueprints', type_key, tiles, priority: 'medium', output_directions};
  }

  private placementPlan(state: WorldState): PlacementPlan {
    const visible = playerVisibility(
      this.groundVision, state.terrain,
      state.entities.map(e => ({...e.tile, type_key: e.type_key, owner: e.owner, lifecycle: e.lifecycle})),
      {missiles: state.missiles ?? [], recon: state.recon ?? []}, this.types, this.config.multiplayer,
      this.player, state.tick, this.net.rulesVersion === 7,
    );
    return new PlacementPlan(state.terrain, this.types, state, this.player, visible, this.fullVision, this.net.rulesVersion >= 14);
  }

  private placementCache?: {state: WorldState; player: number | null; fullVision: boolean; commands: DraftCommand[] | null; plan: PlacementPlan};

  private placementPlanAtPlayhead(): PlacementPlan | undefined {
    const tick = Math.floor(this.playhead);
    if (this.exact?.revision !== this.current || this.exact.tick !== tick) return;
    const state = this.exact.state;
    const commands = this.current === this.latest && this.draft.tick === tick ? this.draft.commands : null;
    const cache = this.placementCache;
    if (cache && cache.state === state && cache.commands === commands && cache.player === this.player && cache.fullVision === this.fullVision) return cache.plan;
    const plan = this.placementPlan(state);
    for (const command of commands ?? []) plan.apply(command);
    this.placementCache = {state, commands, player: this.player, fullVision: this.fullVision, plan};
    return plan;
  }

  placementIssue(typeKey: string, tiles: Tile[]): string | null {
    const plan = this.placementPlanAtPlayhead();
    return plan ? plan.issue(this.placementCommand(typeKey, tiles)) : 'Exact state is still loading.';
  }

  validPlacement(tile: Tile, factory: boolean): boolean {
    return this.placementIssue(factory ? 'factory' : 'wall', [tile]) === null;
  }
  timelineEnd(): number {
    return Math.max(1, this.viewingPreview ? this.preview!.end_tick : this.rev()?.outcome.terminal_state_tick ?? 1, ...(this.experience?.turns.get(this.current) ?? []).filter(t => t.commands.length).map(t => t.tick));
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
    return [...new Set(this.selectedViews().filter(v=>this.ownSelectable(v)).flatMap(v=>this.types.get(v.type_key)?.production?.recipes??[]))].sort();
  }
  structures(): string[] {
    return ['factory', 'turret', 'wall'].filter(k => this.types.has(k)).concat([...this.types.values()].filter(t => t.kind === 'structure' && !['factory', 'turret', 'wall'].includes(t.key)).map(t => t.key));
  }

  actionAvailable(key: string): boolean {
    const views=this.selectedViews().filter(v=>this.ownSelectable(v));
    const has=(cap:'movement'|'mining'|'construction'|'production')=>views.some(v=>!!this.types.get(v.type_key)?.[cap]);
    const template=this.factorySelection().length>0||this.recalledGroup!==null;
    switch(key){
      case 'f': case 'g': return has('movement')||template;
      case 'm': return has('mining')||template;
      case 'c': return has('construction')||template;
      case 'x': return has('movement')||template;
      case 'b': return this.structures().length>0;
      case 'q': return this.producible().length>0;
      case 'p': return views.some(v=>this.priorityEligible(v));
      case 'l': return has('production');
      case 'h': return views.some(v=>!v.blueprint);
      case 'j': return views.some(v=>!v.blueprint&&!!this.types.get(v.type_key)?.production&&!this.types.get(v.type_key)?.silo);
      default: return true;
    }
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
      this.digit(Number(k),e.shiftKey);
      return;
    }
    if ('fgmcxbqplhj'.includes(k) && k.length===1 && (!this.canStage().ok || !this.actionAvailable(k))) return;
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
        if(!this.factoryEditing&&this.factorySelection().length>1){this.editFactories(()=>this.key(e));break;}
        const enabled = !this.selectedViews().filter(v=>this.types.get(v.type_key)?.production).every(v=>factoryPlan(this,v).loop);
        const ghosts = this.configureGhosts(s => { s.loop_enabled = enabled; }, v=>!!this.types.get(v.type_key)?.production);
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
          this.removeDraftCommand(this.draft.selected);
        }
        break;
      }
      case 'Enter': if (this.mode.kind === 'none' && !this.drag) void this.commit(); else this.toast('Finish or cancel the action before committing.'); break;
      case ' ': this.togglePlay(); e.preventDefault(); break;
      case ',': this.step(e.shiftKey ? -100 : -1); break;
      case '.': this.step(e.shiftKey ? 100 : 1); break;
      case '<': this.step(-100); break;
      case '>': this.step(100); break;
      case 'Home': this.seek(this.editableFrom); break;
      case 'End': this.seek(this.viewingPreview ? this.preview!.through_tick : this.availableThrough); break;
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

  priorityEligible(v: EntityView): boolean {
    const t=this.types.get(v.type_key);
    return !!t && (!!t.construction || !!t.production || !!t.self_repair || !!t.healing || v.lifecycle!=='complete');
  }
  setSelectionPriority(priority: Priority): void {
    if(!this.factoryEditing&&this.factorySelection().length>1){this.editFactories(()=>this.setSelectionPriority(priority));return;}
    this.configureGhosts(s=>{s.priority=priority;});
    const entities=this.selectedViews().filter(v=>this.ownSelectable(v)&&!v.blueprint&&this.priorityEligible(v)).map(v=>v.id);
    if(entities.length)this.stage({kind:'set_priority',entities,priority});
  }
  digit(n: number, five = false): void {
    const mode = this.mode;
    if(mode.kind==='recipe'&&!this.factoryEditing&&this.factorySelection().length>1){this.editFactories(()=>this.digit(n,five));return;}
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
      const factories = key ? this.selectedIds(t => !!t.production?.recipes.includes(key)) : [];
      const ghosts = key ? this.configureGhosts(s => { s.queue_loop_flags ??= s.queue.map(()=>false);s.queue.push(...Array(five?5:1).fill(key));s.queue_loop_flags.push(...Array(five?5:1).fill(s.loop_enabled)); }, v=>!!this.types.get(v.type_key)?.production?.recipes.includes(key)) : false;
      if (key && factories.length) this.stage({ kind: 'edit_production', factories, edit: { kind: 'append', items: Array(five?5:1).fill(key) } });
      else if (!ghosts) this.toast(key ? 'Select a factory first.' : 'No such recipe.');
      this.mode = { kind: 'none' };
    } else if (mode.kind === 'priority') {
      const priority = (['high', 'medium', 'low', 'off'] as Priority[])[n - 1];
      if (priority) this.setSelectionPriority(priority);
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
        case 'launch': return `Target ${missileName(this.mode.type_key)}: click the map · Shift-click to keep targeting · Esc cancels`;
        case 'attack': return 'Attack-move: click a destination tile (Esc cancels)';
        case 'support': return 'Support: click one of your units';
        case 'area': return `${this.mode.order === 'mine' ? 'Mine' : 'Construct'}: drag a rectangle`;
        case 'build': return `Build: ${this.structures().map((k, i) => `${i + 1}=${k}`).join('  ')}`;
        case 'place': return `Place ${this.mode.type_key}: click a floor tile${this.types.get(this.mode.type_key)?.production && !this.types.get(this.mode.type_key)?.silo ? ` (output ${this.renderer.outputDirection.toUpperCase()}, R rotates)` : ''}`;
        case 'recipe': return `Queue: ${this.producible().map((k, i) => `${i + 1}=${k}`).join('  ')}`;
        case 'priority': return 'Priority: 1=high 2=medium 3=low 4=off';
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
    const waiting=this.committed.includes(this.player??-1)&&this.committed.length<this.config.player_count&&!this.progress&&!this.preview&&!this.finished&&this.phase?.revision===this.latest;
    const othersWaiting=!this.spectator && !waiting && !this.committed.includes(this.player!) && this.committed.length>0 && this.committed.length<this.config.player_count && !this.finished && !this.preview && !this.progress && this.phase?.revision===this.latest;
    const banner = $('turn-banner'), message = $('turn-banner-message'), toggle = $('turn-banner-toggle');
    const compact = othersWaiting && this.waitingNoticeCompact;
    banner.hidden = !waiting && !othersWaiting;
    banner.dataset.other = String(othersWaiting);
    banner.dataset.compact = String(compact);
    message.hidden = compact;
    toggle.hidden = !othersWaiting;
    toggle.setAttribute('aria-expanded', String(!compact));
    toggle.textContent = compact ? 'Player waiting' : 'Minimize';
    toggle.title = compact ? 'Show waiting notification' : 'Minimize waiting notification';
    const remaining = this.config.player_count - this.committed.length;
    message.textContent = othersWaiting
      ? this.committed.map(p => this.name(p)).join(', ') + (this.committed.length === 1 ? ' is' : ' are') + ' waiting for your turn'
      : `TURN COMMITTED · Waiting for ${remaining} player${remaining === 1 ? '' : 's'} · Uncommit to edit your moves`;
    const rev = this.rev();
    const planningRound = practice ? this.latest + 1 : this.round;
    const historicalRound = practice ? this.current : this.experience?.rounds.get(this.current)?.round ?? '…';
    const phase = this.viewingPreview ? 'Simulating · preview (read-only)' : this.replayErrors.has(this.current) ? `Replay unavailable: ${/mismatch/i.test(this.replayErrors.get(this.current)!) ? 'local replay mismatch' : 'loading failed'}` : this.current !== this.latest ? `Historical round ${historicalRound} (read-only)` : this.finished ? this.finished : this.committed.length >= this.config.player_count ? `Round ${planningRound}: simulating…` : this.committed.includes(this.player ?? -1) ? `Round ${planningRound}: committed, waiting` : this.phase && this.phase.revision === this.current ? `Round ${planningRound}: planning` : this.progress ? `Simulating ${this.progress.tick}/${this.progress.end}` : 'Simulating…';
    $('top-phase').textContent = `${practice ? 'Practice · ' : ''}${this.spectator ? 'Spectator' : this.name(this.player!)} · ${phase} · revision ${this.current}`;
    $('top-tick').innerHTML = `tick <b>${Math.floor(this.playhead)}</b> / ${rev?.outcome.terminal_state_tick ?? 0} · editable ${this.editableFrom}–${this.availableThrough}`;
    if(this.viewingPreview) $('top-tick').textContent=`tick ${Math.floor(this.playhead)} · available 0–${this.preview!.through_tick} / ${this.preview!.end_tick}`;
    const sample = this.sampleAt(this.playhead);
    const banks = this.exact && this.exact.tick === Math.floor(this.playhead) && this.exact.revision === this.current
      ? this.exact.state.players.filter(p => this.spectator || p.player_id === this.player).map(p => `${this.name(p.player_id)} ${p.bank.toFixed(0)}${p.currently_eliminated ? ' (eliminated)' : ''}`)
      : sample?.players.filter(p => this.spectator || p.player_id === this.player).map(p => `${this.name(p.player_id)} ${p.bank.toFixed(0)}${p.currently_eliminated ? ' (eliminated)' : ''}`) ?? [];
    $('top-bank').textContent = `MATTER · ${banks.join(' · ')} · tick ${Math.floor(this.playhead)}${this.exact?.tick === Math.floor(this.playhead) ? '' : ' (sampled)'}`;
    const round = this.experience?.rounds.get(this.current);
    if(this.viewingPreview) $('top-sim').textContent=`Simulation continues · completed through tick ${this.preview!.through_tick} · final result pending`;
    else if (round) {
      const fastest = Math.max(1000,Math.min(...round.time_totals.map(t=>t.total_ms)));
      const penalty = this.config.objective.kind !== 'timed' ? this.config.objective.rules.time_penalty : 'none';
      $('top-sim').textContent = `Sim ${round.sim_duration_ms}ms · committed time ratio ${round.time_totals.map(t=>`${this.name(t.player_id)} ${(Math.max(1000,t.total_ms)/fastest).toFixed(2)}×`).join(' / ')} · penalty ${penalty}${!this.finished && this.current === this.latest && this.phase && !this.committed.includes(this.player ?? -1) && !this.spectator ? ` · live planning ${((performance.now()-this.planningSince)/1000).toFixed(0)}s` : ''}`;
    }
  }

  updatePreviewStatus(): void {
    if(this.viewingPreview){
      const banner=$('outcome-banner');banner.dataset.outcome='preview';banner.textContent=`${this.previewSourceReady?'SIMULATING':'FINALIZING REPLAY'} · Replay available through tick ${this.preview!.through_tick} · Result pending`;
      $('result').textContent='Read-only preview. Orders unlock after the complete replay is verified.';
      $('timeline-info').textContent=`Available 0–${this.preview!.through_tick}; simulation continues`;
    }
  }

  private lastSavedCommands?: DraftCommand[];
  private lastSavedState = '';

  private persistDraft(): void {
    if (!this.draftRestored) return;
    const d = this.draft;
    const state = JSON.stringify([this.round, d.tick, d.selected, d.policy, d.undo.length, d.redo.length]);
    if (this.lastSavedCommands === d.commands && this.lastSavedState === state) return;
    this.lastSavedCommands = d.commands;
    this.lastSavedState = state;
    try {
      if (practice || !d.commands.length) sessionStorage.removeItem(this.draftKey);
      else sessionStorage.setItem(this.draftKey, JSON.stringify({round:this.round, draft:packDraft(d)}));
    } catch {
      try {
        sessionStorage.removeItem(this.draftKey);
        if (d.commands.length && !practice) {
          sessionStorage.setItem(this.draftKey, JSON.stringify({round:this.round, draft:{...d,undo:[],redo:[]}}));
        }
      } catch { this.toast('Browser storage is full or unavailable. Keep this tab open until you commit.'); }
    }
  }

  updatePanels(): void {
    this.persistDraft();
    updateRematch(this);
    this.updateTop();
    this.experience?.update();
    const rev = this.rev();
    $('play').textContent = this.playing ? 'Pause' : 'Play';
    $('rate').textContent = `${this.rate}×`;
    if(document.activeElement !== $('tick-input')) $<HTMLInputElement>('tick-input').value = String(Math.floor(this.playhead));
    $('timeline-info').textContent = rev ? `${rev.outcome.kind} · ${rev.outcome.stop_reason.replace('_', ' ')} at ${rev.outcome.terminal_state_tick}` : '';
    // Result panel.
    const result = $('result');
    if (rev) {
      const o = rev.outcome;
      const verdict = o.kind === 'win' ? this.spectator ? 'WIN' : o.survivors.includes(this.player!) ? 'WIN' : 'LOSS' : o.kind.toUpperCase();
      result.dataset.outcome = verdict.toLowerCase();
      const banner = $('outcome-banner');
      banner.dataset.outcome = verdict.toLowerCase();
      const title=document.createElement('strong');
      title.textContent=`TIMELINE ${rev.revision} · ${verdict}${this.finished ? ' · MATCH ENDED' : ''}`;
      const explanation=document.createElement('div');
      const eliminated=o.eliminated.includes(this.player!);
      const transition=[...o.survival_transitions].reverse().find(t=>t.player_id===this.player && t.status==='eliminated');
      const reasons=transition?.reasons.map(r=>r==='no_build_ability'?'no completed constructors or factories remained':'no completed survival buildings remained').join('; ');
      explanation.textContent=eliminated ? `At the final tick (${o.terminal_state_tick}), ${reasons || 'your side was eliminated'}. Buildings visible at earlier ticks do not change that final result.` : `Final result at tick ${o.terminal_state_tick}. ${o.kind==='stalemate'?'Both sides still survived.':o.kind==='draw'?'No players survived.':'Survivors: '+o.survivors.map(p=>this.name(p)).join(', ')+'.'}`;
      const end=document.createElement('button');end.textContent='View final state';end.onclick=()=>this.seek(o.terminal_state_tick);
      const details=document.createElement('details');details.open=banner.querySelector('details')?.open??false;
      const summary=document.createElement('summary');summary.textContent='Explain result';details.append(summary,explanation);
      banner.replaceChildren(title,details,end);
      const lines = [`<b>Revision ${rev.revision}</b>: ${o.kind} (${o.stop_reason.replace('_', ' ')} at tick ${o.terminal_state_tick}, last progress ${o.last_progress_tick})`];
      lines.push(`survivors: ${o.survivors.map(p => this.name(p)).join(', ') || 'none'}`);
      if (rev.score) lines.push(rev.score.entries.map(e => `${e.side_id.kind === 'player' ? this.name(e.side_id.player_id) : e.side_id.team_id}: +${e.raw_delta} → ${e.raw_total} (adjusted ${e.adjusted_total.toFixed(2)})`).join(' · '));
      if (this.phase) lines.push(`committed: ${this.committed.map(p => this.name(p)).join(', ') || 'nobody yet'}`);
      if (this.finished) lines.push(this.finished);
      result.replaceChildren(); for (const line of lines) {const row=document.createElement('div');row.textContent=line.replace(/<\/?b>/g,'');result.append(row);}
    }
    this.updatePreviewStatus();
    if (this.replayErrors.has(this.current)) {
      const banner = $('outcome-banner');
      banner.dataset.outcome = 'loss';
      banner.textContent = /mismatch/i.test(this.replayErrors.get(this.current)!) ? 'REPLAY MISMATCH · Local replay differs from the controller. Orders disabled. Restart the peripheral, then refresh.' : 'REPLAY UNAVAILABLE · Loading failed. Orders disabled. Reconnect and refresh.';
      result.textContent = this.replayErrors.get(this.current)!;
    }
    // Selection panel.
    const body = $('selection-body');
    const views = this.selectedViews();
    $('selection').classList.toggle('silo-selected', views.length===1 && !!this.types.get(views[0].type_key)?.silo);
    $('selection').classList.toggle('factory-selected', views.length === 1 && !!this.types.get(views[0].type_key)?.production);

    if (!views.length) {
      body.innerHTML = this.viewingPreview ? '<p>Select units to inspect them, or scrub the completed replay. Orders reopen after verification.</p>' : this.finished ? '<p>Match ended. Select units, seek the timeline or open Statistics to inspect the replay.</p>' : this.spectator ? '<span class="muted">Spectating. Click or drag to inspect units, or seek any replay tick.</span>' : `<p>Select units to see their stats and orders.</p><p>Plan buildings with B. Separately select constructors and use C to assign their work area.</p><a href="${guideUrl}" target="_blank">How to play ↗</a>`;
    } else {
      body.replaceChildren();
      const icons=document.createElement('div'); icons.id='selection-icons';
      const counts=new Map<string,{v:EntityView,count:number}>();
      for(const v of views){const key=`${v.owner}:${v.type_key}`;const entry=counts.get(key);if(entry)entry.count++;else counts.set(key,{v,count:1});}
      for(const {v,count} of counts.values()) {
        const chip=document.createElement('span'); chip.className='unit-chip';
        const matches=(e:EntityView)=>e.owner===v.owner && e.type_key===v.type_key;
        const only=document.createElement('button'); only.setAttribute('aria-label',`Only ${v.type_key}`); only.title=`Keep only ${this.name(v.owner)} ${v.type_key}s selected`;
        only.append(unitIcon(v.type_key,this.color(v.owner)),document.createTextNode(`${v.type_key} ×${count}`));
        only.onclick=()=>this.selectViews(this.selectedViews().filter(matches));
        const remove=document.createElement('button'); remove.textContent='×'; remove.setAttribute('aria-label',`Remove ${v.type_key} from selection`); remove.title=`Drop ${v.type_key}s from selection`;
        remove.onclick=()=>this.selectViews(this.selectedViews().filter(e=>!matches(e)));
        chip.append(only,remove); icons.append(chip);
      }
      if(counts.size>3){
        const filter=document.createElement('select');filter.setAttribute('aria-label','Filter selection by type');filter.add(new Option(`${views.length} selected · filter by type…`,''));
        for(const [key,{v,count}] of counts){filter.add(new Option(`Only ${v.type_key} ×${count}`,`only:${key}`));filter.add(new Option(`Remove ${v.type_key} ×${count}`,`remove:${key}`));}
        filter.onchange=()=>{const [action,owner,...type]=filter.value.split(':');this.selectViews(views.filter(v=>(v.owner===Number(owner)&&v.type_key===type.join(':'))===(action==='only')));};
        icons.replaceChildren(filter);
      }
      body.append(icons);
      if(views.length===1){const v=views[0],t=this.types.get(v.type_key)!;const title=document.createElement('strong');title.textContent=`${v.type_key} · ${v.lifecycle==='site'?'Under construction':v.lifecycle==='blueprint'?'Planned':this.name(v.owner)}`;body.append(title);
        const stats=document.createElement('div');stats.id='unit-stats';stats.className='unit-stats';
        const values=[['Health',v.blueprint ? `${t.max_hp} when built` : `${v.hp.toFixed(0)} / ${v.maxHp.toFixed(0)}`],['Cost',`${t.matter_cost} matter`],['Vision',`${t.vision} tiles`]];
        if(t.weapon)values.push(['Damage',`${t.weapon.damage} / ${t.weapon.cooldown} ticks`],['Range',`${t.weapon.range} tiles`]);
        if(t.movement)values.push(['Move',`1 tile / ${t.movement.cooldown} ticks`]);
        if(t.mining)values.push(['Mining',`${t.mining.rate} / ${t.mining.cooldown} ticks`]);
        if(t.silo)values.push(['Auto range',`${t.silo.auto_range} tiles`],['Launch range','Unlimited']);
        if(t.self_repair)values.push(['Repair',`${t.self_repair.rate*t.self_repair.hp_per_matter} HP / tick · ${t.self_repair.hp_per_matter} HP / matter`]);
        if(t.construction)values.push(['Build',`${t.construction.rate} / ${t.construction.cooldown} ticks`]);
        for(const [label,value] of values){const item=document.createElement('span');item.textContent=`${label}: ${value}`;stats.append(item);}const details=document.createElement('details');details.className='selection-info';details.innerHTML='<summary>Unit info</summary>';details.append(title,stats);body.append(details);
        if (!t.production) {
          const order = document.createElement('div');
          order.className = 'unit-order';
          order.textContent = `HP ${v.hp.toFixed(0)}/${v.maxHp.toFixed(0)}`;
          if (this.canInspectPlan(v.owner)) order.textContent += ` · ${orderLabel(this.effectiveOrder(v) ?? {kind:'idle'})}`;
          body.append(order);
        }
      } else {const label=document.createElement('strong');label.textContent=`${views.length} units selected`;body.append(label);}
      const eligible=views.filter(v=>this.canInspectPlan(v.owner) && this.priorityEligible(v));
      const priorities=new Set(eligible.map(v=>this.effectivePriority(v)));
      const priority=document.createElement('div');priority.id='selection-priority';priority.className='selection-value';
      priority.dataset.value=priorities.size===1?[...priorities][0]:'mixed';
      priority.hidden=!eligible.length;
      const label=document.createElement('label');label.textContent='Matter priority ';
      const select=document.createElement('select');select.setAttribute('aria-label','Selection priority');
      if(priorities.size!==1){const mixed=new Option('Mixed','mixed');mixed.disabled=true;select.add(mixed);}
      for(const value of ['high','medium','low','off'])select.add(new Option(value==='off'?'Off · no spending':value[0].toUpperCase()+value.slice(1),value));
      select.value=priority.dataset.value;select.disabled=!this.canStage().ok||!eligible.some(v=>this.ownSelectable(v));select.onchange=()=>this.setSelectionPriority(select.value as Priority);
      label.append(select);priority.append(label);body.insertBefore(priority,icons.nextSibling);
      if(views.length>1){
        const orders=views.filter(v=>this.canInspectPlan(v.owner)).map(v=>this.effectiveOrder(v)??{kind:'idle'} as Order);
        const summary=document.createElement('div');summary.id='selection-orders';summary.className='selection-value';
        summary.textContent=`Orders: ${new Set(orders.map(o=>JSON.stringify(o))).size===1?orderLabel(orders[0]):'Mixed'}`;if(orders.length)body.insertBefore(summary,priority.nextSibling);
        const factories=views.filter(v=>this.canInspectPlan(v.owner) && !!this.types.get(v.type_key)?.production);
        if(factories.length>1){const loops=new Set(factories.map(v=>factoryPlan(this,v).loop));const loop=document.createElement('div');loop.className='selection-value';loop.textContent=`Loop new items: ${loops.size>1?'Mixed':[...loops][0]?'On':'Off'}`;body.insertBefore(loop,summary.nextSibling);}
      }
      const groups=this.experience.groups();
      const memberships=views.map(v=>groups.filter(g=>g.members.some(id=>idKey(id)===idKey(v.id))).map(g=>g.id.slot).sort().join(', '));
      if(memberships.some(Boolean)){const group=document.createElement('div');group.id='selection-groups';group.className='selection-value';group.title=[...new Set(memberships)].map(s=>s||'ungrouped').join(' / ');group.textContent=`Groups: ${new Set(memberships).size===1?memberships[0]:'Mixed'}`;body.insertBefore(group,priority.nextSibling);}
      if(views.some(v=>!!this.types.get(v.type_key)?.construction)){const note=document.createElement('small');note.textContent='Construction uses the building’s priority; Off pauses this constructor’s spending.';body.append(note);}
      const cancellable=views.filter(v=>this.ownSelectable(v)&&this.blueprintRef(v));
      if(cancellable.length){const cancel=document.createElement('button');cancel.id='cancel-blueprints';cancel.textContent=cancellable.some(v=>v.lifecycle==='site')?'Cancel selected construction':'Delete selected blueprints';cancel.title='Remove these plans and unfinished buildings; invested matter is lost. Undo restores an uncommitted change.';cancel.disabled=!this.canStage().ok;cancel.onclick=()=>this.cancelSelectedBlueprints();body.insertBefore(cancel,priority.nextSibling);}
    }
    const helpers=document.createElement('div'); helpers.id='selection-helpers';
    for(const [label,army] of [['All army',true],['All units',false]] as const) {
      const button=document.createElement('button'); button.textContent=label;
      button.title=army?'Select all your mobile combat units at this tick, including offscreen units':'Select all your units, buildings and blueprints at this tick';
      button.disabled=this.player===null || this.spectator;
      button.onclick=()=>this.selectViews(this.entities().filter(v=>{
        const t=this.types.get(v.type_key);
        return this.ownSelectable(v) && (!army || (v.lifecycle==='complete' && !!t?.movement && !!t.weapon && t.weapon.damage>0 && !t.mining && !t.construction));
      }));
      helpers.append(button);
    }
    const matching=document.createElement('button');
    matching.textContent='All matching';
    matching.setAttribute('aria-label','Select all matching');
    matching.title='Select every visible unit or building with the same owner and type as your selection';
    matching.disabled=!views.length;
    matching.onclick=()=>{
      const types=new Set(views.map(v=>v.owner+':'+v.type_key));
      this.selectViews(this.entities().filter(v=>!v.remembered && types.has(v.owner+':'+v.type_key)));
    };
    helpers.append(matching);
    body.prepend(helpers);
    updateOrderWarnings(this);
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
    const reset=$<HTMLButtonElement>('clear-future');
    reset.disabled=!this.canStage().ok || this.draft.commands.some(d=>d.command.kind==='clear_future_orders');
    reset.hidden=this.spectator || !!this.finished || this.current!==this.latest;
    reset.onclick=()=>this.stage({kind:'clear_future_orders'});
    const commit = $<HTMLButtonElement>('commit');
    const canCommit = this.net.connected && !this.spectator && !!this.phase && this.phase.revision === this.current && !this.committed.includes(this.player ?? -1) && !this.finished;
    const commitTick = this.draft.tick ?? Math.floor(this.playhead);
    commit.disabled = !!this.preview || !canCommit || this.current !== this.latest || commitTick < this.editableFrom || commitTick > this.availableThrough;
    commit.textContent = this.draft.commands.length ? `Commit turn (${this.draft.commands.length} at tick ${this.draft.tick})` : `Pass turn (tick ${Math.floor(this.playhead)})`;
    if (this.committed.includes(this.player ?? -1)) {
      const waiting = !this.preview && !!this.phase && this.phase.revision === this.latest && !this.progress && this.committed.length < this.config.player_count && !this.finished;
      commit.textContent = waiting ? 'Uncommit · edit my moves' : 'Turn started';
      commit.disabled = !waiting || !this.net.connected || this.spectator;
    }
    if(this.preview){commit.textContent='Simulating · replay available';commit.disabled=true;}
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
      ['P then 1/2/3/4', 'Priority high / medium / low / Off'], ['Q then 1–7', 'Queue a unit at selected factories'], ['L', 'Toggle factory loop'],
      ['H then digit / Clear group', 'Add members / empty a group'], ['J then digit / Backspace', 'Bind / clear factory output group'], ['Factory + F/G/M/C/X', 'Starting order for newborns'], ['V', 'Statistics graphs'], ['- / =, Shift+[ / Shift+]', 'Timeline zoom / pan'], ['Delete', 'Remove selected draft / blueprint / queue item'], ['Ctrl+Z / Ctrl+U', 'Undo / redo draft'],
      ['Enter', 'Commit / pass / uncommit while waiting'], ['Space', 'Play / pause'], [', .', 'Step one tick'], ['< >', 'Jump 100 ticks'], ['Home / End', 'Editable start / end'],
      ['[ ]', 'Playback speed'], ['T', 'Return to draft tick'], ['Esc', 'Cancel mode, then clear selection'], ['?', 'This help'],
    ];
    $('help').innerHTML = `<b>Hotkeys · ? or Esc to close</b><div class="help-columns">${[rows.slice(0,15),rows.slice(15)].map(part=>`<table>${part.map(([k,v])=>`<tr><td><kbd>${k}</kbd></td><td>${v}</td></tr>`).join('')}</table>`).join('')}</div>`;
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
    case 'clear_future_orders': return 'Clear all my previously scheduled orders after this tick';
    case 'set_silo_plan': return `Silo launches: ${cmd.plan.launches.length} queued · automatic ${cmd.plan.automatic?'on':'off'}`;
    case 'set_stored_order': return `stored ${cmd.order.kind} × ${cmd.factories.length}`;
    case 'configure_blueprints': return `Blueprint settings: ${cmd.settings.queue.length} queued · ${cmd.settings.priority} · loop ${cmd.settings.loop_enabled?'on':'off'} · ${cmd.settings.order.kind.replace('_',' ')}`;
    case 'set_priority': return `priority ${cmd.priority} × ${cmd.entities.length}`;
    default: return cmd.kind;
  }
}
