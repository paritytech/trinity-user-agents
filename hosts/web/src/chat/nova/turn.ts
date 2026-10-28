import { bytesToHex } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import { blake2b256 } from "./crypto.js";
import { concat, utf8 } from "./codec.js";

/**
 * TURN relay credentials for calls, bought with a personhood proof.
 *
 * The identity backend (DUB) issues short-lived relay credentials at
 * `POST /api/v1/turn/issue-with-proof` to anyone who proves, anonymously, that
 * they are a person: a ring-VRF proof in one of the two People collections,
 * made under a product's context over
 * `blake2b256("dub/turn-credential/v1" || timestamp as big-endian u64)`, with
 * the timestamp within 60 seconds of the backend's clock. Written from DUB's
 * `crates/turn` (`http/proof_routes.rs`, `proof/message.rs`,
 * `proof/roots.rs`).
 *
 * The proof comes from the core through TrUAPI `account.createAccountProof`,
 * asked by the chat product (`chat.<tld>`) for the key Humanity owns
 * (`peopl.<tld>`) in the context of another product, the game
 * (`dim2.<tld>`), which DUB accepts. The core gives the proof only when both
 * owners' manifests grant `chat` the `context` scope; without them it answers
 * `NotAllowlisted`, and nothing is sent to DUB. The host does not get around
 * that check.
 *
 * The credentials live in memory only, until a little before they expire, and
 * are never logged or reported.
 */

/** The backend route, under the network's identity backend. */
export const TURN_ISSUE_PATH = "/api/v1/turn/issue-with-proof";

const LABEL = utf8("dub/turn-credential/v1");

/** How far from the backend's clock a proof's timestamp may be, in seconds. */
export const TURN_SKEW_SECS = 60;
/** Credentials are let go this long before they expire. */
const EXPIRY_MARGIN_MS = 30_000;
/** How long the backend may take to answer. */
const ISSUE_TIMEOUT_MS = 10_000;
/** After a refusal, how long to wait before asking again, so a call never retries in a loop. */
export const TURN_RETRY_AFTER_MS = {
  /** A grant is missing: it changes only when a manifest does. */
  "grant-missing": 10 * 60_000,
  other: 60_000,
} as const;

/** The message a proof for `timestampSecs` is made over. */
export function turnMessage(timestampSecs: number): Uint8Array {
  const stamp = new Uint8Array(8);
  new DataView(stamp.buffer).setBigUint64(0, BigInt(timestampSecs), false);
  return blake2b256(concat(LABEL, stamp));
}

/** The two collections DUB accepts, by their on-chain ids. */
export const PERSONHOOD_COLLECTIONS = {
  lite: bytesToHex(utf8("pop:polkadot.network/people-lite")) as HexString,
  full: bytesToHex(utf8("pop:polkadot.network/people     ")) as HexString,
} as const;

/** Who proves, whose key signs, and whose context the proof is bound to. */
export interface TurnIdentity {
  /** The product asking, as the core sees it: `chat.<tld>`. */
  caller: string;
  /** The owner of the personhood key: `peopl.<tld>`. */
  keyOwner: string;
  /** The product whose context DUB checks: `dim2.<tld>`. */
  context: string;
}

/** The ids on the network with dotNS TLD and identity suffix `suffix`. */
export const turnIdentity = (suffix: string): TurnIdentity => ({
  caller: `chat.${suffix}`,
  keyOwner: `peopl.${suffix}`,
  context: `dim2.${suffix}`,
});

/** A registered ring-VRF key, as `account.listRingVrfKeys` gives it. */
export interface RingKey {
  handle: {
    dotNsIdentifier: string;
    derivationIndex:
      | { tag: "Index"; value: number }
      | { tag: "Raw"; value: HexString };
  };
  rings: {
    chainId: HexString;
    junctions: (
      | { tag: "PalletInstance"; value: number }
      | { tag: "CollectionId"; value: HexString }
    )[];
  }[];
}

/** A refusal from the core, by its error tag and any reason it gave. */
export class ProofRefused extends Error {
  constructor(
    readonly tag: string,
    reason?: string,
  ) {
    super(reason === undefined ? tag : `${tag}: ${reason}`);
  }
}

/**
 * The two TrUAPI account calls a proof needs, made as the caller. A refusal
 * throws {@link ProofRefused}.
 */
export interface ProofSource {
  listRingVrfKeys(owner: string): Promise<RingKey[]>;
  createAccountProof(request: {
    keyHandle: RingKey["handle"];
    context: { productId: string; suffix: { tag: "Index"; value: number } };
    ringLocation: RingKey["rings"][number];
    message: HexString;
  }): Promise<{ proof: HexString; ringIndex: number; ringRevision: number }>;
}

/** What DUB takes, per `IssueWithProofBody`. */
export interface TurnProofBody {
  productId: string;
  collection: HexString;
  proof: HexString;
  ringIndex: number;
  ringRevision: number;
  timestamp: number;
}

