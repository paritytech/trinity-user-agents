/**
 * Which copy of a stack's descriptor a tab runs on, keeping that copy current,
 * and adding a custom stack.
 *
 * A tab starts from the newest copy it already has, without waiting on the
 * network: for a built-in stack the one this build bundles or the last one
 * accepted from its server, for a custom stack the one accepted when it was
 * added or since. After boot it fetches the published descriptor once. A
 * newer copy that this host can run is stored for the next load; the running
 * tab never switches. Every decision about a copy (signature under the link's
 * key, rollback, conflict, staleness, expiry, a new generation) is
 * `@parity/trinity-stack`'s `accept`; this module adds the host's own rules
 * and the storage.
 *
 * A copy that changes the stack's `id` or `tld` from the one first accepted is
 * never used: every account derives under the TLD, and a different `id` is
 * another stack under the same name.
 *
 * Trust: a built-in stack's link pins the key its copies must be signed with.
 * A custom stack added by a link with a key is pinned to that key, which the
 * user vouched for when adding it. A custom stack added by a plain URL is
 * unsigned: whoever serves that URL decides its chains and services, and an
 * older copy can replace a newer one.
 */

import {
  accept,
  decodeLink,
  encodeLink,
  fetchDescriptor,
  isTrinityStackError,
  stackId,
  type Accepted,
  type Descriptor,
  type Fetch,
  type Pointer,
} from "@parity/trinity-stack";
import {
  BUILT_IN_STACKS,
  NETWORKS,
  StackRefusedError,
  bundledCopy,
  customPolicy,
  customStackId,
  isNetworkId,
  networkConfigFrom,
  stackName,
  type BuiltInStack,
  type NetworkConfig,
  type NetworkId,
  type StackPolicy,
} from "./network-config.js";

/**
 * Where the last copy accepted from a built-in stack's server is kept: by the
 * key in its link, which is the stack's identity. Shared by every tab and
 * wallet, and outside any wallet's backup.
 */
export const storedStackKey = (builtIn: BuiltInStack): string =>
  `truapi-web-signing-host:stack:${decodeLink(builtIn.pointer).key}:v1`;

/** Where the custom stacks the user added are kept, with their accepted copies. */
export const CUSTOM_STACKS_KEY = "truapi-web-signing-host:custom-stacks:v1";

/** A custom stack as it is kept. */
export interface CustomStackRecord {
  /** The stack's link: its URL, and the key it is pinned to when it has one. */
  link: string;
  /** The last accepted document, as served. */
  document: string;
  /** The descriptor `id` and `tld` when the stack was added, which no later copy may change. */
  pinned: { id: string; tld: string };
}

/** The copy a tab starts from. */
export interface StartupStack {
  config: NetworkConfig;
  /** `bundled` when the build's copy is in use, `stored` for one fetched before. */
  source: "bundled" | "stored";
  /** Past `validUntil`: still used, but shown as stale. */
  stale: boolean;
  /** Whether the copy was checked against a key: always for a built-in stack. */
  signed: boolean;
  /** Why a stored copy was passed over, or null. */
  problem: string | null;
}

/** What one refresh did. */
export type StackRefresh =
  /** The server did not answer with a document. */
  | { outcome: "unreachable" }
  /** Nothing newer than the copy in use; `status` is `accept`'s. */
  | { outcome: "current"; status: string }
  /** The server's copy breaks a rule; the copy in use stays. */
  | { outcome: "refused"; reason: string }
  /**
   * A newer copy is stored for the next load. `changes` is whether it moves
   * anything this host reads; `newGeneration` whether the chains restarted.
   */
  | {
      outcome: "stored";
      sequence: number;
      changes: boolean;
      newGeneration: boolean;
    };

/** One stack's descriptor source: its policy, name, link, pins and stored copy. */
interface StackSource {
  policy: StackPolicy;
  /** The stack's menu name, for messages. */
  name: string;
  pointer: Pointer;
  /** The copy this build ships, for a built-in stack. */
  bundled: Uint8Array | undefined;
  pinned: { id: string; tld: string };
  read(): Uint8Array | undefined;
  write(document: Uint8Array): void;
}

const utf8 = (bytes: Uint8Array): string => new TextDecoder().decode(bytes);
const bytes = (text: string): Uint8Array => new TextEncoder().encode(text);

const errorText = (error: unknown): string =>
  isTrinityStackError(error)
    ? `${error.message} (${error.code})`
    : error instanceof Error
      ? error.message
      : String(error);

function builtInSource(builtIn: BuiltInStack, storage: Storage): StackSource {
  const { id, tld } = bundledCopy(builtIn).descriptor;
  return {
    policy: builtIn,
    name: NETWORKS[builtIn.id].displayName,
    pointer: decodeLink(builtIn.pointer),
    bundled: builtIn.bundled,
    pinned: { id, tld },
    read() {
      const text = storage.getItem(storedStackKey(builtIn));
      return text === null ? undefined : bytes(text);
    },
    write(document) {
      storage.setItem(storedStackKey(builtIn), utf8(document));
    },
  };
}

