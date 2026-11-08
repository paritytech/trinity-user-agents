import { productSlotPrefix } from "../platform/storage.js";

/**
 * What a backup copies of one wallet's local state on one network, and how a
 * restore writes it back under another browser's id for the same wallet.
 *
 * Two parts, read from `localStorage` and nothing else:
 *
 * - The host's own records of the wallet, by kind ({@link HOST_RECORD_KINDS}):
 *   the Inbox, the request service's lists, Nova Chat's peers, contacts,
 *   product rooms, the product workers the user added, and Pocket's faces.
 *   Each is copied as the text it is stored as; nothing here reads it.
 * - Every product's TrUAPI storage under the wallet, as the core stored it:
 *   whatever a product keeps there, its conversation accounts' indexes and
 *   its sealed state included. The bytes are opaque here.
 *
 * Keys carry the wallet's id in this browser, which another browser does not
 * share, so a snapshot keeps each entry without it and a restore puts in the
 * id of the wallet signed in there. Nothing else is copied: no recovery
 * phrase, no core storage (session, device key, AutoSigning keys, grants and
 * permissions stay with the browser that made them), no preferences.
 */

/** The host records a backup copies, by the kind their key names. */
export const HOST_RECORD_KINDS = [
  "notifications",
  "requests",
  "nova-chat",
  "chat",
  "product-chat",
  "workers",
  "pocket-faces",
] as const;

type RecordKind = (typeof HOST_RECORD_KINDS)[number];

const HOST_PREFIX = "truapi-web-signing-host:network:";
const PRODUCT_STORAGE = "truapi:product-storage:v1:";

/** One wallet on one network, as its keys in this browser name it. */
export interface BackupScope {
  network: string;
  walletId: string;
}

/** One wallet's local state on one network, without the wallet's id. */
export interface Snapshot {
  /** Host records: their kind, the rest of their key after the wallet id, their text. */
  records: [RecordKind, string, string][];
  /** Product storage: the core's key, the stored value. */
  products: [string, string][];
}

const recordKey = (scope: BackupScope, kind: string, rest: string): string =>
  `${HOST_PREFIX}${scope.network}:${kind}:${scope.walletId}${rest}`;

/** The host record `key` is in `scope`, as its kind and the rest of its key, or null. */
function recordOf(
  key: string,
  scope: BackupScope,
): [RecordKind, string] | null {
  for (const kind of HOST_RECORD_KINDS) {
    const named = recordKey(scope, kind, "");
    if (!key.startsWith(named)) continue;
    const rest = key.slice(named.length);
    if (rest === "" || rest.startsWith(":")) return [kind, rest];
  }
  return null;
}

function keysOf(storage: Storage): string[] {
  const keys: string[] = [];
  for (let index = 0; index < storage.length; index += 1) {
    const key = storage.key(index);
    if (key !== null) keys.push(key);
  }
  return keys.sort();
}

/** What `scope` holds in `storage` now, in a fixed order. */
export function collectSnapshot(
  storage: Storage,
  scope: BackupScope,
): Snapshot {
  const products = `${productSlotPrefix(scope.walletId, scope.network)}${PRODUCT_STORAGE}`;
  const snapshot: Snapshot = { records: [], products: [] };
  for (const key of keysOf(storage)) {
    const value = storage.getItem(key);
    if (value === null) continue;
    const record = recordOf(key, scope);
    if (record !== null) snapshot.records.push([...record, value]);
    else if (key.startsWith(products))
      snapshot.products.push([
        key.slice(productSlotPrefix(scope.walletId, scope.network).length),
        value,
      ]);
  }
  return snapshot;
}

/** Whether `snapshot` holds nothing worth keeping. */
export const isEmpty = (snapshot: Snapshot): boolean =>
  snapshot.records.length === 0 && snapshot.products.length === 0;

/** Whether `scope` holds nothing a backup would copy: a wallet this browser has not used on this network. */
export const isFresh = (storage: Storage, scope: BackupScope): boolean =>
  isEmpty(collectSnapshot(storage, scope));

/**
 * Write `snapshot` into `scope`, which must be fresh. Every entry is written
 * or none is: a write that fails, such as a full `localStorage`, removes the
 * ones written before it, which the scope did not hold.
 */
export function applySnapshot(
  storage: Storage,
  scope: BackupScope,
  snapshot: Snapshot,
): void {
  if (!isFresh(storage, scope))
    throw new Error(
      "this browser already holds state for this wallet; a backup never replaces it",
    );
  const prefix = productSlotPrefix(scope.walletId, scope.network);
  const entries: [string, string][] = [
    ...snapshot.records.map(([kind, rest, value]): [string, string] => [
      recordKey(scope, kind, rest),
      value,
    ]),
    ...snapshot.products.map(([key, value]): [string, string] => [
      `${prefix}${key}`,
      value,
    ]),
  ];
  const written: string[] = [];
  try {
    for (const [key, value] of entries) {
      storage.setItem(key, value);
      written.push(key);
    }
  } catch (error) {
    for (const key of written) storage.removeItem(key);
    throw new Error(
      `the backup could not be written, and nothing of it was kept: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

/** Whether `key` frames a product id the way the core's product storage keys do: `<byte length>:<id>:<key>` after the prefix. */
function framedProductKey(key: string): boolean {
  const framed = /^(\d+):/.exec(key.slice(PRODUCT_STORAGE.length));
  if (framed === null) return false;
  const length = Number(framed[1]);
  const rest = new TextEncoder().encode(
    key.slice(PRODUCT_STORAGE.length + framed[0].length),
  );
  return length > 0 && rest.length > length && rest[length] === 0x3a;
}

const HEX_VALUE = /^0x(?:[0-9a-f]{2})*$/i;

/**
 * `value` as a snapshot, or why it is not one. Every entry is checked
 * before any is used: a host record of a kind this host keeps, a product
 * storage key framed as the core frames it with a hex value, and no two
 * entries for one key. Product values stay opaque.
 */
export function readSnapshot(value: unknown): Snapshot {
  const fail = (why: string): never => {
    throw new Error(`the backup's snapshot does not read: ${why}`);
  };
  if (typeof value !== "object" || value === null) return fail("not an object");
  const { records, products } = value as Record<string, unknown>;
  if (!Array.isArray(records) || !Array.isArray(products))
    return fail("it lacks its records or its product storage");
  const kinds: readonly string[] = HOST_RECORD_KINDS;
  const destinations = new Set<string>();
  const once = (destination: string) => {
    if (destinations.has(destination)) fail("it names one key twice");
    destinations.add(destination);
  };
  for (const record of records as unknown[]) {
    if (
      !Array.isArray(record) ||
      record.length !== 3 ||
      !kinds.includes(record[0] as string) ||
      typeof record[1] !== "string" ||
      !(record[1] === "" || record[1].startsWith(":")) ||
      typeof record[2] !== "string"
    )
      return fail("a host record is not one this host keeps");
    once(`record ${record[0]}${record[1]}`);
  }
  for (const product of products as unknown[]) {
    if (
      !Array.isArray(product) ||
      product.length !== 2 ||
      typeof product[0] !== "string" ||
      !product[0].startsWith(PRODUCT_STORAGE) ||
      !framedProductKey(product[0]) ||
      typeof product[1] !== "string" ||
      !HEX_VALUE.test(product[1])
    )
      return fail("a product storage entry is not one the core keeps");
    once(`product ${product[0]}`);
  }
  return { records, products } as Snapshot;
}
