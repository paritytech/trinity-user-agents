import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import { localNovaIdentity } from "../chat/identity.js";
import type { ChatKeys } from "../chat/keys.js";
import { chatRecordKey } from "../chat/store.js";
import {
  NOVA_WORKER_PATH,
  type DemoWorkerAnswer,
} from "../chat-v2/local-demo-route.js";
import {
  parseWorkerManifest,
  type ResolvedWorker,
} from "../worker-manifest.js";
import {
  refuseNovaIdentity,
  serveNovaIdentity,
  type IdentityPort,
  type IdentityService,
} from "./identity-bridge.js";

/**
 * DEV ONLY. Which engine runs the signed-in wallet's Nova chat: the host's
 * own (the default), or the Nova chat worker, a product worker this dev
 * server builds from `./worker.ts` and runs through the ordinary loader.
 *
 * The two never run at once for one identity: both would acknowledge the
 * same messages and replace each other's statements. Switching to the worker
 * stops the host's engine first (the caller's {@link NovaWorkerModeDeps.changed}
 * does that before the worker starts); switching back stops the worker, and
 * with it the identity service, before the host's engine starts again. A
 * worker that fails to start leaves chat off, never the host's engine on.
 *
 * The choice is kept per network and wallet, so a reload does not silently
 * swap engines. The two keep separate histories: what one receives the
 * other does not see. Copying the host's history into the worker is an
 * explicit choice, made once per switch, and the host keeps its copy.
 *
 * Only {@link NOVA_WORKER_PRODUCT}'s worker frame gets the identity service,
 * only while the worker mode is on, and only for the session it was granted
 * in. Every other frame that asks is refused.
 */

export type ChatEngineMode = "host" | "worker";

/**
 * The Nova chat worker's product id: the bare development id, which no
 * published name can take and which does not change with the dev server's
 * port, so the worker's stored history survives a port change.
 */
export const NOVA_WORKER_PRODUCT = "localhost";

