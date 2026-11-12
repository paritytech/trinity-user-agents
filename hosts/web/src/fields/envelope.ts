import { blake2b } from "@noble/hashes/blake2.js";
import {
  concat,
  fromHex,
  fromUtf8,
  toHex,
  utf8,
  type RandomBytes,
} from "../../bridges/primitives/bytes.js";
import {
  FIELD_SCHEMAS,
  FieldError,
  MAX_ADDRESS_LINES,
  MAX_FIELDS,
  MAX_VALUE_CHARS,
  isFieldId,
  isFieldKind,
  isHex32,
  type FieldKind,
} from "../../bridges/fields/protocol.js";
import {
  digest,
  openSealed,
  sealTo,
  type X25519Pair,
} from "../invitations/sealing.js";

/**
 * PROTOTYPE. The sealed field envelope, version 1, for one recipient:
 *
 *   version (1) = 1 || suite (1) = 1 || key id (16) || sealed
 *
 * `sealed` is {@link sealTo}'s output (X25519, HKDF-SHA256,
 * ChaCha20-Poly1305). Its key is derived with the version, suite, key id and
 * the recipient's public key, so changing any of them, or any sealed byte,
 * fails to open. Inside, padded with zeros to a multiple of
 * {@link PAD_BYTES}:
 *
 *   length (2, big endian) || JSON { product, account, context, fields: [{ id, kind, schema, value }] }
 *
 * The recipient's host checks that `product` and `account` are its own and
 * `context` is the one its product expects. The context labels the envelope
 * for one use the product defines; it proves nothing about why it was sent.
 * Nothing here depends on how the product carries the bytes. The sender is not
 * authenticated: anyone with the public key can seal to it. Size shows only
 * how many padding buckets the contents took.
 */

export const ENVELOPE_VERSION = 1;
/** X25519, HKDF-SHA256, ChaCha20-Poly1305, as {@link sealTo} composes them. */
const SUITE = 1;
const HEADER_BYTES = 2 + 16;
const PAD_BYTES = 256;
/** The largest plaintext, padding included. */
const MAX_PLAINTEXT = 1024;
/** What {@link sealTo} adds: the ephemeral public key and the tag. */
const SEAL_OVERHEAD = 32 + 16;
export const MAX_ENVELOPE_BYTES = HEADER_BYTES + SEAL_OVERHEAD + MAX_PLAINTEXT;

/** One field inside an envelope. */
export interface SealedField {
  id: string;
  kind: FieldKind;
  schema: string;
  value: string;
}

/** Who an envelope is for, and the use it is labelled with. */
export interface EnvelopeBinding {
  product: string;
  /** The recipient's product account, `0x` hex. */
  account: string;
  /** 32 bytes, `0x` hex, chosen by the product. */
  context: string;
}

/** What a sender seals to. */
export interface EnvelopeRecipient {
  keyId: string;
  publicKey: string;
}

const purposeOf = (header: Uint8Array, publicKey: string): string =>
  toHex(digest("web-fields/envelope/v1", header, publicKey.toLowerCase()));

/** A value's lines as a host view shows them: trimmed and non-empty. */
export function valueLines(kind: FieldKind, value: string): string[] {
  const lines = value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  return kind === "address" ? lines : [lines.join(" ")];
}

/** Whether `value` is a valid value of `kind` for this draft. */
export function validValue(kind: FieldKind, value: unknown): value is string {
  if (
    typeof value !== "string" ||
    value.trim().length === 0 ||
    value.length > MAX_VALUE_CHARS
  )
    return false;
  return kind === "address"
    ? valueLines(kind, value).length <= MAX_ADDRESS_LINES
    : !/[\r\n]/.test(value);
}

/** Whether `fields` fit an envelope: 1 to {@link MAX_FIELDS}, valid, no id or kind twice. */
function validFields(fields: readonly SealedField[]): boolean {
  return (
    fields.length > 0 &&
    fields.length <= MAX_FIELDS &&
    new Set(fields.map((field) => field.id)).size === fields.length &&
    new Set(fields.map((field) => field.kind)).size === fields.length &&
    fields.every(
      (field) =>
        isFieldId(field.id) &&
        isFieldKind(field.kind) &&
        field.schema === FIELD_SCHEMAS[field.kind] &&
        validValue(field.kind, field.value),
    )
  );
}