/**
 * Fails unless `candidate` keeps the pinned `id` and `tld`, which the stack's
 * identity and every derived account depend on.
 */
function checkPins(source: StackSource, candidate: Descriptor): void {
  const { pinned } = source;
  if (candidate.id !== pinned.id)
    throw new StackRefusedError(
      `The ${source.name} stack descriptor (sequence ${candidate.sequence}) changes the stack's id from ${pinned.id} to ${candidate.id}.`,
    );
  if (candidate.tld !== pinned.tld)
    throw new StackRefusedError(
      `The ${source.name} stack descriptor (sequence ${candidate.sequence}) changes the TLD from ${pinned.tld} to ${candidate.tld}, which every account derives under.`,
    );
}

/** The host's config from `accepted`, after the host's own rules. */
function admit(source: StackSource, accepted: Accepted): NetworkConfig {
  if (accepted.expired)
    throw new StackRefusedError(
      `The ${source.name} stack descriptor (sequence ${accepted.descriptor.sequence}) is past its expiresAt: the stack is shut down.`,
    );
  checkPins(source, accepted.descriptor);
  return networkConfigFrom(source.policy, {
    pointer: source.pointer,
    document: accepted.document,
    descriptor: accepted.descriptor,
  });
}

function startFrom(source: StackSource, now: Date): StartupStack {
  const { pointer, bundled } = source;
  const stored = source.read();
  const signed = pointer.key !== undefined;
  if (bundled === undefined) {
    if (stored === undefined)
      throw new StackRefusedError(
        `The ${source.name} stack has no stored descriptor.`,
      );
    const accepted = accept(pointer, undefined, stored, now);
    return {
      config: admit(source, accepted),
      source: "stored",
      stale: accepted.stale,
      signed,
      problem: null,
    };
  }
  const fromBundle = () => {
    const accepted = accept(pointer, undefined, bundled, now);
    return { config: admit(source, accepted), stale: accepted.stale };
  };
  if (stored !== undefined) {
    try {
      const accepted = accept(pointer, stored, bundled, now);
      return {
        config: admit(source, accepted),
        source:
          utf8(accepted.document) === utf8(bundled) ? "bundled" : "stored",
        stale: accepted.stale,
        signed,
        problem: null,
      };
    } catch (error) {
      return {
        ...fromBundle(),
        source: "bundled",
        signed,
        problem: `The stored ${source.name} stack descriptor is not used: ${errorText(error)}`,
      };
    }
  }
  return { ...fromBundle(), source: "bundled", signed, problem: null };
}

/**
 * The copy of `builtIn` a tab starts from: the stored copy when it is newer
 * than the bundled one and passes the host's rules, otherwise the bundled
 * one. Reads nothing from the network. Throws when the bundled copy itself
 * cannot run, which is a broken build.
 */
export function startupStack(
  builtIn: BuiltInStack,
  storage: Storage,
  now: Date = new Date(),
): StartupStack {
  return startFrom(builtInSource(builtIn, storage), now);
}

/** The parts of a config this host reads, to tell whether a new copy moves any. */
function readParts(config: NetworkConfig): string {
  const { networkSuffix, webGateway, chains, genesis, dotns, identityBackend } =
    config;
  return JSON.stringify({
    networkSuffix,
    webGateway,
    chains,
    genesis,
    dotns,
    identityBackend,
  });
}

/**
 * Fetch `source`'s published descriptor once and store it when it is newer
 * than the copy `running` started from, or, when no copy runs, than the one
 * stored.
 */
async function refreshFrom(
  source: StackSource,
  running: NetworkConfig | null,
  options: { fetch?: Fetch; timeoutMs?: number; now?: Date },
): Promise<StackRefresh> {
  const { pointer } = source;
  let fetched: Uint8Array | undefined;
  try {
    fetched = await fetchDescriptor(pointer.url, options);
  } catch {
    fetched = undefined;
  }
  if (fetched === undefined) return { outcome: "unreachable" };

  const now = options.now ?? new Date();
  let candidate: NetworkConfig;
  let accepted: Accepted;
  try {
    accepted = accept(
      pointer,
      running?.stack.document ?? source.read(),
      fetched,
      now,
    );
    if (accepted.status === "refused")
      return {
        outcome: "refused",
        reason: `the published copy breaks the rule ${accepted.refused}`,
      };
    if (accepted.status !== "updated")
      return { outcome: "current", status: accepted.status };
    candidate = admit(source, accepted);
  } catch (error) {
    return { outcome: "refused", reason: errorText(error) };
  }

  const stored = source.read();
  if (stored !== undefined) {
    const latest = accept(pointer, stored, fetched, now);
    if (latest.status !== "updated")
      return { outcome: "current", status: latest.status };
  }
  source.write(accepted.document);
  return {
    outcome: "stored",
    sequence: accepted.descriptor.sequence,
    changes: running === null || readParts(candidate) !== readParts(running),
    newGeneration: accepted.newGeneration,
  };
}

