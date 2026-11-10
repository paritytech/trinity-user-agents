import { asProductId, createClient, createTransport } from "@parity/truapi";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { StatementStoreLike } from "../platform/statement-store.js";
import { isStored } from "../platform/statement-store.js";
import {
  decodeStatement,
  encodeSigned,
  verifyStatement,
} from "../../bridges/primitives/statement.js";
import { utf8, fromUtf8 } from "../../bridges/primitives/bytes.js";
import { digest } from "../invitations/sealing.js";
import {
  FIELD_DEMO,
  FieldError,
  readMessage,
  type FieldNetwork,
  type Received,
  type Listing,
  type Purchase,
} from "../../bridges/fields/protocol.js";

/** Public-only transport used by the host's declared demo bridge. */
export interface FieldTransport {
  account: string;
  network: FieldNetwork;
  check(): void;
  /** Post `message`; answers when its statement expires, in milliseconds since the epoch. */
  post(message: Listing | Purchase): Promise<number>;
  close(): void;
}

/** How long a posted statement lives. */
const LIFETIME_S = 3600;
/**
 * The furthest ahead a received statement may expire: one lifetime and some
 * clock skew. Later ones are not this demo's, and would outlive the board.
 */
const LATEST_EXPIRY_MS = 2 * LIFETIME_S * 1000;

/** The one topic every demo statement carries. */
export const FIELD_TOPIC = digest(FIELD_DEMO, "listings-and-purchases");

/**
 * The demo message `encoded` carries, with its statement's raw expiry, or null
 * when it is not one: too large, badly signed, off the demo's topic, already
 * expired, expiring further ahead than this demo posts, unreadable, or a
 * listing that names a seller other than its signer. `now` is in milliseconds.
 */
export function readStatement(
  encoded: Uint8Array,
  now: number,
): { received: Received; expiry: bigint } | null {
  try {
    const statement = decodeStatement(encoded);
    const expires = Number(statement.expiry >> 32n) * 1000;
    if (
      statement.data.length > 4096 ||
      !verifyStatement(statement) ||
      expires <= now ||
      expires > now + LATEST_EXPIRY_MS ||
      !statement.topics.some(
        (topic) => bytesToHex(topic) === bytesToHex(FIELD_TOPIC),
      )
    )
      return null;
    const message = readMessage(JSON.parse(fromUtf8(statement.data)));
    const sender = bytesToHex(statement.signer).toLowerCase();
    if (
      message === null ||
      (message.type === "listing" && message.seller !== sender)
    )
      return null;
    return { received: { sender, message, expires }, expiry: statement.expiry };
  } catch {
    // Untrusted network bytes are not diagnostics or product responses.
    return null;
  }
}

/** One product account for the two bounded statements this minimal demo needs. */
export const FIELD_INDEX = digest(FIELD_DEMO, "market-account");

