import type { StatementAccountClaim } from "@parity/truapi-host/web";
import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import { equalBytes, utf8 } from "../../bridges/primitives/codec.js";
import {
  blake2b256,
  cryptoRandom,
  type RandomBytes,
} from "../../bridges/primitives/crypto.js";
import {
  decodeStatement,
  encodeSigned,
  signingPayload,
  verifyStatement,
} from "../../bridges/primitives/statement.js";
import {
  isStored,
  type StatementStoreLike,
} from "../platform/statement-store.js";
import {
  POINTER_LIFETIME_SECONDS,
  hex,
  newestPointer,
  pointerExpiry,
  pointerStatement,
  type Pointer,
  type PointerBody,
} from "./pointer.js";
import {
  backupKeys,
  compress,
  decompress,
  openSnapshot,
  sealSnapshot,
  type BackupBinding,
  type BackupKeys,
} from "./sealing.js";
import {
  applySnapshot,
  collectSnapshot,
  isEmpty,
  isFresh,
  readSnapshot,
  type BackupScope,
  type Snapshot,
} from "./snapshot.js";

/**
 * PROTOTYPE. The host's automatic backup of one signed-in wallet's local
 * state (`./snapshot.ts`), for a browser that signs in with the same wallet
 * later.
 *
 * While a wallet is signed in, one tab of this browser keeps its backup: it
 * takes a snapshot when the state changed and then held still for a minute
 * (what a crash at that moment would leave, not a checkpoint products
 * agreed to),
 * seals it (`./sealing.ts`), stores it on Bulletin under the host's backup
 * Bulletin account, reads it back, and points to it with a new pointer
 * (`./pointer.ts`) once the backup statement account's slot is final. Both
 * accounts are the core's, derived from the wallet; the identity signs
 * nothing. Once a day it claims that slot again, so the pointer lasts while
 * the browser that published it is open, and posts the pointer again when
 * the store lost it.
 *
 * A browser that signs in with a wallet it holds nothing of looks for the
 * pointer before the wallet's workers start, and restores the snapshot it
 * names. A browser that already holds state for the wallet keeps it: it
 * only says so when another browser published a different snapshot.
 *
 * Not sync. Two browsers of one wallet each back up their own state, and the
 * last pointer published wins; neither merges the other's. A restored
 * browser runs the same product state as the one it came from, Kettle's MLS
 * state included, so using both at once can fork a conversation.
 */

/**
 * The switch that turns the backup off in this browser: `"off"` under this
 * `localStorage` key. The backup is on otherwise. It runs only where the
 * host's core has the backup accounts ({@link HostBackup.prepare}).
 */
export const BACKUP_SWITCH_KEY = "truapi-web-signing-host:backup:switch";

/** Whether the backup is on in this browser: unless it was switched off. */
export const backupOn = (storage: Storage): boolean =>
  storage.getItem(BACKUP_SWITCH_KEY) !== "off";

/** The core's backup statement account: its key, and its signature over a pointer. */
export interface BackupCore {
  account(): Promise<Uint8Array>;
  signPointer(payload: Uint8Array): Promise<Uint8Array>;
}

/**
 * What the backup does on the network: claim the backup statement
 * account's slot and wait until it is final, store and fetch the sealed
 * snapshot on Bulletin, and read and post pointers in the Statement Store.
 */
export interface BackupNetwork {
  claimSlot(): Promise<StatementAccountClaim>;
  /** Resolves once `claim`'s allowance holds at the finalized head. */
  settled(claim: StatementAccountClaim): Promise<void>;
  /** Store `sealed` on Bulletin; answers its preimage key. */
  storeSnapshot(sealed: Uint8Array): Promise<Uint8Array>;
  /** The bytes Bulletin serves under `key`, or undefined when it serves none. */
  fetchSnapshot(key: Uint8Array): Promise<Uint8Array | undefined>;
  /** Open the People chain's Statement Store. */
  openStore(): Promise<StatementStoreLike>;
}

/** A signed-in wallet, as its backup needs it. */
export interface BackupSession extends BackupBinding, BackupScope {
  entropy: Uint8Array;
}

