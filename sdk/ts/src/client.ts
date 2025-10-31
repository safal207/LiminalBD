import type { CommandEnvelope, EventEnvelope } from './protocol-types';

type JsonValue = Record<string, unknown>;

type EventKind = EventEnvelope extends { kind: infer K }
  ? K extends string
    ? K
    : never
  : never;

type EventFor<K extends EventKind> = Extract<EventEnvelope, { kind: K }>;

type CommandOp = CommandEnvelope extends { op: infer O }
  ? O extends string
    ? O
    : never
  : never;

const hasCrypto = typeof globalThis !== 'undefined' && typeof (globalThis as any).crypto !== 'undefined';

function createId(): string {
  if (hasCrypto && typeof (globalThis as any).crypto.randomUUID === 'function') {
    return (globalThis as any).crypto.randomUUID();
  }
  return Math.random().toString(36).slice(2);
}

interface BackoffOptions {
  min: number;
  max: number;
  factor?: number;
}

interface ClientOptions {
  url: string;
  keyId?: string;
  secret?: string;
  ns?: string;
  format?: 'json' | 'cbor';
  reconnect?: boolean;
  backoff?: BackoffOptions;
  qpsLimit?: number;
  bufferLimit?: number;
  heartbeatIntervalMs?: number;
  websocketFactory?: (url: string, protocols?: string[]) => IWebSocket;
  telemetry?: (metrics: ClientMetrics) => void;
}

interface ClientMetrics {
  state: 'connecting' | 'connected' | 'disconnected';
  queueDepth: number;
  sentCommands: number;
  receivedEvents: number;
  reconnectAttempts: number;
}

interface IWebSocket {
  readonly readyState: number;
  readonly OPEN: number;
  send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void;
  close(code?: number, reason?: string): void;
  addEventListener(type: 'open', listener: () => void): void;
  addEventListener(type: 'close', listener: (ev: CloseEventLike) => void): void;
  addEventListener(type: 'message', listener: (ev: MessageEventLike) => void): void;
  addEventListener(type: 'error', listener: (ev: Event) => void): void;
  removeEventListener(type: 'open', listener: () => void): void;
  removeEventListener(type: 'close', listener: (ev: CloseEventLike) => void): void;
  removeEventListener(type: 'message', listener: (ev: MessageEventLike) => void): void;
  removeEventListener(type: 'error', listener: (ev: Event) => void): void;
}

interface CloseEventLike {
  code: number;
  reason: string;
  wasClean: boolean;
}

interface MessageEventLike {
  data: unknown;
}

class TypedEmitter {
  private listeners = new Map<string, Set<(payload: unknown) => void>>();

  on(event: string, handler: (payload: unknown) => void) {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set());
    }
    this.listeners.get(event)!.add(handler);
  }

  off(event: string, handler: (payload: unknown) => void) {
    this.listeners.get(event)?.delete(handler);
  }

  emit(event: string, payload: unknown) {
    this.listeners.get(event)?.forEach((handler) => handler(payload));
  }
}

class TokenBucket {
  private tokens: number;
  private lastRefill: number;

  constructor(private readonly rate: number) {
    this.tokens = rate;
    this.lastRefill = Date.now();
  }

  consume(): boolean {
    if (this.rate <= 0) {
      return true;
    }
    this.refill();
    if (this.tokens >= 1) {
      this.tokens -= 1;
      return true;
    }
    return false;
  }

  private refill() {
    const now = Date.now();
    const elapsed = now - this.lastRefill;
    const tokensToAdd = (elapsed / 1000) * this.rate;
    if (tokensToAdd >= 1) {
      this.tokens = Math.min(this.rate, this.tokens + tokensToAdd);
      this.lastRefill = now;
    }
  }
}

function defaultWebSocketFactory(url: string) {
  const ctor = (globalThis as unknown as { WebSocket?: new (url: string, protocols?: string | string[]) => IWebSocket }).WebSocket;
  if (!ctor) {
    throw new Error('No WebSocket implementation available; provide websocketFactory');
  }
  return new ctor(url);
}

