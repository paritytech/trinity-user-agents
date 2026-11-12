/**
 * DRAFT, web-only. The field bridge between a host and its products. It is not
 * TrUAPI and not a cross-platform agreement: it is the web prototype of a
 * contract meant to read the same on iOS and Android, so nothing here names a
 * DOM node, a URL or a browser store. How a platform draws a field is behind
 * {@link FieldSurface}; the web form is in `web.ts`.
 *
 * - A provider product declares the kinds of field it holds and saves values.
 * - A consumer product asks for typed fields. The host picks the provider with
 *   the user and shows the exact values. The product gets opaque handles, and,
 *   when it names a recipient, a sealed envelope only that recipient's host
 *   opens. It never gets a value.
 * - A recipient's host makes the receiving key, keeps its secret, and turns a
 *   delivered envelope back into handles.
 * - Delivery is the product's: it carries and stores envelopes and keys as
 *   opaque bytes over any transport it chooses. Size, lifetime and delivery
 *   on that transport are the product's to handle.
 */
export const FIELDS_PROTOCOL = "web-fields/2";
/** The message type a host offers the bridge's port with, after a document loads. */
export const FIELDS_OFFER = "web-fields-offer";

/** The kinds of field this draft understands. */
export type FieldKind = "address" | "phone";
export const FIELD_KINDS: readonly FieldKind[] = ["address", "phone"];

/** The value schema of each kind. An envelope names it, so a reader can refuse one it does not know. */
export const FIELD_SCHEMAS: Readonly<Record<FieldKind, string>> = {
  address: "postal-address/1",
  phone: "phone-number/1",
};

/** The most fields one request or envelope carries. */
export const MAX_FIELDS = 4;
/** The most characters of one value, and lines of an address. */
export const MAX_VALUE_CHARS = 160;
export const MAX_ADDRESS_LINES = 6;

/** Which product role the host gave a connection. */
export type FieldRole = "provider" | "consumer";

/**
 * Why a bridge request failed, for a product to choose its reaction without
 * reading the message: `cancelled` is the user's choice in a host sheet.
 */
export type FieldErrorCode =
  | "cancelled"
  | "invalid"
  | "unavailable"
  | "limit"
  | "busy"
  | "session"
  | "unsupported";

/** A bridge failure with its code. The message is for people; it never names a protected value. */
export class FieldError extends Error {
  constructor(
    readonly code: FieldErrorCode,
    message: string,
  ) {
    super(message);
  }
}

/** A failed reply as it crosses the bridge. */
export interface FieldErrorReply {
  code: FieldErrorCode;
  message: string;
}

/** What a provider product may learn about itself: never the values. */
export interface ProviderStatus {
  registered: boolean;
  name: string | null;
  fields: FieldKind[];
  /** The declared fields that hold a value in this host session. */
  saved: FieldKind[];
}

/**
 * Opaque reference to one field value the host holds for this product
 * connection. Random, not derived from the value, valid only on the
 * connection that got it, and gone when that connection, the product document
 * or the session ends. It cannot be stored or passed to another product.
 */
export type FieldHandle = string;

/** One field a consumer asks for. `id` is the product's own name for it. */
export interface FieldRequestItem {
  id: string;
  kind: FieldKind;
  /** The user may leave an optional field out. */
  optional?: boolean;
}

/** A field the host holds for the product: its id, kind and schema, never its value. */
export interface FieldDescriptor {
  id: string;
  kind: FieldKind;
  schema: string;
  handle: FieldHandle;
}

/**
 * What says a receiving key belongs to its account: the recipient's host
 * signed, as that product account, the key id, the public key and the
 * network, under a signing context no product signing call uses
 * (`host-signature`, the core's `host_logic::field_key_binding`). A sender's
 * host checks it without reaching the recipient's host. It shows that a
 * holder of the account's wallet keys bound the key; it does not show who
 * the person is, or that their host is honest.
 */
export interface KeyBinding {
  kind: "host-signature";
  /** sr25519, 64 bytes, `0x` hex. */
  signature: string;
}

/**
 * A recipient's public receiving key, as its host made it: one per wallet
 * and product. Public: the product publishes it however it delivers.
 */
export interface ReceiveKey {
  v: 1;
  /** 16 bytes, `0x` hex: names the key in an envelope. */
  keyId: string;
  /** X25519 public key, 32 bytes, `0x` hex. */
  publicKey: string;
  /** The product the key opens envelopes for. */
  product: string;
  /** That product's account the key is bound to, 32 bytes, `0x` hex. */
  account: string;
  /** The genesis hash of the network's People chain, 32 bytes, `0x` hex. */
  network: string;
  binding: KeyBinding;
}

