import type {
  EntityRef, Outcome, ReplayFrontier, RoundScore, Sample, ServerMessage, TimelineBucket, WorldEvent, WorldState,
} from './contracts.generated';
import {DisconnectedError, type Net} from './net';

export const CHUNK = 1000;

export interface RevisionView {
  revision: number;
  outcome: Outcome;
  timeline: TimelineBucket[];
  score: RoundScore | null;
  dictionary: EntityRef[];
  samples: Map<number, Sample>;
  events: WorldEvent[];
}

interface ReplayView {
  revision(): RevisionView | null;
  tick(): number;
  playing(): boolean;
  exact(rev: RevisionView, state: WorldState, elapsed: number): void;
  preview(rev: RevisionView, state: WorldState, samples: Sample[], atPlayhead: boolean): void;
  chunk(rev: RevisionView): void;
  failure(rev: RevisionView, error: unknown, exact: boolean): void;
}

type ExactRequest = Promise<Extract<ServerMessage, {kind: 'exact_state'}>>;
type Exact = {revision: number; tick: number; state: WorldState};
type ChunkCache = {loaded: Set<number>; pending: Set<number>};

export class ReplayLoader {
  private currentExact: Exact | null = null;
  get exact() { return this.currentExact; }
  preview: ReplayFrontier | null = null;
  previewView: RevisionView | null = null;
  previewSourceReady = true;
  private previewRequest = 0;
  private previewPending: string | null = null;
  private previewCoverage = new Map<number, number>();
  private exactPending: ExactRequest | null = null;
  private requests = new Map<string, ExactRequest>();
  private chunks = new WeakMap<RevisionView, ChunkCache>();

  constructor(private readonly net: Net, private readonly stride: number, private readonly view: ReplayView) {}

  updatePreview(frontier: ReplayFrontier): boolean {
    this.previewSourceReady = true;
    const fresh = this.preview?.generation !== frontier.generation;
    this.preview = frontier;
    if (fresh) {
      this.previewPending = null;
      this.previewCoverage.clear();
      this.clearExact();
      this.previewView = {
        revision: frontier.revision,
        // Bounds only: provisional results are never presented as an outcome.
        outcome: {kind:'stalemate',stop_reason:'inactivity',terminal_state_tick:frontier.through_tick,last_progress_tick:0,survivors:[],eliminated:[],surviving_sides:[],survival_transitions:[]},
        timeline: [], score: null, dictionary: [], samples: new Map(), events: [],
      };
    }
    this.previewView!.outcome.terminal_state_tick = frontier.through_tick;
    return fresh;
  }

  finishPreview(): void {
    this.previewSourceReady = false;
    this.previewPending = null;
  }

  clearPreview(): void {
    this.finishPreview();
    this.preview = null;
    this.previewView = null;
    this.previewCoverage.clear();
    this.clearExact();
  }

  clearExact(): void {
    this.currentExact = null;
    this.exactPending = null;
  }

  seedExact(rev: RevisionView, state: WorldState): void {
    this.currentExact = {revision: rev.revision, tick: state.tick, state};
  }

  fetchExact(revision: number, tick: number): ExactRequest {
    const key = `${revision}:${tick}`;
    const existing = this.requests.get(key);
    if (existing) return existing;
    const request = this.net.request({kind:'get_exact_state', revision, tick}, 'exact_state',
      m => m.revision === revision && m.tick === tick).finally(() => {
        if (this.requests.get(key) === request) this.requests.delete(key);
      });
    this.requests.set(key, request);
    return request;
  }

  async requestExact(rev: RevisionView): Promise<void> {
    if (!this.net.connected) return;
    if (rev === this.previewView) { await this.loadPreview(); return; }
    const tick = Math.min(Math.floor(this.view.tick()), rev.outcome.terminal_state_tick);
    if (this.exact?.revision === rev.revision && this.exact.tick === tick) {
      this.exactPending = null;
      return;
    }
    const request = this.fetchExact(rev.revision, tick);
    if (request === this.exactPending) return;
    this.exactPending = request;
    const started = performance.now();
    const isCurrent = () => this.exactPending === request && this.view.revision() === rev &&
      Math.floor(this.view.tick()) === tick;
    try {
      const result = await request;
      if (!isCurrent()) return;
      this.exactPending = null;
      this.seedExact(rev, result.snapshot);
      this.view.exact(rev, result.snapshot, Math.round(performance.now() - started));
    } catch (error) {
      if (isCurrent() && !(error instanceof DisconnectedError)) this.view.failure(rev, error, true);
    } finally {
      if (this.exactPending === request) this.exactPending = null;
    }
  }

  private evictChunk(rev: RevisionView, k: number): void {
    for (const tick of rev.samples.keys()) if (Math.floor(tick / CHUNK) === k) rev.samples.delete(tick);
    rev.events = rev.events.filter(e => Math.floor(e.tick / CHUNK) !== k);
  }

  clearChunks(rev: RevisionView): void {
    this.chunks.delete(rev);
    rev.samples.clear();
    rev.events = [];
  }