export class Client {
  private readonly opts: Required<ClientOptions>;
  private socket: IWebSocket | null = null;
  private backoffMs: number;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private readonly emitter = new TypedEmitter();
  private readonly queue: CommandEnvelope[] = [];
  private readonly maxBuffer: number;
  private readonly bucket: TokenBucket;
  private metrics: ClientMetrics = {
    state: 'disconnected',
    queueDepth: 0,
    sentCommands: 0,
    receivedEvents: 0,
    reconnectAttempts: 0,
  };

  constructor(options: ClientOptions) {
    if (!options.url) {
      throw new Error('Client requires url');
    }

    this.opts = {
      reconnect: true,
      format: 'json',
      backoff: { min: 250, max: 8000, factor: 2 },
      qpsLimit: 50,
      bufferLimit: 256,
      heartbeatIntervalMs: 15000,
      websocketFactory: defaultWebSocketFactory,
      telemetry: () => undefined,
      keyId: options.keyId,
      secret: options.secret,
      ns: options.ns,
      ...options,
    } as Required<ClientOptions>;

    this.backoffMs = this.opts.backoff.min;
    this.maxBuffer = this.opts.bufferLimit;
    this.bucket = new TokenBucket(this.opts.qpsLimit ?? 0);
  }

  on<K extends EventKind>(kind: K, handler: (event: EventFor<K>) => void) {
    this.emitter.on(kind, handler as (payload: unknown) => void);
  }

  off<K extends EventKind>(kind: K, handler: (event: EventFor<K>) => void) {
    this.emitter.off(kind, handler as (payload: unknown) => void);
  }

  connect() {
    if (this.socket && this.socket.readyState === this.socket.OPEN) {
      return;
    }
    this.setMetrics({ state: 'connecting' });
    const socket = this.opts.websocketFactory(this.opts.url, this.opts.format === 'cbor' ? ['cbor'] : undefined);
    this.socket = socket;

    const handleOpen = () => {
      this.setMetrics({ state: 'connected' });
      this.backoffMs = this.opts.backoff.min;
      this.flushQueue();
      if (this.opts.keyId && this.opts.secret) {
        this.auth();
      }
      this.startHeartbeat();
    };

    const handleClose = () => {
      this.stopHeartbeat();
      this.setMetrics({ state: 'disconnected' });
      if (this.opts.reconnect) {
        this.scheduleReconnect();
      }
    };

    const handleMessage = (event: MessageEventLike) => {
      const payload = typeof event.data === 'string' ? JSON.parse(event.data) : event.data;
      if (!payload || typeof payload !== 'object') {
        return;
      }
      const eventPayload = (payload as { event?: EventEnvelope }).event;
      if (eventPayload && typeof eventPayload === 'object' && 'kind' in eventPayload) {
        this.metrics.receivedEvents += 1;
        this.emitter.emit((eventPayload as EventEnvelope).kind as string, eventPayload);
        this.publishTelemetry();
      }
    };

    const handleError = () => {
      if (this.socket?.readyState !== this.socket?.OPEN) {
        this.scheduleReconnect();
      }
    };

    socket.addEventListener('open', handleOpen);
    socket.addEventListener('close', handleClose);
    socket.addEventListener('message', handleMessage);
    socket.addEventListener('error', handleError);
  }

  disconnect(code?: number, reason?: string) {
    this.opts.reconnect = false;
    this.stopHeartbeat();
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.socket?.close(code, reason);
    this.socket = null;
    this.setMetrics({ state: 'disconnected' });
  }

  auth() {
    const command = {
      op: 'auth',
      keyId: this.opts.keyId,
      secret: this.opts.secret,
      namespace: this.opts.ns,
    } as CommandEnvelope;
    this.enqueue(command);
  }

  lql(query: string, parameters?: JsonValue) {
    const id = createId();
    const command = {
      op: 'lql',
      id,
      query,
      parameters,
    } as CommandEnvelope;
    this.enqueue(command);
    return id;
  }

  subscribe(pattern: string, options?: JsonValue) {
    const id = createId();
    const command = {
      op: 'subscribe',
      id,
      pattern,
      options,
    } as CommandEnvelope;
    this.enqueue(command);
    return id;
  }

  unsubscribe(id: string) {
    const command = {
      op: 'unsubscribe',
      id,
    } as CommandEnvelope;
    this.enqueue(command);
  }

