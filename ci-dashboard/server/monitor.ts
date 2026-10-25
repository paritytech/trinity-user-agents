// The scheduled checks: RPC nodes and published links every minute, TLS
// certificates hourly, the smoke checks once a day. State is kept on disk so
// a restart neither forgets an outage nor posts it twice.

import { randomUUID } from "node:crypto";
import { readFile, rename, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { checkLink, checkRpc, type CheckResult } from "../src/checks";
import type { Config } from "../src/config";
import { advance, message, type State } from "../src/state";
import { checkCertificate, httpsHosts } from "../src/tls";
import { escape, type Message } from "./matrix";

export type SmokeResult = {
  ok: boolean;
  ranAt: string;
  rows: { ok: boolean; name: string; detail: string }[];
  /** The end of the script's output, for when it fails before its table. */
  output: string;
};

const OUTPUT_LIMIT = 10_000;

/** A normal run takes about two minutes; one that hangs is stopped. */
export const SMOKE_TIME_LIMIT_MS = 15 * 60_000;

/** What the checks call, so tests can stand in for the network. */
export type Checks = {
  rpc: typeof checkRpc;
  link: typeof checkLink;
  certificate: typeof checkCertificate;
};

const network: Checks = {
  rpc: checkRpc,
  link: checkLink,
  certificate: checkCertificate,
};

export type Status = {
  rpc: (CheckResult & { block?: number; peers?: number })[];
  links: CheckResult[];
  certificates: CheckResult[];
  checkedAt?: string;
  certificatesCheckedAt?: string;
  smoke?: SmokeResult;
};

type Saved = { state: State; status: Status };

export class Monitor {
  private state: State = {};
  readonly status: Status = { rpc: [], links: [], certificates: [] };

  constructor(
    private readonly config: Config,
    private readonly stateDir: string,
    private readonly notify: (message: Message) => Promise<void>,
    private readonly dashboardUrl: string,
    private readonly checks: Checks = network,
  ) {}

  private get file() {
    return join(this.stateDir, "monitor.json");
  }

  async load() {
    try {
      const saved = JSON.parse(await readFile(this.file, "utf8")) as Saved;
      this.state = saved.state ?? {};
      Object.assign(this.status, saved.status ?? {});
    } catch {
      // First start, or nothing saved yet.
    }
  }

  // Checks can save at the same moment, and a torn file would load as empty
  // and post every open outage again, so each save writes its own file and
  // renames it into place.
  private async save() {
    const saved: Saved = { state: this.state, status: this.status };
    const temporary = `${this.file}.${randomUUID()}.tmp`;
    await writeFile(temporary, JSON.stringify(saved));
    await rename(temporary, this.file);
  }

  /** Runs the given results through the alert state and posts what changed. */
  private async record(results: CheckResult[]) {
    // Only the checks just run move; the others keep their state.
    const urls = new Set(results.map((r) => r.url));
    const { state, transitions } = advance(this.state, results);
    for (const [url, entry] of Object.entries(this.state)) {
      if (!urls.has(url)) state[url] = entry;
    }
    this.state = state;
    const alert = message(transitions, this.dashboardUrl);
    if (alert) await this.notify(alert);
  }

  async checkEndpoints() {
    const [rpc, links] = await Promise.all([
      Promise.all(this.config.rpcEndpoints.map((e) => this.checks.rpc(e))),
      Promise.all(this.config.links.map((l) => this.checks.link(l))),
    ]);
    this.status.rpc = rpc;
    this.status.links = links;
    this.status.checkedAt = new Date().toISOString();
    await this.record([...rpc, ...links]);
    await this.save();
  }

  async checkCertificates() {
    const hosts = httpsHosts(this.config.links.map((l) => l.url));
    this.status.certificates = await Promise.all(
      hosts.map((host) => this.checks.certificate(host)),
    );
    this.status.certificatesCheckedAt = new Date().toISOString();
    await this.record(this.status.certificates);
    await this.save();
  }

  /**
   * smoke.sh prints a Markdown table; each row is one check. Matrix hears
   * only when the verdict changes: the first failure, then the recovery, not
   * every morning in between.
   *
   * The script installs and runs what it downloads, so it gets no secret:
   * only PATH and HOME. A run that hangs is stopped, so the next one is still
   * scheduled.
   */
  async runSmoke(script: string, timeLimitMs = SMOKE_TIME_LIMIT_MS) {
    const child = Bun.spawn(["bash", script], {
      env: { PATH: process.env.PATH ?? "", HOME: process.env.HOME ?? "" },
      stdout: "pipe",
      stderr: "pipe",
    });
    let timer: ReturnType<typeof setTimeout> | undefined;
    const stopped = new Promise<"stopped">((resolve) => {
      timer = setTimeout(() => {
        child.kill("SIGKILL");
        resolve("stopped");
      }, timeLimitMs);
    });
    // A download the script started can hold its output open after it is
    // killed, so the output is not waited for past the limit.
    const finished = await Promise.race([
      Promise.all([
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
        child.exited,
      ]),
      stopped,
    ]);
    clearTimeout(timer);
    const minutes = Math.round(timeLimitMs / 60_000);
    const [out, err, code] =
      finished === "stopped"
        ? ["", `smoke.sh was stopped after ${minutes} minutes\n`, -1]
        : finished;
    const rows = out
      .split("\n")
      .map((line) => /^\| (✅|❌) \| (.+?) \| (.*) \|$/.exec(line))
      .filter((m): m is RegExpExecArray => m !== null)
      .map(([, mark, name, detail]) => ({
        ok: mark === "✅",
        name: name!,
        detail: detail!,
      }));
    const before = this.status.smoke?.ok;
    const ok = code === 0;
    this.status.smoke = {
      ok,
      ranAt: new Date().toISOString(),
      rows,
      output: `${out}${err}`.slice(-OUTPUT_LIMIT),
    };
    await this.save();
    if (ok && before === false) {
      await this.notify({
        text: `✅ Smoke checks pass again\n${this.dashboardUrl}`,
        html: `✅ <b>Smoke checks pass again</b><br><a href="${escape(this.dashboardUrl)}">Dashboard</a>`,
      });
    }
    if (!ok && before !== false) {
      const failed = rows.filter((r) => !r.ok);
      const lines = failed.length
        ? failed.map((r) => `${r.name}: ${r.detail}`)
        : [
            code === -1
              ? `smoke.sh did not finish within ${minutes} minutes`
              : `smoke.sh exited with ${code}`,
          ];
      await this.notify({
        text: ["❌ Smoke checks failed", ...lines, this.dashboardUrl].join(
          "\n",
        ),
        html: [
          "❌ <b>Smoke checks failed</b>",
          ...lines.map(escape),
          `<a href="${escape(this.dashboardUrl)}">Dashboard</a>`,
        ].join("<br>"),
      });
    }
  }
}
