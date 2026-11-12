/**
 * DRAFT, web-only. Locket: the bridge between a host and its products for
 * the user's private details, such as a postal address or a phone number. It
 * is not TrUAPI and not a cross-platform agreement: it is the web prototype
 * of a contract meant to read the same on iOS and Android, so nothing here
 * names a DOM node, a URL or a browser store. How a platform draws a detail
 * is behind {@link DetailSurface}; the web form is in `web.ts`.
 *
 * - A provider product (`locketProvider`) declares the kinds of detail it
 *   holds and sets their values.
 * - A consumer product (`locket`) asks for typed details. The host picks the
 *   provider with the user and shows the exact values. The product gets
 *   opaque refs, and, when it names a recipient, a sealed envelope only that
 *   recipient's host opens. It never gets a value.
 * - A recipient's host makes the recipient key, keeps its secret, and turns
 *   a delivered envelope back into refs.
 * - Delivery is the product's: it carries and stores envelopes and keys as
 *   opaque bytes over any transport it chooses. Size, lifetime and delivery
 *   on that transport are the product's to handle.
 */
export const LOCKET_PROTOCOL = "web-locket/1";
/** The message type a host offers the bridge's port with, after a document loads. */
export const LOCKET_OFFER = "web-locket-offer";

/** The kinds of detail this draft understands. */
export type DetailKind = "address" | "phone";
export const DETAIL_KINDS: readonly DetailKind[] = ["address", "phone"];

/** The value schema of each kind. An envelope names it, so a reader can refuse one it does not know. */
export const DETAIL_SCHEMAS: Readonly<Record<DetailKind, string>> = {
  address: "postal-address/1",
  phone: "phone-number/1",
};

/** The most details one request or envelope carries. */
export const MAX_DETAILS = 4;
/** The most characters of one value, and lines of an address. */
export const MAX_VALUE_CHARS = 160;
export const MAX_ADDRESS_LINES = 6;

/** Which product role the host gave a connection. */
export type LocketRole = "provider" | "consumer";

/**
 * Why a bridge request failed, for a product to choose its reaction without
 * reading the message.
 *
 * - `cancelled`: the user declined in a host sheet.
 * - `timeout`: the call took too long and the client withdrew it. The host
 *   stopped its pending work: a sheet closes, and nothing it would have
 *   returned (refs, an envelope, a key, an image) is kept. Work already
 *   done stays done, such as refs a `release` had let go, so retry.
 * - `missing-key`: no recipient key of this app on this device matches the
 *   envelope's key id, such as after site data was cleared, on another
 *   device, or for bytes that were never sealed here. Nothing restores a lost
 *   key. It says nothing about who sealed the envelope or for whom: if it
 *   was meant for this account, the sender has to send again.
 * - `missing-ref`: the ref was released, or belongs to another connection
 *   or document. Open or request the detail again.
 * - `unavailable`: what was asked for is not there now, such as no provider
 *   holding a requested kind, the host failing to draw, or this device's
 *   stored recipient keys being unreadable (they are kept, not replaced).
 * - `unsupported`: a version, suite, detail kind or schema, role or
 *   operation this host does not handle.
 * - `invalid`: malformed input, or an envelope that does not open or check
 *   out (changed bytes, another key, product, account or context).
 * - `limit`, `busy`, `session`: a bound was reached; a single-use resource is
 *   in use or too many calls are unanswered at once; the session or
 *   connection ended.
 */
export type LocketErrorCode =
  | "cancelled"
  | "timeout"
  | "missing-key"
  | "missing-ref"
  | "invalid"
  | "unavailable"
  | "limit"
  | "busy"
  | "session"
  | "unsupported";

/** A bridge failure with its code. The message is for people; it never names a protected value. */
export class LocketError extends Error {
  constructor(
    readonly code: LocketErrorCode,
    message: string,
  ) {
    super(message);
  }
}

/** A failed reply as it crosses the bridge. */
export interface LocketErrorReply {
  code: LocketErrorCode;
  message: string;
}

/** What a provider product may learn about itself: never the values. */
export interface ProviderStatus {
  registered: boolean;
  name: string | null;
  details: DetailKind[];
  /** The declared kinds that hold a value in this host session. */
  saved: DetailKind[];
}

/**
 * Opaque reference to one detail value the host holds for this product
 * connection. Random, not derived from the value, valid only on the
 * connection that got it, and gone when that connection, the product document
 * or the session ends. It cannot be stored or passed to another product.
 */
export type DetailRef = string;

/** One detail a consumer asks for. `id` is the product's own name for it. */
export interface DetailRequestItem {
  id: string;
  kind: DetailKind;
  /** The user may leave an optional detail out. */
  optional?: boolean;
}

