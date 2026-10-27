/**
 * DEV ONLY. A key-value store the tabs of this browser share, as the local
 * fixtures use it in place of a network: `localStorage` in the page, and an
 * in-memory one for tests. A listener hears writes made by other documents
 * only, as the `storage` event does.
 */
export interface SharedStore {
  keys(prefix: string): string[];
  get(key: string): string | null;
  set(key: string, value: string): void;
  remove(key: string): void;
  /** Calls back with the key another document wrote, or null when it may have changed everything. */
  onChange(listener: (key: string | null) => void): () => void;
}

/** This origin's `localStorage`, shared with the other tabs. */
export function pageStore(): SharedStore {
  return {
    keys: (prefix) =>
      Object.keys(localStorage).filter((key) => key.startsWith(prefix)),
    get: (key) => localStorage.getItem(key),
    set: (key, value) => localStorage.setItem(key, value),
    remove: (key) => localStorage.removeItem(key),
    onChange: (listener) => {
      const heard = (event: StorageEvent) => {
        if (event.storageArea === localStorage) listener(event.key);
      };
      window.addEventListener("storage", heard);
      return () => window.removeEventListener("storage", heard);
    },
  };
}

/** One in-memory space; each {@link MemoryWorld.store} is a document in it. */
export class MemoryWorld {
  readonly items = new Map<string, string>();
  private readonly documents = new Set<(key: string, from: object) => void>();

  store(): SharedStore {
    const self = {};
    const notify = (key: string) => {
      for (const listener of [...this.documents]) listener(key, self);
    };
    return {
      keys: (prefix) =>
        [...this.items.keys()].filter((key) => key.startsWith(prefix)),
      get: (key) => this.items.get(key) ?? null,
      set: (key, value) => {
        if (this.items.get(key) === value) return;
        this.items.set(key, value);
        notify(key);
      },
      remove: (key) => {
        if (this.items.delete(key)) notify(key);
      },
      onChange: (listener) => {
        const heard = (key: string, from: object) => {
          if (from !== self) listener(key);
        };
        this.documents.add(heard);
        return () => this.documents.delete(heard);
      },
    };
  }
}