/** Ask the host for fields, sealed to `recipient` when one is named. */
export interface FieldRequest {
  fields: FieldRequestItem[];
  /** One line the sheet shows as the product's own words, 1 to 80 characters. */
  purpose: string;
  /**
   * 32 bytes, `0x` hex, chosen by the product for one use, such as a hash
   * of an order or a message id. An envelope opens only with the same
   * context. It labels the envelope; it proves nothing about that use.
   */
  context: string;
  recipient?: ReceiveKey;
}

/** What a granted request answers. */
export interface FieldGrant {
  fields: FieldDescriptor[];
  /** Present when the request named a recipient: the sealed envelope, `0x` hex. */
  envelope?: string;
}

/** What opening an envelope answers. */
export interface OpenedEnvelope {
  /** 32 bytes, `0x` hex: the same for the same envelope bytes, for the product's own dedupe. */
  envelopeId: string;
  fields: FieldDescriptor[];
}

/**
 * Where a platform draws a protected field. Its shape belongs to the
 * platform's adapter, never to a product's own code.
 */
export interface FieldSurface {
  platform: string;
}

/** How a host-drawn field looks. The product picks one, never CSS. */
export type FieldTheme = "paper" | "dark";

/** The consumer operations, typed in one place. */
export interface ConsumerCalls {
  /** This app's receiving key, bound to its account at `Raw(index)` (32 bytes, `0x` hex). */
  createReceiveKey(input: { index: string }): ReceiveKey;
  request(input: FieldRequest): FieldGrant;
  open(input: { envelope: string; context: string }): OpenedEnvelope;
  release(input: { handles: FieldHandle[] }): { released: number };
  display(input: { handle: FieldHandle; theme: FieldTheme }): {
    surface: FieldSurface;
  };
  /**
   * Print a label: the app's own `title` and `lines`, which are public, then
   * the values of `handles`, drawn and printed by the host.
   */
  print(input: { title: string; lines: string[]; handles: FieldHandle[] }): {
    printed: true;
  };
}

/** The provider operations. */
export interface ProviderCalls {
  status(input: Record<string, never>): ProviderStatus;
  declare(input: { name: string; fields: FieldKind[] }): {
    registered: FieldKind[];
  };
  save(input: { values: Partial<Record<FieldKind, string>> }): {
    saved: FieldKind[];
  };
}

const HEX = (bytes: number) => new RegExp(`^0x[0-9a-f]{${bytes * 2}}$`);
const HEX16 = HEX(16);
const HEX32 = HEX(32);
const HEX64 = HEX(64);

/** Whether `value` is `0x` and exactly 32 bytes of lowercase hex. */
export const isHex32 = (value: unknown): value is string =>
  typeof value === "string" && HEX32.test(value);

/** A product's own field id: a lowercase letter, then up to 31 of `a-z`, `0-9`, `-`. */
export const isFieldId = (value: unknown): value is string =>
  typeof value === "string" && /^[a-z][a-z0-9-]{0,31}$/.test(value);

export const isFieldKind = (value: unknown): value is FieldKind =>
  FIELD_KINDS.includes(value as FieldKind);

/**
 * Read a receiving key from untrusted bytes, such as a product's own message, or
 * null. Copies only the known members.
 */
export function readReceiveKey(value: unknown): ReceiveKey | null {
  if (typeof value !== "object" || value === null) return null;
  const key = value as Record<string, unknown>;
  const binding = key.binding as Record<string, unknown> | null | undefined;
  if (
    key.v !== 1 ||
    typeof key.keyId !== "string" ||
    !HEX16.test(key.keyId) ||
    !isHex32(key.publicKey) ||
    !isHex32(key.account) ||
    typeof key.product !== "string" ||
    key.product.length === 0 ||
    key.product.length > 253 ||
    !isHex32(key.network) ||
    binding?.kind !== "host-signature" ||
    typeof binding.signature !== "string" ||
    !HEX64.test(binding.signature)
  )
    return null;
  return {
    v: 1,
    keyId: key.keyId,
    publicKey: key.publicKey,
    product: key.product,
    account: key.account,
    network: key.network,
    binding: { kind: "host-signature", signature: binding.signature },
  };
}
