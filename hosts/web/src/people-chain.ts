import { blake2b } from "@noble/hashes/blake2.js";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { ChainProvider, JsonRpcConnection } from "@parity/truapi-host";
import { Reader } from "./scale-reader.js";

/**
 * Read-only reads of one account's standing on the People chain.
 *
 * `Resources.Consumers` holds the usernames and the credibility of a registered
 * person, and `PeopleLite.LitePeople` marks a Lite person and names its ring
 * key. Ring membership is then read from
 * `Members.Members[(collection, ring key)]`. The ring key of a Lite person is in
 * its `LitePeople` record. The key of a full person is reached through
 * `People.AccountToPersonalId` and `People.People`. The layouts follow the
 * committed People chain metadata of the product SDK. A runtime that changed
 * one fails the decode and is reported as such, never as "not found".
 *
 * Nothing is sent to the chain but storage reads against one finalized block.
 */

/** `twox128("Resources") ++ twox128("Consumers")`. */
const CONSUMERS_PREFIX =
  "2111e0df19de9563b58301e5f7e0074311ab70ef474cf12409dfe509d5efe3b2";
/** `twox128("PeopleLite") ++ twox128("LitePeople")`. */
const LITE_PEOPLE_PREFIX =
  "276fc15f94f88f19ef554a8ff4374855e2491c72cc063f0098c08930123488ce";

/** `twox128("Members") ++ twox128("Members")`. */
const MEMBERS_PREFIX =
  "ba7fb8745735dc3be2a2c61a72c39e78ba7fb8745735dc3be2a2c61a72c39e78";
/** `twox128("People") ++ twox128("AccountToPersonalId")`. */
const ACCOUNT_TO_PERSONAL_ID_PREFIX =
  "43b4b15ea5f08d8e3af42ba6b1f2e2b848a12cdd285062997937704d6c41f7d4";
/** `twox128("People") ++ twox128("People")`. */
const PEOPLE_RECORD_PREFIX =
  "43b4b15ea5f08d8e3af42ba6b1f2e2b843b4b15ea5f08d8e3af42ba6b1f2e2b8";

/** The 32-byte ASCII collection identifiers of the `Members` pallet. */
const COLLECTION_IDS = {
  LitePeople: new TextEncoder().encode("pop:polkadot.network/people-lite"),
  People: new TextEncoder().encode("pop:polkadot.network/people     "),
} as const;

/** How long a whole read may take, from opening the connection to the last value. */
export const READ_TIMEOUT_MS = 45_000;

/** The storage key of `account` in a `Blake2_128Concat` map under `prefix`. */
export function mapKey(prefix: string, account: Uint8Array): HexString {
  const hashed = blake2b(account, { dkLen: 16 });
  return `0x${prefix}${bytesToHex(hashed).slice(2)}${bytesToHex(account).slice(2)}`;
}

/** The credibility a person's record carries. */
export type Credibility =
  | { tag: "Lite" }
  | { tag: "Person"; alias: HexString; lastUpdate: bigint; demoted: boolean };

/** A `Resources.Consumers` record: the public facts about one registered account. */
export interface ConsumerRecord {
  fullUsername: string | null;
  liteUsername: string;
  credibility: Credibility;
}

/**
 * Decode a `Resources.Consumers` value: identifier key, full and lite username,
 * and credibility, as the live People runtimes store it. Throws when it is not
 * that layout.
 */
export function decodeConsumer(bytes: Uint8Array): ConsumerRecord {
  const reader = new Reader(bytes);
  reader.take(65);
  const hasFull = reader.u8();
  if (hasFull > 1) throw new Error("an option flag is invalid");
  const fullUsername = hasFull === 1 ? reader.text() : null;
  const liteUsername = reader.text();
  const credibilityTag = reader.u8();
  let credibility: Credibility;
  if (credibilityTag === 0) credibility = { tag: "Lite" };
  else if (credibilityTag === 1) {
    const alias = bytesToHex(reader.take(32));
    const lastUpdate = reader.u64();
    const demoted = reader.u8();
    if (demoted > 1) throw new Error("a flag is invalid");
    credibility = { tag: "Person", alias, lastUpdate, demoted: demoted === 1 };
  } else throw new Error("an unknown credibility variant");
  reader.finish();
  return { fullUsername, liteUsername, credibility };
}

/** Where a ring key stands in `Members.Members`, as the chain stores it. */
export type RingPosition =
  | { tag: "Onboarding"; queuePage: number; queuedAt: bigint }
  | {
      tag: "Included";
      ringIndex: number;
      ringPage: number;
      ringPosition: number;
    }
  | { tag: "Suspended" };