/** Seal `fields` for `binding` to `recipient`'s key. */
export function sealEnvelope(
  recipient: EnvelopeRecipient,
  binding: EnvelopeBinding,
  fields: readonly SealedField[],
  random: RandomBytes,
): Uint8Array {
  if (
    !validFields(fields) ||
    !isHex32(binding.account) ||
    !isHex32(binding.context)
  )
    throw new FieldError("invalid", "The fields do not fit an envelope.");
  const json = utf8(
    JSON.stringify({
      product: binding.product,
      account: binding.account.toLowerCase(),
      context: binding.context.toLowerCase(),
      fields: fields.map(({ id, kind, schema, value }) => ({
        id,
        kind,
        schema,
        value,
      })),
    }),
  );
  const size = Math.ceil((2 + json.length) / PAD_BYTES) * PAD_BYTES;
  if (size > MAX_PLAINTEXT)
    throw new FieldError("limit", "These values are too long to send.");
  const plaintext = new Uint8Array(size);
  new DataView(plaintext.buffer).setUint16(0, json.length);
  plaintext.set(json, 2);
  const header = concat(
    Uint8Array.of(ENVELOPE_VERSION, SUITE),
    fromHex(recipient.keyId),
  );
  try {
    return concat(
      header,
      sealTo(
        fromHex(recipient.publicKey),
        plaintext,
        purposeOf(header, recipient.publicKey),
        random,
      ),
    );
  } finally {
    plaintext.fill(0);
    json.fill(0);
  }
}

/** The key id an envelope names, after checking its version, suite and size. */
export function envelopeKeyId(envelope: Uint8Array): string {
  if (envelope.length < 2 || envelope[0] !== ENVELOPE_VERSION)
    throw new FieldError(
      "unsupported",
      "This envelope's version is not supported.",
    );
  if (envelope[1] !== SUITE)
    throw new FieldError(
      "unsupported",
      "This envelope's suite is not supported.",
    );
  const body = envelope.length - HEADER_BYTES - SEAL_OVERHEAD;
  if (
    envelope.length > MAX_ENVELOPE_BYTES ||
    body < PAD_BYTES ||
    body % PAD_BYTES !== 0
  )
    throw new FieldError("invalid", "This envelope is malformed.");
  return toHex(envelope.subarray(2, HEADER_BYTES));
}

/** 32 bytes naming these envelope bytes, for a product to tell copies apart. */
export const envelopeId = (envelope: Uint8Array): string =>
  toHex(blake2b(envelope, { dkLen: 32 }));

const malformed = () =>
  new FieldError("invalid", "This envelope's contents are malformed.");

/**
 * Open `envelope` with the recipient's pair and check it is for `expected`.
 * Throws `invalid` when it does not open, its contents break a rule, or it
 * names another product, account or context; `unsupported` for a version,
 * suite, field kind or schema this host does not read. Either refuses the
 * whole envelope. Answers the fields and which of `expected.accounts` the
 * envelope names.
 */
export function openEnvelope(
  pair: X25519Pair,
  envelope: Uint8Array,
  expected: { product: string; accounts: readonly string[]; context: string },
): { account: string; fields: SealedField[] } {
  envelopeKeyId(envelope);
  const header = envelope.subarray(0, HEADER_BYTES);
  const plaintext = openSealed(
    pair,
    envelope.subarray(HEADER_BYTES),
    purposeOf(header, toHex(pair.public)),
  );
  if (plaintext === null)
    throw new FieldError(
      "invalid",
      "This envelope does not open with this app's key: it was changed or sealed to another key.",
    );
  let inner: Record<string, unknown>;
  try {
    const length = new DataView(
      plaintext.buffer,
      plaintext.byteOffset,
    ).getUint16(0);
    if (
      2 + length > plaintext.length ||
      plaintext.subarray(2 + length).some((byte) => byte !== 0)
    )
      throw malformed();
    try {
      inner = JSON.parse(fromUtf8(plaintext.subarray(2, 2 + length))) as Record<
        string,
        unknown
      >;
    } catch {
      throw malformed();
    }
  } finally {
    plaintext.fill(0);
  }
  if (
    typeof inner !== "object" ||
    inner === null ||
    !Array.isArray(inner.fields)
  )
    throw malformed();
  if (
    inner.product !== expected.product ||
    typeof inner.account !== "string" ||
    !expected.accounts.includes(inner.account)
  )
    throw new FieldError(
      "invalid",
      "This envelope is for another app or account.",
    );
  if (inner.context !== expected.context.toLowerCase())
    throw new FieldError("invalid", "This envelope is for another context.");
  const fields = (inner.fields as unknown[]).map((item): SealedField => {
    const field = (item ?? {}) as Record<string, unknown>;
    return {
      id: field.id as string,
      kind: field.kind as FieldKind,
      schema: field.schema as string,
      value: field.value as string,
    };
  });
  // A field this host does not read refuses the whole envelope, as
  // `unsupported`, so the product can tell it from a broken one.
  if (
    fields.some(
      (field) =>
        typeof field.kind === "string" &&
        typeof field.schema === "string" &&
        (!isFieldKind(field.kind) ||
          field.schema !== FIELD_SCHEMAS[field.kind]),
    )
  )
    throw new FieldError(
      "unsupported",
      "This envelope holds a field kind or schema this host does not read.",
    );
  if (!validFields(fields)) throw malformed();
  return { account: inner.account, fields };
}
