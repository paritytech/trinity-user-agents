import type { ReceivingEvent, ReceivingWatch } from "@parity/truapi";
import type { ReceivingAuthority } from "./runtime.js";

export interface BrowserReceivingClientOptions {
  registration: ServiceWorkerRegistration;
  /** Trusted host UI. An OS notification grant is not receiving consent. */
  consent(authority: ReceivingAuthority, watches: ReceivingWatch[]): Promise<boolean>;
  /** Return true only after the exact verified product is ready in the unlocked account. */
  activate(authority: ReceivingAuthority, event: ReceivingEvent): Promise<boolean>;
}

export interface BrowserReceivingExecution {
  command(action: number, payload: Uint8Array): Promise<Uint8Array>;
  ready(): Promise<void>;
  close(): void;
}

export interface BrowserReceivingAuthorityState extends ReceivingAuthority {
  revoked: boolean;
}

export interface BrowserReceivingClient {
  getAuthority(productId: string): Promise<BrowserReceivingAuthorityState | undefined>;
  updateAuthority(authority: ReceivingAuthority): Promise<void>;
  /** Update only from the current trusted host selection, never a product claim.
   * Undefined pauses; closing a product/page must not call this method. */
  setActiveAccount(account: string | undefined, environment: string, genesis: string): Promise<void>;
  bindExecution(authority: ReceivingAuthority): Promise<BrowserReceivingExecution>;
  enableWebPush(vapidPublicKey: string): Promise<void>;
  revoke(productId: string): Promise<void>;
  /** Explicit host logout/erase, including products with no open execution. */
  revokeAll(): Promise<void>;
  refresh(): Promise<void>;
  close(): void;
}

/** Use only in the trusted host page, never in a product iframe. */
export function createBrowserReceivingClient(options: BrowserReceivingClientOptions): BrowserReceivingClient {
  const { registration } = options;
  let closed = false;
  const executions = new Set<string>();
  async function request<T>(operation: string, value?: unknown): Promise<T> {
    if (closed) throw new Error("receiving client closed");
    const worker = registration.active;
    if (!worker) throw new Error("receiving service worker unavailable");
    const channel = new MessageChannel();
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => finish(new Error("receiving service worker stalled")), 45_000);
      function finish(error?: Error, result?: T) {
        clearTimeout(timer);
        channel.port1.close();
        if (error) reject(error); else resolve(result as T);
      }
      channel.port1.onmessage = event => {
        const reply = event.data;
        if (!reply || typeof reply.ok !== "boolean") { finish(new Error("invalid receiving worker reply")); return; }
        finish(reply.ok ? undefined : new Error(String(reply.error)), reply.value);
      };
      channel.port1.onmessageerror = () => finish(new Error("receiving worker message failed"));
      try { worker.postMessage({ type: "truapi:receiving", operation, value }, [channel.port2]); }
      catch (error) { finish(error instanceof Error ? error : new Error(String(error))); }
    });
  }
  const onMessage = (message: MessageEvent) => {
    if (closed || message.source !== registration.active || message.data?.type !== "truapi:receiving-host" || !message.ports[0]) return;
    const port = message.ports[0];
    const { operation, authority, watches, event } = message.data;
    void Promise.resolve().then(() => operation === "consent" ? options.consent(authority, watches)
      : operation === "activate" ? options.activate(authority, event) : false)
      .then(value => port.postMessage(value === true), () => port.postMessage(false)).finally(() => port.close());
  };
  navigator.serviceWorker.addEventListener("message", onMessage);
  const onOnline = () => { void request("refresh").catch(() => {}); };
  window.addEventListener("online", onOnline);
  return {
    getAuthority: (productId: string) => request<BrowserReceivingAuthorityState | undefined>("getAuthority", productId),
    updateAuthority: (authority: ReceivingAuthority) => request<void>("authority", structuredClone(authority)),
    setActiveAccount: (account, environment, genesis) => request<void>("activeAccount", { account, environment, genesis }),
    async bindExecution(authority: ReceivingAuthority): Promise<BrowserReceivingExecution> {
      const snapshot = structuredClone(authority);
      let id = await request<string>("bind", snapshot);
      executions.add(id);
      let disposed = false;
      let ready = false;
      async function execute<T>(operation: string, value: Record<string, unknown>): Promise<T> {
        if (disposed || !executions.has(id)) throw new Error("receiving execution closed");
        try { return await request<T>(operation, { ...value, id }); }
        catch (error) {
          // A missing lease guarantees the command never reached core. Never retry an ambiguous timeout.
          if (!(error instanceof Error) || error.message !== "receiving execution unavailable; bind again after worker restart" || !executions.has(id)) throw error;
          executions.delete(id);
          id = await request<string>("bind", snapshot);
          if (disposed || closed) {
            void request("unbind", { id }).catch(() => {});
            throw new Error("receiving execution closed");
          }
          executions.add(id);
          if (ready && operation !== "ready") await request("ready", { id });
          return request<T>(operation, { ...value, id });
        }
      }
      return {
        command: (action, payload) => execute<Uint8Array>("command", { action, payload }),
        ready: async () => { await execute<void>("ready", {}); ready = true; },
        close: () => {
          disposed = true;
          if (!executions.delete(id)) return;
          void request("unbind", { id }).catch(() => {});
        },
      };
    },
    /** Call directly from a user gesture; never from product startup. */
    async enableWebPush(vapidPublicKey: string): Promise<void> {
      if (!navigator.userActivation?.isActive) throw new Error("WebPush permission requires a user gesture");
      if (!("PushManager" in window) || !("Notification" in window)) throw new Error("WebPush unavailable");
      const permission = await Notification.requestPermission();
      if (permission !== "granted") { await request("refresh"); throw new Error("notification permission denied"); }
      const base64 = vapidPublicKey.replace(/-/g, "+").replace(/_/g, "/");
      const key = Uint8Array.from(atob(base64 + "=".repeat((4 - base64.length % 4) % 4)), value => value.charCodeAt(0));
      if (key.length !== 65 || key[0] !== 4) throw new Error("invalid WebPush VAPID public key");
      const existing = await registration.pushManager.getSubscription();
      const existingKey = existing?.options.applicationServerKey;
      if (existing && (!existingKey || new Uint8Array(existingKey).some((byte, index) => byte !== key[index]) || existingKey.byteLength !== key.length)) {
        await existing.unsubscribe();
      }
      try { await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key }); }
      finally { await request("refresh"); }
    },
    revoke: (productId: string) => request<void>("revoke", productId),
    revokeAll: () => request<void>("revokeAll"),
    refresh: () => request<void>("refresh"),
    close() {
      for (const id of executions) void request("unbind", { id }).catch(() => {});
      executions.clear();
      closed = true;
      navigator.serviceWorker.removeEventListener("message", onMessage);
      window.removeEventListener("online", onOnline);
    },
  };
}

export type { ReceivingAuthority } from "./runtime.js";