export interface HostBackupDeps {
  storage: Storage;
  core: BackupCore;
  network: BackupNetwork;
  /**
   * Wait until this browser's lock `name` is held, then resolve with its
   * release. Rejects when `signal` aborts first, or when the browser has no
   * locks: then nothing is restored or kept.
   */
  acquire(name: string, signal: AbortSignal): Promise<() => void>;
  log(line: string, failed?: boolean): void;
  now?(): number;
  /** Run `task` after `delayMs`; answers a cancel. */
  schedule?(task: () => void, delayMs: number): () => void;
  random?: RandomBytes;
  /** How long a pointer read waits for the store. */
  readMs?: number;
  /** How often the writer looks, and how long the state must hold still. Defaults: {@link CHECK_MS}, {@link QUIET_MS}. */
  pace?: { checkMs: number; quietMs: number };
}

/**
 * How a sign-in's look for a backup ended. Only `"restored"`, `"none"` and
 * `"kept"` let the session keep a backup: after `"failed"` a backup may
 * exist that this browser could not read, and its own would replace it;
 * `"unavailable"` means the host's core has no backup accounts, so nothing
 * was read or written.
 */
export type PrepareOutcome =
  | "restored"
  | "none"
  | "kept"
  | "failed"
  | "unavailable";

const SCHEMA = "truapi-web-host/backup/1";
const INSTALL_KEY = "truapi-web-signing-host:backup-install:v1";
const DAY_MS = 86_400_000;
/** How often the writer looks at the state. */
export const CHECK_MS = 60_000;
/** How long the state must hold still before it is taken. */
export const QUIET_MS = 60_000;
/** The shortest time between two snapshots. */
export const MIN_INTERVAL_MS = 30 * 60_000;
/** The most snapshots one wallet stores per UTC day from this browser, failed ones included. */
export const MAX_WRITES_PER_DAY = 4;
/** An unchanged snapshot is stored again this long after the last, before Bulletin lets it lapse. */
export const REFRESH_AFTER_MS = 12 * DAY_MS;
/** One Bulletin store's limit, `MaxTransactionSize` (2 MiB, read 2026-11-07), less a margin. */
export const MAX_SEALED_BYTES = 2_000_000;
const RETRY_FIRST_MS = 5 * 60_000;
const RETRY_MAX_MS = 6 * 60 * 60_000;
const READ_MS = 8_000;
/** How long a read waits after the first pointer it heard, for a newer one another node holds. */
const READ_MORE_MS = 750;
const READ_BACK_TRIES = 6;
const READ_BACK_WAIT_MS = 10_000;

/** What this browser keeps of a wallet's backup, next to the wallet's state and not in it. */
interface Kept {
  /** The last revision this browser published or restored. */
  revision: number;
  /** The pointer this browser last published or restored from. */
  pointer: PointerBody | null;
  /** The UTC day this browser last claimed the backup account's slot. */
  claimedDay: number | null;
  /** The UTC day `writes` counts. */
  day: number;
  writes: number;
}

const NOTHING_KEPT: Kept = {
  revision: 0,
  pointer: null,
  claimedDay: null,
  day: 0,
  writes: 0,
};

const keptKey = (scope: BackupScope) =>
  `truapi-web-signing-host:network:${scope.network}:backup:${scope.walletId}:v1`;

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

/** The digest of a snapshot's state: the same state, the same digest. */
export const digestOf = (snapshot: Snapshot): HexString =>
  hex(blake2b256(utf8(JSON.stringify(snapshot))));

/** The writer lock of `session`'s identity on its network. */
export const lockName = (session: BackupBinding): string =>
  `truapi-web-signing-host:backup:${session.network}:${session.identity.toLowerCase()}`;

/** The lock a restore holds from its freshness check to its last write, so two tabs never restore into one wallet. */
const restoreLockName = (session: BackupBinding): string =>
  `${lockName(session)}:restore`;

