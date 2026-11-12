import { describe, expect, test } from "bun:test";
import {
  fromHex,
  toHex,
  utf8,
  concat,
} from "../../../bridges/primitives/bytes.js";
import {
  LocketError,
  DETAIL_SCHEMAS,
} from "../../../bridges/locket/protocol.js";
import {
  envelopeId,
  envelopeKeyId,
  openEnvelope,
  sealEnvelope,
  type SealedDetail,
} from "../../../src/locket/envelope.js";
import {
  sealTo,
  x25519Pair,
  digest,
} from "../../../src/invitations/sealing.js";

let seed = 1;
const random = (length: number) =>
  Uint8Array.from({ length }, () => (seed = (seed * 31 + 7) % 251));
const pair = x25519Pair(random(32));
const recipient = { keyId: toHex(random(16)), publicKey: toHex(pair.public) };
const ACCOUNT = `0x${"ab".repeat(32)}`;
const CONTEXT = `0x${"cd".repeat(32)}`;
const binding = {
  product: "market.example",
  account: ACCOUNT,
  context: CONTEXT,
};
const expected = {
  product: "market.example",
  accounts: [ACCOUNT],
  context: CONTEXT,
};
const FIELDS: SealedDetail[] = [
  {
    id: "ship-to",
    kind: "address",
    schema: DETAIL_SCHEMAS.address,
    value: "Ada Example\n24 Sample Lane",
  },
  {
    id: "phone",
    kind: "phone",
    schema: DETAIL_SCHEMAS.phone,
    value: "+1 555 010 0123",
  },
];

const failure = (run: () => unknown): string | null => {
  try {
    run();
    return null;
  } catch (error) {
    return error instanceof LocketError ? error.code : "not a LocketError";
  }
};

describe("the Locket envelope", () => {
  test("opens for its recipient into the same details, in padded buckets", () => {
    const sealed = sealEnvelope(recipient, binding, FIELDS, random);
    expect({
      opened: openEnvelope(pair, sealed, {
        ...expected,
        accounts: [`0x${"ef".repeat(32)}`, ACCOUNT],
      }),
      keyId: envelopeKeyId(sealed),
      bucketed: (sealed.length - 18 - 48) % 256,
      id: envelopeId(sealed) === envelopeId(sealed.slice()),
    }).toEqual({
      opened: { account: ACCOUNT, details: FIELDS },
      keyId: recipient.keyId,
      bucketed: 0,
      id: true,
    });
  });

  test("fails on any changed byte, and says which header it does not read", () => {
    const sealed = sealEnvelope(recipient, binding, FIELDS, random);
    const changed = (at: number) => {
      const copy = sealed.slice();
      copy[at]! ^= 1;
      return failure(() => openEnvelope(pair, copy, expected));
    };
    expect({
      version: changed(0),
      suite: changed(1),
      keyId: changed(5),
      ephemeral: changed(30),
      body: changed(sealed.length - 40),
      tag: changed(sealed.length - 1),
      cut: failure(() => openEnvelope(pair, sealed.subarray(0, 100), expected)),
    }).toEqual({
      version: "unsupported",
      suite: "unsupported",
      keyId: "invalid",
      ephemeral: "invalid",
      body: "invalid",
      tag: "invalid",
      cut: "invalid",
    });
  });

  test("opens only for its product, account and context, and only with its key", () => {
    const sealed = sealEnvelope(recipient, binding, FIELDS, random);
    expect({
      product: failure(() =>
        openEnvelope(pair, sealed, { ...expected, product: "other.example" }),
      ),
      account: failure(() =>
        openEnvelope(pair, sealed, {
          ...expected,
          accounts: [`0x${"ef".repeat(32)}`],
        }),
      ),
      context: failure(() =>
        openEnvelope(pair, sealed, {
          ...expected,
          context: `0x${"00".repeat(32)}`,
        }),
      ),
      key: failure(() =>
        openEnvelope(x25519Pair(random(32)), sealed, expected),
      ),
    }).toEqual({
      product: "invalid",
      account: "invalid",
      context: "invalid",
      key: "invalid",
    });
  });

  test("refuses duplicate ids or kinds, wrong schemas and values too long, when sealing", () => {
    const [address, phone] = FIELDS as [SealedDetail, SealedDetail];
    expect({
      sameId: failure(() =>
        sealEnvelope(
          recipient,
          binding,
          [address, { ...phone, id: address.id }],
          random,
        ),
      ),
      sameKind: failure(() =>
        sealEnvelope(
          recipient,
          binding,
          [address, { ...address, id: "other" }],
          random,
        ),
      ),
      schema: failure(() =>
        sealEnvelope(
          recipient,
          binding,
          [{ ...address, schema: "postal-address/2" }],
          random,
        ),
      ),
      phoneLines: failure(() =>
        sealEnvelope(recipient, binding, [{ ...phone, value: "1\n2" }], random),
      ),
      none: failure(() => sealEnvelope(recipient, binding, [], random)),
      tooLong: failure(() =>
        sealEnvelope(
          recipient,
          binding,
          [
            { ...address, value: "é".repeat(160) },
            { ...phone, value: "€".repeat(160) },
          ],
          random,
        ),
      ),
    }).toEqual({
      sameId: "invalid",
      sameKind: "invalid",
      schema: "invalid",
      phoneLines: "invalid",
      none: "invalid",
      tooLong: "limit",
    });
  });

  test("refuses well-sealed contents that break a rule, such as a kind twice", () => {
    const craft = (fields: unknown[]) => {
      const header = concat(Uint8Array.of(1, 1), fromHex(recipient.keyId));
      const json = utf8(
        JSON.stringify({
          product: binding.product,
          account: ACCOUNT,
          context: CONTEXT,
          fields,
        }),
      );
      const plaintext = new Uint8Array(512);
      new DataView(plaintext.buffer).setUint16(0, json.length);
      plaintext.set(json, 2);
      return concat(
        header,
        sealTo(
          pair.public,
          plaintext,
          toHex(digest("web-fields/envelope/v1", header, recipient.publicKey)),
          random,
        ),
      );
    };
    expect({
      valid: openEnvelope(pair, craft([FIELDS[0]]), expected),
      twice: failure(() =>
        openEnvelope(
          pair,
          craft([FIELDS[0], { ...FIELDS[0], id: "again" }]),
          expected,
        ),
      ),
      unknownKind: failure(() =>
        openEnvelope(pair, craft([{ ...FIELDS[0], kind: "email" }]), expected),
      ),
      // A known kind in a schema version this host does not read, beside a field it does.
      newerSchema: failure(() =>
        openEnvelope(
          pair,
          craft([{ ...FIELDS[0], schema: "postal-address/2" }, FIELDS[1]]),
          expected,
        ),
      ),
      kindNotText: failure(() =>
        openEnvelope(pair, craft([{ ...FIELDS[0], kind: 7 }]), expected),
      ),
    }).toEqual({
      valid: { account: ACCOUNT, details: [FIELDS[0]!] },
      twice: "invalid",
      unknownKind: "unsupported",
      newerSchema: "unsupported",
      kindNotText: "invalid",
    });
  });
});