  intent = Object.assign(
    (kind: string, payload: JsonValue) => {
      const command = {
        op: 'intent',
        kind,
        payload,
      } as CommandEnvelope;
      this.enqueue(command);
      return command;
    },
    {
      text: (text: string, lang = 'en') => {
        const command = {
          op: 'intent.text',
          text,
          lang,
        } as CommandEnvelope;
        this.enqueue(command);
        return command;
      },
    }
  );

  seed = {
    plant: (spec: JsonValue) => {
      const id = createId();
      const command = {
        op: 'seed.plant',
        id,
        spec,
      } as CommandEnvelope;
      this.enqueue(command);
      return id;
    },
    abort: (id: string, reason?: string) => {
      const command = {
        op: 'seed.abort',
        id,
        reason,
      } as CommandEnvelope;
      this.enqueue(command);
    },
    garden: () => {
      const command = {
        op: 'seed.garden',
      } as CommandEnvelope;
      this.enqueue(command);
    },
  };

  dream = {
    now: (intensity?: number) => {
      const command = {
        op: 'dream.now',
        intensity,
      } as CommandEnvelope;
      this.enqueue(command);
    },
  };

  sync = {
    now: (scope?: string) => {
      const command = {
        op: 'sync.now',
        scope,
      } as CommandEnvelope;
      this.enqueue(command);
    },
  };

  awaken = {
    now: () => {
      const command = {
        op: 'awaken.now',
      } as CommandEnvelope;
      this.enqueue(command);
    },
  };

  mirror = {
    timeline: (range: { from: string; to: string }) => {
      const command = {
        op: 'mirror.timeline',
        range,
      } as CommandEnvelope;
      this.enqueue(command);
    },
    replay: (cursor?: string) => {
      const command = {
        op: 'mirror.replay',
        cursor,
      } as CommandEnvelope;
      this.enqueue(command);
    },
  };

  noetic = {
    propose: (proposal: JsonValue) => {
      const command = {
        op: 'noetic.propose',
        proposal,
      } as CommandEnvelope;
      this.enqueue(command);
    },
  };

  peers = {
    add: (peer: JsonValue) => {
      const command = {
        op: 'peers.add',
        peer,
      } as CommandEnvelope;
      this.enqueue(command);
    },
    list: () => {
      const command = {
        op: 'peers.list',
      } as CommandEnvelope;
      this.enqueue(command);
    },
  };

  private startHeartbeat() {
    this.stopHeartbeat();
    if (this.opts.heartbeatIntervalMs <= 0) {
      return;
    }
    this.heartbeatTimer = setInterval(() => {
      this.enqueue({ op: 'echo' } as unknown as CommandEnvelope);
    }, this.opts.heartbeatIntervalMs);
  }

  private stopHeartbeat() {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  private enqueue(command: CommandEnvelope) {
    if (!this.bucket.consume()) {
      return; // drop when rate limited
    }
    if (this.queue.length >= this.maxBuffer) {
      this.queue.shift();
    }
    this.queue.push(command);
    this.metrics.queueDepth = this.queue.length;
    this.publishTelemetry();
    this.flushQueue();
  }

  private flushQueue() {
    if (!this.socket || this.socket.readyState !== this.socket.OPEN) {
      this.connect();
      return;
    }
    while (this.queue.length > 0) {
      const command = this.queue.shift()!;
      this.metrics.queueDepth = this.queue.length;
      const payload = JSON.stringify({ version: '1.0.0', command });
      this.socket.send(payload);
      this.metrics.sentCommands += 1;
    }
    this.publishTelemetry();
  }

  private scheduleReconnect() {
    if (!this.opts.reconnect) {
      return;
    }
    if (this.reconnectTimer) {
      return;
    }
    this.metrics.reconnectAttempts += 1;
    this.publishTelemetry();
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
      this.backoffMs = Math.min(this.opts.backoff.max, this.backoffMs * (this.opts.backoff.factor ?? 2));
    }, this.backoffMs);
  }

  private setMetrics(patch: Partial<ClientMetrics>) {
    this.metrics = { ...this.metrics, ...patch };
    this.publishTelemetry();
  }

  private publishTelemetry() {
    this.opts.telemetry(this.metrics);
  }
}

export type { ClientOptions, ClientMetrics, EventKind, EventFor, CommandOp };
