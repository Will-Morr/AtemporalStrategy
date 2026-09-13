import type { ClientMessage, ServerMessage, ServerEnvelope, LobbyState } from './contracts.generated';

type Kind = ServerMessage['kind'];
type Of<K extends Kind> = Extract<ServerMessage, { kind: K }>;
type Waiter = { test: (m: ServerMessage) => boolean; resolve: (m: ServerMessage) => void; reject: (e: Error) => void };

/** Same-origin WebSocket with broadcast handlers and request/response matching by predicate. */
export class Net {
  private ws!: WebSocket;
  private handlers = new Map<Kind, ((m: ServerMessage) => void)[]>();
  private waiters: Waiter[] = [];
  private ranges = new Map<Kind, Promise<unknown>>();
  instance: string | null = null;
  lobby: LobbyState | null = null;
  published = false;
  bytes = 0;
  connected = false;
  onInstanceChange: () => void = () => {};
  onClose: () => void = () => {};

  constructor() {
    window.addEventListener('offline', () => { this.connected = false; this.ws?.close(); this.onClose(); });
  }
  connect(): Promise<void> {
    const url = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`;
    this.ws = new WebSocket(url);
    this.ws.onmessage = event => {
      const text = String(event.data);
      this.bytes += text.length;
      const envelope: ServerEnvelope = JSON.parse(text);
      if (this.instance && envelope.server_instance_id !== this.instance) {
        this.connected = false;
        this.onInstanceChange();
        this.ws.close();
        return;
      }
      this.instance = envelope.server_instance_id;
      const message = envelope.message;
      if (message.kind === 'welcome' || message.kind === 'lobby_updated' || message.kind === 'lobby_update_rejected') {
        if (!this.lobby || message.lobby.revision >= this.lobby.revision) this.lobby = message.lobby;
      }
      if (message.kind === 'revision_published') this.published = true;
      const waiter = this.waiters.find(w => w.test(message));
      if (waiter) {
        this.waiters.splice(this.waiters.indexOf(waiter), 1);
        waiter.resolve(message);
      }
      for (const handler of this.handlers.get(message.kind) ?? []) handler(message);
    };
    this.ws.onclose = () => {
      this.connected = false;
      for (const w of this.waiters) w.reject(new Error('connection closed'));
      this.waiters = [];
      this.onClose();
    };
    return new Promise((resolve, reject) => {
      this.ws.onopen = () => { this.connected = true; resolve(); };
      this.ws.onerror = () => reject(new Error('cannot connect'));
    });
  }
  on<K extends Kind>(kind: K, handler: (m: Of<K>) => void): void {
    const list = this.handlers.get(kind) ?? [];
    list.push(handler as (m: ServerMessage) => void);
    this.handlers.set(kind, list);
  }
  send(message: ClientMessage): void {
    if (!this.connected) return;
    this.ws.send(JSON.stringify({ schema_version: 2, message }));
  }
  request<K extends Kind>(message: ClientMessage, kind: K, test: (m: Of<K>) => boolean = () => true, timeoutMs = 60000): Promise<Of<K>> {
    // These replies have no range/request ID: serialize them so responses cannot be
    // mistaken for a different chunk requested concurrently by playback or inspection.
    if (!['events','stats_range','commands','snapshot_range'].includes(kind)) return this.performRequest(message,kind,test,timeoutMs);
    const pending = (this.ranges.get(kind) ?? Promise.resolve()).catch(() => {}).then(() => this.performRequest(message,kind,test,timeoutMs));
    this.ranges.set(kind,pending);
    return pending;
  }
  private performRequest<K extends Kind>(message: ClientMessage, kind: K, test: (m: Of<K>) => boolean = () => true, timeoutMs = 60000): Promise<Of<K>> {
    if (!this.connected) return Promise.reject(new Error('Disconnected'));
    return new Promise((resolve, reject) => {
      const waiter: Waiter = {
        test: m => (m.kind === kind && test(m as Of<K>)) || (m.kind === 'commit_rejected' && ('request_id' in message ? m.request_id === message.request_id : message.kind === 'commit' ? m.request_id === message.request.request_id : !m.request_id)),
        resolve: m => {
          clearTimeout(timer);
          if (m.kind === kind) resolve(m as Of<K>);
          else reject(new Error((m as Of<'commit_rejected'>).message));
        },
        reject: error => { clearTimeout(timer); reject(error); },
      };
      const timer = setTimeout(() => {
        this.waiters.splice(this.waiters.indexOf(waiter), 1);
        reject(new Error(`timeout waiting for ${kind}`));
      }, timeoutMs);
      this.waiters.push(waiter);
      this.send(message);
    });
  }
}