/**
 * Fetch `builtIn`'s published descriptor once and store it for the next load
 * when it is newer than `running`'s copy and this host can run it. Before
 * storing, the copy is weighed again against the one stored now, so a tab
 * holding an older answer never replaces a newer copy another tab stored.
 */
export function refreshStack(
  builtIn: BuiltInStack,
  running: NetworkConfig,
  storage: Storage,
  options: { fetch?: Fetch; timeoutMs?: number; now?: Date } = {},
): Promise<StackRefresh> {
  return refreshFrom(builtInSource(builtIn, storage), running, options);
}

/** The custom stacks the user added, in the order they were added. Unreadable entries are left out. */
export function customStacks(storage: Storage): CustomStackRecord[] {
  try {
    const parsed: unknown = JSON.parse(
      storage.getItem(CUSTOM_STACKS_KEY) ?? "[]",
    );
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (entry): entry is CustomStackRecord =>
        typeof entry?.link === "string" &&
        typeof entry?.document === "string" &&
        typeof entry?.pinned?.id === "string" &&
        typeof entry?.pinned?.tld === "string",
    );
  } catch {
    return [];
  }
}

function writeCustomStacks(
  storage: Storage,
  records: CustomStackRecord[],
): void {
  storage.setItem(CUSTOM_STACKS_KEY, JSON.stringify(records));
}

/** The namespace `record`'s state is kept under. */
export const customRecordId = (record: CustomStackRecord): string =>
  customStackId(decodeLink(record.link));

/** The custom stack kept under `id`, or undefined. */
export function findCustomStack(
  storage: Storage,
  id: string,
): CustomStackRecord | undefined {
  return customStacks(storage).find((record) => customRecordId(record) === id);
}

/** Keep `record`, replacing the one with the same identity. */
export function saveCustomStack(
  storage: Storage,
  record: CustomStackRecord,
): void {
  const id = customRecordId(record);
  writeCustomStacks(storage, [
    ...customStacks(storage).filter((entry) => customRecordId(entry) !== id),
    record,
  ]);
}

/** Forget the custom stack kept under `id`. Its state in the browser stays under its namespace. */
export function forgetCustomStack(storage: Storage, id: string): void {
  writeCustomStacks(
    storage,
    customStacks(storage).filter((entry) => customRecordId(entry) !== id),
  );
}

/**
 * The source of the custom stack `record`. The menu name comes from the copy
 * kept with it; `storage` keeps later copies, and is absent while a stack is
 * only being checked.
 */
function customSource(
  record: CustomStackRecord,
  storage?: Storage,
): StackSource {
  const pointer = decodeLink(record.link);
  const id = customStackId(pointer);
  const current = () =>
    (storage === undefined
      ? undefined
      : customStacks(storage).find((entry) => customRecordId(entry) === id)) ??
    record;
  const { descriptor } = accept(pointer, undefined, bytes(record.document));
  return {
    policy: customPolicy(pointer),
    name: stackName(customPolicy(pointer), descriptor.name),
    pointer,
    bundled: undefined,
    pinned: record.pinned,
    read: () => bytes(current().document),
    write(document) {
      if (storage !== undefined)
        saveCustomStack(storage, { ...current(), document: utf8(document) });
    },
  };
}

/** The copy of the custom stack `record` a tab starts from. Throws when it can no longer run. */
export function startupCustomStack(
  record: CustomStackRecord,
  storage: Storage,
  now: Date = new Date(),
): StartupStack {
  return startFrom(customSource(record, storage), now);
}

/** Fetch a custom stack's published descriptor once, as {@link refreshStack} does for a built-in. */
export function refreshCustomStack(
  record: CustomStackRecord,
  running: NetworkConfig,
  storage: Storage,
  options: { fetch?: Fetch; timeoutMs?: number; now?: Date } = {},
): Promise<StackRefresh> {
  return refreshFrom(customSource(record, storage), running, options);
}

/** What checking a URL or link the user typed found. */
export type CustomStackCheck =
  /** It names a stack this build ships, which is used with the build's pinned key. */
  | { kind: "built-in"; id: NetworkId }
  /** A stack the host can run, waiting for the user to confirm it. */
  | {
      kind: "custom";
      config: NetworkConfig;
      record: CustomStackRecord;
      signed: boolean;
      stale: boolean;
    };