export class HostBackup {
  private session: BackupSession | null = null;
  private stopping = new AbortController();
  private cancelTimer = () => {};
  /** The digest seen at the last look, and since when: the state is taken once it held still. */
  private candidate: { digest: string; since: number } | null = null;
  private failures = 0;
  private retryAt = 0;
  /** Settles once no look of a stopped session is still running. */
  private idle: Promise<void> = Promise.resolve();
  private readonly now: () => number;
  private readonly schedule: (task: () => void, delayMs: number) => () => void;
  private readonly random: RandomBytes;
  private readonly pace: { checkMs: number; quietMs: number };

  constructor(private readonly deps: HostBackupDeps) {
    this.pace = deps.pace ?? { checkMs: CHECK_MS, quietMs: QUIET_MS };
    this.now = deps.now ?? (() => Date.now());
    this.schedule =
      deps.schedule ??
      ((task, delayMs) => {
        const timer = setTimeout(task, delayMs);
        return () => clearTimeout(timer);
      });
    this.random = deps.random ?? cryptoRandom;
  }

  /** This browser's random id, which names no wallet. */
  private source(): string {
    const kept = this.deps.storage.getItem(INSTALL_KEY);
    if (kept !== null) return kept;
    const made = bytesToHex(this.random(16));
    this.deps.storage.setItem(INSTALL_KEY, made);
    return made;
  }

  private kept(scope: BackupScope): Kept {
    try {
      return {
        ...NOTHING_KEPT,
        ...(JSON.parse(
          this.deps.storage.getItem(keptKey(scope)) ?? "{}",
        ) as Partial<Kept>),
      };
    } catch {
      return NOTHING_KEPT;
    }
  }

  private keep(scope: BackupScope, kept: Kept): void {
    this.deps.storage.setItem(keptKey(scope), JSON.stringify(kept));
  }

  /**
   * Before `session`'s workers and products start: restore its backup when
   * this browser holds nothing of the wallet on this network, or keep what
   * it holds. Only reads the network; never claims or stores. A core
   * without the backup accounts reads nothing. A failure leaves the browser
   * as it was, and so does a session `wanted` no longer wants once the
   * snapshot is read.
   */
  async prepare(
    session: BackupSession,
    wanted: () => boolean = () => true,
  ): Promise<PrepareOutcome> {
    try {
      await this.deps.core.account();
    } catch (error) {
      this.deps.log(
        `Backup: off in this session: this host's core has no backup accounts (${errorText(error)}).`,
      );
      return "unavailable";
    }
    let release = () => {};
    try {
      release = await this.deps.acquire(
        restoreLockName(session),
        new AbortController().signal,
      );
    } catch (error) {
      this.deps.log(`Backup: off in this browser: ${errorText(error)}`, true);
      return "failed";
    }
    try {
      return await this.restore(session, wanted);
    } finally {
      release();
    }
  }

  private async restore(
    session: BackupSession,
    wanted: () => boolean,
  ): Promise<PrepareOutcome> {
    if (!isFresh(this.deps.storage, session)) {
      void this.compare(session);
      return "kept";
    }
    const { log } = this.deps;
    log(
      "Backup: this browser holds nothing of this wallet on this network yet; looking for its backup.",
    );
    let pointer: Pointer | null;
    let snapshot: Snapshot;
    try {
      const keys = backupKeys(session.entropy, session);
      pointer = await this.read(keys, session);
      if (pointer === null) {
        log("Backup: no backup of this wallet was found.");
        return "none";
      }
      const sealed = await this.deps.network.fetchSnapshot(
        hexToBytes(pointer.body.snapshot),
      );
      if (sealed === undefined)
        throw new Error(
          `Bulletin does not serve snapshot ${pointer.body.snapshot}; it may have lapsed`,
        );
      if (!equalBytes(blake2b256(sealed), hexToBytes(pointer.body.snapshot)))
        throw new Error("Bulletin served bytes that are not the snapshot");
      const document = JSON.parse(
        new TextDecoder().decode(
          await decompress(openSnapshot(keys, session, sealed)),
        ),
      ) as Record<string, unknown>;
      if (
        document.schema !== SCHEMA ||
        document.network !== session.network ||
        document.peopleGenesis !== session.peopleGenesis.toLowerCase() ||
        document.identity !== session.identity.toLowerCase()
      )
        throw new Error(
          "the snapshot is of another schema, network or identity",
        );
      snapshot = readSnapshot(document);
      if (digestOf(snapshot) !== pointer.body.digest)
        throw new Error("the snapshot is not the state its pointer names");
      if (!wanted()) throw new Error("the session ended before the restore");
      applySnapshot(this.deps.storage, session, snapshot);
    } catch (error) {
      log(
        `Backup: nothing was restored, and this browser starts empty: ${errorText(error)}`,
        true,
      );
      return "failed";
    }
    try {
      this.keep(session, {
        ...NOTHING_KEPT,
        revision: pointer.body.revision,
        pointer: pointer.body,
      });
    } catch (error) {
      log(
        `Backup: restored, but which revision it was is not kept: ${errorText(error)}`,
        true,
      );
    }
    log(
      `Backup: restored revision ${pointer.body.revision}, taken ${new Date(pointer.body.createdAt).toISOString()} by another browser (${snapshot.records.length} host records, ${snapshot.products.length} product storage entries). It is that browser's state then, not now.`,
    );
    return "restored";
  }

