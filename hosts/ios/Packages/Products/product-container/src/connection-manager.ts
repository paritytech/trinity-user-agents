import { createWsJsonRpcProvider, type PausableJsonRpcProvider } from '@novasamatech/host-substrate-chain-connection';

export type ChainConnectionEntry = {
  urls: string[];
  onMessage: (msg: string) => void;
  provider: PausableJsonRpcProvider | null;
  inner: { send(msg: string): void; disconnect(): void } | null;
  buffer: string[];
};

export class ConnectionManager {
  private connections = new Map<string, ChainConnectionEntry>();
  private websocketClass: typeof WebSocket;

  constructor(websocketClass: typeof WebSocket) {
    this.websocketClass = websocketClass;
  }

  add(genesisHash: string, onMessage: (msg: string) => void): ChainConnectionEntry {
    const entry: ChainConnectionEntry = {
      urls: [],
      onMessage,
      provider: null,
      inner: null,
      buffer: [],
    };
    this.connections.set(genesisHash, entry);
    return entry;
  }

  connect(entry: ChainConnectionEntry, genesisHash: string): void {
    const provider = createWsJsonRpcProvider({
      endpoints: entry.urls,
      websocketClass: this.websocketClass,
      onStatusChanged: (status) => {
        console.log('[chainConnection] status:', status, 'genesisHash:', genesisHash);
      },
    });

    entry.provider = provider;
    entry.inner = provider(entry.onMessage);
    for (const msg of entry.buffer) entry.inner.send(msg);
    entry.buffer.length = 0;
  }

  disconnect(genesisHash: string): void {
    this.connections.get(genesisHash)?.inner?.disconnect();
    this.connections.delete(genesisHash);
  }

  pauseAll(): void {
    console.log('[chainConnection] pausing all connections');
    for (const [, entry] of this.connections) {
      entry.inner?.disconnect();
      entry.inner = null;
      entry.provider = null;
    }
  }

  resumeAll(): void {
    console.log('[chainConnection] resuming all connections');
    for (const [genesisHash, entry] of this.connections) {
      if (!entry.inner && entry.urls.length > 0) {
        this.connect(entry, genesisHash);
      }
    }
  }
}