/** Open the real product-scoped signer and the supplied Statement Store, never a fallback. */
export async function openFieldTransport(options: {
  runtime: Pick<WorkerPairingHostRuntime, "createProvider">;
  product: string;
  store(): Promise<StatementStoreLike>;
  allow(signal: AbortSignal): Promise<void>;
  signal: AbortSignal;
  network: FieldNetwork;
  receive(message: Received): void;
}): Promise<FieldTransport> {
  if (options.signal.aborted) throw new FieldError("session", "Session ended.");
  let provider: Awaited<
    ReturnType<WorkerPairingHostRuntime["createProvider"]>
  > | null = null;
  let transport: ReturnType<typeof createTransport> | null = null;
  let store: StatementStoreLike | null = null;
  let stop = () => {};
  let closed = false;
  let failure: Error = new FieldError("session", "Session ended.");
  let rejectStopped!: (error: Error) => void;
  const stopped = new Promise<never>((_resolve, reject) => {
    rejectStopped = reject;
  });
  const check = () => {
    if (closed) throw failure;
  };
  const close = () => {
    if (closed) return;
    closed = true;
    options.signal.removeEventListener("abort", close);
    rejectStopped(failure);
    stop();
    store?.close();
    transport?.dispose();
    provider?.dispose();
  };
  options.signal.addEventListener("abort", close, { once: true });
  const wait = <T>(
    pending: Promise<T>,
    acquired: (value: T) => void = () => {},
    late: (value: T) => void = () => {},
  ): Promise<T> =>
    Promise.race([
      pending.then((value) => {
        if (closed) {
          late(value);
          throw failure;
        }
        acquired(value);
        return value;
      }),
      stopped,
    ]);
  const productAccountId = {
    dotNsIdentifier: asProductId(options.product),
    derivationIndex: { tag: "Raw" as const, value: bytesToHex(FIELD_INDEX) },
  };
  try {
    await wait(
      options.runtime.createProvider({
        productId: asProductId(options.product),
        executionKind: "App",
      }),
      (value) => {
        provider = value;
      },
      (late) => late.dispose(),
    );
    check();
    transport = createTransport(provider!, { requestTimeoutMs: 30_000 });
    const client = createClient(transport);
    const account = await wait(
      Promise.resolve(client.account.getAccount({ productAccountId })),
    );
    check();
    if (account.isErr())
      throw new FieldError(
        "session",
        "Product account unavailable. Sign in first.",
      );
    const signer = account.value.account.publicKey.toLowerCase();
    await wait(
      options.store(),
      (value) => {
        store = value;
      },
      (late) => late.close(),
    );
    check();
    void store!.closed.then(() => {
      if (closed) return;
      failure = new FieldError(
        "network",
        "Statement Store disconnected. Reopen the marketplace.",
      );
      close();
    });
    let previousExpiry = 0n;
    await wait(
      store!.subscribe({ matchAll: [FIELD_TOPIC] }, (batch) => {
        if (closed) return;
        for (const encoded of batch) {
          const read = readStatement(encoded, Date.now());
          if (read === null) continue;
          if (read.received.sender === signer && read.expiry > previousExpiry)
            previousExpiry = read.expiry;
          options.receive(read.received);
        }
      }),
      (value) => {
        stop = value;
      },
      (late) => late(),
    );
    check();
    return {
      account: signer,
      network: options.network,
      close,
      check,
      async post(message) {
        check();
        await wait(options.allow(options.signal));
        check();
        const data = utf8(JSON.stringify(message));
        if (data.length > 4096)
          throw new FieldError("invalid", "Message too large.");
        const nowExpiry =
          BigInt(Math.floor(Date.now() / 1000) + LIFETIME_S) << 32n;
        const expiry =
          nowExpiry > previousExpiry ? nowExpiry : previousExpiry + 1n;
        previousExpiry = expiry;
        const statement = {
          expiry,
          channel: bytesToHex(digest(FIELD_DEMO, message.type)),
          topics: [bytesToHex(FIELD_TOPIC)],
          data: bytesToHex(data),
        };
        const proof = await wait(
          Promise.resolve(
            client.statementStore.createProof({ productAccountId, statement }),
          ),
        );
        check();
        if (proof.isErr() || proof.value.proof.tag !== "Sr25519")
          throw new FieldError("network", "Product signing failed.");
        const signed = proof.value.proof.value;
        const encoded = encodeSigned(
          {
            expiry: statement.expiry,
            channel: hexToBytes(statement.channel),
            topics: [FIELD_TOPIC],
            data,
          },
          hexToBytes(signed.signer),
          () => hexToBytes(signed.signature),
        );
        if (!verifyStatement(decodeStatement(encoded)))
          throw new FieldError("network", "Product proof invalid.");
        const result = await wait(store!.submit(encoded));
        if (!isStored(result))
          throw new FieldError(
            "network",
            "Statement Store refused the post. Check allowance and capacity.",
          );
        return Number(expiry >> 32n) * 1000;
      },
    };
  } catch (error) {
    close();
    throw error;
  }
}