export const chatModeKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:chat-engine:${walletId}:v1`;

export interface NovaWorkerSession {
  network: string;
  walletId: string;
  /** What names the session's identity, such as the account and chat key the core reports. */
  identity: string;
  /** Derive and check the session's chat keys, or say why chat is off. Asked once, when the worker connects. */
  keys(): ChatKeys | string;
}

/** The identity service's state, for the developer view. */
export type IdentityLink =
  | { state: "off" }
  | { state: "waiting" }
  | { state: "connected"; calls: number; refused: number }
  | { state: "refused"; reason: string };

export interface NovaWorkerModeDeps {
  /** The host's own storage: the mode and the host's chat record live here. */
  storage: Storage;
  base: string;
  origin: string;
  /** Called after the mode or the link changed. The caller stops the host's engine first, then starts or stops workers. */
  changed(): void;
  log(text: string): void;
  /** Called when the link's counts change, for the developer view only. */
  linkChanged?(): void;
  /** Whether the user blocked `account` in the host. A blocked person's reference resolves to no one. */
  blocked?(account: string): boolean;
  now?(): number;
  fetch?: typeof fetch;
}

/** How long a start reference resolves after the user chose someone. */
export const START_REFERENCE_TTL_MS = 10 * 60_000;

interface StartReference {
  account: HexString;
  madeAt: number;
}

export class NovaWorkerMode {
  private session: NovaWorkerSession | null = null;
  private keys: ChatKeys | string | null = null;
  private service: IdentityService | null = null;
  private copyNext = false;
  private status: IdentityLink = { state: "off" };
  /** Start references of the current grant, by reference. They end with it. */
  private references = new Map<string, StartReference>();

  constructor(private readonly deps: NovaWorkerModeDeps) {}

  /** The mode of the signed-in session; the host's engine when signed out. */
  mode(): ChatEngineMode {
    const session = this.session;
    if (session === null) return "host";
    return this.deps.storage.getItem(
      chatModeKey(session.network, session.walletId),
    ) === "worker"
      ? "worker"
      : "host";
  }

  /** Whether the host's own engine may run now. */
  hostRuns(): boolean {
    return this.mode() === "host";
  }

  link(): IdentityLink {
    return this.status;
  }

  /** Follow the signed-in session. A new one ends the old one's service. */
  setSession(session: NovaWorkerSession | null): void {
    if (
      session?.network === this.session?.network &&
      session?.walletId === this.session?.walletId &&
      session?.identity === this.session?.identity
    )
      return;
    this.endService();
    this.copyNext = false;
    this.session = session;
    this.keys = null;
    this.status =
      this.mode() === "worker" ? { state: "waiting" } : { state: "off" };
  }

  /**
   * Switch engines. With `copyHistory`, the worker started by this switch
   * may take one copy of the host's chat record; the host keeps its own.
   */
  setMode(mode: ChatEngineMode, options: { copyHistory?: boolean } = {}): void {
    const session = this.session;
    if (session === null) throw new Error("Sign in to choose a chat engine.");
    if (mode === this.mode()) return;
    const key = chatModeKey(session.network, session.walletId);
    this.endService();
    if (mode === "worker") {
      this.deps.storage.setItem(key, "worker");
      this.copyNext = options.copyHistory === true;
      this.status = { state: "waiting" };
    } else {
      this.deps.storage.removeItem(key);
      this.copyNext = false;
      this.status = { state: "off" };
    }
    this.deps.log(
      mode === "worker"
        ? "Chat now runs in the Nova chat worker; the host's chat engine stopped."
        : "Chat now runs in the host; the Nova chat worker stopped.",
    );
    this.deps.changed();
  }

  /** Whether `productId` is the chat product the user switched chat to, with its identity lent. */
  isChatProduct(productId: string): boolean {
    return (
      productId === NOVA_WORKER_PRODUCT &&
      this.mode() === "worker" &&
      this.service !== null
    );
  }

  /**
   * A reference to `account` for one `start_conversation`: random, single
   * use, valid {@link START_REFERENCE_TTL_MS} and only in the current grant,
   * so it names no one once the worker restarts, the user switches back or
   * the session changes. It says nothing about the person by itself.
   */
  startReference(account: string): string {
    if (this.service === null)
      throw new Error("The Nova chat worker is not connected.");
    if (!/^0x[0-9a-fA-F]{64}$/.test(account))
      throw new Error("That is not an account.");
    const reference = bytesToHex(crypto.getRandomValues(new Uint8Array(16)));
    this.references.set(reference, {
      account: account.toLowerCase() as HexString,
      madeAt: this.now(),
    });
    return reference;
  }

  private now(): number {
    return this.deps.now?.() ?? Date.now();
  }

  private resolveReference(reference: string): Uint8Array {
    const entry = this.references.get(reference);
    this.references.delete(reference);
    if (
      entry === undefined ||
      this.now() - entry.madeAt > START_REFERENCE_TTL_MS
    )
      throw new Error("That reference names no one: choose the person again.");
    if (this.deps.blocked?.(entry.account))
      throw new Error("You blocked this person.");
    return hexToBytes(entry.account);
  }

  /** The workers the host adds: the Nova chat worker, in worker mode. */
  workerNames(): string[] {
    return this.mode() === "worker" ? [NOVA_WORKER_PRODUCT] : [];
  }

  resolves(name: string): boolean {
    return name === NOVA_WORKER_PRODUCT && this.mode() === "worker";
  }

  /** The worker, as the dev server describes the archive it built. */
  async resolve(): Promise<ResolvedWorker> {
    const response = await (this.deps.fetch ?? fetch)(
      `${this.deps.base}${NOVA_WORKER_PATH}`,
      { cache: "no-store" },
    );
    if (!response.ok)
      throw new Error(
        `The dev server did not build the Nova chat worker: ${await response.text()}`,
      );
    const answer = (await response.json()) as DemoWorkerAnswer;
    return {
      record: answer.record,
      cid: answer.cid,
      kind: answer.kind,
      manifest: parseWorkerManifest(answer.manifest),
      gateway: this.deps.origin,
    };
  }

  /**
   * A worker frame of `productId` asks for the identity service. Only the
   * Nova chat worker, in worker mode, with checked keys, gets it.
   */
  connect(productId: string, port: IdentityPort): void {
    const session = this.session;
    const keys =
      productId === NOVA_WORKER_PRODUCT &&
      session !== null &&
      this.mode() === "worker"
        ? (this.keys ??= session.keys())
        : null;
    const refusal =
      productId !== NOVA_WORKER_PRODUCT
        ? `${productId} may not use this wallet's chat identity.`
        : session === null
          ? "No wallet is signed in."
          : this.mode() !== "worker"
            ? "Chat runs in the host, not in this worker."
            : typeof keys === "string"
              ? keys
              : null;
    if (
      refusal !== null ||
      session === null ||
      keys === null ||
      typeof keys === "string"
    ) {
      refuseNovaIdentity(port, refusal ?? "Refused.");
      this.deps.log(`chat identity refused to ${productId}: ${refusal}`);
      if (productId === NOVA_WORKER_PRODUCT && this.mode() === "worker") {
        this.status = { state: "refused", reason: refusal ?? "Refused." };
        this.deps.changed();
      }
      return;
    }
    this.endService();
    const counts = { calls: 0, refused: 0 };
    this.status = { state: "connected", ...counts };
    this.service = serveNovaIdentity(port, {
      identity: localNovaIdentity(keys),
      resolveContact: (reference) => this.resolveReference(reference),
      // Taken when the worker asks, so a worker that stops before it asks
      // still gets the copy when it connects again.
      history: () => {
        if (!this.copyNext) return null;
        this.copyNext = false;
        return this.deps.storage.getItem(
          chatRecordKey(session.network, session.walletId),
        );
      },
      onCall: (_method, ok) => {
        if (ok) counts.calls += 1;
        else counts.refused += 1;
        this.status = { state: "connected", ...counts };
        this.deps.linkChanged?.();
      },
    });
    this.deps.log(
      "The Nova chat worker connected to this wallet's chat identity.",
    );
    this.deps.changed();
  }

  /** The worker of `productId` stopped: its service ends with it. */
  disconnect(productId: string): void {
    if (productId !== NOVA_WORKER_PRODUCT || this.service === null) return;
    this.endService();
    this.status =
      this.mode() === "worker" ? { state: "waiting" } : { state: "off" };
  }

  private endService(): void {
    this.service?.close();
    this.service = null;
    this.references.clear();
  }
}
