import type { ClientMessage, ServerMessage, ServerEnvelope } from './contracts.generated';

type Kind = ServerMessage['kind'];
type Of<K extends Kind> = Extract<ServerMessage, { kind: K }>;
type Waiter = { test: (m: ServerMessage) => boolean; resolve: (m: ServerMessage) => void; reject: (e: Error) => void };

/** Same-origin WebSocket with broadcast handlers and request/response matching by predicate. */
export class Net {
  private ws!: WebSocket;
  private handlers = new Map<Kind, ((m: ServerMessage) => void)[]>();
  private waiters: Waiter[] = [];
  instance: string | null = null;
  bytes = 0;
  onClose: () => void = () => {};

  connect(): Promise<void> {
    const url = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`;
    this.ws = new WebSocket(url);
    this.ws.onmessage = event => {
      const text = String(event.data);
      this.bytes += text.length;
      const envelope: ServerEnvelope = JSON.parse(text);
      if (this.instance && envelope.server_instance_id !== this.instance) {
        this.onClose();
        return;
      }
      this.instance = envelope.server_instance_id;
      const message = envelope.message;
      const waiter = this.waiters.find(w => w.test(message));
      if (waiter) {
        this.waiters.splice(this.waiters.indexOf(waiter), 1);
        waiter.resolve(message);
      }
      for (const handler of this.handlers.get(message.kind) ?? []) handler(message);
    };
    this.ws.onclose = () => {
      for (const w of this.waiters) w.reject(new Error('connection closed'));
      this.waiters = [];
      this.onClose();
    };
    return new Promise((resolve, reject) => {
      this.ws.onopen = () => resolve();
      this.ws.onerror = () => reject(new Error('cannot connect'));
    });
  }
  on<K extends Kind>(kind: K, handler: (m: Of<K>) => void): void {
    const list = this.handlers.get(kind) ?? [];
    list.push(handler as (m: ServerMessage) => void);
    this.handlers.set(kind, list);
  }
  send(message: ClientMessage): void {
    this.ws.send(JSON.stringify({ schema_version: 2, message }));
  }
  request<K extends Kind>(message: ClientMessage, kind: K, test: (m: Of<K>) => boolean = () => true, timeoutMs = 60000): Promise<Of<K>> {
    return new Promise((resolve, reject) => {
      const waiter: Waiter = {
        test: m => (m.kind === kind && test(m as Of<K>)) || m.kind === 'commit_rejected',
        resolve: m => {
          clearTimeout(timer);
          if (m.kind === kind) resolve(m as Of<K>);
          else reject(new Error((m as Of<'commit_rejected'>).message));
        },
        reject,
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
