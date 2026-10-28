import { blake2b } from "@noble/hashes/blake2.js";
import { bytesToHex } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import { USERNAME_SEARCH_PATH } from "./username-search.js";

/**
 * Finding people by username for the host's contact picker.
 *
 * On a wallet session the names come from the identity backend's public
 * username index, through the dev server's same-origin route
 * (`username-search-plugin.ts`). A test user searches the other test users
 * instead, so made-up and real people never mix. The index is a listing, not
 * a proof, so a person found there is checked on chain before they are
 * picked: the username must name the same account there.
 */

/** One person the index lists. */
export interface ListedUser {
  /** The person's 32-byte account. */
  account: HexString;
  /** The username as the index holds it, such as `alice.06`. Shown by the host only. */
  username: string;
}

export interface UsernamePage {
  users: ListedUser[];
  /** Where the next page starts, or null after the last. */
  nextCursor: string | null;
}

export interface UsernameDirectory {
  /** Where the names come from, for the screen. */
  source: string;
  search(
    prefix: string,
    limit: number,
    cursor: string | null,
    signal: AbortSignal,
  ): Promise<UsernamePage>;
  /**
   * Check that `user.username` names `user.account` where it counts. Resolves
   * with null when it does, else with why not, for the screen.
   */
  confirm(user: ListedUser): Promise<string | null>;
}

/** A search that failed for a reason the user can be told. */
export class SearchFailure extends Error {
  constructor(
    message: string,
    /** How long the index asked not to be searched again, when it did. */
    readonly retryAfterMs: number | null = null,
  ) {
    super(message);
  }
}

const MAX_PREFIX_LENGTH = 64;
/** The identity backend's prefix rule: letters and digits, then optionally a dot and digits. */
const PREFIX = /^[a-z0-9]+(\.[0-9]*)?$/;
const USERNAME = /^[A-Za-z0-9]+(\.[0-9]+)?$/;
const DEFAULT_RETRY_MS = 60_000;

/** `typed` as the index takes a prefix, or null when it cannot be one. */
export function usernamePrefix(typed: string): string | null {
  const prefix = typed.trim().toLowerCase();
  return prefix.length <= MAX_PREFIX_LENGTH && PREFIX.test(prefix)
    ? prefix
    : null;
}

const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const SS58_PREFIX = new TextEncoder().encode("SS58PRE");

function base58(text: string): Uint8Array | null {
  let value = 0n;
  for (const char of text) {
    const digit = BASE58.indexOf(char);
    if (digit < 0) return null;
    value = value * 58n + BigInt(digit);
  }
  const bytes: number[] = [];
  for (; value > 0n; value /= 256n) bytes.unshift(Number(value % 256n));
  for (const char of text) {
    if (char !== "1") break;
    bytes.unshift(0);
  }
  return Uint8Array.from(bytes);
}

