/**
 * A chat worker's status, and the test probe its host may ask for.
 *
 * The status (running, failed, and the errors it reports) goes to the
 * worker's console, which the host shows in the developer tools' Workers
 * view. The probe keeps more on the worker's window for a browser test to
 * read, such as what the worker received and its stored record. A worker has
 * a probe only when the dev server built it (`__WORKER_PROBE__`) and its host
 * started it with {@link PROBE_PARAM} set to `1`; nothing is kept before.
 */

/** The query parameter of a worker's page that asks for its probe. */
export const PROBE_PARAM = "probe";

/** What every chat worker's probe holds: where the worker stands, and the errors it reported. */
export interface ProbeStatus {
  stage: "starting" | "running" | "failed";
  reason?: string;
  errors: string[];
}

/** Whether this worker's host asked for its probe in the page's address. */
export function probeAsked(): boolean {
  return new URLSearchParams(location.search).get(PROBE_PARAM) === "1";
}

/** Put `probe` on the window as `key`, for a test to read, and answer it. */
export function installProbe<T extends ProbeStatus>(key: string, probe: T): T {
  (window as unknown as Record<string, T>)[key] = probe;
  return probe;
}

/** How a chat worker says where it stands. */
export interface WorkerStatus {
  report(reason: string): void;
  running(): void;
  failed(reason: string): void;
}

/**
 * The status of the worker `name`, said on its console and kept on `probe`
 * when it has one, its last `kept` errors only.
 */
export function workerStatus(
  name: string,
  probe: ProbeStatus | null,
  kept = 20,
): WorkerStatus {
  return {
    report(reason) {
      console.warn(`${name}: ${reason}`);
      if (probe !== null) probe.errors = [...probe.errors, reason].slice(-kept);
    },
    running() {
      console.info(`${name} is running.`);
      if (probe !== null) probe.stage = "running";
    },
    failed(reason) {
      console.error(`${name} failed: ${reason}`);
      if (probe === null) return;
      probe.stage = "failed";
      probe.reason = reason;
    },
  };
}