  async sightSamples(rev: RevisionView, k: number): Promise<Sample[] | null> {
    const from = k * CHUNK;
    const to = Math.min(from + CHUNK - 1, rev.outcome.terminal_state_tick);
    if (this.chunks.get(rev)?.loaded.has(k)) {
      return [...rev.samples.values()].filter(s => s.tick >= from && s.tick <= to).sort((a,b) => a.tick - b.tick);
    }
    const range = await this.fetchRange(rev, from, to);
    if (this.view.revision() !== rev) return null;
    rev.dictionary = range.entity_dictionary;
    return range.samples;
  }

  private fetchRange(rev: RevisionView, from: number, to: number) {
    return this.net.request({kind:'get_snapshot_range', revision: rev.revision, from_tick: from - from % this.stride,
      to_tick: to, stride: this.stride}, 'snapshot_range', m => m.revision === rev.revision);
  }

  async ensureChunk(rev: RevisionView, k: number): Promise<void> {
    if (!this.net.connected) return;
    if (rev === this.previewView) {
      if (k === Math.floor(this.view.tick() / CHUNK)) await this.loadPreview();
      return;
    }
    const from = k * CHUNK;
    if (from > rev.outcome.terminal_state_tick) return;
    let cache = this.chunks.get(rev);
    if (!cache) this.chunks.set(rev, cache = {loaded: new Set(), pending: new Set()});
    if (cache.loaded.has(k)) { this.view.chunk(rev); return; }
    if (cache.pending.has(k)) return;
    cache.pending.add(k);
    const to = Math.min(from + CHUNK - 1, rev.outcome.terminal_state_tick);
    const started = performance.now();
    try {
      const range = await this.fetchRange(rev, from, to);
      if (this.chunks.get(rev) !== cache) return;
      const events = await this.net.request({
        kind:'get_events', revision: rev.revision, from_tick: from, to_tick: to, effects_only:true,
      }, 'events', m => m.revision === rev.revision);
      if (this.chunks.get(rev) !== cache) return;
      rev.dictionary = range.entity_dictionary;
      for (const sample of range.samples) rev.samples.set(sample.tick, sample);
      rev.events = rev.events.concat(events.events);
      cache.loaded.add(k);
      while (cache.loaded.size > 8) {
        const oldest = cache.loaded.values().next().value!;
        cache.loaded.delete(oldest);
        this.evictChunk(rev, oldest);
      }
      if (this.view.revision() === rev) this.view.chunk(rev);
      console.log(`chunk ${k} of revision ${rev.revision}: ${range.samples.length} samples, ${events.events.length} events in ${Math.round(performance.now() - started)} ms`);
    } catch (error) {
      if (this.chunks.get(rev) === cache && this.view.revision() === rev &&
          k === Math.floor(this.view.tick() / CHUNK) && !(error instanceof DisconnectedError)) {
        this.view.failure(rev, error, false);
      }
    } finally {
      cache.pending.delete(k);
    }
  }

  async loadPreview(): Promise<void> {
    const frontier = this.preview, rev = this.previewView;
    if (!frontier || !rev || !this.previewSourceReady || this.view.revision() !== rev ||
        this.previewPending || !this.net.connected) return;
    const tick = Math.min(Math.floor(this.view.tick()), frontier.through_tick);
    const k = Math.floor(tick / CHUNK), from = k * CHUNK, to = Math.min(from + CHUNK - 1, frontier.through_tick);
    const loaded = (this.previewCoverage.get(k) ?? -1) >= to;
    if (loaded && this.view.playing() && tick < frontier.through_tick) return;
    if (loaded && this.exact?.revision === frontier.revision && this.exact.tick === tick) return;
    const request_id = `preview-${++this.previewRequest}`;
    this.previewPending = request_id;
    const ownsRequest = () => this.previewPending === request_id && this.previewView === rev;
    try {
      const result = await this.net.request({kind:'get_replay_preview', request_id, generation:frontier.generation,
        tick, from_tick:loaded ? tick : from, to_tick:loaded ? tick : to}, 'replay_preview',
        m => m.request_id === request_id && m.generation === frontier.generation && m.revision === rev.revision);
      if (!ownsRequest()) return;
      rev.dictionary = result.entity_dictionary;
      for (const sample of result.samples) rev.samples.set(sample.tick, sample);
      if (!loaded) {
        rev.events = rev.events.filter(e => e.tick < from || e.tick > to).concat(result.events);
        this.previewCoverage.set(k, to);
      }
      while (this.previewCoverage.size > 8) {
        const oldest = this.previewCoverage.keys().next().value!;
        this.previewCoverage.delete(oldest);
        this.evictChunk(rev, oldest);
      }
      const atPlayhead = this.view.revision() === rev && Math.floor(this.view.tick()) === tick;
      if (atPlayhead) this.seedExact(rev, result.snapshot);
      this.view.preview(rev, result.snapshot, result.samples, atPlayhead);
    } catch (error) {
      // Publication or retry can expire an in-flight provisional query.
      if (ownsRequest() && !(error instanceof DisconnectedError)) this.net.send({kind:'get_replay_progress'});
    } finally {
      if (ownsRequest()) {
        this.previewPending = null;
        // A paused seek may have arrived during this request with no further playback frame or progress event.
        if (!this.view.playing() && Math.floor(this.view.tick()) !== tick) void this.loadPreview();
      }
    }
  }
}