/** The 32-byte account an SS58 address names, or null when it is not a valid one. */
export function ss58Account(address: string): HexString | null {
  const decoded = base58(address);
  if (decoded === null || decoded.length === 0 || decoded[0] >= 128)
    return null;
  const prefixLength = decoded[0] < 64 ? 1 : 2;
  if (decoded.length !== prefixLength + 32 + 2) return null;
  const body = decoded.subarray(0, prefixLength + 32);
  const checksum = blake2b(new Uint8Array([...SS58_PREFIX, ...body]), {
    dkLen: 64,
  });
  const given = decoded.subarray(prefixLength + 32);
  if (checksum[0] !== given[0] || checksum[1] !== given[1]) return null;
  return bytesToHex(
    decoded.subarray(prefixLength, prefixLength + 32),
  ) as HexString;
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

function parsePage(body: unknown): UsernamePage {
  const malformed = new SearchFailure(
    "The username index answered with something that is not a list of usernames.",
  );
  if (!isObject(body) || !Array.isArray(body.usernames)) throw malformed;
  const { nextCursor } = body;
  if (nextCursor !== null && typeof nextCursor !== "string") throw malformed;
  const users = body.usernames.map((entry): ListedUser => {
    if (!isObject(entry)) throw malformed;
    const { accountId, username } = entry;
    if (typeof username !== "string" || !USERNAME.test(username))
      throw malformed;
    const account =
      typeof accountId === "string" ? ss58Account(accountId) : null;
    if (account === null) throw malformed;
    return { account, username };
  });
  return { users, nextCursor };
}

function retryAfterMs(response: Response): number {
  const seconds = Number(response.headers.get("retry-after"));
  return Number.isInteger(seconds) && seconds > 0
    ? seconds * 1000
    : DEFAULT_RETRY_MS;
}

async function readPage(response: Response): Promise<UsernamePage> {
  const contentType = response.headers.get("content-type") ?? "";
  if (!/^application\/json\b/i.test(contentType))
    throw new SearchFailure("Username search is not available on this server.");
  const body: unknown = await response.json().catch(() => undefined);
  if (response.status === 429) {
    const waitMs = retryAfterMs(response);
    throw new SearchFailure(
      `Too many searches. Try again in ${Math.ceil(waitMs / 1000)} s.`,
      waitMs,
    );
  }
  if (!response.ok)
    throw new SearchFailure(
      isObject(body) && typeof body.error === "string"
        ? body.error
        : `Username search failed (${response.status}).`,
    );
  return parsePage(body);
}

/**
 * The identity backend's index for `network`, through the dev server's route
 * under `base`. No cookie or credential is sent.
 */
export function routeDirectory(options: {
  base: string;
  network: string;
  /** The account `username` names on chain, or null when it names none. */
  owner(username: string): Promise<HexString | null>;
  fetch?: typeof fetch;
}): UsernameDirectory {
  const send = options.fetch ?? fetch.bind(globalThis);
  return {
    source: "the username index",
    async confirm({ username, account }) {
      let owner: HexString | null;
      try {
        owner = await options.owner(username);
      } catch (error) {
        return `Could not check ${username} on chain: ${error instanceof Error ? error.message : String(error)}`;
      }
      if (owner === null) return `No account holds ${username} on chain.`;
      return owner.toLowerCase() === account.toLowerCase()
        ? null
        : `On chain, ${username} names another account than the index lists.`;
    },
    async search(prefix, limit, cursor, signal) {
      const query = new URLSearchParams({
        network: options.network,
        prefix,
        limit: String(limit),
      });
      if (cursor !== null) query.set("cursor", cursor);
      let response: Response;
      try {
        response = await send(
          `${options.base}${USERNAME_SEARCH_PATH}?${query}`,
          {
            credentials: "omit",
            cache: "no-store",
            headers: { accept: "application/json" },
            signal,
          },
        );
      } catch (error) {
        if (signal.aborted) throw error;
        throw new SearchFailure("Username search is unreachable.");
      }
      return readPage(response);
    },
  };
}

/**
 * The chain itself, where there is no username index: what is typed is taken
 * as a whole username and read on chain (Asset Hub and People), so a person
 * is found only by their exact username, never by a prefix.
 */
export function exactUsernameDirectory(
  /** The account `username` names on chain, or null when it names none. */
  owner: (username: string) => Promise<HexString | null>,
): UsernameDirectory {
  return {
    source: "the chain, by exact username",
    confirm: async () => null,
    async search(username, _limit, cursor, signal) {
      if (cursor !== null) return { users: [], nextCursor: null };
      let account: HexString | null;
      try {
        account = await owner(username);
      } catch (error) {
        throw new SearchFailure(
          `Could not read ${username} on chain: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
      signal.throwIfAborted();
      return {
        users: account === null ? [] : [{ account, username }],
        nextCursor: null,
      };
    },
  };
}

/** A directory over a fixed list, matched by username prefix and paged by position. The list is the whole truth, so every user on it is confirmed. */
export function fixtureDirectory(
  source: string,
  users: readonly ListedUser[],
): UsernameDirectory {
  const sorted = [...users].sort((left, right) =>
    left.username.localeCompare(right.username),
  );
  return {
    source,
    confirm: async () => null,
    async search(prefix, limit, cursor, signal) {
      signal.throwIfAborted();
      const matches = sorted.filter((user) =>
        user.username.toLowerCase().startsWith(prefix),
      );
      const start = cursor === null ? 0 : Number(cursor);
      const end = start + limit;
      return {
        users: matches.slice(start, end),
        nextCursor: end < matches.length ? String(end) : null,
      };
    },
  };
}

/** What the search shows now. */
export type DiscoveryView =
  | { tag: "idle" }
  | { tag: "invalid" }
  | { tag: "searching"; users: ListedUser[] }
  | { tag: "found"; users: ListedUser[]; more: boolean }
  | { tag: "failed"; users: ListedUser[]; message: string };

export interface DiscoveryOptions {
  directory: UsernameDirectory;
  /** Whether `account` is already listed as a contact, so it is not shown twice. */
  known(account: HexString): boolean;
  show(view: DiscoveryView): void;
  pageSize?: number;
  /** How long typing must pause before a search is sent. */
  delayMs?: number;
  now?: () => number;
}

/**
 * Follows what the user types and searches the directory for it: one search
 * at a time, sent once typing pauses, a page at a time. A newer query cancels
 * the one in flight and an answer to an older one is dropped. After the index
 * asks to wait, nothing is sent until the wait is over. Nothing is sent once
 * closed.
 */
export class UsernameDiscovery {
  private prefix: string | null = null;
  private users: ListedUser[] = [];
  private cursor: string | null = null;
  private inFlight: AbortController | null = null;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private waitUntil = 0;
  private closed = false;

  constructor(private readonly options: DiscoveryOptions) {}

  type(text: string): void {
    if (this.closed) return;
    this.cancel();
    this.users = [];
    this.cursor = null;
    this.prefix = usernamePrefix(text);
    if (text.trim() === "") return this.options.show({ tag: "idle" });
    if (this.prefix === null) return this.options.show({ tag: "invalid" });
    this.options.show({ tag: "searching", users: [] });
    this.timer = setTimeout(
      () => void this.next(),
      this.options.delayMs ?? 300,
    );
  }

  /** Ask for the page after the ones shown, when there is one. */
  more(): void {
    if (this.closed || this.inFlight !== null || this.cursor === null) return;
    this.options.show({ tag: "searching", users: [...this.users] });
    void this.next();
  }

  close(): void {
    this.closed = true;
    this.cancel();
  }

  private cancel(): void {
    clearTimeout(this.timer);
    this.inFlight?.abort();
    this.inFlight = null;
  }

  private async next(): Promise<void> {
    const prefix = this.prefix;
    if (prefix === null) return;
    const now = this.options.now ?? Date.now;
    const waitMs = this.waitUntil - now();
    if (waitMs > 0)
      return this.options.show({
        tag: "failed",
        users: [...this.users],
        message: `Too many searches. Try again in ${Math.ceil(waitMs / 1000)} s.`,
      });
    const request = new AbortController();
    this.inFlight = request;
    try {
      const page = await this.options.directory.search(
        prefix,
        this.options.pageSize ?? 20,
        this.cursor,
        request.signal,
      );
      if (this.inFlight !== request) return;
      this.inFlight = null;
      const seen = new Set(
        this.users.map((user) => user.account.toLowerCase()),
      );
      for (const user of page.users) {
        const account = user.account.toLowerCase();
        if (seen.has(account) || this.options.known(user.account)) continue;
        seen.add(account);
        this.users.push(user);
      }
      this.cursor = page.nextCursor;
      this.options.show({
        tag: "found",
        users: [...this.users],
        more: this.cursor !== null,
      });
    } catch (error) {
      if (this.inFlight !== request) return;
      this.inFlight = null;
      if (error instanceof SearchFailure && error.retryAfterMs !== null)
        this.waitUntil = now() + error.retryAfterMs;
      this.options.show({
        tag: "failed",
        users: [...this.users],
        message:
          error instanceof SearchFailure
            ? error.message
            : "Username search failed.",
      });
    }
  }
}