/** Decode a `Members.Members` value. Throws when it is not the layout this reads. */
export function decodeRingPosition(bytes: Uint8Array): RingPosition {
  const reader = new Reader(bytes);
  const tag = reader.u8();
  let position: RingPosition;
  if (tag === 0)
    position = {
      tag: "Onboarding",
      queuePage: reader.u32(),
      queuedAt: reader.u64(),
    };
  else if (tag === 1)
    position = {
      tag: "Included",
      ringIndex: reader.u32(),
      ringPage: reader.u32(),
      ringPosition: reader.u32(),
    };
  else if (tag === 2) position = { tag: "Suspended" };
  else throw new Error("an unknown ring position variant");
  reader.finish();
  return position;
}

/** The ring key a `PeopleLite.LitePeople` value names. */
export function decodeLitePersonKey(bytes: Uint8Array): HexString {
  const reader = new Reader(bytes);
  const key = bytesToHex(reader.take(32));
  if (reader.u8() !== 0) throw new Error("an unknown recognition method");
  reader.take(32);
  reader.finish();
  return key;
}

/** The ring key a `People.People` value names. */
export function decodePersonKey(bytes: Uint8Array): HexString {
  const reader = new Reader(bytes);
  const key = bytesToHex(reader.take(32));
  const hasAccount = reader.u8();
  if (hasAccount > 1) throw new Error("an option flag is invalid");
  if (hasAccount === 1) reader.take(32);
  reader.finish();
  return key;
}

/** What one ring collection holds for the ring key an account's records name. */
export type RingLookup =
  /** No record of this account names a ring key in this collection. */
  | { tag: "NoKey" }
  /** A key is named, and `Members.Members` has no entry for it. */
  | { tag: "NotListed"; key: HexString }
  | { tag: "Listed"; key: HexString; position: RingPosition }
  /** A record on the way to the key or its position is not the layout this reads. */
  | { tag: "Unreadable"; reason: string };

/** The ring lookup of one collection for one account. */
export interface RingMembership {
  collection: "LitePeople" | "People";
  lookup: RingLookup;
}

/** The accounts a read looks at, each named for what it is. */
export interface AccountToRead {
  role: "identity" | "root";
  accountId: HexString;
}

/** What the chain holds for one queried account. */
export interface AccountReading extends AccountToRead {
  /** The Consumers record, or null when there is none. */
  consumer: ConsumerRecord | null;
  /** Whether `PeopleLite.LitePeople` has an entry. */
  litePerson: boolean;
  /** Ring membership in each collection, from the ring keys this account's records name. */
  rings: RingMembership[];
  /** Set when a stored value could not be decoded; `consumer` is then unknown, not absent. */
  problem?: string;
}

/** The result of one read: every account at one finalized block. */
export interface PeopleChainReading {
  blockHash: HexString;
  readings: AccountReading[];
}

interface RpcMessage {
  id?: number;
  result?: unknown;
  error?: { message?: string };
  method?: string;
  params?: { subscription?: string; result?: Record<string, unknown> };
}

/** Follow-up reads: each gets the values of every round so far and names the next keys. */
export type FollowUp = (rounds: (HexString | null)[][]) => HexString[];

/** The JSON-RPC id of the storage request of round `round`; the continue request takes the next one. */
const storageRequestId = (round: number): number =>
  round === 0 ? 2 : 3 + 2 * round;

/**
 * Read `keys` at the newest finalized block of the chain behind `connection`,
 * with `chainHead_v1`. Closes the connection when done, and rejects when the
 * chain stops following, refuses the read or takes longer than `timeoutMs`.
 *
 * Each of `followUps` runs after the round before it, on the same follow
 * subscription and the same block, so a key that depends on an earlier value
 * is read at the block that value came from. A follow-up that names no keys
 * ends the read, and the rounds it skips come back empty.
 */
