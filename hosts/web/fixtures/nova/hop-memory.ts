import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import { blake2b256, sr25519Verify } from "../../src/chat/crypto.js";
import {
  HOP_ERRORS,
  HopError,
  ackPayload,
  claimPayload,
  fromSr25519Multi,
  verifySubmission,
  type HopConnection,
} from "../../src/chat/hop.js";

/**
 * A HOP node in memory, for tests: it checks submissions and claims the way
 * the protocol asks (the sender's signature, the recipient's signature, the
 * entry's hash) and keeps entries until they are acknowledged. Nothing leaves
 * the process. It holds only sealed bytes and public keys.
 */
export class MemoryHopNode {
  readonly entries = new Map<
    string,
    { data: Uint8Array; recipients: Uint8Array[] }
  >();
  /** Every call, by method, in order. */
  readonly calls: string[] = [];
  /** Senders allowed to submit, as 0x hex of their public key. Empty allows every sender. */
  readonly authorized = new Set<string>();

  connect(): HopConnection {
    let open = true;
    return {
      call: async (method, params) => {
        if (!open) throw new HopError(null, "closed");
        this.calls.push(method);
        return this.answer(method, params);
      },
      close: () => (open = false),
    };
  }

  private answer(method: string, params: Record<string, unknown>): unknown {
    const bytesOf = (key: string) => {
      const value = params[key];
      if (typeof value !== "string")
        throw new HopError(-32602, `missing ${key}`);
      return hexToBytes(value);
    };
    if (method === "hop_submit") {
      const data = bytesOf("data");
      const signer = bytesOf("signer");
      const timestamp = params.submit_timestamp;
      if (typeof timestamp !== "number")
        throw new HopError(-32602, "missing submit_timestamp");
      if (
        !verifySubmission({
          data,
          signer,
          signature: bytesOf("signature"),
          timestampMs: BigInt(timestamp),
        })
      )
        throw new HopError(-32602, "bad sender signature");
      const sender = bytesToHex(fromSr25519Multi(signer, 32)!);
      if (this.authorized.size > 0 && !this.authorized.has(sender))
        throw new HopError(1010, "the sender has no storage authorization");
      const recipients = (params.recipients as string[]).map((value) => {
        const key = fromSr25519Multi(hexToBytes(value), 32);
        if (key === null)
          throw new HopError(-32602, "not an sr25519 recipient");
        return key;
      });
      this.entries.set(bytesToHex(blake2b256(data)), { data, recipients });
      return {
        poolStatus: {
          entryCount: this.entries.size,
          totalBytes: 0,
          maxBytes: 0,
        },
      };
    }
    if (method === "hop_claim" || method === "hop_ack") {
      const hash = bytesOf("raw_hash");
      const signature = fromSr25519Multi(bytesOf("signature"), 64);
      const entry = this.entries.get(bytesToHex(hash));
      if (entry === undefined)
        throw new HopError(HOP_ERRORS.notFound, "not found");
      const payload =
        method === "hop_claim" ? claimPayload(hash) : ackPayload(hash);
      if (
        signature === null ||
        !entry.recipients.some((key) => sr25519Verify(payload, signature, key))
      )
        throw new HopError(-32602, "bad recipient signature");
      if (method === "hop_ack") {
        this.entries.delete(bytesToHex(hash));
        return null;
      }
      return bytesToHex(entry.data);
    }
    throw new HopError(-32601, `unknown method ${method}`);
  }
}
