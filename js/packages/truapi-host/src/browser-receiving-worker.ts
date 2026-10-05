import type { ReceivingEvent, ReceivingWatch } from "@parity/truapi";
import {
  ReceivingAuthority, createNotificationReceiverCallbacks,
  receivingRegistrationsCodec, receivingEventsCodec, receivingEventCodec,
  type RawNotificationReceiver, type ReceivingRegistration,
} from "./runtime.js";
import {
  BrowserReceivingTransport, createReceiverEnrollment, randomReceiverToken,
  type BrowserReceiverEnrollment,
} from "./browser-receiving-transport.js";

export interface BrowserReceivingWorkerOptions {
  scope: ServiceWorkerGlobalScope;
  createReceiver(callbacks: {
    receiverAuthority(productId: string): Promise<Uint8Array | undefined>;
    receiverConsent(authority: Uint8Array, watches: Uint8Array): Promise<boolean>;
    receiverChanged(): Promise<void>;
    readReceivingState(): Promise<Uint8Array | undefined>;
    writeReceivingState(bytes: Uint8Array): Promise<void>;
  }): Promise<RawNotificationReceiver> | RawNotificationReceiver;
  relayUrl: string;
  /** Must equal the relay's configured PUSH_ORIGIN, not a product URL. */
  pushOrigin: string;
  /** Fixed trusted same-origin host entry. Routes in events are never navigated. */
  hostEntryUrl: string;
  notificationTitle: string;
  databaseName?: string;
}

interface AuthorityEntry { authority: ReceivingAuthority; revoked: boolean }
interface AccountScope { account: string; environment: string; genesis: string }
interface Execution { authority: ReceivingAuthority; clientId: string; ready: boolean }
interface Activation { authority: ReceivingAuthority; revision: bigint; eventId: string; expiresAt: bigint }
interface Wake { v: 2; deviceId: string; routeToken: string; messageId: string; revision: number }
interface SyncRegistration { enrollment: BrowserReceiverEnrollment; registration?: ReceivingRegistration }

function sameScope(left: ReceivingAuthority, right: ReceivingAuthority): boolean {
  return left.productId === right.productId && left.account === right.account &&
    left.environment === right.environment && left.artifact === right.artifact &&
    left.genesis === right.genesis && left.generation === right.generation;
}

function sameAccount(scope: AccountScope, authority: ReceivingAuthority): boolean {
  return scope.account === authority.account && scope.environment === authority.environment && scope.genesis === authority.genesis;
}

class ReceiverDatabase {
  private readonly opened: Promise<IDBDatabase>;
  constructor(name: string) {
    this.opened = new Promise((resolve, reject) => {
      const request = indexedDB.open(name, 1);
      request.onupgradeneeded = () => request.result.createObjectStore("receiver");
      request.onerror = () => reject(request.error);
      request.onblocked = () => reject(new Error("receiving database upgrade blocked"));
      request.onsuccess = () => {
        request.result.onversionchange = () => request.result.close();
        resolve(request.result);
      };
    });
  }
  async get<T>(key: string): Promise<T | undefined> {
    const database = await this.opened;
    return new Promise((resolve, reject) => {
      const transaction = database.transaction("receiver", "readonly");
      const request = transaction.objectStore("receiver").get(key);
      transaction.oncomplete = () => resolve(request.result as T | undefined);
      transaction.onabort = () => reject(transaction.error);
      transaction.onerror = () => reject(transaction.error);
    });
  }
  async update<T>(key: string, change: (previous: T | undefined) => T | undefined): Promise<void> {
    const database = await this.opened;
    return new Promise((resolve, reject) => {
      const transaction = database.transaction("receiver", "readwrite");
      const store = transaction.objectStore("receiver");
      const request = store.get(key);
      let failure: unknown;
      request.onsuccess = () => {
        try {
          const next = change(request.result as T | undefined);
          if (next === undefined) store.delete(key); else store.put(next, key);
        } catch (error) { failure = error; transaction.abort(); }
      };
      transaction.oncomplete = () => resolve();
      transaction.onabort = () => reject(failure ?? transaction.error);
      transaction.onerror = () => reject(transaction.error);
    });
  }
  put<T>(key: string, value: T): Promise<void> { return this.update<T>(key, () => value); }
  async entries<T>(prefix: string): Promise<Array<[string, T]>> {
    const database = await this.opened;
    return new Promise((resolve, reject) => {
      const transaction = database.transaction("receiver", "readonly");
      const request = transaction.objectStore("receiver").openCursor(IDBKeyRange.bound(prefix, `${prefix}\uffff`));
      const values: Array<[string, T]> = [];
      request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) return;
        values.push([String(cursor.key), cursor.value as T]);
        cursor.continue();
      };
      transaction.oncomplete = () => resolve(values);
      transaction.onabort = () => reject(transaction.error);
      transaction.onerror = () => reject(transaction.error);
    });
  }
}

