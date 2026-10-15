import { Worker as Thread } from "node:worker_threads";

/**
 * A worker thread behind the Web Worker interface, so the host driver the web
 * entry uses runs a Node thread unchanged.
 */
export class ThreadWorker extends EventTarget {
  readonly #thread: Thread;

  constructor(entry: URL) {
    super();
    this.#thread = new Thread(entry);
    this.#thread.on("message", (data) =>
      this.dispatchEvent(new MessageEvent("message", { data })),
    );
    this.#thread.on("messageerror", () =>
      this.dispatchEvent(new MessageEvent("messageerror")),
    );
    this.#thread.on("error", (error: Error) =>
      this.dispatchEvent(
        Object.assign(new Event("error"), { message: error.message, error }),
      ),
    );
  }

  postMessage(message: unknown, transfer?: Transferable[]): void {
    this.#thread.postMessage(message, transfer as never);
  }

  terminate(): void {
    void this.#thread.terminate();
  }
}