/** Why there are no relay credentials, in words a person can act on. */
export type TurnProblem =
  | { code: "grant-missing"; reason: string }
  | { code: "no-person-key"; reason: string }
  | { code: "proof-refused"; reason: string }
  | { code: "issuer-refused"; status: number; reason: string }
  | { code: "issuer-unreachable"; reason: string }
  | { code: "not-configured"; reason: string };

const collectionOf = (ring: RingKey["rings"][number]): HexString | null => {
  const junction = ring.junctions.find((entry) => entry.tag === "CollectionId");
  return junction?.tag === "CollectionId" ? junction.value : null;
};

/**
 * Ask the core for a proof, Lite membership first, then full. Resolves to
 * DUB's request body, or to why there is none.
 */
export async function proveForTurn(
  source: ProofSource,
  identity: TurnIdentity,
  timestampSecs: number,
): Promise<TurnProofBody | TurnProblem> {
  let keys: RingKey[];
  try {
    keys = await source.listRingVrfKeys(identity.keyOwner);
  } catch (error) {
    return refusal(error, identity);
  }
  const message = bytesToHex(turnMessage(timestampSecs)) as HexString;
  for (const wanted of [
    PERSONHOOD_COLLECTIONS.lite,
    PERSONHOOD_COLLECTIONS.full,
  ])
    for (const key of keys)
      for (const ring of key.rings) {
        if (collectionOf(ring)?.toLowerCase() !== wanted) continue;
        try {
          const proved = await source.createAccountProof({
            keyHandle: key.handle,
            context: {
              productId: identity.context,
              suffix: { tag: "Index", value: 0 },
            },
            ringLocation: ring,
            message,
          });
          return {
            productId: identity.context,
            collection: wanted,
            proof: proved.proof,
            ringIndex: proved.ringIndex,
            ringRevision: proved.ringRevision,
            timestamp: timestampSecs,
          };
        } catch (error) {
          // Not a member of this ring: the person may be in the other one.
          if (
            error instanceof ProofRefused &&
            (error.tag === "NotMember" || error.tag === "KeyNotInRing")
          )
            continue;
          return refusal(error, identity);
        }
      }
  return {
    code: "no-person-key",
    reason: `This wallet has no ${identity.keyOwner} key in a People ring, so it cannot prove it is a person.`,
  };
}

function refusal(error: unknown, identity: TurnIdentity): TurnProblem {
  if (error instanceof ProofRefused && error.tag === "NotAllowlisted")
    return {
      code: "grant-missing",
      reason: `The core refused the proof: ${identity.keyOwner} and ${identity.context} must both grant ${identity.caller} the "context" scope in their manifests.`,
    };
  if (error instanceof ProofRefused && error.tag === "Rejected")
    return {
      code: "proof-refused",
      reason: `Access to ${identity.keyOwner}'s account was refused.`,
    };
  return {
    code: "proof-refused",
    reason: `The core gave no proof: ${error instanceof Error ? error.message : String(error)}`,
  };
}

/** DUB's answer to a proof, per `IssueResponse`. */
interface IssueResponse {
  servers: string[];
  username: string;
  password: string;
  ttl: number;
}

const isIssueResponse = (value: unknown): value is IssueResponse => {
  if (typeof value !== "object" || value === null) return false;
  const answer = value as Record<string, unknown>;
  return (
    Array.isArray(answer.servers) &&
    answer.servers.every(
      (server) => typeof server === "string" && /^(stun|turns?):/.test(server),
    ) &&
    typeof answer.username === "string" &&
    typeof answer.password === "string" &&
    typeof answer.ttl === "number" &&
    answer.ttl > 0
  );
};

export type FetchLike = (
  url: string,
  init: {
    method: "POST";
    headers: Record<string, string>;
    body: string;
    signal: AbortSignal;
  },
) => Promise<{
  status: number;
  json(): Promise<unknown>;
  text(): Promise<string>;
}>;

/** Relay servers ready for `RTCPeerConnection`, and when they stop working. */
export interface TurnPass {
  iceServers: RTCIceServer[];
  expiresAt: number;
}