/** Compose into the host's existing service worker. Installs no competing worker. */
export function installBrowserReceivingWorker(options: BrowserReceivingWorkerOptions): void {
  const { scope } = options;
  const entryUrl = new URL(options.hostEntryUrl, scope.location.origin);
  if (entryUrl.origin !== scope.location.origin || entryUrl.username || entryUrl.password) throw new Error("receiving host entry must be same-origin");
  if (options.pushOrigin !== scope.location.origin) throw new Error("receiving origin must match the trusted host");
  if (!scope.navigator.locks) throw new Error("durable receiving requires Web Locks");
  const databaseName = options.databaseName ?? "truapi-browser-receiving";
  const database = new ReceiverDatabase(databaseName);
  const transport = new BrowserReceivingTransport(options.relayUrl, options.pushOrigin);
  const executions = new Map<string, Execution>();
  const activating = new Set<string>();
  let synchronization: Promise<void> | undefined;

  async function askHost(clientId: string, operation: "consent" | "activate", authority: ReceivingAuthority, value: ReceivingWatch[] | ReceivingEvent): Promise<boolean> {
    const client = await scope.clients.get(clientId);
    if (!client || client.type !== "window" || new URL(client.url).origin !== scope.location.origin || (client as WindowClient).frameType !== "top-level") return false;
    const channel = new MessageChannel();
    return new Promise(resolve => {
      const timer = setTimeout(() => finish(false), 25_000);
      function finish(value: boolean) { clearTimeout(timer); channel.port1.close(); resolve(value); }
      channel.port1.onmessage = event => finish(event.data === true);
      channel.port1.onmessageerror = () => finish(false);
      client.postMessage({ type: "truapi:receiving-host", operation, authority,
        ...(operation === "consent" ? { watches: value } : { event: value }) }, [channel.port2]);
    });
  }

  async function liveAuthority(product: string): Promise<ReceivingAuthority | undefined> {
    const entry = await database.get<AuthorityEntry>(`authority:${product}`);
    if (!entry || entry.revoked) return undefined;
    const account = await database.get<AccountScope>("activeAccount");
    if (!account || !sameAccount(account, entry.authority)) return undefined;
    const destination = await database.get<PushSubscriptionJSON>("destination");
    return { ...entry.authority, osPermission: Notification.permission === "granted",
      transportReady: Boolean(destination?.endpoint) };
  }

  // Reconstruct for every exclusive lease: a replaced worker must never use a stale in-memory ledger.
  async function withCore<T>(operation: (receiver: RawNotificationReceiver) => Promise<T>, consentClient?: string, executionLive?: () => boolean): Promise<T> {
    const abort = new AbortController();
    const timer = setTimeout(() => abort.abort(), 40_000);
    try {
      return await scope.navigator.locks.request(`${databaseName}:writer`, { signal: abort.signal }, async () => {
        clearTimeout(timer);
        const receiver = await options.createReceiver(createNotificationReceiverCallbacks({
          receiverAuthority: async product => {
            if (executionLive && !executionLive()) return undefined;
            const authority = await liveAuthority(product);
            return executionLive && !executionLive() ? undefined : authority;
          },
          receiverConsent: (authority, watches) => consentClient && (!executionLive || executionLive())
            ? askHost(consentClient, "consent", authority, watches) : false,
          receiverChanged: () => database.put("syncRequested", true),
          readReceivingState: () => database.get<Uint8Array>("ledger"),
          writeReceivingState: bytes => database.put("ledger", bytes),
        }));
        try { return await operation(receiver); } finally { receiver.free(); }
      });
    } finally { clearTimeout(timer); }
  }

  async function queueRetry(): Promise<void> {
    await database.put("syncRequested", true);
    const registration = scope.registration as ServiceWorkerRegistration & { sync?: { register(tag: string): Promise<void> } };
    await registration.sync?.register("truapi:receiving").catch(() => {});
    const periodic = scope.registration as ServiceWorkerRegistration & { periodicSync?: { register(tag: string, options: { minInterval: number }): Promise<void> } };
    await periodic.periodicSync?.register("truapi:receiving", { minInterval: 12 * 60 * 60 * 1000 }).catch(() => {});
  }

  async function revokeEnrollments(product: string, authority?: ReceivingAuthority): Promise<void> {
    for (const [key, enrollment] of await database.entries<BrowserReceiverEnrollment>("enrollment:")) {
      if (enrollment.authority.productId !== product || enrollment.revoked) continue;
      if (authority && !sameScope(enrollment.authority, authority)) continue;
      await database.put(key, { ...enrollment, revoked: true, acknowledged: false, relayRevision: enrollment.relayRevision + 1 });
    }
  }

  async function refreshDestination(): Promise<void> {
    const subscription = await scope.registration.pushManager.getSubscription();
    const destination = subscription?.toJSON();
    const prior = await database.get<PushSubscriptionJSON>("destination");
    if (JSON.stringify(destination) === JSON.stringify(prior)) return;
    await withCore(async receiver => {
      await database.put("destination", destination);
      for (const registration of receivingRegistrationsCodec.dec(await receiver.receivingPending())) {
        if (registration.enabled) await receiver.receivingMarkTransportChanged(registration.authority.productId);
      }
    });
  }

  async function prepareSync(): Promise<SyncRegistration[]> {
    return withCore(async receiver => {
      const registrations = receivingRegistrationsCodec.dec(await receiver.receivingPending());
      for (const [, enrollment] of await database.entries<BrowserReceiverEnrollment>("enrollment:")) {
        const authority = await database.get<AuthorityEntry>(`authority:${enrollment.authority.productId}`);
        if (!enrollment.revoked && (authority?.revoked || (authority && !sameScope(authority.authority, enrollment.authority)))) {
          await database.put(`enrollment:${enrollment.deviceId}`, {
            ...enrollment, revoked: true, acknowledged: false, relayRevision: enrollment.relayRevision + 1,
          });
        }
      }
      const destination = await database.get<PushSubscriptionJSON>("destination");
      const pending: SyncRegistration[] = [];
      for (const registration of registrations) {
        const product = registration.authority.productId;
        if (!registration.enabled) {
          await revokeEnrollments(product, registration.authority);
          const existing = (await database.entries<BrowserReceiverEnrollment>("enrollment:")).some(([, item]) => sameScope(item.authority, registration.authority));
          if (!existing && registration.syncPending) await receiver.receivingSynchronized(product, registration.revision);
          continue;
        }
        if (!destination?.endpoint) continue;
        let enrollment = (await database.entries<BrowserReceiverEnrollment>("enrollment:"))
          .map(([, value]) => value).find(value => !value.revoked && sameScope(value.authority, registration.authority));
        if (!enrollment) enrollment = await createReceiverEnrollment(registration.authority);
        if (!enrollment.acknowledged || registration.syncPending || enrollment.coreRevision !== registration.revision) {
          if (enrollment.acknowledged || enrollment.relayRevision === 0 || enrollment.coreRevision !== registration.revision) {
            enrollment.relayRevision++;
            if (!Number.isSafeInteger(enrollment.relayRevision)) throw new Error("relay revision exhausted");
            enrollment.coreRevision = registration.revision;
            enrollment.routes = Object.fromEntries(registration.watches.map(watch => [watch.id, enrollment!.routes[watch.id] ?? randomReceiverToken()]));
          }
          enrollment.acknowledged = false;
          await database.put(`enrollment:${enrollment.deviceId}`, enrollment);
          pending.push({ enrollment, registration });
        }
      }
      for (const [, enrollment] of await database.entries<BrowserReceiverEnrollment>("enrollment:")) {
        if (enrollment.revoked && !enrollment.acknowledged) pending.push({ enrollment });
      }
      return pending;
    });
  }

  function synchronize(): Promise<void> {
    if (synchronization) return synchronization;
    synchronization = scope.navigator.locks.request<void>(`${databaseName}:relay`, async () => {
      const retry = await database.get<{ attempts: number; after: number }>("retry");
      if (retry && retry.after > Date.now()) { await queueRetry(); return; }
      const deadline = Date.now() + 25_000;
      await database.put("syncRequested", false);
      const jobs = await prepareSync();
      let failed = false;
      for (const { enrollment, registration } of jobs) {
        if (Date.now() >= deadline) { failed = true; break; }
        try {
          if (enrollment.revoked) await transport.revoke(enrollment);
          else {
            const authority = await liveAuthority(enrollment.authority.productId);
            const current = await database.get<BrowserReceiverEnrollment>(`enrollment:${enrollment.deviceId}`);
            if (!authority || !sameScope(authority, enrollment.authority) || !current || current.revoked || current.relayRevision !== enrollment.relayRevision) continue;
            const destination = await database.get<PushSubscriptionJSON>("destination");
            if (!destination || !registration) continue;
            await transport.register(enrollment, registration, destination);
          }
          await withCore(async receiver => {
            const current = await database.get<BrowserReceiverEnrollment>(`enrollment:${enrollment.deviceId}`);
            if (!current || current.relayRevision !== enrollment.relayRevision || current.revoked !== enrollment.revoked) return;
            await database.put(`enrollment:${enrollment.deviceId}`, enrollment.revoked ? undefined : { ...current, acknowledged: true });
            const pending = receivingRegistrationsCodec.dec(await receiver.receivingPending())
              .find(value => sameScope(value.authority, enrollment.authority));
            const deletionPending = (await database.entries<BrowserReceiverEnrollment>("enrollment:"))
              .some(([, value]) => sameScope(value.authority, enrollment.authority) && value.revoked && !value.acknowledged);
            if (pending && pending.enabled === !enrollment.revoked &&
                (enrollment.revoked ? !deletionPending : pending.revision === enrollment.coreRevision && sameScope(pending.authority, enrollment.authority))) {
              await receiver.receivingSynchronized(pending.authority.productId, pending.revision);
            }
          });
        } catch { failed = true; }
      }
      for (const [key, pending] of await database.entries<{ wake: Wake; receivedAt: number }>("wake:")) {
        if (Date.now() >= deadline) { failed = true; break; }
        try {
          if (pending.receivedAt + 60 * 60 * 1000 > Date.now()) await receivePush(pending.wake);
          await database.put(key, undefined);
        } catch { failed = true; }
      }
      if (failed) {
        const attempts = Math.min((retry?.attempts ?? 0) + 1, 8);
        await database.put("retry", { attempts, after: Date.now() + Math.min(30_000 * 2 ** (attempts - 1), 60 * 60 * 1000) });
      } else await database.put("retry", undefined);
      if (failed || await database.get<boolean>("syncRequested")) await queueRetry();
      const periodic = scope.registration as ServiceWorkerRegistration & { periodicSync?: { register(tag: string, options: { minInterval: number }): Promise<void> } };
      await periodic.periodicSync?.register("truapi:receiving", { minInterval: 12 * 60 * 60 * 1000 }).catch(() => {});
    }).catch(async () => { await queueRetry(); }).finally(() => { synchronization = undefined; });
    return synchronization;
  }

  async function trustedClient(event: ExtendableMessageEvent): Promise<WindowClient> {
    if (!event.source || !("id" in event.source)) throw new Error("receiving requires a host window");
    const client = await scope.clients.get(event.source.id);
    if (!client || client.type !== "window" || new URL(client.url).origin !== scope.location.origin || (client as WindowClient).frameType !== "top-level") {
      throw new Error("untrusted receiving message source");
    }
    return client as WindowClient;
  }

  function boundExecution(id: string, clientId: string): Execution {
    const execution = executions.get(id);
    if (!execution || execution.clientId !== clientId) throw new Error("receiving execution unavailable; bind again after worker restart");
    return execution;
  }

  async function deliverActivation(key: string, activation: Activation, clientId: string): Promise<void> {
    if (activating.has(key)) return;
    activating.add(key);
    try {
      if (activation.expiresAt <= BigInt(Date.now())) { await database.put(key, undefined); return; }
      const preview = await withCore(async receiver => receivingEventCodec.dec(await receiver.receivingValidateActivation(
        activation.authority.productId, activation.revision, activation.eventId)));
      if (!preview) { await database.put(key, undefined); return; }
      const authority = await liveAuthority(activation.authority.productId);
      if (!authority || !sameScope(authority, activation.authority)) return;
      if (!await askHost(clientId, "activate", activation.authority, preview)) return;
      const executionLive = () => {
        for (const execution of executions.values()) {
          if (execution.ready && execution.clientId === clientId && sameScope(execution.authority, activation.authority)) return true;
        }
        return false;
      };
      await withCore(async receiver => {
        const current = await liveAuthority(activation.authority.productId);
        if (!current || !sameScope(current, activation.authority)) return;
        if (!executionLive()) return;
        const event = receivingEventCodec.dec(await receiver.receivingActivate(activation.authority.productId, activation.revision, activation.eventId));
        if (event) await database.put(key, undefined);
      }, undefined, executionLive);
    } finally { activating.delete(key); }
  }

  async function dispatch(event: ExtendableMessageEvent): Promise<unknown> {
    const client = await trustedClient(event);
    const { operation, value } = event.data;
    switch (operation) {
      case "getAuthority": {
        const entry = await database.get<AuthorityEntry>(`authority:${String(value)}`);
        return entry ? { ...entry.authority, revoked: entry.revoked } : undefined;
      }
      case "activeAccount": {
        if (!value || (value.account !== undefined && (typeof value.account !== "string" || !/^[0-9a-f]{64}$/.test(value.account))) ||
            typeof value.environment !== "string" || value.environment.length < 1 || value.environment.length > 128 ||
            typeof value.genesis !== "string" || !/^[0-9a-f]{64}$/.test(value.genesis)) throw new Error("invalid receiving account scope");
        const account: AccountScope | undefined = value.account === undefined ? undefined
          : { account: value.account, environment: value.environment, genesis: value.genesis };
        // Commit this fence without waiting behind an outstanding consent prompt.
        await database.put("activeAccount", account);
        for (const [id, execution] of executions) {
          if (!account || !sameAccount(account, execution.authority)) executions.delete(id);
        }
        if (account) {
          for (const [key, entry] of await database.entries<AuthorityEntry>("authority:")) {
            if (entry.revoked || sameAccount(account, entry.authority)) continue;
            await database.update<AuthorityEntry>(key, current => current && !sameAccount(account, current.authority)
              ? { ...current, revoked: true } : current);
            await withCore(async receiver => {
              const current = await database.get<AuthorityEntry>(key);
              if (!current?.revoked) return;
              await receiver.receivingRevoke(entry.authority.productId);
              await revokeEnrollments(entry.authority.productId);
            });
          }
        }
        return;
      }
      case "revokeAll": {
        // Durable host-global fence comes first; absent products are included.
        await database.put("activeAccount", undefined);
        executions.clear();
        const entries = await database.entries<AuthorityEntry>("authority:");
        for (const [key] of entries) {
          await database.update<AuthorityEntry>(key, current => current ? { ...current, revoked: true } : current);
        }
        await withCore(async receiver => {
          for (const [, entry] of entries) {
            await receiver.receivingRevoke(entry.authority.productId);
            await revokeEnrollments(entry.authority.productId);
          }
        });
        return;
      }
      case "authority": {
        const authority = ReceivingAuthority.dec(ReceivingAuthority.enc(value));
        const account = await database.get<AccountScope>("activeAccount");
        if (!account || !sameAccount(account, authority)) throw new Error("receiving account is not active");
        let replaced = false;
        // This transaction deliberately does not wait behind a consent prompt. Core rechecks it after consent.
        await database.update<AuthorityEntry>(`authority:${authority.productId}`, previous => {
          if (previous && (authority.generation < previous.authority.generation ||
              ((!sameScope(previous.authority, authority) || previous.revoked) && authority.generation <= previous.authority.generation))) {
            throw new Error("stale receiving authority generation");
          }
          replaced = Boolean(previous && (!sameScope(previous.authority, authority) || previous.revoked));
          return { authority, revoked: false };
        });
        if (replaced) await withCore(async receiver => {
          await receiver.receivingRevoke(authority.productId);
          await revokeEnrollments(authority.productId);
        });
        return;
      }
      case "revoke": {
        const product = String(value);
        await database.update<AuthorityEntry>(`authority:${product}`, previous => previous ? { ...previous, revoked: true } : undefined);
        await withCore(async receiver => { await receiver.receivingRevoke(product); await revokeEnrollments(product); });
        for (const [id, execution] of executions) if (execution.authority.productId === product) executions.delete(id);
        return;
      }
      case "bind": {
        const authority = ReceivingAuthority.dec(ReceivingAuthority.enc(value));
        const current = await liveAuthority(authority.productId);
        if (!current || !sameScope(current, authority)) throw new Error("receiving authority mismatch");
        if (executions.size >= 512) {
          for (const [id, execution] of executions) if (!await scope.clients.get(execution.clientId)) executions.delete(id);
          if (executions.size >= 512) throw new Error("receiving execution capacity");
        }
        const id = randomReceiverToken();
        executions.set(id, { authority, clientId: client.id, ready: false });
        return id;
      }
      case "command": {
        const execution = boundExecution(value.id, client.id);
        if (!Number.isInteger(value.action) || value.action < 2 || value.action > 7 || !(value.payload instanceof Uint8Array) || value.payload.byteLength > 2 * 1024 * 1024) throw new Error("invalid receiving command");
        return withCore(receiver => receiver.commandForExecution(ReceivingAuthority.enc(execution.authority), value.action, value.payload),
          client.id, () => executions.get(value.id) === execution);
      }
      case "unbind": boundExecution(value.id, client.id); executions.delete(value.id); return;
      case "ready": {
        const execution = boundExecution(value.id, client.id);
        const authority = await liveAuthority(execution.authority.productId);
        if (!authority || !sameScope(authority, execution.authority)) throw new Error("receiving execution expired");
        execution.ready = true;
        event.waitUntil((async () => {
          for (const [key, activation] of await database.entries<Activation>("activation:")) {
            if (sameScope(activation.authority, execution.authority)) await deliverActivation(key, activation, client.id);
          }
        })());
        return;
      }
      case "refresh": await refreshDestination(); return;
      default: throw new Error("unknown receiving operation");
    }
  }

  scope.addEventListener("message", event => {
    if (event.data?.type !== "truapi:receiving" || !event.ports[0]) return;
    const port = event.ports[0];
    event.waitUntil(dispatch(event).then(value => port.postMessage({ ok: true, value }), error => {
      port.postMessage({ ok: false, error: error instanceof Error ? error.message : "receiving operation failed" });
    }).finally(() => port.close()).then(() => synchronize()));
  });

  async function receivePush(wake: Wake): Promise<void> {
    if (wake.v !== 2 || !/^[a-f0-9]{64}$/.test(wake.deviceId) || !/^[a-f0-9]{64}$/.test(wake.routeToken) || !Number.isSafeInteger(wake.revision)) return;
    const enrollment = await database.get<BrowserReceiverEnrollment>(`enrollment:${wake.deviceId}`);
    if (!enrollment || enrollment.revoked || enrollment.relayRevision !== wake.revision) return;
    const product = enrollment.authority.productId;
    const authority = await liveAuthority(product);
    if (!authority || !sameScope(authority, enrollment.authority)) return;
    const retained = await transport.event(enrollment, wake.messageId);
    if (enrollment.routes[retained.watchId] !== wake.routeToken) return;
    const events = await withCore(async receiver => {
      const current = await database.get<BrowserReceiverEnrollment>(`enrollment:${wake.deviceId}`);
      if (!current || current.revoked || current.relayRevision !== wake.revision) return [];
      return receivingEventsCodec.dec(await receiver.receivingIngest(product, enrollment.coreRevision,
        retained.watchId, retained.genesis, retained.channel, retained.topics, retained.frame));
    });
    // The relay supplies the authenticated frame plus actual source metadata, not a SCALE statement.
    // Other signed siblings may become durable events, but this wake may display only its exact event.
    const accepted = events.find(event => event.eventId === wake.messageId);
    // A retry can find an already-ingested event whose prior OS display explicitly failed.
    const eventId = accepted?.eventId ?? wake.messageId;
    await new Promise(resolve => setTimeout(resolve, 2100));
    await withCore(async receiver => {
      const display = receivingEventCodec.dec(await receiver.receivingPrepareDisplay(product, enrollment.coreRevision, eventId));
      if (!display) return;
      const current = await liveAuthority(product);
      if (!current || !sameScope(current, enrollment.authority) || !current.osPermission) return;
      try {
        await scope.registration.showNotification(options.notificationTitle, {
          body: "New activity", tag: `truapi:${wake.deviceId}:${eventId}`,
          data: { type: "truapi:receiving", authority: enrollment.authority, revision: enrollment.coreRevision.toString(), eventId },
        });
      } catch (error) {
        await receiver.receivingCancelDisplay(product, enrollment.coreRevision, eventId);
        throw error;
      }
      await receiver.receivingConfirmDisplay(product, enrollment.coreRevision, eventId);
    });
  }

  scope.addEventListener("push", event => {
    if (!event.data) return;
    let wake: Wake;
    try { if (event.data.text().length > 4096) return; wake = event.data.json() as Wake; } catch { return; }
    if (wake?.v !== 2) return;
    event.waitUntil((async () => {
      if (!/^[a-f0-9]{64}$/.test(wake.deviceId) || typeof wake.messageId !== "string" || !/^[A-Za-z0-9._:-]{1,256}$/.test(wake.messageId)) return;
      const prefix = `wake:${wake.deviceId}:`;
      const pending = await database.entries<{ wake: Wake; receivedAt: number }>(prefix);
      pending.sort((left, right) => left[1].receivedAt - right[1].receivedAt);
      for (const [key] of pending.slice(0, Math.max(0, pending.length - 3))) await database.put(key, undefined);
      const key = `${prefix}${wake.messageId}`;
      await database.put(key, { wake, receivedAt: Date.now() });
      try { await receivePush(wake); await database.put(key, undefined); } catch { await queueRetry(); }
      await synchronize();
    })());
  });

  scope.addEventListener("notificationclick", event => {
    if (event.notification.data?.type !== "truapi:receiving") return;
    event.notification.close();
    event.waitUntil((async () => {
      const { authority, revision, eventId } = event.notification.data as { authority: ReceivingAuthority; revision: string; eventId: string };
      const current = await liveAuthority(authority.productId);
      if (!current || !sameScope(current, authority)) return;
      const preview = await withCore(async receiver => receivingEventCodec.dec(await receiver.receivingValidateActivation(authority.productId, BigInt(revision), eventId)));
      if (!preview) return;
      const activation: Activation = { authority, revision: BigInt(revision), eventId, expiresAt: preview.expiresAt };
      const key = `activation:${authority.productId}:${eventId}`;
      await database.put(key, activation);
      for (const execution of executions.values()) {
        if (!execution.ready || !sameScope(execution.authority, authority)) continue;
        const client = await scope.clients.get(execution.clientId) as WindowClient | undefined;
        if (!client) continue;
        await client.focus();
        await deliverActivation(key, activation, client.id);
        return;
      }
      const clients = await scope.clients.matchAll({ type: "window", includeUncontrolled: true });
      const host = clients.find(client => client.frameType === "top-level" && new URL(client.url).origin === scope.location.origin);
      if (host) { await host.focus(); await deliverActivation(key, activation, host.id); }
      else {
        const opened = await scope.clients.openWindow(entryUrl.href);
        if (opened) await deliverActivation(key, activation, opened.id);
      }
    })());
  });

  scope.addEventListener("pushsubscriptionchange", event => {
    const changed = event as ExtendableEvent & { oldSubscription?: PushSubscription | null };
    changed.waitUntil((async () => {
      const key = changed.oldSubscription?.options.applicationServerKey;
      if (key && Notification.permission === "granted") await scope.registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key });
      await refreshDestination();
      await synchronize();
    })().catch(() => queueRetry()));
  });
  scope.addEventListener("activate", event => { event.waitUntil(refreshDestination().then(() => synchronize()).catch(() => queueRetry())); });
  for (const type of ["sync", "periodicsync"] as const) {
    scope.addEventListener(type, ((event: ExtendableEvent & { tag: string }) => {
      if (event.tag === "truapi:receiving") event.waitUntil(refreshDestination().then(() => synchronize()).then(async () => {
        if (await database.get<boolean>("syncRequested")) throw new Error("receiving synchronization remains pending");
      }));
    }) as EventListener);
  }
}
