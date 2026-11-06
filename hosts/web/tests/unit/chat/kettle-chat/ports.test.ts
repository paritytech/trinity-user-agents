import { asProductId } from "@parity/truapi";
import { describe, expect, test } from "bun:test";
import * as sr25519 from "@scure/sr25519";
import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import {
  encodeSigned,
  signingPayload,
  statementHash,
} from "../../../../src/platform/statement.js";
import {
  conversationPorts,
  type KettleSubmission,
} from "../../../../src/chat/kettle-chat/ports.js";

const secret = sr25519.secretFromSeed(new Uint8Array(32).fill(5));
const signer = bytesToHex(sr25519.getPublicKey(secret));
const statement = {
  expiry: (1_900_000_000n << 32n) | 3n,
  channel: `0x${"11".repeat(32)}` as HexString,
  topics: [`0x${"22".repeat(32)}`, `0x${"33".repeat(32)}`] as HexString[],
  data: "0x01020304" as HexString,
};

/** A TrUAPI client whose core signs with `secret` and whose store accepts or refuses. */
function client(accept: boolean) {
  const submitted: unknown[] = [];
  const ok = <T>(value: T) => ({ isOk: () => true, isErr: () => false, value });
  const err = (error: unknown) => ({
    isOk: () => false,
    isErr: () => true,
    error,
  });
  return {
    submitted,
    client: {
      statementStore: {
        createProof: async ({
          statement: unsigned,
        }: {
          statement: typeof statement;
        }) => {
          const payload = signingPayload({
            expiry: unsigned.expiry,
            channel: hexToBytes(unsigned.channel),
            topics: unsigned.topics.map((topic) => hexToBytes(topic)),
            data: hexToBytes(unsigned.data),
          });
          return ok({
            proof: {
              tag: "Sr25519",
              value: {
                signer,
                signature: bytesToHex(sr25519.sign(secret, payload)),
              },
            },
          });
        },
        submit: async (signed: unknown) => {
          submitted.push(signed);
          return accept
            ? ok(undefined)
            : err({
                tag: "Domain",
                value: { reason: "statement-store submit failed: noAllowance" },
              });
        },
      },
    } as never,
  };
}

describe("Kettle Chat's statement ports", () => {
  test("record each submission under the hash the store names it by, with its answer", async () => {
    for (const accept of [true, false]) {
      const seen: KettleSubmission[] = [];
      const made = client(accept);
      const ports = conversationPorts({
        client: made.client,
        productId: asProductId("chat.example"),
        index: `0x${"44".repeat(32)}`,
        allow: async () => null,
        report: () => {},
        observe: (submission) => seen.push(submission),
      });
      const answer = await ports.submit(statement);
      // The store's name for what was submitted: the signed statement's encoding, hashed.
      const [sent] = made.submitted as (typeof statement & {
        proof: { value: { signer: HexString; signature: HexString } };
      })[];
      const expected = bytesToHex(
        statementHash(
          encodeSigned(
            {
              expiry: sent!.expiry,
              channel: hexToBytes(sent!.channel),
              topics: sent!.topics.map((topic) => hexToBytes(topic)),
              data: hexToBytes(sent!.data),
            },
            hexToBytes(sent!.proof.value.signer),
            () => hexToBytes(sent!.proof.value.signature),
          ),
        ),
      );
      expect(seen.map(({ at: _at, ...rest }) => rest)).toEqual([
        {
          signer,
          hash: expected,
          channel: statement.channel,
          topics: statement.topics,
          expiry: statement.expiry.toString(),
          dataBytes: 4,
          outcome: accept
            ? "new"
            : "error: statement-store submit failed: noAllowance",
        },
      ]);
      expect(answer).toEqual(
        accept
          ? { status: "new" }
          : { status: "rejected", reason: "noAllowance" },
      );
    }
  });

  test("submit nothing while the host does not allow the account", async () => {
    const { client: refusing, submitted } = client(true);
    const seen: KettleSubmission[] = [];
    const ports = conversationPorts({
      client: refusing,
      productId: asProductId("chat.example"),
      index: `0x${"44".repeat(32)}`,
      allow: async () => "not final yet",
      report: () => {},
      observe: (submission) => seen.push(submission),
    });
    expect(await ports.submit(statement)).toEqual({
      status: "rejected",
      reason: "noAllowance",
    });
    expect([submitted, seen]).toEqual([[], []]);
  });
});