  /** Say, without changing anything, when another browser published a different snapshot of this wallet. */
  private async compare(session: BackupSession): Promise<void> {
    try {
      const pointer = await this.read(
        backupKeys(session.entropy, session),
        session,
      );
      const kept = this.kept(session);
      if (
        pointer !== null &&
        pointer.body.source !== this.source() &&
        pointer.body.digest !== kept.pointer?.digest
      )
        this.deps.log(
          `Backup: another browser published revision ${pointer.body.revision} of this wallet, taken ${new Date(pointer.body.createdAt).toISOString()}. This browser keeps its own state; its next backup replaces that one.`,
        );
    } catch (error) {
      this.deps.log(
        `Backup: the latest backup could not be read: ${errorText(error)}`,
      );
    }
  }

  /**
   * Keep `session`'s backup while it lasts: once this tab holds the
   * browser's writer lock for the identity, look at the state every
   * {@link CHECK_MS}.
   */
  start(session: BackupSession): void {
    this.stop();
    this.session = session;
    this.failures = 0;
    this.retryAt = 0;
    const stopping = new AbortController();
    this.stopping = stopping;
    this.deps.acquire(lockName(session), stopping.signal).then(
      (release) => {
        if (stopping.signal.aborted) return release();
        // The lock goes once nothing of this session can still write.
        stopping.signal.addEventListener("abort", () => {
          void this.idle.then(release);
        });
        this.next(session, this.pace.checkMs);
      },
      (error: unknown) => {
        if (!stopping.signal.aborted)
          this.deps.log(
            `Backup: not kept in this browser: ${errorText(error)}`,
            true,
          );
      },
    );
  }

  /** Stop keeping the backup: a sign-out, a wallet switch or the page closing. Whatever was in flight changes nothing after. */
  stop(): void {
    this.session = null;
    this.stopping.abort();
    this.cancelTimer();
    this.candidate = null;
  }

  /** Whether this backup is kept for `session` now. */
  keeps(session: BackupSession): boolean {
    return this.current(session);
  }

  private current(session: BackupSession): boolean {
    return this.session === session;
  }

  private next(session: BackupSession, delayMs: number): void {
    this.cancelTimer = this.schedule(() => {
      const look = this.check(session);
      this.idle = look.then(
        () => {},
        () => {},
      );
      void look.finally(() => {
        if (this.current(session)) this.next(session, this.pace.checkMs);
      });
    }, delayMs);
  }

