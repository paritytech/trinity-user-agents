import { describe, expect, test } from "bun:test";
import { blake2b } from "@noble/hashes/blake2.js";
import { bytesToHex } from "@parity/truapi/scale";
import * as sr25519 from "@scure/sr25519";
import {
  blake2b256,
  cryptoRandom,
  unseal,
} from "../../../../src/chat/nova/crypto.js";
import { MemoryHopNode } from "../../../../fixtures/nova/hop-memory.js";
import {
  HOP_CHUNK_BYTES,
  claimPayload,
  decodeRoot,
  encodeRoot,
  hopDownload,
  hopUpload,
  submitPayload,
  ticketKeys,
  type HopSender,
} from "../../../../src/chat/nova/hop.js";

const hex = (value: Uint8Array) => Buffer.from(value).toString("hex");

/** A synthetic sender account. It holds nothing. */
function sender(): HopSender {
  const secret = sr25519.secretFromSeed(new Uint8Array(32).fill(7));
  return {
    publicKey: sr25519.getPublicKey(secret),
    sign: (payload) => sr25519.sign(secret, payload),
  };
}

const deps = () => {
  let clock = 1_760_000_000_000;
  return { random: cryptoRandom, now: () => (clock += 1) };
};

describe("HOP root entry, in the iOS app's test vectors", () => {
  test("an inline file is version 0, payload 0, then the bytes", () => {
    // `VersionedUploadedFileTests.inlineEnvelopeMatchesSpecVector`.
    expect(
      hex(encodeRoot({ kind: "inline", data: Uint8Array.of(0xde, 0xad) })),
    ).toBe("000008dead");
  });

  test("a chunked file is version 0, payload 1, the size as u64, then the hashes", () => {
    const chunks = [new Uint8Array(32).fill(0xab)];
    const encoded = encodeRoot({ kind: "chunked", totalSize: 300n, chunks });
    expect(hex(encoded)).toBe(
      `0001${"2c01000000000000"}04${"80"}${"ab".repeat(32)}`,
    );
    expect(decodeRoot(encoded)).toEqual({
      kind: "chunked",
      totalSize: 300n,
      chunks,
    });
    expect(() => decodeRoot(Uint8Array.of(1, 0))).toThrow("version tag 1");
    expect(() => decodeRoot(Uint8Array.of(0, 2))).toThrow("payload tag 2");
  });
});

describe("HOP keys and signed payloads", () => {
  test("a ticket's keys are BLAKE2b keyed by the ticket over the context name", () => {
    const ticket = new Uint8Array(32).map((_, index) => index);
    const keyed = (context: string) =>
      blake2b(new TextEncoder().encode(context), { key: ticket, dkLen: 32 });
    const keys = ticketKeys(ticket);
    expect(keys.encryption).toEqual(keyed("encryption"));
    expect(keys.signer).toEqual(
      sr25519.getPublicKey(sr25519.secretFromSeed(keyed("signer"))),
    );
    expect(() => ticketKeys(new Uint8Array(31))).toThrow("32 bytes");
  });

  test("the submit payload hashes its context, the data hash and the time", () => {
    const hash = new Uint8Array(32).fill(1);
    expect(submitPayload(hash, 5n)).toEqual(
      blake2b256(
        Buffer.concat([
          Buffer.from("hop-submit-v1:"),
          hash,
          Buffer.from("0500000000000000", "hex"),
        ]),
      ),
    );
    expect(claimPayload(hash)).toEqual(
      blake2b256(Buffer.concat([Buffer.from("hop-claim-v1:"), hash])),
    );
  });
});

describe("HOP transfer over a node in memory", () => {
  test("a small file goes whole in the root entry and comes back", async () => {
    const node = new MemoryHopNode();
    const file = new TextEncoder().encode("a synthetic note");
    const sent = await hopUpload(node.connect(), file, sender(), deps());
    expect(node.calls).toEqual(["hop_submit"]);
    // The node holds only sealed bytes, addressed by their hash.
    const [stored] = [...node.entries.values()];
    expect(bytesToHex(blake2b256(stored.data))).toBe(
      bytesToHex(sent.identifier),
    );
    expect(unseal(ticketKeys(sent.ticket).encryption, stored.data)).toEqual(
      encodeRoot({ kind: "inline", data: file }),
    );
    expect(await hopDownload(node.connect(), sent, 1024)).toEqual(file);
  });

  test("a large file goes in chunks, in order, and the root names them", async () => {
    const node = new MemoryHopNode();
    const file = new Uint8Array(HOP_CHUNK_BYTES * 2 + 123).map(
      (_, index) => index % 251,
    );
    const progress: number[] = [];
    const sent = await hopUpload(node.connect(), file, sender(), deps(), {
      progress: (done) => progress.push(done),
    });
    expect(node.calls).toEqual([
      "hop_submit",
      "hop_submit",
      "hop_submit",
      "hop_submit",
    ]);
    expect(progress.at(-1)).toBe(file.length);
    const back = await hopDownload(node.connect(), sent, file.length);
    expect(back.length).toBe(file.length);
    expect(Buffer.from(back).equals(Buffer.from(file))).toBe(true);
  });

  test("refuses changed bytes, a wrong ticket, and a file larger than allowed", async () => {
    const node = new MemoryHopNode();
    const file = new Uint8Array(64).fill(9);
    const sent = await hopUpload(node.connect(), file, sender(), deps());
    await expect(hopDownload(node.connect(), sent, 10)).rejects.toThrow(
      "larger than this host accepts",
    );
    const wrong = { ...sent, ticket: new Uint8Array(32).fill(1) };
    await expect(hopDownload(node.connect(), wrong, 1024)).rejects.toThrow(
      "bad recipient signature",
    );
    // A node that answers with other bytes than the hash names is caught.
    const entry = [...node.entries.values()][0];
    entry.data = entry.data.slice();
    entry.data[20] ^= 1;
    await expect(hopDownload(node.connect(), sent, 1024)).rejects.toThrow(
      "do not match their hash",
    );
  });

  test("an unauthorized sender is refused, and a canceled upload stops", async () => {
    const node = new MemoryHopNode();
    node.authorized.add(`0x${"00".repeat(32)}`);
    await expect(
      hopUpload(node.connect(), new Uint8Array(8), sender(), deps()),
    ).rejects.toThrow("no storage authorization");
    const open = new MemoryHopNode();
    const controller = new AbortController();
    controller.abort();
    await expect(
      hopUpload(open.connect(), new Uint8Array(8), sender(), deps(), {
        signal: controller.signal,
      }),
    ).rejects.toThrow("canceled");
    expect(open.entries.size).toBe(0);
  });
});