/**
 * The pointer `input` names: a stack link, whose `url` and `key` are read
 * whatever its scheme, or a plain descriptor URL, which carries no key.
 */
export function pointerOf(input: string): Pointer {
  const typed = input.trim();
  let pointer: Pointer;
  try {
    pointer = decodeLink(typed);
  } catch {
    pointer = { url: typed };
  }
  // Applies the descriptor RFC's URL rules; throws on a URL they refuse.
  stackId(pointer);
  return pointer;
}

/** The built-in stack `pointer` names: the same key, or, for a pointer without one, the same URL. */
function builtInFor(pointer: Pointer): NetworkId | undefined {
  return Object.values(BUILT_IN_STACKS).find((builtIn) => {
    const pinned = decodeLink(builtIn.pointer);
    return pointer.key !== undefined
      ? pointer.key.toLowerCase() === pinned.key?.toLowerCase()
      : pointer.url === pinned.url;
  })?.id;
}

/**
 * Fetch and check the stack `input` names, without keeping anything. Throws
 * with the reason when the URL is refused, the descriptor cannot be fetched
 * from this page, does not verify under the link's key, is expired, or breaks
 * one of the host's rules (`networkConfigFrom`).
 */
export async function checkCustomStack(
  input: string,
  options: { fetch?: Fetch; timeoutMs?: number; now?: Date } = {},
): Promise<CustomStackCheck> {
  let pointer: Pointer;
  try {
    pointer = pointerOf(input);
  } catch (error) {
    throw new StackRefusedError(
      `That is not a stack descriptor URL or stack link: ${errorText(error)}`,
    );
  }
  const builtIn = builtInFor(pointer);
  if (builtIn !== undefined) return { kind: "built-in", id: builtIn };

  const fetched = await fetchDescriptor(pointer.url, options).catch(
    () => undefined,
  );
  if (fetched === undefined)
    throw new StackRefusedError(
      `No descriptor was read from ${pointer.url}. The server did not answer, answered with an error or a redirect, sent more than 1 MiB, or does not allow this page to read it (CORS).`,
    );
  let accepted: Accepted;
  try {
    accepted = accept(pointer, undefined, fetched, options.now ?? new Date());
  } catch (error) {
    throw new StackRefusedError(
      `The descriptor at ${pointer.url} is refused: ${errorText(error)}`,
    );
  }
  const record: CustomStackRecord = {
    link: encodeLink(pointer),
    document: utf8(accepted.document),
    pinned: { id: accepted.descriptor.id, tld: accepted.descriptor.tld },
  };
  return {
    kind: "custom",
    config: admit(customSource(record), accepted),
    record,
    signed: accepted.signed,
    stale: accepted.stale,
  };
}

/** Fetch the stack's published descriptor once, storing a newer copy this host can run. */
export type RefreshStack = (options?: {
  fetch?: Fetch;
  timeoutMs?: number;
  now?: Date;
}) => Promise<StackRefresh>;

/** The stack a tab runs on, and how to refresh its descriptor. */
export type ActiveStack =
  | { status: "ready"; startup: StartupStack; refresh: RefreshStack }
  /**
   * The chosen custom stack cannot run, and the tab does not start: `reason`
   * says why. `refresh` can store a copy that runs, for the next load; it is
   * null when the stack is no longer kept.
   */
  | { status: "failed"; reason: string; refresh: RefreshStack | null };

/**
 * The stack `id` names, as a tab starts on it: a built-in stack, or a custom
 * stack this browser keeps. A custom stack that cannot run (expired, its kept
 * copy refused, or no longer kept) is `failed`; the tab is never moved to
 * another stack. Throws when a built-in stack's bundled copy cannot run,
 * which is a broken build.
 */
export function activeStack(
  id: string,
  storage: Storage,
  now: Date = new Date(),
): ActiveStack {
  if (isNetworkId(id)) {
    const stack = BUILT_IN_STACKS[id];
    const startup = startupStack(stack, storage, now);
    return {
      status: "ready",
      startup,
      refresh: (options = {}) =>
        refreshStack(stack, startup.config, storage, options),
    };
  }
  const record = findCustomStack(storage, id);
  if (record === undefined)
    return {
      status: "failed",
      reason: `No stack ${id} is kept in this browser.`,
      refresh: null,
    };
  try {
    const startup = startupCustomStack(record, storage, now);
    return {
      status: "ready",
      startup,
      refresh: (options = {}) =>
        refreshCustomStack(record, startup.config, storage, options),
    };
  } catch (error) {
    return {
      status: "failed",
      reason: `The custom stack at ${decodeLink(record.link).url} cannot run: ${errorText(error)}`,
      // Async, so a kept copy too broken to name the stack rejects.
      refresh: async (options = {}) =>
        refreshFrom(customSource(record, storage), null, options),
    };
  }
}