/** Post a proof to DUB once. The answer's body is read only on success, and never kept on failure beyond a short reason. */
export async function redeemTurnProof(
  fetch: FetchLike,
  issuer: string,
  body: TurnProofBody,
  now: () => number,
): Promise<TurnPass | TurnProblem> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), ISSUE_TIMEOUT_MS);
  try {
    const answer = await fetch(`${issuer}${TURN_ISSUE_PATH}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
      signal: controller.signal,
    });
    if (answer.status !== 200 && answer.status !== 201) {
      let reason = "";
      try {
        const text = (await answer.text()).slice(0, 200);
        const parsed: unknown = JSON.parse(text);
        reason =
          typeof parsed === "object" &&
          parsed !== null &&
          typeof (parsed as { error?: unknown }).error === "string"
            ? (parsed as { error: string }).error
            : text;
      } catch {
        // No readable reason; the status says enough.
      }
      return {
        code: "issuer-refused",
        status: answer.status,
        reason: `The identity backend refused the proof (${answer.status})${reason === "" ? "" : `: ${reason}`}.`,
      };
    }
    const data = await answer.json();
    if (!isIssueResponse(data))
      return {
        code: "issuer-refused",
        status: answer.status,
        reason: "The identity backend answered in an unexpected shape.",
      };
    return {
      iceServers: data.servers.map((urls) =>
        urls.startsWith("stun:")
          ? { urls }
          : { urls, username: data.username, credential: data.password },
      ),
      expiresAt: now() + data.ttl * 1000 - EXPIRY_MARGIN_MS,
    };
  } catch (error) {
    return {
      code: "issuer-unreachable",
      reason: controller.signal.aborted
        ? "The identity backend did not answer in time."
        : `The identity backend could not be reached: ${error instanceof Error ? error.message : String(error)}`,
    };
  } finally {
    clearTimeout(timer);
  }
}

export interface TurnCredentialsDeps {
  identity: TurnIdentity;
  /** The network's identity backend origin, or null when it has none. */
  issuer: string | null;
  /** Opens a proof source for the caller, used once and disposed. */
  proofs(): Promise<{ source: ProofSource; dispose(): void }>;
  fetch: FetchLike;
  now?: () => number;
}

/** What the screen may show about relay credentials. Never the credentials. */
export type TurnStatus =
  | { state: "idle" }
  | { state: "fetching" }
  | { state: "ready"; expiresAt: number; servers: number }
  | { state: "unavailable"; problem: TurnProblem; retryAt: number };

/**
 * One session's relay credentials: asked for when a call needs them, kept in
 * memory until shortly before they expire, and asked for again after that.
 * One request is in flight at a time. After a refusal, nothing is asked until
 * the wait in {@link TURN_RETRY_AFTER_MS} has passed.
 */
export class TurnCredentials {
  private pass: TurnPass | null = null;
  private inFlight: Promise<RTCIceServer[]> | null = null;
  private failure: { problem: TurnProblem; retryAt: number } | null = null;
  private generation = 0;
  private readonly now: () => number;
  private readonly listeners = new Set<() => void>();

  constructor(private readonly deps: TurnCredentialsDeps) {
    this.now = deps.now ?? Date.now;
  }

  status(): TurnStatus {
    if (this.pass !== null && this.pass.expiresAt > this.now())
      return {
        state: "ready",
        expiresAt: this.pass.expiresAt,
        servers: this.pass.iceServers.length,
      };
    if (this.inFlight !== null) return { state: "fetching" };
    if (this.failure !== null) return { state: "unavailable", ...this.failure };
    return { state: "idle" };
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private changed(): void {
    for (const listener of [...this.listeners]) listener();
  }

  /**
   * Relay servers for a call, or none when there are no credentials. Never
   * throws: a call goes on with the other ICE servers it has.
   */
  iceServers(): Promise<RTCIceServer[]> {
    if (this.pass !== null && this.pass.expiresAt > this.now())
      return Promise.resolve(this.pass.iceServers);
    this.pass = null;
    if (this.inFlight !== null) return this.inFlight;
    if (this.failure !== null && this.failure.retryAt > this.now())
      return Promise.resolve([]);
    const generation = this.generation;
    const work = this.fetchPass().then((result) => {
      if (generation !== this.generation) return [];
      this.inFlight = null;
      if ("iceServers" in result) {
        this.pass = result;
        this.failure = null;
        this.changed();
        return result.iceServers;
      }
      this.failure = {
        problem: result,
        retryAt:
          this.now() +
          (result.code === "grant-missing"
            ? TURN_RETRY_AFTER_MS["grant-missing"]
            : TURN_RETRY_AFTER_MS.other),
      };
      this.changed();
      return [];
    });
    this.inFlight = work;
    this.changed();
    return work;
  }

  private async fetchPass(): Promise<TurnPass | TurnProblem> {
    const issuer = this.deps.issuer;
    if (issuer === null)
      return {
        code: "not-configured",
        reason:
          "This network has no identity backend to issue relay credentials.",
      };
    let opened: { source: ProofSource; dispose(): void };
    try {
      opened = await this.deps.proofs();
    } catch (error) {
      return {
        code: "proof-refused",
        reason: `The core could not be asked for a proof: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
    const { source, dispose } = opened;
    let proved: TurnProofBody | TurnProblem;
    try {
      proved = await proveForTurn(
        source,
        this.deps.identity,
        Math.floor(this.now() / 1000),
      );
    } finally {
      dispose();
    }
    if ("code" in proved) return proved;
    return redeemTurnProof(this.deps.fetch, issuer, proved, this.now);
  }

  /** Forget everything, as when the session ends; a request in flight answers no one. */
  reset(): void {
    this.generation += 1;
    this.pass = null;
    this.inFlight = null;
    this.failure = null;
    this.changed();
  }
}