export async function readStorageValues(
  connection: JsonRpcConnection,
  keys: HexString[],
  timeoutMs: number = READ_TIMEOUT_MS,
  followUps: FollowUp[] = [],
  chainName = "People",
): Promise<{
  blockHash: HexString;
  values: (HexString | null)[];
  followUpValues?: (HexString | null)[][];
}> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      reject(
        new Error(
          `no answer from the ${chainName} chain after ${timeoutMs / 1000}s`,
        ),
      );
    }, timeoutMs);
  });
  let subscription: string | undefined;
  const call = (id: number, method: string, params: unknown[]): void =>
    connection.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));

  const run = async () => {
    call(1, "chainHead_v1_follow", [false]);
    let blockHash: HexString | undefined;
    let operation: string | undefined;
    let round = 0;
    let roundKeys = keys;
    let found = new Map<string, HexString | null>();
    const rounds: (HexString | null)[][] = [];
    const finish = () => {
      const [values, ...rest] = rounds;
      while (rest.length < followUps.length) rest.push([]);
      return {
        blockHash: blockHash as HexString,
        values,
        ...(followUps.length > 0 ? { followUpValues: rest } : {}),
      };
    };
    const start = (hash: HexString) =>
      call(storageRequestId(round), "chainHead_v1_storage", [
        subscription,
        hash,
        roundKeys.map((key) => ({ key, type: "value" })),
        null,
      ]);
    for await (const text of connection.responses()) {
      const message = JSON.parse(text) as RpcMessage;
      if (message.error !== undefined)
        throw new Error(message.error.message ?? "the chain refused a request");
      if (message.id === 1) {
        if (typeof message.result !== "string")
          throw new Error("the chain gave no follow subscription");
        subscription = message.result;
        continue;
      }
      if (message.id === storageRequestId(round)) {
        const started = message.result as
          | { result?: string; operationId?: string }
          | undefined;
        if (started?.result !== "started" || !started.operationId)
          throw new Error("the chain could not start the storage read");
        operation = started.operationId;
        continue;
      }
      if (
        message.method !== "chainHead_v1_followEvent" ||
        message.params?.subscription !== subscription
      )
        continue;
      const event = message.params?.result ?? {};
      switch (event.event) {
        case "initialized": {
          const hashes = (event.finalizedBlockHashes ?? [
            event.finalizedBlockHash,
          ]) as HexString[];
          blockHash = hashes[hashes.length - 1];
          start(blockHash);
          break;
        }
        case "operationStorageItems":
          if (event.operationId !== operation) break;
          for (const item of event.items as {
            key: string;
            value?: HexString;
          }[])
            found.set(item.key, item.value ?? null);
          break;
        case "operationWaitingForContinue":
          if (event.operationId === operation)
            call(storageRequestId(round) + 1, "chainHead_v1_continue", [
              subscription,
              operation,
            ]);
          break;
        case "operationStorageDone": {
          if (event.operationId !== operation || blockHash === undefined) break;
          const read = found;
          rounds.push(roundKeys.map((key) => read.get(key) ?? null));
          const followUp = followUps[round];
          if (followUp === undefined) return finish();
          roundKeys = followUp(rounds);
          if (roundKeys.length === 0) return finish();
          round += 1;
          found = new Map();
          operation = undefined;
          start(blockHash);
          break;
        }
        case "operationInaccessible":
        case "operationError":
          if (event.operationId === operation)
            throw new Error("the chain could not serve the storage read");
          break;
        case "stop":
          throw new Error("the chain stopped following before it answered");
      }
    }
    throw new Error("the connection closed before the chain answered");
  };

  try {
    return await Promise.race([run(), timeout]);
  } finally {
    clearTimeout(timer);
    if (subscription !== undefined)
      try {
        call(4, "chainHead_v1_unfollow", [subscription]);
      } catch {
        // the connection is already gone
      }
    connection.close();
  }
}

/** One storage read an account's ring lookup depends on, or why it cannot be made. */
type RingStep =
  | { tag: "Unreadable"; reason: string }
  | { tag: "Read"; key: HexString; index: number };

/** How far each account's ring lookup got, filled in as the rounds run. */
interface RingPlan {
  /** The `Members.Members` read for the ring key of `PeopleLite.LitePeople`. */
  lite?: RingStep;
  /** The `People.People` read for the personal id of `People.AccountToPersonalId`. */
  person?: RingStep;
  /** The `Members.Members` read for the ring key of that `People.People` record. */
  full?: RingStep;
}

const reasonOf = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

/** The storage key of `Members.Members` for `ringKey` in `collection`. */
function membersKey(
  collection: keyof typeof COLLECTION_IDS,
  ringKey: HexString,
): HexString {
  return mapKey(
    MEMBERS_PREFIX + bytesToHex(COLLECTION_IDS[collection]).slice(2),
    hexToBytes(ringKey),
  );
}

function personalIdBytes(bytes: Uint8Array): Uint8Array {
  const reader = new Reader(bytes);
  const id = reader.take(8).slice();
  reader.finish();
  return id;
}

/** What a finished plan step says, given the value read for it. */
function ringLookup(
  step: RingStep | undefined,
  value: HexString | null | undefined,
  key: HexString | undefined,
): RingLookup {
  if (step === undefined) return { tag: "NoKey" };
  if (step.tag === "Unreadable") return step;
  if (key === undefined) return { tag: "NoKey" };
  if (value === null || value === undefined) return { tag: "NotListed", key };
  try {
    return {
      tag: "Listed",
      key,
      position: decodeRingPosition(hexToBytes(value)),
    };
  } catch (error) {
    return {
      tag: "Unreadable",
      reason: `the Members record is not the layout this reads (${reasonOf(error)})`,
    };
  }
}