  /** One look at the state: store it when it changed and held still, and keep the pointer alive for today. */
  async check(session: BackupSession): Promise<void> {
    const now = this.now();
    if (!this.current(session) || now < this.retryAt) return;
    const day = Math.floor(now / DAY_MS);
    try {
      await this.renew(session, day);
      const kept = this.kept(session);
      const snapshot = collectSnapshot(this.deps.storage, session);
      if (isEmpty(snapshot)) return;
      const digest = digestOf(snapshot);
      const taken = kept.pointer?.createdAt ?? null;
      const due = taken !== null && now - taken >= REFRESH_AFTER_MS;
      if (digest === kept.pointer?.digest && !due) return;
      if (this.candidate?.digest !== digest) {
        this.candidate = { digest, since: now };
        return;
      }
      if (now - this.candidate.since < this.pace.quietMs) return;
      if (taken !== null && now - taken < MIN_INTERVAL_MS) return;
      const writes = kept.day === day ? kept.writes : 0;
      if (writes >= MAX_WRITES_PER_DAY) return;
      this.keep(session, { ...kept, day, writes: writes + 1 });
      await this.publish(session, snapshot, digest, now);
      this.failures = 0;
    } catch (error) {
      if (!this.current(session)) return;
      this.failures += 1;
      const wait = Math.min(
        RETRY_FIRST_MS * 2 ** (this.failures - 1),
        RETRY_MAX_MS,
      );
      this.retryAt = this.now() + wait;
      this.deps.log(
        `Backup: not kept, trying again in ${Math.round(wait / 60_000)} min: ${errorText(error)}`,
        true,
      );
    }
  }

  /**
   * Once a UTC day, while the pointer this browser published is alive and
   * names a snapshot Bulletin still keeps, claim the backup account's slot,
   * and post the pointer again when the store no longer holds it. A pointer
   * this browser only restored is left to the browser that published it.
   */
  private async renew(session: BackupSession, day: number): Promise<void> {
    const kept = this.kept(session);
    const pointer = kept.pointer;
    if (
      pointer === null ||
      pointer.source !== this.source() ||
      kept.claimedDay === day ||
      this.now() - pointer.createdAt >= POINTER_LIFETIME_SECONDS * 1000
    )
      return;
    const ensure = this.guard(session);
    const claim = await this.deps.network.claimSlot();
    await this.deps.network.settled(claim);
    ensure();
    const keys = backupKeys(session.entropy, session);
    const held = await this.read(keys, session);
    ensure();
    if (held === null) {
      const revision = kept.revision + 1;
      await this.point(
        keys,
        session,
        { ...pointer, revision },
        pointerExpiry(revision, null, pointer.createdAt),
      );
      this.keep(session, { ...this.kept(session), revision });
      this.deps.log(
        `Backup: the store had lost the pointer; revision ${revision} points to the same snapshot again.`,
      );
    }
    this.keep(session, { ...this.kept(session), claimedDay: day });
  }

  /** A check that throws once `session` is no longer the one backed up. */
  private guard(session: BackupSession): () => void {
    return () => {
      if (!this.current(session)) throw new Error("the session ended");
    };
  }

  private async publish(
    session: BackupSession,
    snapshot: Snapshot,
    digest: HexString,
    takenAt: number,
  ): Promise<void> {
    const { network, log } = this.deps;
    const ensure = this.guard(session);
    const keys = backupKeys(session.entropy, session);
    const source = this.source();
    const document = {
      schema: SCHEMA,
      network: session.network,
      peopleGenesis: session.peopleGenesis.toLowerCase(),
      identity: session.identity.toLowerCase(),
      createdAt: takenAt,
      source,
      ...snapshot,
    };
    const sealed = sealSnapshot(
      keys,
      session,
      await compress(utf8(JSON.stringify(document))),
      this.random,
    );
    if (sealed.length > MAX_SEALED_BYTES)
      throw new Error(
        `the snapshot is ${sealed.length} bytes sealed, more than one Bulletin store holds (${MAX_SEALED_BYTES})`,
      );
    ensure();
    const key = await network.storeSnapshot(sealed);
    ensure();
    if (!equalBytes(key, blake2b256(sealed)))
      throw new Error("Bulletin answered a key that is not the snapshot's");
    await this.readBack(key, sealed);
    ensure();
    const previous = await this.read(keys, session);
    ensure();
    const kept = this.kept(session);
    if (
      previous !== null &&
      previous.body.source !== source &&
      previous.body.revision > kept.revision
    )
      log(
        `Backup: another browser published revision ${previous.body.revision}; this one replaces it.`,
      );
    const revision = Math.max(previous?.body.revision ?? 0, kept.revision) + 1;
    const claim = await network.claimSlot();
    await this.deps.network.settled(claim);
    ensure();
    const body: PointerBody = {
      revision,
      snapshot: hex(key),
      bytes: sealed.length,
      digest,
      createdAt: takenAt,
      source,
    };
    await this.point(
      keys,
      session,
      body,
      pointerExpiry(revision, previous?.expiry ?? null, takenAt),
    );
    this.keep(session, {
      ...this.kept(session),
      revision,
      pointer: body,
      claimedDay: Math.floor(this.now() / DAY_MS),
    });
    log(
      `Backup: revision ${revision} kept: a ${sealed.length}-byte sealed snapshot on Bulletin, and a pointer to it that reads back.`,
    );
  }

