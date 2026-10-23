/** An in-memory `Storage`, standing in for `localStorage` in tests. */
export class MemoryStorage implements Storage {
  private readonly items = new Map<string, string>();

  get length(): number {
    return this.items.size;
  }

  clear(): void {
    this.items.clear();
  }

  getItem(key: string): string | null {
    return this.items.get(key) ?? null;
  }

  key(index: number): string | null {
    return [...this.items.keys()][index] ?? null;
  }

  removeItem(key: string): void {
    this.items.delete(key);
  }

  setItem(key: string, value: string): void {
    this.items.set(key, value);
  }

  keys(): string[] {
    return [...this.items.keys()];
  }
}

/** A clock and a scheduler that move only when a test says so. */
export class ManualClock {
  at = Date.UTC(2026, 9, 21, 12, 0, 0);
  private tasks: { due: number; task: () => void }[] = [];

  readonly now = (): number => this.at;

  readonly schedule = (task: () => void, delayMs: number): void => {
    this.tasks.push({ due: this.at + delayMs, task });
  };

  /** Move the clock on, running what falls due in order and letting replies arrive. */
  async advance(ms: number): Promise<void> {
    const end = this.at + ms;
    await settle();
    for (;;) {
      this.tasks.sort((left, right) => left.due - right.due);
      const next = this.tasks[0];
      if (next === undefined || next.due > end) break;
      this.tasks.shift();
      this.at = next.due;
      next.task();
      await settle();
    }
    this.at = end;
    await settle();
  }
}

/** Let queued microtasks and the replies they carry run. */
export async function settle(): Promise<void> {
  for (let round = 0; round < 5; round += 1)
    await new Promise((resolve) => setTimeout(resolve, 0));
}