/**
 * Look `accounts` up on the People chain, in one connection and at one block.
 * Each account is read on its own terms: an account with neither record reads
 * as absent for that account only.
 *
 * Ring membership is read from the ring keys the account's own records name,
 * never from a key derived here, so an account whose records name no key has
 * no ring lookup, which is not the same as not being a member.
 */
export async function readPeopleChain(
  chain: ChainProvider,
  peopleGenesis: HexString,
  accounts: AccountToRead[],
  timeoutMs: number = READ_TIMEOUT_MS,
): Promise<PeopleChainReading> {
  const keys = accounts.flatMap(({ accountId }) => {
    const id = hexToBytes(accountId);
    return [
      mapKey(CONSUMERS_PREFIX, id),
      mapKey(LITE_PEOPLE_PREFIX, id),
      mapKey(ACCOUNT_TO_PERSONAL_ID_PREFIX, id),
    ];
  });
  const plans: RingPlan[] = accounts.map(() => ({}));
  const ringKeys: { lite?: HexString; full?: HexString }[] = accounts.map(
    () => ({}),
  );

  // Round 1: the Lite ring key's membership, and the full person's record.
  const afterAccounts: FollowUp = ([first]) => {
    const next: HexString[] = [];
    plans.forEach((plan, index) => {
      const lite = first[index * 3 + 1];
      if (lite !== null)
        try {
          const key = decodeLitePersonKey(hexToBytes(lite));
          ringKeys[index].lite = key;
          plan.lite = {
            tag: "Read",
            key: membersKey("LitePeople", key),
            index: next.length,
          };
          next.push(plan.lite.key);
        } catch (error) {
          plan.lite = {
            tag: "Unreadable",
            reason: `the LitePeople record is not the layout this reads (${reasonOf(error)})`,
          };
        }
      const personalId = first[index * 3 + 2];
      if (personalId !== null)
        try {
          const key = mapKey(
            PEOPLE_RECORD_PREFIX,
            personalIdBytes(hexToBytes(personalId)),
          );
          plan.person = { tag: "Read", key, index: next.length };
          next.push(key);
        } catch (error) {
          plan.person = {
            tag: "Unreadable",
            reason: `the AccountToPersonalId record is not the layout this reads (${reasonOf(error)})`,
          };
        }
    });
    return next;
  };

  // Round 2: the full person's ring key, looked up in `Members`.
  const afterRecords: FollowUp = ([, second]) => {
    const next: HexString[] = [];
    plans.forEach((plan, index) => {
      if (plan.person?.tag !== "Read") {
        if (plan.person?.tag === "Unreadable") plan.full = plan.person;
        return;
      }
      const record = second[plan.person.index];
      if (record === null) {
        plan.full = {
          tag: "Unreadable",
          reason: "the account's personal id has no People record",
        };
        return;
      }
      try {
        const key = decodePersonKey(hexToBytes(record));
        ringKeys[index].full = key;
        plan.full = {
          tag: "Read",
          key: membersKey("People", key),
          index: next.length,
        };
        next.push(plan.full.key);
      } catch (error) {
        plan.full = {
          tag: "Unreadable",
          reason: `the People record is not the layout this reads (${reasonOf(error)})`,
        };
      }
    });
    return next;
  };

  const connection = await chain.connect(hexToBytes(peopleGenesis));
  const { blockHash, values, followUpValues } = await readStorageValues(
    connection,
    keys,
    timeoutMs,
    [afterAccounts, afterRecords],
  );
  const [liteRound = [], fullRound = []] = followUpValues ?? [];
  const readings = accounts.map((account, index): AccountReading => {
    const consumer = values[index * 3];
    const lite = values[index * 3 + 1];
    const plan = plans[index];
    const rings: RingMembership[] = [
      {
        collection: "LitePeople",
        lookup: ringLookup(
          plan.lite,
          plan.lite?.tag === "Read" ? liteRound[plan.lite.index] : undefined,
          ringKeys[index].lite,
        ),
      },
      {
        collection: "People",
        lookup: ringLookup(
          plan.full,
          plan.full?.tag === "Read" ? fullRound[plan.full.index] : undefined,
          ringKeys[index].full,
        ),
      },
    ];
    if (consumer === null)
      return { ...account, consumer: null, litePerson: lite !== null, rings };
    try {
      return {
        ...account,
        consumer: decodeConsumer(hexToBytes(consumer)),
        litePerson: lite !== null,
        rings,
      };
    } catch (error) {
      return {
        ...account,
        consumer: null,
        litePerson: lite !== null,
        rings,
        problem: `the Consumers record is not the layout this reads (${reasonOf(error)})`,
      };
    }
  });
  return { blockHash, readings };
}