  /** Read the stored snapshot back from Bulletin until it is the bytes stored. */
  private async readBack(key: Uint8Array, sealed: Uint8Array): Promise<void> {
    for (let tries = 1; ; tries += 1) {
      const served = await this.deps.network.fetchSnapshot(key);
      if (served !== undefined && equalBytes(served, sealed)) return;
      if (served !== undefined)
        throw new Error("Bulletin serves other bytes under the snapshot's key");
      if (tries >= READ_BACK_TRIES)
        throw new Error("Bulletin does not serve the stored snapshot back yet");
      await new Promise<void>((resolve) =>
        this.schedule(resolve, READ_BACK_WAIT_MS),
      );
    }
  }

  /**
   * Sign and submit the pointer to `body`, then read it back. The signature
   * is checked against the account read first, so a pointer signed after a
   * wallet switch is never submitted.
   */
  private async point(
    keys: BackupKeys,
    session: BackupSession,
    body: PointerBody,
    expiry: bigint,
  ): Promise<void> {
    const { core } = this.deps;
    const ensure = this.guard(session);
    const account = await core.account();
    const statement = pointerStatement(keys, session, body, expiry);
    ensure();
    const signature = await core.signPointer(signingPayload(statement));
    const signed = encodeSigned(statement, account, () => signature);
    if (!verifyStatement(decodeStatement(signed)))
      throw new Error("the pointer's signature is not the backup account's");
    const store = await this.deps.network.openStore();
    try {
      ensure();
      const answer = await store.submit(signed);
      if (!isStored(answer))
        throw new Error(
          `the Statement Store answered ${answer.status}${answer.reason === undefined ? "" : ` (${answer.reason})`} to the pointer`,
        );
    } finally {
      store.close();
    }
    const after = await this.read(keys, session, expiry);
    if (after?.expiry !== expiry || after.body.snapshot !== body.snapshot)
      throw new Error(
        "the Statement Store does not serve the new pointer back",
      );
  }

  /**
   * The pointer of `session`'s backup with the highest expiry the store
   * holds, which is the one the store keeps, or null after
   * {@link HostBackupDeps.readMs}. Once one is heard it waits briefly for a
   * higher one; given `expiry`, it waits for one at least that high.
   */
  private async read(
    keys: BackupKeys,
    session: BackupSession,
    expiry = 0n,
  ): Promise<Pointer | null> {
    const account = await this.deps.core.account();
    const store = await this.deps.network.openStore();
    try {
      return await new Promise<Pointer | null>((resolve, reject) => {
        const heard: Uint8Array[] = [];
        let unsubscribe = () => {};
        let done = false;
        let cancelMore: (() => void) | null = null;
        const best = () => newestPointer(heard, account, keys, session);
        const finish = () => {
          if (done) return;
          done = true;
          cancel();
          cancelMore?.();
          unsubscribe();
          resolve(best());
        };
        const cancel = this.schedule(finish, this.deps.readMs ?? READ_MS);
        store
          .subscribe({ matchAll: [keys.topic] }, (statements) => {
            heard.push(...statements);
            const pointer = best();
            if (pointer === null || pointer.expiry < expiry) return;
            cancelMore ??= this.schedule(finish, READ_MORE_MS);
          })
          .then(
            (stop) => (done ? stop() : (unsubscribe = stop)),
            (error: unknown) => {
              done = true;
              cancel();
              cancelMore?.();
              reject(error instanceof Error ? error : new Error(String(error)));
            },
          );
      });
    } finally {
      store.close();
    }
  }
}