/** A detail the host holds for the product: its id, kind and schema, never its value. */
export interface DetailDescriptor {
  id: string;
  kind: DetailKind;
  schema: string;
  ref: DetailRef;
}

/**
 * What says a recipient key belongs to its account: the recipient's host
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
 * A recipient's public key, as its host made it: one per wallet, network,
 * product and product account, so two accounts never share one. Public: the
 * product publishes it however it delivers.
 */
export interface RecipientKey {
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

/** Ask the host for details, sealed to `recipient` when one is named. */
export interface DetailRequest {
  details: DetailRequestItem[];
  /** One line the sheet shows as the product's own words, 1 to 80 characters. */
  purpose: string;
  /**
   * 32 bytes, `0x` hex, chosen by the product for one use, such as a hash
   * of an order or a message id. An envelope opens only with the same
   * context. It labels the envelope; it proves nothing about that use.
   */
  context: string;
  recipient?: RecipientKey;
}

/** What a granted request answers. */
export interface DetailGrant {
  details: DetailDescriptor[];
  /** Present when the request named a recipient: the sealed envelope, `0x` hex. */
  envelope?: string;
}

/** What opening an envelope answers. */
export interface OpenedEnvelope {
  /** 32 bytes, `0x` hex: the same for the same envelope bytes, for the product's own dedupe. */
  envelopeId: string;
  /** The product account, `0x` hex, the envelope was sealed for. */
  account: string;
  details: DetailDescriptor[];
}

/**
 * Where a platform draws a protected detail. Its shape belongs to the
 * platform's adapter, never to a product's own code.
 */
export interface DetailSurface {
  platform: string;
}

/** How a host-drawn detail looks. The product picks one, never CSS. */
export type DetailTheme = "paper" | "dark";

/** The consumer operations (`locket`), typed in one place. */
export interface LocketCalls {
  /**
   * The recipient key of this app's account at `Raw(index)` (32 bytes, `0x`
   * hex): made on first use, then the same one for that account. Another
   * index gets another key.
   */
  recipientKey(input: { index: string }): RecipientKey;
  request(input: DetailRequest): DetailGrant;
  open(input: { envelope: string; context: string }): OpenedEnvelope;
  release(input: { refs: DetailRef[] }): { released: number };
  /** Where the host draws `ref`: a surface only the platform's adapter places. */
  display(input: { ref: DetailRef; theme: DetailTheme }): {
    surface: DetailSurface;
  };
  /**
   * Print a label: the app's own `title` and `lines`, which are public, then
   * the values of `refs`, drawn and printed by the host.
   */
  print(input: { title: string; lines: string[]; refs: DetailRef[] }): {
    printed: true;
  };
}

/**
 * A provider's complete set of values for `setValues`: every kind it
 * declared, each a value or `null` for none, and no other kind.
 */
export type DetailValues = Partial<Record<DetailKind, string | null>>;

/** The provider operations (`locketProvider`). */
export interface LocketProviderCalls {
  status(input: Record<string, never>): ProviderStatus;
  /**
   * Say which kinds this app holds and what to call it. Declaring the same
   * name and kinds again answers as the first time; a different declaration
   * fails with `invalid`, as nothing updates one yet.
   */
  declare(input: { name: string; details: DetailKind[] }): {
    registered: DetailKind[];
  };
  /**
   * Replace every saved value: `values` names each declared kind, a value or
   * `null`, and the kinds not `null` are what is saved. It never merges.
   */
  setValues(input: { values: DetailValues }): {
    saved: DetailKind[];
  };
}

const HEX = (bytes: number) => new RegExp(`^0x[0-9a-f]{${bytes * 2}}$`);
const HEX16 = HEX(16);
const HEX32 = HEX(32);
const HEX64 = HEX(64);

/** Whether `value` is `0x` and exactly 32 bytes of lowercase hex. */
export const isHex32 = (value: unknown): value is string =>
  typeof value === "string" && HEX32.test(value);

/** A product's own detail id: a lowercase letter, then up to 31 of `a-z`, `0-9`, `-`. */
export const isDetailId = (value: unknown): value is string =>
  typeof value === "string" && /^[a-z][a-z0-9-]{0,31}$/.test(value);

export const isDetailKind = (value: unknown): value is DetailKind =>
  DETAIL_KINDS.includes(value as DetailKind);

/**
 * Read a recipient key from untrusted bytes, such as a product's own message, or
 * null. Copies only the known members.
 */
export function readRecipientKey(value: unknown): RecipientKey | null {
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
