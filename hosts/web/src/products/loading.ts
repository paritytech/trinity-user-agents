/**
 * The one loading state of the product stage.
 *
 * Every open starts an operation with `begin`, and whatever finishes later,
 * such as a lookup, a frame load or a loader error, names the operation it
 * belongs to. A completion from an operation that has been replaced or cleared
 * changes nothing, so a slow earlier open cannot hide the loading of a newer
 * one.
 */
export class StageLoading {
  private current: { id: number; text: string } | null = null;
  private last = 0;

  constructor(private readonly onChange: (text: string | null) => void) {}

  /** The text shown while loading, or null when nothing is loading. */
  get text(): string | null {
    return this.current?.text ?? null;
  }

  /** Start an operation, replacing any running one. Returns its id. */
  begin(text: string): number {
    this.last += 1;
    this.current = { id: this.last, text };
    this.onChange(text);
    return this.last;
  }

  /** End `id`. Does nothing when a newer operation has taken over. */
  finish(id: number): void {
    if (this.current?.id === id) this.clear();
  }

  /** End whatever is loading, for a product that was closed. */
  clear(): void {
    if (this.current === null) return;
    this.current = null;
    this.onChange(null);
  }
}

/**
 * How many documents the product frame loads before the product itself is
 * there. A name is opened through the sandbox loader, which loads first and
 * then navigates to the product; a URL is the product.
 */
export function documentsBeforeProduct(via: "url" | "name"): number {
  return via === "name" ? 2 : 1;
}
