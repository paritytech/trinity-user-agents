/**
 * Runs tasks one at a time, in the order they were queued.
 *
 * A failing task does not stop the ones queued after it; its error goes to
 * `onError`. `onChange` runs whenever the number of unfinished tasks changes.
 */
export class SerialQueue {
  private tail: Promise<void> = Promise.resolve();
  private unfinished = 0;

  constructor(
    private readonly onChange: () => void,
    private readonly onError: (error: unknown) => void,
  ) {}

  /** Whether a task is running or waiting. */
  get busy(): boolean {
    return this.unfinished > 0;
  }

  run(task: () => Promise<void>): Promise<void> {
    this.unfinished += 1;
    this.onChange();
    const run = this.tail
      .then(task)
      .catch(this.onError)
      .finally(() => {
        this.unfinished -= 1;
        this.onChange();
      });
    this.tail = run;
    return run;
  }
}
