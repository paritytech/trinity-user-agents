import { Worker as Thread } from "node:worker_threads";

/**
 * A worker thread behind the Web Worker interface, so the host driver the web
 * entry uses runs a Node thread unchanged.
 */
export class ThreadWorker extends EventTarget {
  readonly #thread: Thread;
  #terminated = false;

  constructor(entry: URL) {
    super();
    this.#thread = new Thread(entry);
    this.#thread.on("message", (data) =>
      this.dispatchEvent(new MessageEvent("message", { data })),
    );
    this.#thread.on("messageerror", () =>
      this.dispatchEvent(new MessageEvent("messageerror")),
    );
    this.#thread.on("error", (error: Error) => this.#fail(error.message));
    // A browser worker cannot exit on its own, so the host driver only listens
    // for errors. Report an exit nobody asked for as one, or calls wait forever.
    this.#thread.on("exit", (code) => {
      if (!this.#terminated)
        this.#fail(`worker thread exited with code ${code}`);
    });
  }

  // A browser reports an exception thrown by a listener and carries on; Node's
  // EventTarget rethrows it as uncaught and ends the process. Behave as a
  // browser does, so one failing callback cannot take the host down.
  readonly #wrapped = new WeakMap<object, EventListener>();

  override addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: AddEventListenerOptions | boolean,
  ): void {
    if (!listener) return;
    let wrapped = this.#wrapped.get(listener);
    if (!wrapped) {
      wrapped = (event) => {
        try {
          if (typeof listener === "function") listener.call(this, event);
          else listener.handleEvent(event);
        } catch (error) {
          console.error(error);
        }
      };
      this.#wrapped.set(listener, wrapped);
    }
    super.addEventListener(type, wrapped, options);
  }

  override removeEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: EventListenerOptions | boolean,
  ): void {
    const wrapped = listener && this.#wrapped.get(listener);
    if (wrapped) super.removeEventListener(type, wrapped, options);
  }

  #fail(message: string): void {
    this.dispatchEvent(Object.assign(new Event("error"), { message }));
  }

  postMessage(message: unknown, transfer?: Transferable[]): void {
    this.#thread.postMessage(message, transfer as never);
  }

  terminate(): void {
    this.#terminated = true;
    void this.#thread.terminate();
  }
}
